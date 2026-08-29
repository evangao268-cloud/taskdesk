//! OAuth 2.0 for a desktop app: PKCE + loopback redirect. The refresh token
//! lives in Windows Credential Manager; access tokens stay in memory.

use std::net::TcpListener;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{ClientConfig, GoogleError};

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const DEFAULT_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_USERINFO_ENDPOINT: &str = "https://openidconnect.googleapis.com/v1/userinfo";
const KEYRING_SERVICE: &str = "taskdesk";
const KEYRING_USER: &str = "google_refresh_token";
const SCOPES: &str = "https://www.googleapis.com/auth/tasks openid email";

fn b64url(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    #[serde(default)]
    error: String,
}

struct CachedAccessToken {
    token: String,
    expires_at: Instant,
}

/// Refresh-token storage, swappable for tests.
pub trait TokenStore: Send + Sync {
    fn get(&self) -> Option<String>;
    fn set(&self, token: &str) -> Result<(), String>;
    fn delete(&self) -> Result<(), String>;
}

pub struct KeyringStore;

impl TokenStore for KeyringStore {
    fn get(&self) -> Option<String> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .ok()?
            .get_password()
            .ok()
    }
    fn set(&self, token: &str) -> Result<(), String> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| e.to_string())?
            .set_password(token)
            .map_err(|e| e.to_string())
    }
    fn delete(&self) -> Result<(), String> {
        match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
            Ok(entry) => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(e.to_string()),
            },
            Err(e) => Err(e.to_string()),
        }
    }
}

pub struct AuthClient {
    config: ClientConfig,
    token_endpoint: String,
    userinfo_endpoint: String,
    store: Box<dyn TokenStore>,
    http: reqwest::Client,
    cached: Mutex<Option<CachedAccessToken>>,
}

impl AuthClient {
    pub fn new(config: ClientConfig) -> Self {
        Self::with_endpoints(
            config,
            DEFAULT_TOKEN_ENDPOINT.into(),
            DEFAULT_USERINFO_ENDPOINT.into(),
            Box::new(KeyringStore),
        )
    }

    pub fn with_endpoints(
        config: ClientConfig,
        token_endpoint: String,
        userinfo_endpoint: String,
        store: Box<dyn TokenStore>,
    ) -> Self {
        Self {
            config,
            token_endpoint,
            userinfo_endpoint,
            store,
            http: reqwest::Client::new(),
            cached: Mutex::new(None),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.store.get().is_some()
    }

    pub fn disconnect(&self) -> Result<(), GoogleError> {
        *self.cached.lock().unwrap() = None;
        self.store.delete().map_err(GoogleError::Other)
    }

    /// Full interactive sign-in. Opens the system browser via `open_url`,
    /// catches the redirect on an ephemeral loopback port, exchanges the code,
    /// stores the refresh token, and returns the account email.
    pub async fn interactive_signin(
        &self,
        open_url: impl FnOnce(String) -> Result<(), String>,
    ) -> Result<String, GoogleError> {
        // PKCE verifier + S256 challenge, plus CSRF state.
        let mut raw = [0u8; 48];
        rand::thread_rng().fill_bytes(&mut raw);
        let verifier = b64url(&raw);
        let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
        let mut raw_state = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut raw_state);
        let state = b64url(&raw_state);

        // Ephemeral loopback port; Google accepts any port for desktop clients.
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| GoogleError::Other(format!("cannot bind loopback: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| GoogleError::Other(e.to_string()))?
            .port();
        let redirect_uri = format!("http://127.0.0.1:{port}");

        let auth_url = url::Url::parse_with_params(
            AUTH_ENDPOINT,
            &[
                ("client_id", self.config.client_id.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("response_type", "code"),
                ("scope", SCOPES),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "S256"),
                ("state", state.as_str()),
                ("access_type", "offline"),
                ("prompt", "consent"),
            ],
        )
        .map_err(|e| GoogleError::Other(e.to_string()))?;

        open_url(auth_url.to_string()).map_err(GoogleError::Other)?;

        // Serve exactly one redirect request (blocking accept on a worker thread).
        let code = tauri::async_runtime::spawn_blocking(move || {
            wait_for_code(listener, &state, Duration::from_secs(300))
        })
        .await
        .map_err(|e| GoogleError::Other(e.to_string()))??;

        // Exchange code + verifier for tokens.
        let resp = self
            .http
            .post(&self.token_endpoint)
            .form(&[
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("code", code.as_str()),
                ("code_verifier", verifier.as_str()),
                ("grant_type", "authorization_code"),
                ("redirect_uri", redirect_uri.as_str()),
            ])
            .send()
            .await?;
        let token = parse_token_response(resp).await?;
        let refresh = token
            .refresh_token
            .ok_or_else(|| GoogleError::Other("Google returned no refresh token".into()))?;
        self.store.set(&refresh).map_err(GoogleError::Other)?;
        self.cache_access(&token.access_token, token.expires_in);

        // Identify the account for display.
        let email = self.fetch_email(&token.access_token).await.unwrap_or_default();
        Ok(email)
    }

    async fn fetch_email(&self, access_token: &str) -> Result<String, GoogleError> {
        #[derive(Deserialize)]
        struct UserInfo {
            #[serde(default)]
            email: String,
        }
        let info: UserInfo = self
            .http
            .get(&self.userinfo_endpoint)
            .bearer_auth(access_token)
            .send()
            .await?
            .json()
            .await?;
        Ok(info.email)
    }

    fn cache_access(&self, token: &str, expires_in: Option<u64>) {
        let ttl = expires_in.unwrap_or(3600);
        *self.cached.lock().unwrap() = Some(CachedAccessToken {
            token: token.to_string(),
            expires_at: Instant::now() + Duration::from_secs(ttl),
        });
    }

    /// A valid access token, refreshing when less than a minute remains.
    pub async fn access_token(&self) -> Result<String, GoogleError> {
        if let Some(cached) = self.cached.lock().unwrap().as_ref() {
            if cached.expires_at > Instant::now() + Duration::from_secs(60) {
                return Ok(cached.token.clone());
            }
        }
        let refresh = self.store.get().ok_or(GoogleError::NotConnected)?;
        let resp = self
            .http
            .post(&self.token_endpoint)
            .form(&[
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("refresh_token", refresh.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;
        match parse_token_response(resp).await {
            Ok(token) => {
                self.cache_access(&token.access_token, token.expires_in);
                Ok(token.access_token)
            }
            Err(GoogleError::InvalidGrant) => {
                // Revoked or expired: wipe so the UI can prompt a reconnect.
                let _ = self.store.delete();
                *self.cached.lock().unwrap() = None;
                Err(GoogleError::InvalidGrant)
            }
            Err(e) => Err(e),
        }
    }
}

async fn parse_token_response(resp: reqwest::Response) -> Result<TokenResponse, GoogleError> {
    let status = resp.status();
    let body = resp.text().await?;
    if status.is_success() {
        serde_json::from_str(&body).map_err(|e| GoogleError::Other(e.to_string()))
    } else {
        let err: TokenErrorResponse = serde_json::from_str(&body).unwrap_or(TokenErrorResponse {
            error: String::new(),
        });
        if err.error == "invalid_grant" {
            Err(GoogleError::InvalidGrant)
        } else {
            Err(GoogleError::Http(status.as_u16(), body))
        }
    }
}

/// Block until the OAuth redirect arrives (or the deadline passes), validate
/// the CSRF state, and hand back the authorization code.
fn wait_for_code(
    listener: TcpListener,
    expected_state: &str,
    timeout: Duration,
) -> Result<String, GoogleError> {
    let server = tiny_http::Server::from_listener(listener, None)
        .map_err(|e| GoogleError::Other(e.to_string()))?;
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(GoogleError::Timeout)?;
        let Some(request) = server
            .recv_timeout(remaining)
            .map_err(|e| GoogleError::Other(e.to_string()))?
        else {
            return Err(GoogleError::Timeout);
        };

        let full_url = format!("http://127.0.0.1{}", request.url());
        let parsed = url::Url::parse(&full_url).map_err(|e| GoogleError::Other(e.to_string()))?;
        let get = |k: &str| {
            parsed
                .query_pairs()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.to_string())
        };

        // Browsers also ask for /favicon.ico — answer only the real redirect.
        let (code, state) = (get("code"), get("state"));
        match (code, state, get("error")) {
            (Some(code), Some(state), _) if state == expected_state => {
                let page = tiny_http::Response::from_string(
                    "<html><body style=\"font-family:sans-serif\"><h2>TaskDesk is connected.</h2>\
                     You can close this tab.</body></html>",
                )
                .with_header("Content-Type: text/html".parse::<tiny_http::Header>().unwrap());
                let _ = request.respond(page);
                return Ok(code);
            }
            (_, _, Some(err)) => {
                let _ = request.respond(tiny_http::Response::from_string("Sign-in failed."));
                return Err(GoogleError::Other(format!("consent denied: {err}")));
            }
            _ => {
                let _ = request.respond(tiny_http::Response::empty(404));
            }
        }
    }
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// In-memory TokenStore so tests never touch Credential Manager.
    pub struct MemStore(pub StdMutex<Option<String>>);

    impl TokenStore for MemStore {
        fn get(&self) -> Option<String> {
            self.0.lock().unwrap().clone()
        }
        fn set(&self, token: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(token.to_string());
            Ok(())
        }
        fn delete(&self) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }
}

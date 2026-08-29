pub mod auth;
pub mod tasks_api;

use serde::Deserialize;

/// OAuth client credentials. Loaded at runtime so the repo never carries them:
/// env vars first, then `google_client.json` in the app data dir. Per Google's
/// docs a desktop client secret is not actually secret — PKCE guards the flow.
#[derive(Debug, Clone, Deserialize)]
pub struct ClientConfig {
    pub client_id: String,
    pub client_secret: String,
}

impl ClientConfig {
    pub fn load(app_data_dir: &std::path::Path) -> Option<Self> {
        if let (Ok(id), Ok(secret)) = (
            std::env::var("TASKDESK_GOOGLE_CLIENT_ID"),
            std::env::var("TASKDESK_GOOGLE_CLIENT_SECRET"),
        ) {
            if !id.is_empty() {
                return Some(Self {
                    client_id: id,
                    client_secret: secret,
                });
            }
        }
        let path = app_data_dir.join("google_client.json");
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GoogleError {
    #[error("Google account not connected")]
    NotConnected,
    #[error(
        "No OAuth client configured. Create google_client.json in the app data folder \
         with {{\"client_id\": \"...\", \"client_secret\": \"...\"}} — see README"
    )]
    NoClientConfig,
    #[error("Sign-in timed out — no browser response within 5 minutes")]
    Timeout,
    #[error("Google rejected the stored credentials; please reconnect")]
    InvalidGrant,
    #[error("HTTP {0}: {1}")]
    Http(u16, String),
    #[error("network error: {0}")]
    Network(String),
    #[error("{0}")]
    Other(String),
}

impl From<reqwest::Error> for GoogleError {
    fn from(e: reqwest::Error) -> Self {
        GoogleError::Network(e.to_string())
    }
}

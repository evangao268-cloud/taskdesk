//! Thin typed client for the Google Tasks REST API. Base URL is injectable so
//! tests run against a mock server. Knows nothing about SQLite.

use serde::{Deserialize, Serialize};

use super::auth::AuthClient;
use super::GoogleError;

const DEFAULT_BASE: &str = "https://tasks.googleapis.com/tasks/v1";

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteTaskList {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub updated: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTask {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// RFC3339 but Google only honors the date part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListPage<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
    #[serde(default)]
    next_page_token: Option<String>,
}

pub struct TasksApi<'a> {
    auth: &'a AuthClient,
    base: String,
    http: reqwest::Client,
}

impl<'a> TasksApi<'a> {
    pub fn new(auth: &'a AuthClient) -> Self {
        Self::with_base(auth, DEFAULT_BASE.into())
    }

    pub fn with_base(auth: &'a AuthClient, base: String) -> Self {
        Self {
            auth,
            base,
            http: reqwest::Client::new(),
        }
    }

    async fn check(resp: reqwest::Response) -> Result<reqwest::Response, GoogleError> {
        let status = resp.status();
        if status.is_success() {
            Ok(resp)
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(GoogleError::Http(status.as_u16(), body))
        }
    }

    pub async fn list_tasklists(&self) -> Result<Vec<RemoteTaskList>, GoogleError> {
        let token = self.auth.access_token().await?;
        let mut items = vec![];
        let mut page_token: Option<String> = None;
        loop {
            let mut req = self
                .http
                .get(format!("{}/users/@me/lists", self.base))
                .bearer_auth(&token)
                .query(&[("maxResults", "100")]);
            if let Some(t) = &page_token {
                req = req.query(&[("pageToken", t)]);
            }
            let page: ListPage<RemoteTaskList> =
                Self::check(req.send().await?).await?.json().await?;
            items.extend(page.items);
            match page.next_page_token {
                Some(t) => page_token = Some(t),
                None => return Ok(items),
            }
        }
    }

    /// All tasks in a list, following pagination. `updated_min` (RFC3339)
    /// switches the call to incremental mode, which requires showDeleted.
    pub async fn list_tasks(
        &self,
        tasklist_id: &str,
        updated_min: Option<&str>,
    ) -> Result<Vec<RemoteTask>, GoogleError> {
        let token = self.auth.access_token().await?;
        let mut items = vec![];
        let mut page_token: Option<String> = None;
        loop {
            let mut req = self
                .http
                .get(format!("{}/lists/{}/tasks", self.base, tasklist_id))
                .bearer_auth(&token)
                .query(&[
                    ("maxResults", "100"),
                    ("showCompleted", "true"),
                    ("showHidden", "true"),
                    ("showDeleted", "true"),
                ]);
            if let Some(min) = updated_min {
                req = req.query(&[("updatedMin", min)]);
            }
            if let Some(t) = &page_token {
                req = req.query(&[("pageToken", t)]);
            }
            let page: ListPage<RemoteTask> = Self::check(req.send().await?).await?.json().await?;
            items.extend(page.items);
            match page.next_page_token {
                Some(t) => page_token = Some(t),
                None => return Ok(items),
            }
        }
    }

    pub async fn insert_task(
        &self,
        tasklist_id: &str,
        task: &RemoteTask,
    ) -> Result<RemoteTask, GoogleError> {
        let token = self.auth.access_token().await?;
        let resp = self
            .http
            .post(format!("{}/lists/{}/tasks", self.base, tasklist_id))
            .bearer_auth(&token)
            .json(task)
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    pub async fn patch_task(
        &self,
        tasklist_id: &str,
        task_id: &str,
        task: &RemoteTask,
    ) -> Result<RemoteTask, GoogleError> {
        let token = self.auth.access_token().await?;
        let resp = self
            .http
            .patch(format!("{}/lists/{}/tasks/{}", self.base, tasklist_id, task_id))
            .bearer_auth(&token)
            .json(task)
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    pub async fn delete_task(&self, tasklist_id: &str, task_id: &str) -> Result<(), GoogleError> {
        let token = self.auth.access_token().await?;
        let resp = self
            .http
            .delete(format!("{}/lists/{}/tasks/{}", self.base, tasklist_id, task_id))
            .bearer_auth(&token)
            .send()
            .await?;
        Self::check(resp).await?;
        Ok(())
    }
}

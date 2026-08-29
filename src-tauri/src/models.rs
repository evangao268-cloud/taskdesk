use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    pub local_id: String,
    pub title: String,
    pub notes: Option<String>,
    /// Calendar date as `YYYY-MM-DD`; Google Tasks due dates carry no time.
    pub due_date: Option<String>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    NeedsAction,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DueNudgeDto {
    pub id: String,
    pub title: String,
    pub interval_days: u32,
    pub days_overdue: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DismissMode {
    Instant,
    Pause,
    Engage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub dismiss_mode: DismissMode,
    pub pause_seconds: u32,
    pub autostart_enabled: bool,
    pub sync_interval_secs: u32,
    pub show_undated: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dismiss_mode: DismissMode::Pause,
            pause_seconds: 7,
            autostart_enabled: false,
            sync_interval_secs: 300,
            show_undated: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DismissStateDto {
    pub mode: DismissMode,
    /// Milliseconds until dismissal is allowed; 0 when allowed now.
    pub blocked_for_ms: u64,
    pub engaged: bool,
    pub allowed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootView {
    pub today: Vec<TaskDto>,
    pub overdue: Vec<TaskDto>,
    pub nudges: Vec<DueNudgeDto>,
    pub undated: Vec<TaskDto>,
    pub settings: Settings,
    pub dismiss: DismissStateDto,
}

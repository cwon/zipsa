use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Space {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub icon: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub space_id: String,
    pub name: String,
    pub quantity: f64,
    pub unit: String,
    pub notes: String,
    pub photo: Option<String>,
    pub expires_on: Option<String>,
    pub purchased_on: Option<String>,
    pub purchase_store: String,
    pub purchase_price: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub space_id: Option<String>,
    pub item_id: Option<String>,
    pub title: String,
    pub notes: String,
    pub mode: String,
    pub every: u32,
    pub unit: String,
    pub anchor_at: String,
    pub due_at: String,
    pub timezone: String,
    pub active: bool,
    pub notified_due_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub target: String,
    pub scheduled_for: String,
    pub completed_at: String,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_after: Option<Task>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub notifications: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub spaces: Vec<Space>,
    pub items: Vec<Item>,
    pub tasks: Vec<Task>,
    pub records: Vec<Record>,
    pub settings: Settings,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub data: Snapshot,
}

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Issue {
    Storage,
    Directory,
    Homepage,
    Feed,
    CatalogUnavailable,
    EmptyCatalog,
    ManagerUnavailable,
    HoldingsUnavailable,
    HoldingsPending,
    Activity,
    History,
    ActivityChanged,
    SourceLink,
    PreviousSync,
}

impl Issue {
    pub fn log(self, error: impl std::fmt::Display) -> Self {
        eprintln!("HoldingScope {self:?}: {error}");
        self
    }
}

impl From<String> for Issue {
    fn from(error: String) -> Self {
        Self::Storage.log(error)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manager {
    pub id: String,
    pub name: String,
    pub updated_on: Option<String>,
    pub period: Option<String>,
    pub value_usd: Option<f64>,
    pub holding_count: Option<i64>,
    pub cached: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub managers: Vec<Manager>,
    pub checked_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub catalog: Catalog,
    pub warnings: Vec<Issue>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub period: String,
    pub report_date: Option<String>,
    pub value_usd: f64,
    pub holdings: Vec<Holding>,
    pub sectors: Vec<Sector>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub ticker: String,
    pub issuer: String,
    pub weight: Option<f64>,
    pub shares: Option<i64>,
    pub reported_price: Option<f64>,
    pub value_usd: Option<f64>,
    pub recent_activity: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Sector {
    pub name: String,
    pub weight: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub period: String,
    pub ticker: String,
    pub issuer: String,
    pub action: String,
    pub change_percent: Option<f64>,
    pub share_change: Option<i64>,
    pub portfolio_impact: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    pub period: String,
    pub value_usd: Option<f64>,
    pub leaders: Vec<HistoryHolding>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HistoryHolding {
    pub ticker: String,
    pub issuer: String,
    pub weight: f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ActivityBatch {
    pub changes: Vec<Activity>,
    pub through: String,
    pub partial_period: Option<String>,
    pub next_page: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Portfolio {
    pub manager: Manager,
    pub snapshots: Vec<Snapshot>,
    pub activities: Vec<Activity>,
    pub history: Vec<HistoryPoint>,
    pub activity_through: Option<String>,
    pub activity_partial_period: Option<String>,
    pub activity_has_more: bool,
    pub history_available: bool,
    pub synced_at: String,
    pub warnings: Vec<Issue>,
}

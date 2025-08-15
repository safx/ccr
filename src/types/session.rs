use super::usage::UsageEntry;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SessionBlock {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub is_active: bool,
    pub cost_usd: f64,
    pub entries: Vec<UsageEntry>,
    pub is_gap: bool,
}

/// Snapshot of usage data at a point in time
#[derive(Debug, Clone)]
pub struct UsageSnapshot {
    pub all_entries: Vec<UsageEntry>,
    pub by_session: HashMap<String, Vec<UsageEntry>>,
    pub today_entries: Vec<UsageEntry>,
    pub processed_hashes: std::collections::HashSet<String>,
}

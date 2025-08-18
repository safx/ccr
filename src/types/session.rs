use super::usage::UsageEntry;
use chrono::{DateTime, Local, Utc};
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
    pub by_session: Option<(String, Vec<UsageEntry>)>,
}

/// Merged snapshot with all session data
#[derive(Debug)]
pub struct MergedUsageSnapshot {
    pub all_entries: Vec<UsageEntry>,
    pub by_session: HashMap<String, Vec<UsageEntry>>,
}

impl MergedUsageSnapshot {
    /// Returns a slice of today's entries from all_entries
    /// Uses binary search since all_entries is sorted by timestamp
    pub fn today_entries(&self) -> &[UsageEntry] {
        if self.all_entries.is_empty() {
            return &self.all_entries;
        }
        
        let today = Local::now().format("%Y-%m-%d").to_string();
        
        // Binary search to find the first entry of today
        let start_idx = self.all_entries.partition_point(|entry| {
            // Convert entry timestamp to local date
            let entry_date = entry
                .timestamp
                .as_ref()
                .and_then(|ts| ts.parse::<DateTime<Utc>>().ok())
                .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            
            // Return true if entry_date is before today (to find the partition point)
            entry_date < today
        });
        
        &self.all_entries[start_idx..]
    }
}

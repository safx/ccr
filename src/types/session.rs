use super::ids::SessionId;
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
    pub by_session: Option<(SessionId, Vec<UsageEntry>)>,
}

/// Merged snapshot with all session data
#[derive(Debug)]
pub struct MergedUsageSnapshot {
    pub all_entries: Vec<UsageEntry>,
    pub by_session: HashMap<SessionId, Vec<UsageEntry>>,
}

impl MergedUsageSnapshot {
    /// Returns a slice of today's entries from all_entries
    /// Uses binary search since all_entries is sorted by timestamp
    pub fn today_entries(&self) -> &[UsageEntry] {
        if self.all_entries.is_empty() {
            return &self.all_entries;
        }

        // Get today's start in the same format as UsageEntry.timestamp (ISO 8601 UTC)
        // This accounts for timezone differences
        let today_start = Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap()
            .with_timezone(&chrono::Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        // Binary search to find the first entry of today
        // Since timestamps are ISO 8601 strings, we can compare them directly
        let start_idx = self.all_entries.partition_point(|entry| {
            entry
                .timestamp
                .as_deref()
                .unwrap_or("")
                < today_start.as_str()
        });

        &self.all_entries[start_idx..]
    }
}

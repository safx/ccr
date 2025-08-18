use crate::pricing::calculate_entry_cost;

use super::ids::SessionId;
use super::usage::UsageEntry;
use chrono::{DateTime, Local, Utc};

#[derive(Debug, Clone)]
pub struct SessionBlock {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub is_active: bool,
    pub cost_usd: f64,
    pub entries: Vec<UsageEntry>,
    pub is_gap: bool,
}

/// Merged snapshot with all session data
#[derive(Debug)]
pub struct MergedUsageSnapshot {
    pub all_entries: Vec<UsageEntry>,
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
            entry.timestamp.as_deref().unwrap_or("") < today_start.as_str()
        });

        &self.all_entries[start_idx..]
    }

    /// Calculate today's cost
    /// Uses today_entries() to get today's data and calculates total cost
    pub fn calculate_today_cost(
        &self,
        pricing_map: &std::collections::HashMap<&str, crate::types::ModelPricing>,
    ) -> f64 {
        use rayon::prelude::*;

        self.today_entries()
            .par_iter()
            .map(|entry| calculate_entry_cost(entry, pricing_map))
            .sum()
    }

    /// Calculate cost for a specific session
    /// Filters entries by session_id and calculates total cost
    pub fn calculate_session_cost(
        &self,
        session_id: &SessionId,
        pricing_map: &std::collections::HashMap<&str, crate::types::ModelPricing>,
    ) -> Option<f64> {
        use rayon::prelude::*;

        let session_cost: f64 = self
            .all_entries
            .par_iter()
            .filter(|entry| entry.session_id == *session_id)
            .map(|entry| calculate_entry_cost(entry, pricing_map))
            .sum();

        if session_cost > 0.0 {
            Some(session_cost)
        } else {
            None
        }
    }
}

use super::ids::SessionId;
use super::usage::UsageEntry;
use chrono::{DateTime, Duration, Local, Utc};

#[derive(Debug, Clone)]
pub enum SessionBlock {
    /// Idle period between sessions
    Idle {
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    },

    /// Currently active session (within 5 hours)
    Active {
        start_time: DateTime<Utc>,
        entries: Vec<UsageEntry>,
    },

    /// Completed past session
    Completed {
        start_time: DateTime<Utc>,
        entries: Vec<UsageEntry>,
    },
}

impl SessionBlock {
    const BLOCK_DURATION: Duration = Duration::hours(5);

    pub fn new(
        block_start: DateTime<Utc>,
        entries: Vec<UsageEntry>,
        last_entry_time: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Self {
        const FIVE_HOURS: Duration = Duration::hours(5);

        let block_end = block_start + FIVE_HOURS;
        let is_active = now.signed_duration_since(last_entry_time) < FIVE_HOURS && now < block_end;

        if is_active {
            SessionBlock::Active {
                start_time: block_start,
                entries,
            }
        } else {
            SessionBlock::Completed {
                start_time: block_start,
                entries,
            }
        }
    }

    pub fn idle(start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> SessionBlock {
        SessionBlock::Idle {
            start_time,
            end_time,
        }
    }

    #[inline(always)]
    pub fn end_time(&self) -> DateTime<Utc> {
        match self {
            SessionBlock::Idle { end_time, .. } => *end_time,
            SessionBlock::Active { start_time, .. } => *start_time + Self::BLOCK_DURATION,
            SessionBlock::Completed { start_time, .. } => *start_time + Self::BLOCK_DURATION,
        }
    }

    pub fn cost_usd(&self) -> f64 {
        match self {
            SessionBlock::Idle { .. } => 0.0,
            SessionBlock::Active { entries, .. } | SessionBlock::Completed { entries, .. } => {
                use crate::pricing::calculate_entry_costs;
                calculate_entry_costs(entries.iter())
            }
        }
    }

    #[inline(always)]
    pub fn entries(&self) -> &[UsageEntry] {
        match self {
            SessionBlock::Idle { .. } => &[],
            SessionBlock::Active { entries, .. } => entries,
            SessionBlock::Completed { entries, .. } => entries,
        }
    }

    #[inline(always)]
    pub fn is_idle(&self) -> bool {
        matches!(self, SessionBlock::Idle { .. })
    }

    #[inline(always)]
    pub fn is_active(&self) -> bool {
        matches!(self, SessionBlock::Active { .. })
    }
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
            entry.data.timestamp.as_deref().unwrap_or("") < today_start.as_str()
        });

        &self.all_entries[start_idx..]
    }

    /// Calculate today's cost
    /// Uses today_entries() to get today's data and calculates total cost
    pub fn calculate_today_cost(&self) -> f64 {
        use crate::pricing::calculate_entry_costs;
        calculate_entry_costs(self.today_entries().iter())
    }

    /// Calculate cost for a specific session
    /// Filters entries by session_id and calculates total cost
    pub fn calculate_session_cost(&self, session_id: &SessionId) -> f64 {
        use crate::pricing::calculate_entry_costs;
        calculate_entry_costs(
            self.all_entries
                .iter()
                .filter(|entry| entry.session_id == *session_id),
        )
    }
}

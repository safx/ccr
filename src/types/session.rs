use super::cost::Cost;
use super::ids::{SessionId, UniqueHash};
use super::usage::UsageEntry;
use chrono::{DateTime, Duration, Local, Timelike, Utc};
use std::collections::HashSet;

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

    pub fn cost(&self) -> Cost {
        Cost::from_session_block(self)
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
    pub fn today_cost(&self) -> Cost {
        Cost::from_entries(self.today_entries().iter())
    }

    /// Calculate cost for a specific session
    /// Filters entries by session_id and calculates total cost
    pub fn session_cost(&self, session_id: &SessionId) -> Cost {
        Cost::from_entries(
            self.all_entries
                .iter()
                .filter(|entry| entry.session_id == *session_id),
        )
    }

    /// Identify session blocks from the snapshot's sorted entries
    /// This matches the TypeScript implementation in ccusage
    pub fn session_blocks(&self) -> Vec<SessionBlock> {
        if self.all_entries.is_empty() {
            return Vec::new();
        }

        const FIVE_HOURS: Duration = Duration::hours(5);
        let now = Local::now().with_timezone(&Utc);
        let mut blocks = Vec::new();
        let mut processed_hashes: HashSet<UniqueHash> = HashSet::new();

        let mut current_block_start: Option<DateTime<Utc>> = None;
        let mut current_block_entries: Vec<UsageEntry> = Vec::new();
        let mut last_entry_time: Option<DateTime<Utc>> = None;

        for entry in self.all_entries.iter() {
            // Parse timestamp
            let Some(entry_time) = entry
                .data
                .timestamp
                .as_ref()
                .and_then(|t| t.parse::<DateTime<Utc>>().ok())
            else {
                continue;
            };

            // Check for duplicate (only when BOTH IDs exist)
            if let Some(message) = &entry.data.message
                && let (Some(msg_id), Some(req_id)) = (&message.id, &entry.data.request_id)
            {
                let hash = UniqueHash::from_ids(msg_id, req_id);
                if processed_hashes.contains(&hash) {
                    continue;
                }
                processed_hashes.insert(hash);
            }

            if current_block_start.is_none() {
                // Start first block
                current_block_start = Some(floor_to_hour(entry_time));
                current_block_entries.push(entry.clone());
                last_entry_time = Some(entry_time);
            } else {
                let block_start = current_block_start.unwrap();
                let time_since_block_start = entry_time.signed_duration_since(block_start);
                let time_since_last_entry = if let Some(last_time) = last_entry_time {
                    entry_time.signed_duration_since(last_time)
                } else {
                    Duration::zero()
                };

                // Check if we need to end the current block
                if time_since_block_start > FIVE_HOURS || time_since_last_entry > FIVE_HOURS {
                    // Create and save the current block
                    let last_time = last_entry_time.unwrap();
                    blocks.push(SessionBlock::new(
                        block_start,
                        current_block_entries.clone(),
                        last_time,
                        now,
                    ));

                    // If there's an idle period, create an idle block
                    if time_since_last_entry > FIVE_HOURS {
                        blocks.push(SessionBlock::idle(last_time + FIVE_HOURS, entry_time));
                    }

                    // Start new block
                    current_block_start = Some(floor_to_hour(entry_time));
                    current_block_entries = vec![entry.clone()];
                } else {
                    // Add to current block
                    current_block_entries.push(entry.clone());
                }

                last_entry_time = Some(entry_time);
            }
        }

        // Create the final block if there are remaining entries
        if !current_block_entries.is_empty() {
            blocks.push(SessionBlock::new(
                current_block_start.unwrap(),
                current_block_entries,
                last_entry_time.unwrap(),
                now,
            ));
        }

        blocks
    }

    /// Find the active block from the session blocks
    pub fn active_block(&self) -> Option<SessionBlock> {
        self.session_blocks().into_iter().find(|b| b.is_active())
    }
}

/// Floor timestamp to the hour (e.g., 14:37:22 → 14:00:00)
fn floor_to_hour(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    timestamp
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap()
}

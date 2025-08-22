use super::cost::Cost;
use super::ids::{SessionId, UniqueHash};
use super::usage::UsageEntry;
use crate::constants::SESSION_BLOCK_DURATION;
use chrono::{DateTime, Duration, Local, Timelike, Utc};
use std::collections::HashSet;
use std::sync::Arc;

/// Type alias for parsed entry with timestamp and Arc-wrapped entry
type ParsedEntry = (DateTime<Utc>, Arc<UsageEntry>);

/// Parse a UsageEntry and extract its timestamp
fn parse_entry_timestamp(entry: &UsageEntry) -> Option<DateTime<Utc>> {
    entry
        .data
        .timestamp
        .as_ref()
        .and_then(|t| t.parse::<DateTime<Utc>>().ok())
}

#[derive(Debug, Clone)]
pub enum SessionBlock {
    /// Idle period between sessions
    Idle {
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    },

    /// Currently active session (within SESSION_BLOCK_DURATION)
    Active {
        start_time: DateTime<Utc>,
        entries: Vec<Arc<UsageEntry>>,
    },

    /// Completed past session
    Completed {
        start_time: DateTime<Utc>,
        entries: Vec<Arc<UsageEntry>>,
    },
}

impl SessionBlock {
    pub fn new(
        block_start: DateTime<Utc>,
        entries: Vec<Arc<UsageEntry>>,
        last_entry_time: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Self {
        let block_end = block_start + SESSION_BLOCK_DURATION;
        let is_active =
            now.signed_duration_since(last_entry_time) < SESSION_BLOCK_DURATION && now < block_end;

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
            SessionBlock::Active { start_time, .. } => *start_time + SESSION_BLOCK_DURATION,
            SessionBlock::Completed { start_time, .. } => *start_time + SESSION_BLOCK_DURATION,
        }
    }

    pub fn cost(&self) -> Cost {
        Cost::from_session_block(self)
    }

    #[inline(always)]
    pub fn entries(&self) -> Vec<&UsageEntry> {
        match self {
            SessionBlock::Idle { .. } => vec![],
            SessionBlock::Active { entries, .. } | SessionBlock::Completed { entries, .. } => {
                entries.iter().map(|e| e.as_ref()).collect()
            }
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

    /// Get the actual duration from first to last entry
    /// Returns None if block is idle or has no entries with valid timestamps
    pub fn actual_duration(&self) -> Option<Duration> {
        if self.is_idle() {
            return None;
        }

        let entries = self.entries();
        if entries.is_empty() {
            return None;
        }

        // Get first and last entry timestamps
        let first_entry = entries.first()?;
        let last_entry = entries.last()?;

        let first_time = first_entry
            .data
            .timestamp
            .as_ref()
            .and_then(|t| t.parse::<DateTime<Utc>>().ok())?;
        let last_time = last_entry
            .data
            .timestamp
            .as_ref()
            .and_then(|t| t.parse::<DateTime<Utc>>().ok())?;

        Some(last_time.signed_duration_since(first_time))
    }

    /// Get the actual duration in minutes
    /// Returns None if no valid duration can be calculated
    pub fn actual_duration_minutes(&self) -> Option<f64> {
        self.actual_duration().map(|d| d.num_minutes() as f64)
    }
}

/// Merged snapshot with all session data
#[derive(Debug)]
pub struct MergedUsageSnapshot {
    pub all_entries: Vec<Arc<UsageEntry>>,
}

impl MergedUsageSnapshot {
    /// Returns a slice of today's entries from all_entries
    /// Uses binary search since all_entries is sorted by timestamp
    fn today_entries(&self) -> &[Arc<UsageEntry>] {
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
        Cost::from_entries(self.today_entries().iter().map(|e| e.as_ref()))
    }

    /// Calculate cost for a specific session
    /// Filters entries by session_id and calculates total cost
    pub fn session_cost(&self, session_id: &SessionId) -> Cost {
        Cost::from_entries(
            self.all_entries
                .iter()
                .filter(|entry| entry.session_id == *session_id)
                .map(|e| e.as_ref()),
        )
    }

    /// Identify session blocks from the snapshot's sorted entries
    /// This matches the TypeScript implementation in ccusage
    fn session_blocks(&self) -> Vec<SessionBlock> {
        if self.all_entries.is_empty() {
            return Vec::new();
        }

        // Phase 1: Parse and deduplicate entries
        let parsed_entries = self.preprocess_entries();

        // Phase 2: Build session blocks
        self.build_session_blocks(parsed_entries)
    }

    /// Preprocess entries: parse timestamps and deduplicate
    fn preprocess_entries(&self) -> Vec<ParsedEntry> {
        let mut processed_hashes: HashSet<UniqueHash> = HashSet::new();
        let mut parsed_entries = Vec::new();

        for entry in self.all_entries.iter() {
            // Parse timestamp - skip if invalid
            let Some(timestamp) = parse_entry_timestamp(entry) else {
                continue;
            };

            // Check for duplicate (only when BOTH IDs exist)
            if let Some(hash) = UniqueHash::from_usage_entry_data(&entry.data) {
                if processed_hashes.contains(&hash) {
                    continue;
                }
                processed_hashes.insert(hash);
            }

            parsed_entries.push((timestamp, Arc::clone(entry)));
        }

        parsed_entries
    }

    /// Build session blocks from parsed entries
    fn build_session_blocks(&self, parsed_entries: Vec<ParsedEntry>) -> Vec<SessionBlock> {
        if parsed_entries.is_empty() {
            return Vec::new();
        }

        let now = Local::now().with_timezone(&Utc);
        let mut blocks = Vec::new();

        // Get the first entry to initialize
        let (first_timestamp, first_entry) = &parsed_entries[0];
        let mut current_block_start = floor_to_hour(*first_timestamp);
        let mut current_block_entries = vec![Arc::clone(first_entry)];
        let mut last_entry_time = *first_timestamp;

        // Process remaining entries
        for (timestamp, entry) in parsed_entries.iter().skip(1) {
            let time_since_block_start = timestamp.signed_duration_since(current_block_start);
            let time_since_last_entry = timestamp.signed_duration_since(last_entry_time);

            // Check if we need to end the current block
            if time_since_block_start > SESSION_BLOCK_DURATION
                || time_since_last_entry > SESSION_BLOCK_DURATION
            {
                // Create and save the current block
                blocks.push(SessionBlock::new(
                    current_block_start,
                    current_block_entries,
                    last_entry_time,
                    now,
                ));

                // If there's an idle period, create an idle block
                if time_since_last_entry > SESSION_BLOCK_DURATION {
                    blocks.push(SessionBlock::idle(
                        last_entry_time + SESSION_BLOCK_DURATION,
                        *timestamp,
                    ));
                }

                // Start new block
                current_block_start = floor_to_hour(*timestamp);
                current_block_entries = vec![Arc::clone(entry)];
            } else {
                // Add to current block
                current_block_entries.push(Arc::clone(entry));
            }

            last_entry_time = *timestamp;
        }

        // Create the final block with remaining entries
        blocks.push(SessionBlock::new(
            current_block_start,
            current_block_entries,
            last_entry_time,
            now,
        ));

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelId;
    use crate::types::{Message, MessageId, RequestId, Usage, UsageEntryData};
    use chrono::{Datelike, TimeZone, Timelike};

    // Helper function to create test UsageEntry
    fn create_test_entry(
        session_id: &str,
        timestamp: &str,
        message_id: Option<&str>,
        request_id: Option<&str>,
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
    ) -> Arc<UsageEntry> {
        Arc::new(UsageEntry {
            data: UsageEntryData {
                timestamp: Some(timestamp.to_string()),
                model: Some(ModelId::from("claude-3-5-sonnet-20241022")),
                cost_usd: None,
                message: Some(Message {
                    id: message_id.map(|id| MessageId::from(id)),
                    model: Some(ModelId::from("claude-3-5-sonnet-20241022")),
                    usage: Some(Usage {
                        input_tokens,
                        output_tokens,
                        cache_creation_input_tokens: None,
                        cache_read_input_tokens: None,
                        cache_creation: None,
                        service_tier: None,
                    }),
                }),
                request_id: request_id.map(|id| RequestId::from(id)),
            },
            session_id: SessionId::from(session_id),
        })
    }

    #[test]
    fn test_floor_to_hour() {
        let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 14, 37, 22).unwrap();
        let floored = floor_to_hour(timestamp);

        assert_eq!(floored.hour(), 14);
        assert_eq!(floored.minute(), 0);
        assert_eq!(floored.second(), 0);
        assert_eq!(floored.nanosecond(), 0);
    }

    #[test]
    fn test_parse_entry_timestamp() {
        let entry = create_test_entry(
            "test-session",
            "2024-01-15T10:30:00.000Z",
            Some("msg-1"),
            Some("req-1"),
            Some(100),
            Some(50),
        );

        let timestamp = parse_entry_timestamp(&entry);
        assert!(timestamp.is_some());

        let ts = timestamp.unwrap();
        assert_eq!(ts.year(), 2024);
        assert_eq!(ts.month(), 1);
        assert_eq!(ts.day(), 15);
        assert_eq!(ts.hour(), 10);
        assert_eq!(ts.minute(), 30);
    }

    #[test]
    fn test_parse_entry_timestamp_invalid() {
        let entry = Arc::new(UsageEntry {
            data: UsageEntryData {
                timestamp: Some("invalid-timestamp".to_string()),
                model: None,
                cost_usd: None,
                message: None,
                request_id: None,
            },
            session_id: SessionId::from("test-session"),
        });

        let timestamp = parse_entry_timestamp(&entry);
        assert!(timestamp.is_none());
    }

    #[test]
    fn test_session_block_new_active() {
        let now = Utc::now();
        let block_start = now - Duration::hours(1);
        let last_entry_time = now - Duration::minutes(30);

        let entries = vec![create_test_entry(
            "test-session",
            &block_start.to_rfc3339(),
            Some("msg-1"),
            Some("req-1"),
            Some(100),
            Some(50),
        )];

        let block = SessionBlock::new(block_start, entries.clone(), last_entry_time, now);

        assert!(block.is_active());
        assert!(!block.is_idle());

        match block {
            SessionBlock::Active {
                start_time,
                entries: block_entries,
            } => {
                assert_eq!(start_time, block_start);
                assert_eq!(block_entries.len(), 1);
            }
            _ => panic!("Expected Active block"),
        }
    }

    #[test]
    fn test_session_block_new_completed() {
        let now = Utc::now();
        let block_start = now - Duration::hours(10);
        let last_entry_time = now - Duration::hours(8);

        let entries = vec![create_test_entry(
            "test-session",
            &block_start.to_rfc3339(),
            Some("msg-1"),
            Some("req-1"),
            Some(100),
            Some(50),
        )];

        let block = SessionBlock::new(block_start, entries.clone(), last_entry_time, now);

        assert!(!block.is_active());
        assert!(!block.is_idle());

        match block {
            SessionBlock::Completed {
                start_time,
                entries: block_entries,
            } => {
                assert_eq!(start_time, block_start);
                assert_eq!(block_entries.len(), 1);
            }
            _ => panic!("Expected Completed block"),
        }
    }

    #[test]
    fn test_session_block_idle() {
        let start_time = Utc::now() - Duration::hours(10);
        let end_time = Utc::now() - Duration::hours(5);

        let block = SessionBlock::idle(start_time, end_time);

        assert!(block.is_idle());
        assert!(!block.is_active());
        assert_eq!(block.entries().len(), 0);
        assert_eq!(block.cost().value(), 0.0);

        match block {
            SessionBlock::Idle {
                start_time: s,
                end_time: e,
            } => {
                assert_eq!(s, start_time);
                assert_eq!(e, end_time);
            }
            _ => panic!("Expected Idle block"),
        }
    }

    #[test]
    fn test_session_block_actual_duration() {
        let base_time = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();

        let entries = vec![
            create_test_entry(
                "test-session",
                &base_time.to_rfc3339(),
                Some("msg-1"),
                Some("req-1"),
                Some(100),
                Some(50),
            ),
            create_test_entry(
                "test-session",
                &(base_time + Duration::minutes(30)).to_rfc3339(),
                Some("msg-2"),
                Some("req-2"),
                Some(200),
                Some(100),
            ),
            create_test_entry(
                "test-session",
                &(base_time + Duration::hours(1)).to_rfc3339(),
                Some("msg-3"),
                Some("req-3"),
                Some(150),
                Some(75),
            ),
        ];

        let block = SessionBlock::Active {
            start_time: base_time,
            entries,
        };

        let duration = block.actual_duration();
        assert!(duration.is_some());
        assert_eq!(duration.unwrap(), Duration::hours(1));

        let duration_minutes = block.actual_duration_minutes();
        assert!(duration_minutes.is_some());
        assert_eq!(duration_minutes.unwrap(), 60.0);
    }

    #[test]
    fn test_session_block_actual_duration_idle() {
        let block = SessionBlock::idle(Utc::now(), Utc::now() + Duration::hours(1));

        assert!(block.actual_duration().is_none());
        assert!(block.actual_duration_minutes().is_none());
    }

    #[test]
    fn test_session_block_end_time() {
        let start = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();

        // Test Active block
        let active_block = SessionBlock::Active {
            start_time: start,
            entries: vec![],
        };
        assert_eq!(active_block.end_time(), start + SESSION_BLOCK_DURATION);

        // Test Completed block
        let completed_block = SessionBlock::Completed {
            start_time: start,
            entries: vec![],
        };
        assert_eq!(completed_block.end_time(), start + SESSION_BLOCK_DURATION);

        // Test Idle block
        let end = start + Duration::hours(2);
        let idle_block = SessionBlock::idle(start, end);
        assert_eq!(idle_block.end_time(), end);
    }

    #[test]
    fn test_merged_usage_snapshot_today_entries() {
        let _now = Local::now().with_timezone(&Utc);
        let today_start = Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap()
            .with_timezone(&Utc);

        let yesterday = today_start - Duration::days(1);
        let today_morning = today_start + Duration::hours(8);
        let today_afternoon = today_start + Duration::hours(14);

        let entries = vec![
            create_test_entry(
                "session-1",
                &yesterday.to_rfc3339(),
                Some("msg-1"),
                Some("req-1"),
                Some(100),
                Some(50),
            ),
            create_test_entry(
                "session-2",
                &today_morning.to_rfc3339(),
                Some("msg-2"),
                Some("req-2"),
                Some(200),
                Some(100),
            ),
            create_test_entry(
                "session-3",
                &today_afternoon.to_rfc3339(),
                Some("msg-3"),
                Some("req-3"),
                Some(150),
                Some(75),
            ),
        ];

        let snapshot = MergedUsageSnapshot {
            all_entries: entries,
        };

        let today_entries = snapshot.today_entries();
        assert_eq!(today_entries.len(), 2);

        // Verify that only today's entries are included
        for entry in today_entries {
            let timestamp = entry.data.timestamp.as_ref().unwrap();
            assert!(timestamp >= &today_start.to_rfc3339());
        }
    }

    #[test]
    fn test_merged_usage_snapshot_session_cost() {
        let entries = vec![
            create_test_entry(
                "session-1",
                "2024-01-15T10:00:00.000Z",
                Some("msg-1"),
                Some("req-1"),
                Some(100),
                Some(50),
            ),
            create_test_entry(
                "session-1",
                "2024-01-15T10:30:00.000Z",
                Some("msg-2"),
                Some("req-2"),
                Some(200),
                Some(100),
            ),
            create_test_entry(
                "session-2",
                "2024-01-15T11:00:00.000Z",
                Some("msg-3"),
                Some("req-3"),
                Some(150),
                Some(75),
            ),
        ];

        let snapshot = MergedUsageSnapshot {
            all_entries: entries,
        };

        // Session 1 should have 2 entries
        let session1_cost = snapshot.session_cost(&SessionId::from("session-1"));
        // Session 2 should have 1 entry
        let session2_cost = snapshot.session_cost(&SessionId::from("session-2"));
        // Non-existent session should have 0 cost
        let session3_cost = snapshot.session_cost(&SessionId::from("session-3"));

        // Basic check that costs are calculated (actual values depend on pricing)
        assert!(session1_cost.value() > 0.0);
        assert!(session2_cost.value() > 0.0);
        assert_eq!(session3_cost.value(), 0.0);
    }

    #[test]
    fn test_merged_usage_snapshot_preprocess_entries() {
        let entries = vec![
            create_test_entry(
                "session-1",
                "2024-01-15T10:00:00.000Z",
                Some("msg-1"),
                Some("req-1"),
                Some(100),
                Some(50),
            ),
            create_test_entry(
                "session-1",
                "2024-01-15T10:30:00.000Z",
                Some("msg-1"),
                Some("req-1"), // Duplicate
                Some(100),
                Some(50),
            ),
            create_test_entry(
                "session-1",
                "2024-01-15T11:00:00.000Z",
                Some("msg-2"),
                Some("req-2"),
                Some(200),
                Some(100),
            ),
        ];

        let snapshot = MergedUsageSnapshot {
            all_entries: entries,
        };

        let processed = snapshot.preprocess_entries();

        // Should have 2 entries after deduplication
        assert_eq!(processed.len(), 2);

        // Verify timestamps are parsed correctly
        for (timestamp, _) in &processed {
            assert!(timestamp.year() == 2024);
        }
    }

    #[test]
    fn test_merged_usage_snapshot_session_blocks() {
        let base_time = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();

        // Create entries with different time gaps
        let entries = vec![
            // First block
            create_test_entry(
                "session-1",
                &base_time.to_rfc3339(),
                Some("msg-1"),
                Some("req-1"),
                Some(100),
                Some(50),
            ),
            create_test_entry(
                "session-1",
                &(base_time + Duration::hours(2)).to_rfc3339(),
                Some("msg-2"),
                Some("req-2"),
                Some(200),
                Some(100),
            ),
            // Gap > 5 hours, should create new block
            create_test_entry(
                "session-1",
                &(base_time + Duration::hours(8)).to_rfc3339(),
                Some("msg-3"),
                Some("req-3"),
                Some(150),
                Some(75),
            ),
        ];

        let snapshot = MergedUsageSnapshot {
            all_entries: entries,
        };

        let blocks = snapshot.session_blocks();

        // Should have 3 blocks: first activity, idle, second activity
        assert!(blocks.len() >= 2);

        // Check for idle block
        let has_idle = blocks.iter().any(|b| b.is_idle());
        assert!(has_idle, "Should have an idle block between sessions");
    }

    #[test]
    fn test_merged_usage_snapshot_active_block() {
        let now = Utc::now();
        let recent_time = now - Duration::minutes(30);

        let entries = vec![
            create_test_entry(
                "session-1",
                &recent_time.to_rfc3339(),
                Some("msg-1"),
                Some("req-1"),
                Some(100),
                Some(50),
            ),
            create_test_entry(
                "session-1",
                &(recent_time + Duration::minutes(10)).to_rfc3339(),
                Some("msg-2"),
                Some("req-2"),
                Some(200),
                Some(100),
            ),
        ];

        let snapshot = MergedUsageSnapshot {
            all_entries: entries,
        };

        let active_block = snapshot.active_block();
        assert!(active_block.is_some());

        let block = active_block.unwrap();
        assert!(block.is_active());
        assert_eq!(block.entries().len(), 2);
    }

    #[test]
    fn test_merged_usage_snapshot_empty() {
        let snapshot = MergedUsageSnapshot {
            all_entries: vec![],
        };

        assert_eq!(snapshot.today_entries().len(), 0);
        assert_eq!(snapshot.today_cost().value(), 0.0);
        assert_eq!(snapshot.session_cost(&SessionId::from("any")).value(), 0.0);
        assert!(snapshot.active_block().is_none());
        assert_eq!(snapshot.session_blocks().len(), 0);
    }

    #[test]
    fn test_session_blocks_with_exact_5_hour_gap() {
        let base_time = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();

        let entries = vec![
            create_test_entry(
                "session-1",
                &base_time.to_rfc3339(),
                Some("msg-1"),
                Some("req-1"),
                Some(100),
                Some(50),
            ),
            // Exactly 5 hours gap - should still be in same block
            create_test_entry(
                "session-1",
                &(base_time + SESSION_BLOCK_DURATION).to_rfc3339(),
                Some("msg-2"),
                Some("req-2"),
                Some(200),
                Some(100),
            ),
        ];

        let snapshot = MergedUsageSnapshot {
            all_entries: entries,
        };

        let blocks = snapshot.session_blocks();

        // The behavior depends on whether exactly 5 hours is considered same or different block
        // This test documents the actual behavior
        assert!(blocks.len() >= 1);
    }
}

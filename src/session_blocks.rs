use crate::types::{SessionBlock, UniqueHash, UsageEntry};
use chrono::{DateTime, Duration, Local, Timelike, Utc};
use std::collections::HashSet;

/// Floor timestamp to the hour (e.g., 14:37:22 → 14:00:00)
pub fn floor_to_hour(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    timestamp
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap()
}

/// Identify session blocks from sorted entries
/// This matches the TypeScript implementation in ccusage
pub fn identify_session_blocks(
    sorted_entries: &[UsageEntry], // Already sorted, passed by reference
) -> Vec<SessionBlock> {
    if sorted_entries.is_empty() {
        return Vec::new();
    }

    const FIVE_HOURS: Duration = Duration::hours(5);
    let now = Local::now().with_timezone(&Utc);
    let mut blocks = Vec::new();
    let mut processed_hashes: HashSet<UniqueHash> = HashSet::new();

    let mut current_block_start: Option<DateTime<Utc>> = None;
    let mut current_block_entries: Vec<UsageEntry> = Vec::new();
    let mut last_entry_time: Option<DateTime<Utc>> = None;

    for entry in sorted_entries.iter() {
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

/// Find the active block from a list of blocks
pub fn find_active_block(blocks: &[SessionBlock]) -> Option<&SessionBlock> {
    blocks.iter().find(|b| b.is_active())
}

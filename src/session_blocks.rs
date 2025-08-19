use crate::types::{ModelPricing, SessionBlock, TokenUsage, UniqueHash, UsageEntry};
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

    let now = Local::now().with_timezone(&Utc);
    let five_hours = Duration::hours(5);
    let mut blocks = Vec::new();
    let mut processed_hashes: HashSet<UniqueHash> = HashSet::new();

    let mut current_block_start: Option<DateTime<Utc>> = None;
    let mut current_block_entries: Vec<UsageEntry> = Vec::new();
    let mut current_block_cost = 0.0;
    let mut last_entry_time: Option<DateTime<Utc>> = None;

    for entry in sorted_entries.iter() {
        // Parse timestamp
        let Some(entry_time) = entry
            .timestamp
            .as_ref()
            .and_then(|t| t.parse::<DateTime<Utc>>().ok())
        else {
            continue;
        };

        // Check for duplicate (only when BOTH IDs exist)
        if let Some(message) = &entry.message
            && let (Some(msg_id), Some(req_id)) = (&message.id, &entry.request_id)
        {
            let hash = UniqueHash::from((msg_id, req_id));
            if processed_hashes.contains(&hash) {
                continue;
            }
            processed_hashes.insert(hash);
        }

        // Calculate entry cost (prefer costUSD, fallback to calculating from tokens)
        let entry_cost = if let Some(cost) = entry.cost_usd {
            cost
        } else if let Some(message) = &entry.message
            && let Some(usage) = &message.usage
            && let Some(model_id) = message.model.as_ref().or(entry.model.as_ref())
        {
            let pricing = ModelPricing::from(model_id);
            let tokens = TokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_tokens: usage.cache_creation_input_tokens,
                cache_read_tokens: usage.cache_read_input_tokens,
            };
            // Calculate cost inline
            let mut cost = 0.0;
            if let (Some(input), Some(price)) = (tokens.input_tokens, pricing.input_cost_per_token)
            {
                cost += input as f64 * price;
            }
            if let (Some(output), Some(price)) =
                (tokens.output_tokens, pricing.output_cost_per_token)
            {
                cost += output as f64 * price;
            }
            if let (Some(cache_creation), Some(price)) = (
                tokens.cache_creation_tokens,
                pricing.cache_creation_input_token_cost,
            ) {
                cost += cache_creation as f64 * price;
            }
            if let (Some(cache_read), Some(price)) = (
                tokens.cache_read_tokens,
                pricing.cache_read_input_token_cost,
            ) {
                cost += cache_read as f64 * price;
            }
            cost
        } else {
            0.0
        };

        if current_block_start.is_none() {
            // Start first block
            current_block_start = Some(floor_to_hour(entry_time));
            current_block_entries.push(entry.clone());
            current_block_cost += entry_cost;
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
            if time_since_block_start > five_hours || time_since_last_entry > five_hours {
                // Create and save the current block
                let block_end = block_start + five_hours;
                let last_time = last_entry_time.unwrap();
                let is_active =
                    now.signed_duration_since(last_time) < five_hours && now < block_end;

                blocks.push(SessionBlock {
                    start_time: block_start,
                    end_time: block_end,
                    is_active,
                    cost_usd: current_block_cost,
                    entries: current_block_entries.clone(),
                    is_gap: false,
                });

                // If there's a gap, create a gap block
                if time_since_last_entry > five_hours {
                    let gap_start = last_time + five_hours;
                    let gap_end = entry_time;

                    blocks.push(SessionBlock {
                        start_time: gap_start,
                        end_time: gap_end,
                        is_active: false,
                        cost_usd: 0.0,
                        entries: Vec::new(),
                        is_gap: true,
                    });
                }

                // Start new block
                current_block_start = Some(floor_to_hour(entry_time));
                current_block_entries = vec![entry.clone()];
                current_block_cost = entry_cost;
            } else {
                // Add to current block
                current_block_entries.push(entry.clone());
                current_block_cost += entry_cost;
            }

            last_entry_time = Some(entry_time);
        }
    }

    // Create the final block if there are remaining entries
    if !current_block_entries.is_empty() {
        let block_start = current_block_start.unwrap();
        let block_end = block_start + five_hours;
        let last_time = last_entry_time.unwrap();
        let is_active = now.signed_duration_since(last_time) < five_hours && now < block_end;

        blocks.push(SessionBlock {
            start_time: block_start,
            end_time: block_end,
            is_active,
            cost_usd: current_block_cost,
            entries: current_block_entries,
            is_gap: false,
        });
    }

    blocks
}

/// Find the active block from a list of blocks
pub fn find_active_block(blocks: &[SessionBlock]) -> Option<&SessionBlock> {
    blocks.iter().find(|b| b.is_active && !b.is_gap)
}

/// Calculate burn rate for a block
pub fn calculate_burn_rate(block: &SessionBlock) -> Option<f64> {
    if block.is_gap || block.entries.is_empty() {
        return None;
    }

    // Get first and last entry timestamps
    let first_entry = block.entries.first()?;
    let last_entry = block.entries.last()?;

    let first_time = first_entry
        .timestamp
        .as_ref()
        .and_then(|t| t.parse::<DateTime<Utc>>().ok())?;
    let last_time = last_entry
        .timestamp
        .as_ref()
        .and_then(|t| t.parse::<DateTime<Utc>>().ok())?;

    // Calculate duration from first to last entry (not from block start)
    let duration_minutes = last_time.signed_duration_since(first_time).num_minutes() as f64;

    // Skip if duration is 0 or negative
    if duration_minutes <= 0.0 {
        return None;
    }

    // Calculate cost per hour
    Some((block.cost_usd / duration_minutes) * 60.0)
}

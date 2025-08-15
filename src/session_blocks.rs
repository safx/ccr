use chrono::{DateTime, Duration, Utc, Timelike};
use std::collections::HashSet;
use crate::{UsageEntry, ModelPricing, calculate_cost, TokenUsage};

/// Session block representing a billing period
#[derive(Debug, Clone)]
pub struct SessionBlock {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub is_active: bool,
    pub cost_usd: f64,
    pub entries: Vec<UsageEntry>,
    pub is_gap: bool,
}

/// Floor timestamp to the hour (e.g., 14:37:22 → 14:00:00)
pub fn floor_to_hour(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    timestamp
        .with_minute(0).unwrap()
        .with_second(0).unwrap()
        .with_nanosecond(0).unwrap()
}

/// Identify session blocks from sorted entries
/// This matches the TypeScript implementation in ccusage
pub fn identify_session_blocks(
    entries: Vec<UsageEntry>,
    pricing_map: &std::collections::HashMap<&str, ModelPricing>,
) -> Vec<SessionBlock> {
    if entries.is_empty() {
        return Vec::new();
    }
    
    let now = Utc::now();
    let five_hours = Duration::hours(5);
    let mut blocks = Vec::new();
    let mut processed_hashes = HashSet::new();
    
    // Sort entries by timestamp
    let mut sorted_entries = entries;
    sorted_entries.sort_by(|a, b| {
        let time_a = a.timestamp.as_ref()
            .and_then(|t| t.parse::<DateTime<Utc>>().ok())
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        let time_b = b.timestamp.as_ref()
            .and_then(|t| t.parse::<DateTime<Utc>>().ok())
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        time_a.cmp(&time_b)
    });
    
    let mut current_block_start: Option<DateTime<Utc>> = None;
    let mut current_block_entries: Vec<UsageEntry> = Vec::new();
    let mut current_block_cost = 0.0;
    let mut last_entry_time: Option<DateTime<Utc>> = None;
    
    for entry in sorted_entries {
        // Parse timestamp
        let entry_time = match entry.timestamp.as_ref()
            .and_then(|t| t.parse::<DateTime<Utc>>().ok()) {
            Some(t) => t,
            None => continue,
        };
        
        // Check for duplicate
        if let (Some(msg_id), Some(req_id)) = (&entry.message_id, &entry.request_id) {
            let hash = format!("{}:{}", msg_id, req_id);
            if processed_hashes.contains(&hash) {
                continue;
            }
            processed_hashes.insert(hash);
        }
        
        // Calculate entry cost
        let entry_cost = if let Some(cost) = entry.cost_usd {
            cost
        } else if let Some(usage) = &entry.message_usage {
            let model_name = entry.message_model.as_deref()
                .or(entry.model.as_deref());
            
            if let Some(model_name) = model_name {
                if let Some(pricing) = get_model_pricing(model_name, pricing_map) {
                    calculate_cost(
                        &TokenUsage {
                            input_tokens: usage.input_tokens,
                            output_tokens: usage.output_tokens,
                            cache_creation_tokens: usage.cache_creation_input_tokens,
                            cache_read_tokens: usage.cache_read_input_tokens,
                        },
                        pricing,
                    )
                } else {
                    0.0
                }
            } else {
                0.0
            }
        } else {
            0.0
        };
        
        if current_block_start.is_none() {
            // Start first block
            current_block_start = Some(floor_to_hour(entry_time));
            current_block_entries.push(entry);
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
                let is_active = now.signed_duration_since(last_time) < five_hours 
                    && now < block_end;
                
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
                current_block_entries = vec![entry];
                current_block_cost = entry_cost;
            } else {
                // Add to current block
                current_block_entries.push(entry);
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
        let is_active = now.signed_duration_since(last_time) < five_hours 
            && now < block_end;
        
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
    
    let elapsed = Utc::now().signed_duration_since(block.start_time);
    let elapsed_minutes = elapsed.num_minutes() as f64;
    
    if elapsed_minutes > 5.0 {
        Some((block.cost_usd / elapsed_minutes) * 60.0) // Cost per hour
    } else {
        None
    }
}

/// Helper function to get model pricing
fn get_model_pricing<'a>(
    model_name: &str,
    pricing_map: &'a std::collections::HashMap<&str, ModelPricing>,
) -> Option<&'a ModelPricing> {
    // Direct match
    if let Some(pricing) = pricing_map.get(model_name) {
        return Some(pricing);
    }
    
    // Partial match
    for (key, pricing) in pricing_map {
        if model_name.contains(key) || key.contains(&model_name) {
            return Some(pricing);
        }
    }
    
    // Fallback based on model type
    if model_name.to_lowercase().contains("opus") {
        pricing_map.get("claude-opus-4-1-20250805")
    } else if model_name.to_lowercase().contains("sonnet") {
        pricing_map.get("claude-sonnet-4-20250514")
    } else {
        None
    }
}

/// Load all entries from all projects (for block identification)
pub async fn load_all_entries(
    claude_paths: &[std::path::PathBuf],
) -> Result<Vec<UsageEntry>, Box<dyn std::error::Error + Send + Sync>> {
    use tokio::task;
    use std::fs;
    use std::io::{BufRead, BufReader};
    
    let mut all_entries = Vec::new();
    
    for base_path in claude_paths {
        let projects_path = base_path.join("projects");
        if !projects_path.exists() {
            continue;
        }
        
        let base_path = base_path.clone();
        let entries = task::spawn_blocking(move || -> Result<Vec<UsageEntry>, Box<dyn std::error::Error + Send + Sync>> {
            let mut entries = Vec::new();
            let projects_path = base_path.join("projects");
            
            for project_entry in fs::read_dir(&projects_path)? {
                let project_entry = project_entry?;
                if !project_entry.file_type()?.is_dir() {
                    continue;
                }
                
                for file_entry in fs::read_dir(project_entry.path())? {
                    let file_entry = file_entry?;
                    if !file_entry.file_name().to_string_lossy().ends_with(".jsonl") {
                        continue;
                    }
                    
                    let file = fs::File::open(file_entry.path())?;
                    let reader = BufReader::with_capacity(128 * 1024, file);
                    
                    for line in reader.lines() {
                        if let Ok(line) = line {
                            if line.trim().is_empty() {
                                continue;
                            }
                            
                            if let Ok(entry) = serde_json::from_str::<UsageEntry>(&line) {
                                entries.push(entry);
                            }
                        }
                    }
                }
            }
            
            Ok(entries)
        }).await??;
        
        all_entries.extend(entries);
    }
    
    Ok(all_entries)
}
use chrono::{Local, Utc};
use rayon::prelude::*;
use serde_json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::task;

use crate::pricing::calculate_cost;
use crate::types::{MergedUsageSnapshot, ModelPricing, TokenUsage, UsageEntry, UsageSnapshot};
use crate::utils::create_entry_hash;

/// Load all data with optimized parallelism
pub async fn load_all_data(
    claude_paths: &[PathBuf],
    session_id: &str,
) -> Result<MergedUsageSnapshot, Box<dyn std::error::Error + Send + Sync>> {
    // Get today's date in local timezone
    let today = Local::now().format("%Y-%m-%d").to_string();
    let target_session = session_id.to_string();

    // Use a shared mutex for deduplication across all threads
    let global_hashes = Arc::new(Mutex::new(HashSet::with_capacity(50000)));

    let tasks: Vec<_> = claude_paths
        .iter()
        .map(|base_path| {
            let base_path = base_path.clone();
            let global_hashes = Arc::clone(&global_hashes);
            let today = today.clone();
            let target_session = target_session.clone();

            task::spawn_blocking(
                move || -> Result<UsageSnapshot, Box<dyn std::error::Error + Send + Sync>> {
                    let projects_path = base_path.join("projects");
                    if !projects_path.exists() {
                        return Ok(UsageSnapshot {
                            all_entries: Vec::new(),
                            by_session: None,
                            today_entries: Vec::new(),
                        });
                    }

                    // Collect all file paths first
                    let mut all_files = Vec::new();
                    for project_entry in fs::read_dir(&projects_path)? {
                        let project_entry = project_entry?;
                        if !project_entry.file_type()?.is_dir() {
                            continue;
                        }

                        for file_entry in fs::read_dir(project_entry.path())? {
                            let file_entry = file_entry?;
                            let file_name = file_entry.file_name();
                            let file_name_str = file_name.to_string_lossy();
                            if file_name_str.ends_with(".jsonl") {
                                let session_from_file =
                                    file_name_str.trim_end_matches(".jsonl").to_string();
                                all_files.push((file_entry.path(), session_from_file));
                            }
                        }
                    }

                    // Process all files in parallel with line-level parallelism
                    let results: Vec<_> = all_files
                        .par_iter()
                        .map(|(path, session_file_id)| {
                            // Read entire file into memory first for faster parsing
                            match fs::read_to_string(path) {
                                Ok(contents) => {
                                    // Parse lines in parallel
                                    let entries: Vec<UsageEntry> = contents
                                        .par_lines()
                                        .filter(|line| !line.trim().is_empty())
                                        .filter_map(|line| serde_json::from_str(line).ok())
                                        .collect();

                                    (session_file_id, entries)
                                }
                                Err(_) => (session_file_id, Vec::new()),
                            }
                        })
                        .collect();

                    // Process results with global deduplication
                    let mut all_entries = Vec::with_capacity(50000);
                    let mut target_session_entries: Vec<UsageEntry> = Vec::new();
                    let mut today_entries = Vec::with_capacity(10000);

                    for (session_file_id, entries) in results {
                        let mut hashes = global_hashes.lock().unwrap();
                        for entry in entries {
                            // Global deduplication check
                            if let Some(message) = &entry.message
                                && let (Some(msg_id), Some(req_id)) =
                                    (&message.id, &entry.request_id)
                            {
                                let hash = create_entry_hash(msg_id, req_id);
                                if hashes.contains(&hash) {
                                    continue;
                                }
                                hashes.insert(hash);
                            };

                            // Check conditions first
                            // Parse timestamp and convert to local date for comparison
                            let is_today = entry
                                .timestamp
                                .as_ref()
                                .and_then(|ts| ts.parse::<chrono::DateTime<Utc>>().ok())
                                .map(|dt| {
                                    dt.with_timezone(&Local).format("%Y-%m-%d").to_string() == today
                                })
                                .unwrap_or(false);
                            let is_target_session = session_file_id == target_session.as_str();

                            // Add to appropriate collections
                            if is_today {
                                today_entries.push(entry.clone());
                            }

                            if is_target_session {
                                target_session_entries.push(entry.clone());
                            }

                            all_entries.push(entry);
                        }
                    }

                    let by_session = if !target_session_entries.is_empty() {
                        Some((target_session, target_session_entries))
                    } else {
                        None
                    };

                    Ok(UsageSnapshot {
                        all_entries,
                        by_session,
                        today_entries,
                    })
                },
            )
        })
        .collect();

    // Merge results from all base paths
    let mut all_entries = Vec::with_capacity(50000);
    let mut by_session: HashMap<String, Vec<UsageEntry>> = HashMap::new();
    let mut today_entries = Vec::with_capacity(2000);

    for task in tasks {
        let data = task.await??;
        all_entries.extend(data.all_entries);
        today_entries.extend(data.today_entries);

        if let Some((session_id, entries)) = data.by_session {
            by_session.entry(session_id).or_default().extend(entries);
        }
    }

    // Sort all entries by timestamp once (string sort is sufficient for ISO 8601)
    all_entries.sort_by(|a, b| a.timestamp.as_deref().cmp(&b.timestamp.as_deref()));

    Ok(MergedUsageSnapshot {
        all_entries,
        by_session,
        today_entries,
    })
}

/// Calculate today's cost from usage snapshot
pub fn calculate_today_cost(
    data: &MergedUsageSnapshot,
    pricing_map: &HashMap<&str, ModelPricing>,
) -> f64 {
    data.today_entries
        .par_iter() // Use parallel iterator for cost calculation
        .map(|entry| calculate_entry_cost(entry, pricing_map))
        .sum()
}

/// Calculate session cost from usage snapshot
pub fn calculate_session_cost(
    data: &MergedUsageSnapshot,
    session_id: &str,
    pricing_map: &HashMap<&str, ModelPricing>,
) -> Option<f64> {
    data.by_session.get(session_id).map(|entries| {
        entries
            .par_iter() // Use parallel iterator for cost calculation
            .map(|entry| calculate_entry_cost(entry, pricing_map))
            .sum()
    })
}

/// Calculate entry cost with pricing map
fn calculate_entry_cost(entry: &UsageEntry, pricing_map: &HashMap<&str, ModelPricing>) -> f64 {
    if let Some(cost) = entry.cost_usd {
        return cost;
    }

    if let Some(message) = &entry.message
        && let Some(usage) = &message.usage
    {
        let model_name = message.model.as_ref().or(entry.model.as_ref());

        if let Some(model_name) = model_name
            && let Some(pricing) = pricing_map.get(model_name.as_str())
        {
            let tokens = TokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_tokens: usage.cache_creation_input_tokens,
                cache_read_tokens: usage.cache_read_input_tokens,
            };
            return calculate_cost(&tokens, pricing);
        }
    }

    0.0
}

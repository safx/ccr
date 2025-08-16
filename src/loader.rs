use chrono::Utc;
use rayon::prelude::*;
use serde_json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::task;

use crate::pricing::calculate_cost;
use crate::types::{ModelPricing, TokenUsage, UsageEntry, UsageSnapshot};
use crate::utils::create_entry_hash;

/// Load all data with optimized parallelism
pub async fn load_all_data(
    claude_paths: &[PathBuf],
    session_id: &str,
) -> Result<UsageSnapshot, Box<dyn std::error::Error + Send + Sync>> {
    // Intentionally leak strings to avoid cloning overhead in spawn_blocking threads
    // These are only allocated once per program run, so the memory impact is minimal
    let today: &'static str = Utc::now().format("%Y-%m-%d").to_string().leak();
    let target_session: &'static str = session_id.to_string().leak();

    // Use a shared mutex for deduplication across all threads
    let global_hashes = Arc::new(Mutex::new(HashSet::with_capacity(100000)));

    let tasks: Vec<_> = claude_paths
        .iter()
        .map(|base_path| {
            let base_path = base_path.clone();
            let global_hashes = Arc::clone(&global_hashes);

            task::spawn_blocking(
                move || -> Result<UsageSnapshot, Box<dyn std::error::Error + Send + Sync>> {
                    let projects_path = base_path.join("projects");
                    if !projects_path.exists() {
                        return Ok(UsageSnapshot {
                            all_entries: Vec::new(),
                            by_session: HashMap::new(),
                            today_entries: Vec::new(),
                            processed_hashes: HashSet::new(),
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

                                    (session_file_id.clone(), entries)
                                }
                                Err(_) => (session_file_id.clone(), Vec::new()),
                            }
                        })
                        .collect();

                    // Process results with global deduplication
                    let mut all_entries = Vec::with_capacity(50000);
                    let mut target_session_entries: Vec<UsageEntry> = Vec::new();
                    let mut today_entries = Vec::with_capacity(10000);

                    for (session_file_id, entries) in results {
                        for entry in entries {
                            // Global deduplication check
                            let should_skip = if let Some(message) = &entry.message
                                && let (Some(msg_id), Some(req_id)) =
                                    (&message.id, &entry.request_id)
                            {
                                let hash = create_entry_hash(msg_id, req_id);
                                let mut hashes = global_hashes.lock().unwrap();
                                if hashes.contains(&hash) {
                                    true
                                } else {
                                    hashes.insert(hash);
                                    false
                                }
                            } else {
                                false
                            };

                            if should_skip {
                                continue;
                            }

                            // Check conditions first
                            let is_today = entry
                                .timestamp
                                .as_ref()
                                .is_some_and(|ts| ts.starts_with(today));
                            let is_target_session = session_file_id.as_str() == target_session;

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

                    let mut by_session = HashMap::new();
                    if !target_session_entries.is_empty() {
                        by_session.insert(target_session.to_string(), target_session_entries);
                    }

                    Ok(UsageSnapshot {
                        all_entries,
                        by_session,
                        today_entries,
                        processed_hashes: HashSet::new(), // Not needed as we use global
                    })
                },
            )
        })
        .collect();

    // Merge results from all base paths
    let mut merged = UsageSnapshot {
        all_entries: Vec::with_capacity(100000),
        by_session: HashMap::new(),
        today_entries: Vec::with_capacity(10000),
        processed_hashes: Arc::try_unwrap(global_hashes)
            .map(|mutex| mutex.into_inner().unwrap())
            .unwrap_or_else(|arc| arc.lock().unwrap().clone()),
    };

    for task in tasks {
        let data = task.await??;
        merged.all_entries.extend(data.all_entries);
        merged.today_entries.extend(data.today_entries);

        for (session_id, entries) in data.by_session {
            merged
                .by_session
                .entry(session_id)
                .or_default()
                .extend(entries);
        }
    }

    // Sort all entries by timestamp once (string sort is sufficient for ISO 8601)
    merged
        .all_entries
        .sort_by(|a, b| a.timestamp.as_deref().cmp(&b.timestamp.as_deref()));

    Ok(merged)
}

/// Calculate today's cost from usage snapshot
pub fn calculate_today_cost(
    data: &UsageSnapshot,
    pricing_map: &HashMap<&str, ModelPricing>,
) -> f64 {
    data.today_entries
        .par_iter() // Use parallel iterator for cost calculation
        .map(|entry| calculate_entry_cost(entry, pricing_map))
        .sum()
}

/// Calculate session cost from usage snapshot
pub fn calculate_session_cost(
    data: &UsageSnapshot,
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

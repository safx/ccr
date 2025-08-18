use crate::types::{MergedUsageSnapshot, ModelPricing, SessionId, UsageEntry, UsageSnapshot};
use crate::utils::create_entry_hash;
use rayon::prelude::*;
use serde_json;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::task;

/// Load all data with optimized parallelism
pub async fn load_all_data(
    claude_paths: &[PathBuf],
    _session_id: &SessionId,
) -> Result<MergedUsageSnapshot, Box<dyn std::error::Error + Send + Sync>> {
    // Use a shared mutex for deduplication across all threads
    let global_hashes = Arc::new(Mutex::new(HashSet::with_capacity(50000)));

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
                                    // Parse lines in parallel and set session_id
                                    let entries: Vec<UsageEntry> = contents
                                        .par_lines()
                                        .filter(|line| !line.trim().is_empty())
                                        .filter_map(|line| {
                                            let mut entry: UsageEntry =
                                                serde_json::from_str(line).ok()?;
                                            entry.session_id =
                                                SessionId::from(session_file_id.as_str());
                                            Some(entry)
                                        })
                                        .collect();

                                    (session_file_id, entries)
                                }
                                Err(_) => (session_file_id, Vec::new()),
                            }
                        })
                        .collect();

                    // Process results with global deduplication
                    let mut all_entries = Vec::with_capacity(50000);

                    for (_session_file_id, entries) in results {
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

                            all_entries.push(entry);
                        }
                    }

                    Ok(UsageSnapshot { all_entries })
                },
            )
        })
        .collect();

    // Merge results from all base paths
    let mut all_entries = Vec::with_capacity(50000);

    for task in tasks {
        let data = task.await??;
        all_entries.extend(data.all_entries);
    }

    // Sort all entries by timestamp once (string sort is sufficient for ISO 8601)
    all_entries.sort_by(|a, b| a.timestamp.as_deref().cmp(&b.timestamp.as_deref()));

    Ok(MergedUsageSnapshot { all_entries })
}

/// Calculate today's cost from usage snapshot
pub fn calculate_today_cost(
    data: &MergedUsageSnapshot,
    pricing_map: &std::collections::HashMap<&str, ModelPricing>,
) -> f64 {
    data.calculate_today_cost(pricing_map)
}

/// Calculate session cost from usage snapshot
pub fn calculate_session_cost(
    data: &MergedUsageSnapshot,
    session_id: &SessionId,
    pricing_map: &std::collections::HashMap<&str, ModelPricing>,
) -> Option<f64> {
    data.calculate_session_cost(session_id, pricing_map)
}

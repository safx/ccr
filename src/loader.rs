use crate::types::{MergedUsageSnapshot, SessionId, UniqueHash, UsageEntry, UsageEntryData};
use chrono::{Duration, Local, Utc};
use rayon::prelude::*;
use serde_json;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::task;

/// Determines if an entry should be kept based on filtering criteria
/// Used for early filtering to reduce memory usage
fn should_keep_entry(
    entry: &UsageEntry,
    current_session_id: &SessionId,
    cutoff_timestamp: &str,
) -> bool {
    // Always keep entries from the current session
    if entry.session_id == *current_session_id {
        return true;
    }

    // Keep entries after the cutoff timestamp
    // (today's entries or last 6 hours, whichever is earlier)
    if let Some(timestamp) = &entry.data.timestamp {
        timestamp.as_str() >= cutoff_timestamp
    } else {
        // Keep entries without timestamps (edge case)
        true
    }
}

/// Load all data with optimized parallelism and early filtering
pub async fn load_all_data(
    claude_paths: &[PathBuf],
    session_id: &SessionId,
) -> Result<MergedUsageSnapshot, Box<dyn std::error::Error + Send + Sync>> {
    // Use a shared mutex for deduplication across all threads
    let global_hashes: Arc<Mutex<HashSet<UniqueHash>>> =
        Arc::new(Mutex::new(HashSet::with_capacity(50000)));

    // Calculate filter boundaries
    // Today's start (in UTC for comparison with timestamps)
    let today_start = Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_local_timezone(Local)
        .unwrap()
        .with_timezone(&Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // Six hours ago (for session blocks - ensures we get the current block)
    // This is important for burn rate calculation
    let six_hours_ago = Utc::now()
        .checked_sub_signed(Duration::hours(5))
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // Use the earlier of today_start or six_hours_ago as the cutoff
    // This ensures we capture all necessary data for both "today" stats and "current block"
    let cutoff_timestamp = if today_start < six_hours_ago {
        today_start
    } else {
        six_hours_ago
    };

    // Current session ID for filtering
    let current_session_id = session_id.clone();

    let tasks: Vec<_> = claude_paths
        .iter()
        .map(|base_path| {
            let base_path = base_path.clone();
            let global_hashes = Arc::clone(&global_hashes);
            let cutoff_timestamp = cutoff_timestamp.clone();
            let current_session_id = current_session_id.clone();

            task::spawn_blocking(
                move || -> Result<Vec<UsageEntry>, Box<dyn std::error::Error + Send + Sync>> {
                    let projects_path = base_path.join("projects");
                    if !projects_path.exists() {
                        return Ok(Vec::new());
                    }

                    // Collect all file paths in parallel
                    let project_dirs: Vec<_> = fs::read_dir(&projects_path)?
                        .filter_map(|entry| entry.ok())
                        .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                        .collect();

                    // Parallel scan of all project directories
                    let all_files: Vec<_> = project_dirs
                        .par_iter()
                        .flat_map(|project_entry| {
                            fs::read_dir(project_entry.path())
                                .ok()
                                .map(|entries| {
                                    entries
                                        .filter_map(|entry| entry.ok())
                                        .filter_map(|file_entry| {
                                            let file_name = file_entry.file_name();
                                            let file_name_str = file_name.to_string_lossy();
                                            if file_name_str.ends_with(".jsonl") {
                                                let session_from_file = file_name_str
                                                    .trim_end_matches(".jsonl")
                                                    .to_string();
                                                Some((file_entry.path(), session_from_file))
                                            } else {
                                                None
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default()
                        })
                        .collect();

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
                                            let data: UsageEntryData =
                                                serde_json::from_str(line).ok()?;
                                            let entry = UsageEntry::from_data(
                                                data,
                                                SessionId::from(session_file_id.as_str()),
                                            );

                                            // Apply early filtering
                                            if should_keep_entry(
                                                &entry,
                                                &current_session_id,
                                                &cutoff_timestamp,
                                            ) {
                                                Some(entry)
                                            } else {
                                                None
                                            }
                                        })
                                        .collect();

                                    entries
                                }
                                Err(_) => Vec::new(),
                            }
                        })
                        .collect();

                    // Process results with global deduplication
                    let mut all_entries = Vec::with_capacity(50);

                    for entries in results {
                        let mut hashes = global_hashes.lock().unwrap();
                        for entry in entries {
                            // Global deduplication check
                            if let Some(message) = &entry.data.message
                                && let (Some(msg_id), Some(req_id)) =
                                    (&message.id, &entry.data.request_id)
                            {
                                let hash = UniqueHash::from((msg_id, req_id));
                                if hashes.contains(&hash) {
                                    continue;
                                }
                                hashes.insert(hash);
                            };

                            all_entries.push(entry);
                        }
                    }

                    Ok(all_entries)
                },
            )
        })
        .collect();

    // Merge results from all base paths
    let mut all_entries = Vec::with_capacity(50000);

    for task in tasks {
        let data = task.await??;
        all_entries.extend(data);
    }

    // Sort all entries by timestamp once (string sort is sufficient for ISO 8601)
    all_entries.sort_by(|a, b| {
        a.data
            .timestamp
            .as_deref()
            .cmp(&b.data.timestamp.as_deref())
    });

    Ok(MergedUsageSnapshot { all_entries })
}

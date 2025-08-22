use crate::constants::SESSION_BLOCK_DURATION;
use crate::error::Result;
use crate::types::{MergedUsageSnapshot, SessionId, UniqueHash, UsageEntry, UsageEntryData};
use chrono::{Local, Utc};
use rayon::prelude::*;
use serde_json;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::task;

// Capacity constants for performance optimization
const INITIAL_HASH_CAPACITY: usize = 1024;
const ENTRIES_BATCH_CAPACITY: usize = 128;
const ALL_ENTRIES_CAPACITY: usize = 1024;

/// Filter boundaries for data loading
struct FilterBoundaries {
    cutoff_timestamp: String,
}

impl FilterBoundaries {
    /// Calculate filter boundaries based on today's start and session block duration
    fn new() -> Self {
        // Today's start (in UTC for comparison with timestamps)
        let today_start = Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap()
            .with_timezone(&Utc);

        // To avoid cutting session blocks in half, go back one full session block
        // before today's start. This ensures we capture complete session blocks
        // that might span across midnight.
        let safe_today_cutoff = today_start
            .checked_sub_signed(SESSION_BLOCK_DURATION)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        // Also ensure we get at least 2 session blocks from current time
        // (current block + previous block for proper cost calculation)
        let minimum_lookback = Utc::now()
            .checked_sub_signed(SESSION_BLOCK_DURATION * 2)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        // Use the earlier timestamp as the cutoff to ensure complete data
        let cutoff_timestamp = if safe_today_cutoff < minimum_lookback {
            safe_today_cutoff
        } else {
            minimum_lookback
        };

        Self { cutoff_timestamp }
    }
}

/// Determines if an entry should be kept based on filtering criteria
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
    if let Some(timestamp) = &entry.data.timestamp {
        timestamp.as_str() >= cutoff_timestamp
    } else {
        // Keep entries without timestamps (edge case)
        true
    }
}

/// Collect all JSONL files from a projects directory
fn collect_jsonl_files(projects_path: &Path) -> Vec<(PathBuf, String)> {
    if !projects_path.exists() {
        return Vec::new();
    }

    // Collect all project directories
    let project_dirs: Vec<_> = fs::read_dir(projects_path)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();

    // Parallel scan of all project directories for JSONL files
    project_dirs
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
                                let session_id =
                                    file_name_str.trim_end_matches(".jsonl").to_string();
                                Some((file_entry.path(), session_id))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect()
}

/// Process a single JSONL file and return filtered entries
fn process_jsonl_file(
    path: &Path,
    session_file_id: &str,
    current_session_id: &SessionId,
    cutoff_timestamp: &str,
) -> Vec<UsageEntry> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            // Parse lines in parallel with early filtering
            contents
                .par_lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| {
                    let data: UsageEntryData = serde_json::from_str(line).ok()?;
                    let entry = UsageEntry::from_data(data, SessionId::from(session_file_id));

                    // Apply early filtering to reduce memory usage
                    if should_keep_entry(&entry, current_session_id, cutoff_timestamp) {
                        Some(entry)
                    } else {
                        None
                    }
                })
                .collect()
        }
        Err(_) => Vec::new(),
    }
}

/// Deduplicate entries using global hash set
fn deduplicate_entries(
    results: Vec<Vec<UsageEntry>>,
    global_hashes: Arc<Mutex<HashSet<UniqueHash>>>,
) -> Result<Vec<Arc<UsageEntry>>> {
    let mut all_entries = Vec::with_capacity(ENTRIES_BATCH_CAPACITY);

    for entries in results {
        // Minimize lock holding time by batching operations
        let mut hashes = global_hashes
            .lock()
            .map_err(|_| crate::error::CcrError::LockPoisoned)?;

        for entry in entries {
            // Check for duplicate only when both IDs exist
            if let Some(hash) = UniqueHash::from_usage_entry_data(&entry.data) {
                if hashes.contains(&hash) {
                    continue;
                }
                hashes.insert(hash);
            }

            all_entries.push(Arc::new(entry));
        }
        // Lock is automatically released here
    }

    Ok(all_entries)
}

/// Process all files from a single base path
async fn process_base_path(
    base_path: PathBuf,
    global_hashes: Arc<Mutex<HashSet<UniqueHash>>>,
    current_session_id: SessionId,
    cutoff_timestamp: String,
) -> Result<Vec<Arc<UsageEntry>>> {
    task::spawn_blocking(move || {
        let projects_path = base_path.join("projects");

        // Collect all JSONL files
        let all_files = collect_jsonl_files(&projects_path);

        // Process files in parallel
        let results: Vec<_> = all_files
            .par_iter()
            .map(|(path, session_file_id)| {
                process_jsonl_file(
                    path,
                    session_file_id,
                    &current_session_id,
                    &cutoff_timestamp,
                )
            })
            .collect();

        // Deduplicate entries
        let entries = deduplicate_entries(results, global_hashes)?;

        Ok(entries)
    })
    .await?
}

/// Load all data with optimized parallelism and early filtering
pub async fn load_all_data(
    claude_paths: &[PathBuf],
    session_id: &SessionId,
) -> Result<MergedUsageSnapshot> {
    // Initialize shared state for deduplication
    let global_hashes: Arc<Mutex<HashSet<UniqueHash>>> =
        Arc::new(Mutex::new(HashSet::with_capacity(INITIAL_HASH_CAPACITY)));

    // Calculate filter boundaries
    let boundaries = FilterBoundaries::new();

    // Process each base path in parallel
    let tasks: Vec<_> = claude_paths
        .iter()
        .map(|base_path| {
            process_base_path(
                base_path.clone(),
                Arc::clone(&global_hashes),
                session_id.clone(),
                boundaries.cutoff_timestamp.clone(),
            )
        })
        .collect();

    // Merge results from all base paths
    let mut all_entries = Vec::with_capacity(ALL_ENTRIES_CAPACITY);

    for task in tasks {
        let data = task.await?;
        all_entries.extend(data);
    }

    // Sort all entries by timestamp (string sort is sufficient for ISO 8601)
    all_entries.sort_by(|a, b| {
        a.data
            .timestamp
            .as_deref()
            .cmp(&b.data.timestamp.as_deref())
    });

    Ok(MergedUsageSnapshot { all_entries })
}

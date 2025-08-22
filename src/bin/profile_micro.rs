use ccr::constants::SESSION_BLOCK_DURATION;
use ccr::types::{SessionId, UniqueHash, UsageEntry, UsageEntryData};
use chrono::{Local, Utc};
use colored::Colorize;
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const INITIAL_HASH_CAPACITY: usize = 1024;

fn collect_jsonl_files(projects_path: &Path) -> Vec<(PathBuf, String)> {
    if !projects_path.exists() {
        return Vec::new();
    }

    let project_dirs: Vec<_> = fs::read_dir(projects_path)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();

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

fn process_jsonl_file(
    path: &Path,
    session_file_id: &str,
    current_session_id: &SessionId,
    cutoff_timestamp: &str,
) -> Vec<UsageEntry> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            contents
                .par_lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| {
                    let data: UsageEntryData = serde_json::from_str(line).ok()?;
                    let entry = UsageEntry::from_data(data, SessionId::from(session_file_id));

                    // Simple filter
                    if entry.session_id == *current_session_id {
                        return Some(entry);
                    }

                    if let Some(timestamp) = &entry.data.timestamp
                        && timestamp.as_str() >= cutoff_timestamp
                    {
                        return Some(entry);
                    }

                    None
                })
                .collect()
        }
        Err(_) => Vec::new(),
    }
}

// Original sequential deduplication (similar to current implementation)
fn deduplicate_sequential_mutex(results: Vec<Vec<UsageEntry>>) -> Vec<Arc<UsageEntry>> {
    let mut all_entries = Vec::new();
    let global_hashes: Arc<Mutex<HashSet<UniqueHash>>> =
        Arc::new(Mutex::new(HashSet::with_capacity(INITIAL_HASH_CAPACITY)));

    for entries in results {
        let mut hashes = global_hashes.lock().unwrap();

        for entry in entries {
            if let Some(hash) = UniqueHash::from_usage_entry_data(&entry.data) {
                if hashes.contains(&hash) {
                    continue;
                }
                hashes.insert(hash);
            }
            all_entries.push(Arc::new(entry));
        }
    }

    all_entries
}

// Optimized: No mutex, just local HashSet
fn deduplicate_sequential_no_mutex(results: Vec<Vec<UsageEntry>>) -> Vec<Arc<UsageEntry>> {
    let mut all_entries = Vec::new();
    let mut hashes = HashSet::with_capacity(INITIAL_HASH_CAPACITY);

    for entries in results {
        for entry in entries {
            if let Some(hash) = UniqueHash::from_usage_entry_data(&entry.data) {
                if hashes.contains(&hash) {
                    continue;
                }
                hashes.insert(hash);
            }
            all_entries.push(Arc::new(entry));
        }
    }

    all_entries
}

// Parallel deduplication with local merge
fn deduplicate_parallel_local_merge(results: Vec<Vec<UsageEntry>>) -> Vec<Arc<UsageEntry>> {
    // Process each batch in parallel, maintaining local hash sets
    let processed: Vec<(Vec<Arc<UsageEntry>>, HashSet<UniqueHash>)> = results
        .into_par_iter()
        .map(|entries| {
            let mut local_entries = Vec::new();
            let mut local_hashes = HashSet::new();

            for entry in entries {
                if let Some(hash) = UniqueHash::from_usage_entry_data(&entry.data) {
                    if !local_hashes.contains(&hash) {
                        local_hashes.insert(hash.clone());
                        local_entries.push(Arc::new(entry));
                    }
                } else {
                    local_entries.push(Arc::new(entry));
                }
            }

            (local_entries, local_hashes)
        })
        .collect();

    // Merge results sequentially
    let mut all_entries = Vec::new();
    let mut global_hashes = HashSet::with_capacity(INITIAL_HASH_CAPACITY);

    for (entries, local_hashes) in processed {
        // Merge hashes
        for hash in local_hashes {
            global_hashes.insert(hash);
        }

        // Add entries, checking against global set
        for entry in entries {
            if let Some(hash) = UniqueHash::from_usage_entry_data(&entry.data) {
                if global_hashes.contains(&hash) {
                    continue;
                }
                global_hashes.insert(hash);
            }
            all_entries.push(entry);
        }
    }

    all_entries
}

fn benchmark_method<F>(name: &str, method: F, results: &[Vec<UsageEntry>]) -> u128
where
    F: Fn(Vec<Vec<UsageEntry>>) -> Vec<Arc<UsageEntry>>,
{
    // Clone the input data
    let input = results.to_vec();

    let start = Instant::now();
    let output = method(input);
    let duration = start.elapsed().as_micros();

    println!("{:<30} {:>8}μs - {} entries", name, duration, output.len());

    duration
}

#[tokio::main]
async fn main() -> ccr::Result<()> {
    println!(
        "{}",
        "=== Deduplication Micro-benchmarking ===".green().bold()
    );

    // Setup
    let claude_paths = ccr::utils::get_claude_paths();
    if claude_paths.is_empty() {
        println!("No Claude paths found");
        return Ok(());
    }

    let session_id = SessionId::from("test-profiling-session");

    // Calculate cutoff timestamp
    let today_start = Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_local_timezone(Local)
        .unwrap()
        .with_timezone(&Utc);

    let cutoff_timestamp = today_start
        .checked_sub_signed(SESSION_BLOCK_DURATION)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // Collect all files
    println!("\n{}", "Collecting and processing files...".yellow());
    let mut all_files = Vec::new();
    for base_path in &claude_paths {
        let projects_path = base_path.join("projects");
        let files = collect_jsonl_files(&projects_path);
        all_files.extend(files);
    }

    println!("Found {} JSONL files", all_files.len());

    // Process all files to get test data
    let start = Instant::now();
    let results: Vec<_> = all_files
        .par_iter()
        .map(|(path, session_file_id)| {
            process_jsonl_file(path, session_file_id, &session_id, &cutoff_timestamp)
        })
        .collect();

    let process_time = start.elapsed().as_millis();
    let total_entries: usize = results.iter().map(|v| v.len()).sum();

    println!("Processed {} files in {}ms", all_files.len(), process_time);
    println!("Total entries before dedup: {}", total_entries);

    // Warm up
    println!("\n{}", "Warming up...".yellow());
    for _ in 0..3 {
        let _ = deduplicate_sequential_no_mutex(results.clone());
    }

    // Benchmark different methods
    println!("\n{}", "=== Benchmarking (5 runs each) ===".cyan().bold());

    let methods = vec![
        (
            "Sequential with Mutex",
            deduplicate_sequential_mutex as fn(_) -> _,
        ),
        (
            "Sequential no Mutex",
            deduplicate_sequential_no_mutex as fn(_) -> _,
        ),
        (
            "Parallel local merge",
            deduplicate_parallel_local_merge as fn(_) -> _,
        ),
    ];

    for (name, method) in methods {
        println!("\n{}:", name.green());
        let mut times = Vec::new();

        for run in 1..=5 {
            print!("  Run {}: ", run);
            let time = benchmark_method("", method, &results);
            times.push(time);
        }

        let avg = times.iter().sum::<u128>() / times.len() as u128;
        let min = times.iter().min().unwrap();
        let max = times.iter().max().unwrap();

        println!(
            "  {} Avg: {}μs, Min: {}μs, Max: {}μs",
            "Summary:".yellow(),
            avg,
            min,
            max
        );
    }

    // Additional analysis
    println!("\n{}", "=== Analysis ===".green().bold());

    // Test with larger dataset (duplicate the data multiple times)
    let mut large_results = Vec::new();
    for _ in 0..10 {
        large_results.extend(results.clone());
    }

    let large_entries: usize = large_results.iter().map(|v| v.len()).sum();
    println!("\nTesting with larger dataset: {} entries", large_entries);

    println!("\n{}", "Large dataset results:".cyan());
    for (name, method) in &[
        (
            "Sequential no Mutex",
            deduplicate_sequential_no_mutex as fn(_) -> _,
        ),
        (
            "Parallel local merge",
            deduplicate_parallel_local_merge as fn(_) -> _,
        ),
    ] {
        let time = benchmark_method(name, *method, &large_results);
        println!("  {}: {}μs", name, time);
    }

    Ok(())
}

use chrono::Utc;
use rayon::prelude::*;
use serde_json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use tokio::task;

use crate::{UsageEntry, ModelPricing, calculate_cost, TokenUsage};

/// Unified data loaded from all files
#[derive(Debug, Clone)]
pub struct UnifiedData {
    /// All entries loaded from files
    pub all_entries: Vec<UsageEntry>,
    /// Entries indexed by session ID
    pub by_session: HashMap<String, Vec<UsageEntry>>,
    /// Today's entries
    pub today_entries: Vec<UsageEntry>,
    /// Deduplicated hashes
    pub processed_hashes: HashSet<String>,
}

/// Load all JSONL files once and process them
pub async fn load_all_data_unified(
    claude_paths: &[PathBuf],
    session_id: &str,
) -> Result<UnifiedData, Box<dyn std::error::Error + Send + Sync>> {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let session_id = session_id.to_string();

    // Process each base path in parallel
    let tasks: Vec<_> = claude_paths
        .iter()
        .map(|base_path| {
            let base_path = base_path.clone();
            let today = today.clone();
            let session_id = session_id.clone();
            
            task::spawn_blocking(move || -> Result<UnifiedData, Box<dyn std::error::Error + Send + Sync>> {
                let projects_path = base_path.join("projects");
                if !projects_path.exists() {
                    return Ok(UnifiedData {
                        all_entries: Vec::new(),
                        by_session: HashMap::new(),
                        today_entries: Vec::new(),
                        processed_hashes: HashSet::new(),
                    });
                }

                // Collect all JSONL file paths first
                let mut jsonl_files = Vec::new();
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
                            // Extract session ID from filename (e.g., "session123.jsonl" -> "session123")
                            let session_from_file = file_name_str.trim_end_matches(".jsonl");
                            jsonl_files.push((file_entry.path(), session_from_file.to_string()));
                        }
                    }
                }

                // Process files in parallel using rayon
                let file_data: Vec<(String, Vec<UsageEntry>)> = jsonl_files
                    .par_iter()
                    .with_min_len(5) // Process at least 5 files per thread
                    .flat_map(|(path, filename)| {
                        match fs::File::open(path) {
                            Ok(file) => {
                                let reader = BufReader::with_capacity(256 * 1024, file);
                                
                                let mut entries = Vec::with_capacity(100);
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
                                vec![(filename.clone(), entries)]
                            }
                            Err(_) => {
                                vec![]
                            }
                        }
                    })
                    .collect();

                // Now process all entries in memory
                let mut all_entries = Vec::new();
                let mut by_session = HashMap::new();
                let mut today_entries = Vec::new();
                let mut processed_hashes = HashSet::new();

                for (session_file_id, entries) in file_data {
                    for entry in entries {
                        // Deduplication check
                        if let Some(message) = &entry.message {
                            if let (Some(msg_id), Some(req_id)) = (&message.id, &entry.request_id) {
                                let hash = format!("{}:{}", msg_id, req_id);
                                if processed_hashes.contains(&hash) {
                                    continue; // Skip duplicate
                                }
                                processed_hashes.insert(hash);
                            }
                        }

                        // Check if it's today's entry
                        if let Some(timestamp) = &entry.timestamp {
                            if timestamp.starts_with(&today) {
                                today_entries.push(entry.clone());
                            }
                        }

                        // Check if it belongs to the session (based on filename)
                        if session_file_id == session_id {
                            by_session.entry(session_id.clone())
                                .or_insert_with(Vec::new)
                                .push(entry.clone());
                        }

                        // Add to all entries
                        all_entries.push(entry);
                    }
                }

                Ok(UnifiedData {
                    all_entries,
                    by_session,
                    today_entries,
                    processed_hashes,
                })
            })
        })
        .collect();

    // Merge results from all tasks
    let mut merged = UnifiedData {
        all_entries: Vec::new(),
        by_session: HashMap::new(),
        today_entries: Vec::new(),
        processed_hashes: HashSet::new(),
    };

    for task in tasks {
        let data = task.await??;
        
        // Merge all entries
        for entry in data.all_entries {
            // Check for duplicates across different base paths
            if let Some(message) = &entry.message {
                if let (Some(msg_id), Some(req_id)) = (&message.id, &entry.request_id) {
                    let hash = format!("{}:{}", msg_id, req_id);
                    if merged.processed_hashes.contains(&hash) {
                        continue;
                    }
                    merged.processed_hashes.insert(hash);
                }
            }
            merged.all_entries.push(entry);
        }

        // Merge today's entries (already deduplicated)
        merged.today_entries.extend(data.today_entries);

        // Merge session entries
        for (session_id, entries) in data.by_session {
            merged.by_session.entry(session_id)
                .or_insert_with(Vec::new)
                .extend(entries);
        }
    }

    Ok(merged)
}

/// Calculate today's cost from unified data
pub fn calculate_today_cost(
    data: &UnifiedData,
    pricing_map: &HashMap<&str, ModelPricing>,
) -> f64 {
    data.today_entries.iter()
        .map(|entry| calculate_entry_cost(entry, pricing_map))
        .sum()
}

/// Calculate session cost from unified data
pub fn calculate_session_cost(
    data: &UnifiedData,
    session_id: &str,
    pricing_map: &HashMap<&str, ModelPricing>,
) -> Option<f64> {
    data.by_session.get(session_id)
        .map(|entries| {
            entries.iter()
                .map(|entry| calculate_entry_cost(entry, pricing_map))
                .sum()
        })
}

/// Calculate entry cost with pricing map
fn calculate_entry_cost(
    entry: &UsageEntry,
    pricing_map: &HashMap<&str, ModelPricing>,
) -> f64 {
    // Check for pre-calculated cost
    if let Some(cost) = entry.cost_usd {
        return cost;
    }

    // Calculate from usage data
    if let Some(message) = &entry.message {
        if let Some(usage) = &message.usage {
            let model_name = message.model.as_ref().or(entry.model.as_ref());
            
            if let Some(model_name) = model_name {
                if let Some(pricing) = pricing_map.get(model_name.as_str()) {
                    let tokens = TokenUsage {
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        cache_creation_tokens: usage.cache_creation_input_tokens,
                        cache_read_tokens: usage.cache_read_input_tokens,
                    };
                    return calculate_cost(&tokens, pricing);
                }
            }
        }
    }

    0.0
}
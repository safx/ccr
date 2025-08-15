use chrono::{DateTime, Timelike, Utc};
use colored::*;
use rayon::prelude::*;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tokio::fs as async_fs;
use tokio::task;

// Re-use structures from lib.rs
use ccr::{ModelPricing, TokenUsage, UsageEntry, calculate_cost};

// Simple Result type alias with Send + Sync for async
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

// Static model pricing data - initialized once
static MODEL_PRICING: LazyLock<HashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut map = HashMap::with_capacity(4);

    map.insert(
        "claude-opus-4-1-20250805",
        ModelPricing {
            input_cost_per_token: Some(0.000015),
            output_cost_per_token: Some(0.000075),
            cache_creation_input_token_cost: Some(0.00001875),
            cache_read_input_token_cost: Some(0.0000015),
        },
    );

    map.insert(
        "claude-sonnet-4-20250514",
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            cache_creation_input_token_cost: Some(0.00000375),
            cache_read_input_token_cost: Some(0.0000003),
        },
    );

    map.insert(
        "claude-3-opus-20240229",
        ModelPricing {
            input_cost_per_token: Some(0.000015),
            output_cost_per_token: Some(0.000075),
            cache_creation_input_token_cost: Some(0.00001875),
            cache_read_input_token_cost: Some(0.0000015),
        },
    );

    map.insert(
        "claude-3.5-sonnet-20241022",
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            cache_creation_input_token_cost: Some(0.00000375),
            cache_read_input_token_cost: Some(0.0000003),
        },
    );

    map
});

// Hook input schema
#[derive(Debug, Deserialize)]
struct StatuslineHookJson {
    session_id: String,
    transcript_path: String,
    cwd: String,
    model: ModelInfo,
    #[serde(default)]
    _workspace: Option<serde_json::Value>,
    #[serde(default)]
    _version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    #[serde(default)]
    _id: String,
    display_name: String,
}

// Transcript message structure for parsing JSONL
#[derive(Debug, Deserialize)]
struct TranscriptMessage {
    #[serde(rename = "type")]
    message_type: String,
    #[serde(default)]
    message: Option<TranscriptMessageContent>,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessageContent {
    #[serde(default)]
    usage: Option<TranscriptUsage>,
}

#[derive(Debug, Deserialize)]
struct TranscriptUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
}

// Optimized model pricing lookup
fn get_model_pricing(model_name: &str) -> Option<&'static ModelPricing> {
    // Direct match first
    if let Some(pricing) = MODEL_PRICING.get(model_name) {
        return Some(pricing);
    }

    // Lowercase once for comparison
    let lower_name = model_name.to_lowercase();

    // Partial match
    for (key, pricing) in MODEL_PRICING.iter() {
        if model_name.contains(key) || key.contains(model_name) {
            return Some(pricing);
        }
    }

    // Default based on model type
    if lower_name.contains("opus") {
        MODEL_PRICING.get("claude-opus-4-1-20250805")
    } else if lower_name.contains("sonnet") {
        MODEL_PRICING.get("claude-sonnet-4-20250514")
    } else {
        None
    }
}

// Get Claude configuration directories - cached
fn get_claude_paths() -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(2);
    let home = home::home_dir().unwrap_or_default();

    // Check environment variable first
    if let Ok(custom_path) = env::var("CLAUDE_CONFIG_DIR") {
        for path in custom_path.split(',') {
            let path = path.trim();
            if !path.is_empty() {
                let p = PathBuf::from(path);
                if p.exists() && p.is_dir() {
                    paths.push(p);
                }
            }
        }
    } else {
        // Default paths
        let config_path = home.join(".config").join("claude");
        if config_path.exists() && config_path.is_dir() {
            paths.push(config_path);
        }
        let home_path = home.join(".claude");
        if home_path.exists() && home_path.is_dir() {
            paths.push(home_path);
        }
    }

    paths
}

// Format currency - simple and clean
#[inline]
fn format_currency(amount: f64) -> String {
    format!("${:.2}", amount)
}

// Format number with thousands separator (like toLocaleString)
#[inline]
fn format_number_with_commas(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;

    for c in s.chars().rev() {
        if count == 3 {
            result.push(',');
            count = 0;
        }
        result.push(c);
        count += 1;
    }

    result.chars().rev().collect()
}

// Format remaining time
#[inline]
fn format_remaining_time(remaining_minutes: u64) -> String {
    let mins = remaining_minutes % 60;
    if remaining_minutes > 60 {
        let hours = remaining_minutes / 60;
        format!("{}h {}m left", hours, mins)
    } else {
        format!("{}m left", mins)
    }
}

// Floor timestamp to hour
#[inline]
fn floor_to_hour(dt: DateTime<Utc>) -> DateTime<Utc> {
    dt.date_naive()
        .and_hms_opt(dt.hour(), 0, 0)
        .unwrap()
        .and_utc()
}

// Optimized duplicate detection
#[inline]
fn is_duplicate_fast(entry: &UsageEntry, processed_hashes: &mut HashSet<u64>) -> bool {
    if let (Some(message), Some(request_id)) = (&entry.message, &entry.request_id)
        && let Some(message_id) = &message.id
    {
        // Use hash instead of string concatenation
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        message_id.hash(&mut hasher);
        request_id.hash(&mut hasher);
        let hash = hasher.finish();

        if processed_hashes.contains(&hash) {
            return true;
        }
        processed_hashes.insert(hash);
    }
    false
}

// Calculate cost from entry - optimized
#[inline]
fn calculate_entry_cost(entry: &UsageEntry) -> f64 {
    // Check for pre-calculated cost
    if let Some(cost) = entry.cost_usd {
        return cost;
    }

    // Calculate from usage data
    if let Some(message) = &entry.message
        && let Some(usage) = &message.usage
    {
        let model_name = message.model.as_ref().or(entry.model.as_ref());

        if let Some(model_name) = model_name
            && let Some(pricing) = get_model_pricing(model_name)
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

// Process JSONL file efficiently with parallel line processing
fn process_jsonl_file(
    path: &Path,
    processor: impl Fn(&UsageEntry) -> Option<f64> + Sync,
    processed_hashes: &mut HashSet<u64>,
) -> Result<f64> {
    use std::sync::{Arc, Mutex};

    let file = fs::File::open(path)?;
    let reader = BufReader::with_capacity(128 * 1024, file); // Increased to 128KB buffer

    // Read all lines into memory for parallel processing
    let lines: Vec<String> = reader
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .collect();

    // Use a mutex for the hash set to avoid duplicates
    let hashes_mutex = Arc::new(Mutex::new(std::mem::take(processed_hashes)));

    // Process lines in parallel using rayon
    let total: f64 = lines
        .par_iter()
        .with_min_len(10) // Process at least 10 lines per task for better efficiency
        .filter_map(|line| {
            if let Ok(entry) = serde_json::from_str::<UsageEntry>(line) {
                let mut hashes = hashes_mutex.lock().unwrap();
                if !is_duplicate_fast(&entry, &mut hashes) {
                    drop(hashes); // Release lock early
                    processor(&entry)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .sum();

    // Update the original hash set
    *processed_hashes = Arc::try_unwrap(hashes_mutex)
        .unwrap_or_else(|_| panic!("Failed to unwrap Arc"))
        .into_inner()
        .unwrap();

    Ok(total)
}

// Load session usage by ID - optimized
async fn load_session_usage_by_id(session_id: &str) -> Result<Option<f64>> {
    let claude_paths = get_claude_paths();
    let session_id = session_id.to_string();

    // Process paths in parallel
    let tasks: Vec<_> = claude_paths
        .into_iter()
        .map(|base_path| {
            let session_id = session_id.clone();
            task::spawn_blocking(move || -> Result<f64> {
                let projects_path = base_path.join("projects");
                if !projects_path.exists() {
                    return Ok(0.0);
                }

                let mut total_cost = 0.0;
                let mut processed_hashes = HashSet::with_capacity(1000);

                for entry in fs::read_dir(&projects_path)? {
                    let entry = entry?;
                    if !entry.file_type()?.is_dir() {
                        continue;
                    }

                    let session_file = entry.path().join(format!("{}.jsonl", session_id));
                    if session_file.exists() {
                        total_cost += process_jsonl_file(
                            &session_file,
                            |e| Some(calculate_entry_cost(e)),
                            &mut processed_hashes,
                        )?;
                    }
                }

                Ok(total_cost)
            })
        })
        .collect();

    let mut total = 0.0;
    for task in tasks {
        total += task.await??;
    }

    Ok(if total > 0.0 { Some(total) } else { None })
}

// Load today's usage data - optimized
async fn load_today_usage_data() -> Result<f64> {
    let claude_paths = get_claude_paths();
    let today = Utc::now().format("%Y-%m-%d").to_string();

    // Process paths in parallel to collect entries
    let tasks: Vec<_> = claude_paths
        .into_iter()
        .map(|base_path| {
            let today = today.clone(); // Simple clone of 10-char string
            task::spawn_blocking(move || -> Result<Vec<UsageEntry>> {
                let projects_path = base_path.join("projects");
                if !projects_path.exists() {
                    return Ok(Vec::new());
                }

                // Collect all JSONL files first
                let mut jsonl_files = Vec::new();
                for project_entry in fs::read_dir(&projects_path)? {
                    let project_entry = project_entry?;
                    if !project_entry.file_type()?.is_dir() {
                        continue;
                    }

                    for file_entry in fs::read_dir(project_entry.path())? {
                        let file_entry = file_entry?;
                        let file_name = file_entry.file_name();
                        if file_name.to_string_lossy().ends_with(".jsonl") {
                            jsonl_files.push(file_entry.path());
                        }
                    }
                }

                // Process files in parallel to collect today's entries
                let entries: Vec<UsageEntry> = jsonl_files
                    .par_iter()
                    .flat_map(|path| {
                        let file = fs::File::open(path).ok()?;
                        let reader = BufReader::with_capacity(128 * 1024, file);

                        let mut entries = Vec::new();
                        for line in reader.lines().flatten() {
                            if line.trim().is_empty() {
                                continue;
                            }

                            if let Ok(entry) = serde_json::from_str::<UsageEntry>(&line)
                                && let Some(timestamp) = &entry.timestamp
                                && timestamp.starts_with(&today)
                            {
                                entries.push(entry);
                            }
                        }
                        Some(entries)
                    })
                    .flatten()
                    .collect();

                Ok(entries)
            })
        })
        .collect();

    // Collect all entries from all paths
    let mut all_entries = Vec::new();
    for task in tasks {
        all_entries.extend(task.await??);
    }

    // Now do deduplication and calculate cost
    let mut processed_hashes = HashSet::with_capacity(all_entries.len());
    let mut total = 0.0;

    for entry in all_entries {
        if !is_duplicate_fast(&entry, &mut processed_hashes) {
            total += calculate_entry_cost(&entry);
        }
    }

    Ok(total)
}

// Active block info
struct BlockInfo {
    block_cost: f64,
    burn_rate_per_hour: Option<f64>,
    remaining_minutes: u64,
}

// Load active block using proper session blocks implementation
async fn load_active_block() -> Result<Option<BlockInfo>> {
    use ccr::session_blocks::{
        calculate_burn_rate, find_active_block, identify_session_blocks, load_all_entries,
    };

    let claude_paths = get_claude_paths();
    let now = Utc::now();

    // Load all entries
    let entries = load_all_entries(&claude_paths).await?;

    // Identify session blocks
    let blocks = identify_session_blocks(entries, &MODEL_PRICING);

    // Find active block
    if let Some(active_block) = find_active_block(&blocks) {
        let remaining = active_block.end_time.signed_duration_since(now);
        let remaining_minutes = remaining.num_minutes().max(0) as u64;

        let burn_rate = calculate_burn_rate(active_block);

        return Ok(Some(BlockInfo {
            block_cost: active_block.cost_usd,
            burn_rate_per_hour: burn_rate,
            remaining_minutes,
        }));
    }

    Ok(None)
}

// Load active block - old implementation (keeping for comparison)
async fn load_active_block_old() -> Result<Option<BlockInfo>> {
    let claude_paths = get_claude_paths();
    let now = Utc::now();
    let five_hours = chrono::Duration::hours(5);

    // Process paths in parallel
    let tasks: Vec<_> = claude_paths
        .into_iter()
        .map(|base_path| {
            task::spawn_blocking(move || -> Result<Option<BlockInfo>> {
                let projects_path = base_path.join("projects");
                if !projects_path.exists() {
                    return Ok(None);
                }

                let mut recent_entries = Vec::with_capacity(1000);
                let mut block_start_time: Option<DateTime<Utc>> = None;
                let mut latest_entry_time: Option<DateTime<Utc>> = None;
                let mut total_cost = 0.0;
                let mut processed_hashes: HashSet<u64> = HashSet::with_capacity(5000);

                // Collect all JSONL files
                let mut jsonl_files = Vec::new();
                for project_entry in fs::read_dir(&projects_path)? {
                    let project_entry = project_entry?;
                    if !project_entry.file_type()?.is_dir() {
                        continue;
                    }

                    for file_entry in fs::read_dir(project_entry.path())? {
                        let file_entry = file_entry?;
                        if file_entry.file_name().to_string_lossy().ends_with(".jsonl") {
                            jsonl_files.push(file_entry.path());
                        }
                    }
                }

                // Process all files in parallel with line-level parallelism
                use std::sync::{Arc, Mutex};

                let shared_state = Arc::new(Mutex::new((
                    block_start_time,
                    latest_entry_time,
                    total_cost,
                    recent_entries,
                    processed_hashes,
                )));

                // Process files in parallel, with each file processing lines in parallel
                let results: Vec<_> = jsonl_files
                    .par_iter()
                    .map(|path| {
                        let file = fs::File::open(path)?;
                        let reader = BufReader::with_capacity(128 * 1024, file);

                        // Collect lines for this file
                        let lines: Vec<String> = reader
                            .lines()
                            .filter_map(|line| line.ok())
                            .filter(|line| !line.trim().is_empty())
                            .collect();

                        // Process lines in parallel
                        let file_entries: Vec<_> = lines
                            .par_iter()
                            .with_min_len(5) // Smaller chunks for better work stealing
                            .filter_map(|line| {
                                if let Ok(entry) = serde_json::from_str::<UsageEntry>(line)
                                    && let Some(timestamp_str) = &entry.timestamp
                                    && let Ok(entry_time) = timestamp_str.parse::<DateTime<Utc>>()
                                {
                                    let time_since = now.signed_duration_since(entry_time);
                                    if time_since <= five_hours {
                                        return Some((entry, entry_time));
                                    }
                                }
                                None
                            })
                            .collect();

                        Ok::<Vec<_>, Box<dyn Error + Send + Sync>>(file_entries)
                    })
                    .collect();

                // Merge results from all files
                for file_entries in results.into_iter().flatten() {
                    for (entry, entry_time) in file_entries {
                        let mut state = shared_state.lock().unwrap();
                        let (block_start, latest, cost, entries, hashes) = &mut *state;

                        if !is_duplicate_fast(&entry, hashes) {
                            if block_start.is_none() || entry_time < block_start.unwrap() {
                                *block_start = Some(entry_time);
                            }
                            if latest.is_none() || entry_time > latest.unwrap() {
                                *latest = Some(entry_time);
                            }

                            *cost += calculate_entry_cost(&entry);
                            entries.push(entry);
                        }
                    }
                }

                let state = Arc::try_unwrap(shared_state)
                    .unwrap_or_else(|_| panic!("Failed to unwrap Arc"))
                    .into_inner()
                    .unwrap();

                block_start_time = state.0;
                latest_entry_time = state.1;
                total_cost = state.2;
                recent_entries = state.3;
                processed_hashes = state.4;

                if recent_entries.is_empty() || latest_entry_time.is_none() {
                    return Ok(None);
                }

                // Calculate block info based on the latest entry
                let latest_time = latest_entry_time.unwrap();
                let block_start = floor_to_hour(latest_time);
                let block_end = block_start + five_hours;
                let remaining = block_end.signed_duration_since(now);
                let remaining_minutes = remaining.num_minutes().max(0) as u64;

                let elapsed_minutes = now.signed_duration_since(block_start).num_minutes() as f64;
                let burn_rate = if elapsed_minutes > 5.0 {
                    Some((total_cost / elapsed_minutes) * 60.0)
                } else {
                    None
                };

                Ok(Some(BlockInfo {
                    block_cost: total_cost,
                    burn_rate_per_hour: burn_rate,
                    remaining_minutes,
                }))
            })
        })
        .collect();

    // Combine results from all paths
    let mut final_block: Option<BlockInfo> = None;
    for task in tasks {
        if let Some(block) = task.await?? {
            if let Some(ref mut fb) = final_block {
                fb.block_cost += block.block_cost;
                // Keep the earliest remaining time
                if block.remaining_minutes < fb.remaining_minutes {
                    fb.remaining_minutes = block.remaining_minutes;
                }
                // Recalculate burn rate if needed
                if let (Some(br1), Some(br2)) = (fb.burn_rate_per_hour, block.burn_rate_per_hour) {
                    fb.burn_rate_per_hour = Some((br1 + br2) / 2.0);
                }
            } else {
                final_block = Some(block);
            }
        }
    }

    Ok(final_block)
}

// Get git branch - optimized
async fn get_git_branch(cwd: &Path) -> Option<String> {
    let head_path = cwd.join(".git").join("HEAD");

    if let Ok(content) = async_fs::read_to_string(&head_path).await {
        let trimmed = content.trim();

        // Parse ref format
        if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
            return Some(branch.to_string());
        }

        // Detached HEAD - return short hash
        if trimmed.len() >= 7 && !trimmed.starts_with("ref:") {
            return Some(trimmed[..7].to_string());
        }
    }

    None
}

// Calculate context tokens from JSONL transcript - matching ccusage implementation
async fn calculate_context_tokens(transcript_path: &Path) -> Option<String> {
    // Try to read the file
    let content = match async_fs::read_to_string(transcript_path).await {
        Ok(content) => content,
        Err(_) => return None,
    };

    // Parse JSONL lines from last to first (most recent usage info)
    let lines: Vec<&str> = content.lines().rev().collect();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Try to parse as TranscriptMessage
        if let Ok(msg) = serde_json::from_str::<TranscriptMessage>(trimmed) {
            // Check if this is an assistant message with usage info
            if msg.message_type == "assistant"
                && let Some(message) = msg.message
                && let Some(usage) = message.usage
                && let Some(input_tokens) = usage.input_tokens
            {
                // Calculate total input tokens including cache
                let total_input = input_tokens
                    + usage.cache_creation_input_tokens.unwrap_or(0)
                    + usage.cache_read_input_tokens.unwrap_or(0);

                // Calculate percentage (capped at 100% for display)
                let max_tokens = 200_000;
                let percentage = ((total_input as usize * 100) / max_tokens).min(9999);

                let percentage_str = format!("{}%", percentage);
                let percentage_str = if percentage < 50 {
                    percentage_str.green()
                } else if percentage < 80 {
                    percentage_str.yellow()
                } else {
                    percentage_str.red()
                };

                // Format with thousands separator
                let formatted = format_number_with_commas(total_input as usize);

                return Some(format!("{} ({})", formatted, percentage_str));
            }
        }
    }

    // No valid usage information found
    None
}

#[tokio::main]
async fn main() -> Result<()> {
    // Configure rayon thread pool for optimal performance
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_cpus::get())
        .thread_name(|i| format!("ccr-worker-{}", i))
        .build_global()
        .unwrap_or_else(|e| eprintln!("Failed to configure thread pool: {}", e));

    // Force colored output even when not in a TTY
    colored::control::set_override(true);

    // Read input JSON from stdin
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    let input_json = buffer;

    let hook_data: StatuslineHookJson = serde_json::from_str(&input_json)?;

    // Check Claude paths exist
    let claude_paths = get_claude_paths();
    if claude_paths.is_empty() {
        eprintln!("{} No Claude data directory found", "❌".red());
        std::process::exit(1);
    }

    // Load all data in parallel
    let (session_data, today_cost, block_data, context_info, git_branch) = tokio::join!(
        load_session_usage_by_id(&hook_data.session_id),
        load_today_usage_data(),
        load_active_block(),
        calculate_context_tokens(Path::new(&hook_data.transcript_path)),
        get_git_branch(Path::new(&hook_data.cwd))
    );

    // Handle results
    let session_cost = session_data?.unwrap_or(0.0);
    let today_cost = today_cost?;
    let block_info = block_data?;

    // Format output
    let current_dir = Path::new(&hook_data.cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&hook_data.cwd)
        .green();

    let branch_display = if let Some(branch) = git_branch {
        format!(" {}", branch.cyan())
    } else {
        String::new()
    };

    let model_name = &hook_data.model.display_name;
    let is_opus = model_name.to_lowercase().contains("opus");
    let colored_model = if is_opus {
        model_name.white()
    } else {
        model_name.yellow().bold()
    };

    let session_display = format_currency(session_cost);

    let (block_display, burn_rate_display, remaining_display) = if let Some(block) = block_info {
        let block_str = format!("{} block", format_currency(block.block_cost));

        let burn_str = if let Some(rate) = block.burn_rate_per_hour {
            let rate_str = format!("{}/hr", format_currency(rate));
            let colored_rate = if rate < 200.0 {
                rate_str.green()
            } else if rate < 400.0 {
                rate_str.yellow()
            } else {
                rate_str.red()
            };
            format!(" 🔥 {}", colored_rate)
        } else {
            String::new()
        };

        let remaining = if block.remaining_minutes > 0 {
            format!(
                " ⏰ {}",
                format_remaining_time(block.remaining_minutes).magenta()
            )
        } else {
            String::new()
        };

        (block_str, burn_str, remaining)
    } else {
        ("No active block".to_string(), String::new(), String::new())
    };

    let context_display = if let Some(ctx) = context_info {
        format!(" ⚖️ {}", ctx)
    } else {
        String::new()
    };

    // Build and print status line
    print!("\x1b[0m"); // Reset color
    print!("{}{} 👤 {}", current_dir, branch_display, colored_model);
    print!("\x1b[0m"); // Reset after model name
    print!(
        "{} 💰 {} today, {} session, {}{}{}",
        remaining_display,
        format_currency(today_cost),
        session_display,
        block_display,
        burn_rate_display,
        context_display
    );
    println!();

    Ok(())
}

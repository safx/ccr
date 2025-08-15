use chrono::Utc;
use colored::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tokio::fs as async_fs;

// Re-use structures from lib.rs
use ccr::{ModelPricing, loader, session_blocks};
use loader::{calculate_session_cost, calculate_today_cost, load_all_data};
use session_blocks::{calculate_burn_rate, find_active_block, identify_session_blocks};

// Simple Result type alias
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

// Static model pricing data
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
        "claude-3-5-sonnet-20241022",
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            cache_creation_input_token_cost: Some(0.00000375),
            cache_read_input_token_cost: Some(0.0000003),
        },
    );

    map
});

// Input structure
#[derive(Debug, Deserialize)]
struct StatuslineHookJson {
    session_id: String,
    cwd: String,
    transcript_path: String,
    model: Model,
}

#[derive(Debug, Deserialize)]
struct Model {
    #[serde(alias = "_id")]
    #[allow(dead_code)]
    id: Option<String>,
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
    #[allow(dead_code)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
}

// Get Claude paths
fn get_claude_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(home) = env::var("HOME") {
        let home_path = PathBuf::from(home);

        // Primary path
        paths.push(home_path.join(".claude"));

        // macOS paths
        paths.push(home_path.join("Library/Application Support/Claude"));

        // Linux paths
        paths.push(home_path.join(".config/Claude"));
        paths.push(home_path.join(".local/share/Claude"));
    }

    // Windows paths
    if let Ok(appdata) = env::var("APPDATA") {
        paths.push(PathBuf::from(appdata).join("Claude"));
    }

    paths.into_iter().filter(|p| p.exists()).collect()
}

// Get git branch
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

// Format currency
fn format_currency(value: f64) -> String {
    format!("${:.2}", value)
}

// Format number with thousands separator
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
fn format_remaining_time(minutes: u64) -> String {
    if minutes == 0 {
        "Block expired".to_string()
    } else if minutes < 60 {
        format!("{}m left", minutes)
    } else {
        let hours = minutes / 60;
        let mins = minutes % 60;
        if mins > 0 {
            format!("{}h {}m left", hours, mins)
        } else {
            format!("{}h left", hours)
        }
    }
}

// Calculate context tokens from JSONL transcript
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
    let hook_data: StatuslineHookJson = serde_json::from_str(&buffer)?;

    // Check Claude paths exist
    let claude_paths = get_claude_paths();
    if claude_paths.is_empty() {
        eprintln!("{} No Claude data directory found", "❌".red());
        std::process::exit(1);
    }

    // Load usage snapshot and context info
    let (usage_snapshot, git_branch, context_info) = tokio::join!(
        load_all_data(&claude_paths, &hook_data.session_id),
        get_git_branch(Path::new(&hook_data.cwd)),
        calculate_context_tokens(Path::new(&hook_data.transcript_path))
    );

    let usage_snapshot = usage_snapshot?;

    // Calculate metrics from the snapshot
    let today_cost = calculate_today_cost(&usage_snapshot, &MODEL_PRICING);
    let session_cost =
        calculate_session_cost(&usage_snapshot, &hook_data.session_id, &MODEL_PRICING)
            .unwrap_or(0.0);

    // Calculate active block
    let blocks = identify_session_blocks(usage_snapshot.all_entries.clone(), &MODEL_PRICING);
    let active_block = find_active_block(&blocks);

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

    let (block_display, burn_rate_display, remaining_display) = if let Some(block) = active_block {
        let now = Utc::now();
        let remaining = block.end_time.signed_duration_since(now);
        let remaining_minutes = remaining.num_minutes().max(0) as u64;

        let block_str = format!("{} block", format_currency(block.cost_usd));

        let burn_str = if let Some(rate) = calculate_burn_rate(block) {
            let rate_str = format!("{}/hr", format_currency(rate));
            let colored_rate = if rate < 30.0 {
                rate_str.green()
            } else if rate < 60.0 {
                rate_str.yellow()
            } else {
                rate_str.red()
            };
            format!(" 🔥 {}", colored_rate)
        } else {
            String::new()
        };

        let remaining = format!(" ⏰ {}", format_remaining_time(remaining_minutes).magenta());

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

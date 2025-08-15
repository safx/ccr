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
use ccr::{ModelPricing, session_blocks, unified_loader};
use session_blocks::{calculate_burn_rate, find_active_block, identify_session_blocks};
use unified_loader::{calculate_session_cost, calculate_today_cost, load_all_data_unified};

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
    #[allow(dead_code)]
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

    // Load ALL data ONCE
    let (unified_data, git_branch) = tokio::join!(
        load_all_data_unified(&claude_paths, &hook_data.session_id),
        get_git_branch(Path::new(&hook_data.cwd))
    );

    let unified_data = unified_data?;

    // Calculate metrics from the unified data
    let today_cost = calculate_today_cost(&unified_data, &MODEL_PRICING);
    let session_cost =
        calculate_session_cost(&unified_data, &hook_data.session_id, &MODEL_PRICING).unwrap_or(0.0);

    // Calculate active block
    let blocks = identify_session_blocks(unified_data.all_entries.clone(), &MODEL_PRICING);
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

    // Build and print status line
    print!("\x1b[0m"); // Reset color
    print!("{}{} 👤 {}", current_dir, branch_display, colored_model);
    print!("\x1b[0m"); // Reset after model name
    print!(
        "{} 💰 {} today, {} session, {}{}",
        remaining_display,
        format_currency(today_cost),
        session_display,
        block_display,
        burn_rate_display
    );
    println!();

    Ok(())
}

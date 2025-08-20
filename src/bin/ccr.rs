use chrono::{Local, Utc};
use colored::*;
use std::error::Error;
use std::io;
use std::path::Path;

// Import from organized modules
use ccr::formatting::format_remaining_time;
use ccr::loader::load_all_data;
use ccr::session_blocks::{find_active_block, identify_session_blocks};
use ccr::types::{BurnRate, ContextTokens, Cost, StatuslineHookJson};
use ccr::utils::{get_claude_paths, get_git_branch};

// Simple Result type alias
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

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

    // Read input JSON directly from stdin using stream processing
    let hook_data: StatuslineHookJson = serde_json::from_reader(io::stdin())?;

    // Check Claude paths exist
    let claude_paths = get_claude_paths();
    if claude_paths.is_empty() {
        eprintln!("{} No Claude data directory found", "❌".red());
        std::process::exit(1);
    }

    // Load usage snapshot and context info
    let (usage_snapshot, git_branch, context_tokens) = tokio::join!(
        load_all_data(&claude_paths, &hook_data.session_id),
        get_git_branch(Path::new(&hook_data.cwd)),
        ContextTokens::from_transcript(Path::new(&hook_data.transcript_path))
    );

    let usage_snapshot = usage_snapshot?;

    // Calculate metrics from the snapshot
    let today_cost = usage_snapshot.today_cost();
    let session_cost = usage_snapshot.session_cost(&hook_data.session_id);

    // Calculate active block
    let blocks = identify_session_blocks(&usage_snapshot.all_entries);
    let (block_cost, burn_rate, remaining_minutes) = if let Some(block) = find_active_block(&blocks)
    {
        (
            block.cost(),
            BurnRate::from_session_block(block),
            block
                .end_time()
                .signed_duration_since(Local::now().with_timezone(&Utc))
                .num_minutes(),
        )
    } else {
        (Cost::new(0.0), None, 0)
    };

    // Build and print status line
    println!(
        "{reset_color}{current_dir}{branch} 👤 {model}{reset_color}{remaining} 💰 {today} today, {session} session{block}{burn_rate}{context}",
        reset_color = "\x1b[0m",
        current_dir = get_current_dir(&hook_data.cwd),
        branch = if let Some(branch) = git_branch {
            format!(" {}", branch.cyan())
        } else {
            String::new()
        },
        model = model_name(&hook_data.model.display_name),
        remaining = if remaining_minutes > 0 {
            format!(" ⏰ {}", format_remaining_time(remaining_minutes).magenta())
        } else {
            String::new()
        },
        today = today_cost,
        session = session_cost,
        block = if block_cost.is_positive() {
            format!(", {} block", block_cost)
        } else {
            String::new()
        },
        burn_rate = if let Some(rate) = burn_rate {
            format!(" 🔥 {}", rate.to_colored_string())
        } else {
            String::new()
        },
        context = if let Some(tokens) = context_tokens {
            format!(" ⚖️ {}", tokens.to_formatted_string())
        } else {
            String::new()
        },
    );

    Ok(())
}

#[inline]
fn model_name(model: &str) -> ColoredString {
    let is_opus = model.to_lowercase().contains("opus");
    if is_opus {
        model.white()
    } else {
        model.yellow().bold()
    }
}

#[inline]
fn get_current_dir(cwd: &str) -> ColoredString {
    Path::new(cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(cwd)
        .green()
}

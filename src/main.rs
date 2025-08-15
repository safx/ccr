use chrono::Utc;
use colored::*;
use std::error::Error;
use std::io::{self, Read};
use std::path::Path;

// Import from organized modules
use ccr::formatting::{format_currency, format_remaining_time};
use ccr::loader::{calculate_session_cost, calculate_today_cost, load_all_data};
use ccr::pricing::MODEL_PRICING;
use ccr::session_blocks::{calculate_burn_rate, find_active_block, identify_session_blocks};
use ccr::types::StatuslineHookJson;
use ccr::utils::{calculate_context_tokens, get_claude_paths, get_git_branch};

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

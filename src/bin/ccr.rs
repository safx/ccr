use colored::{ColoredString, Colorize};
use std::io;
use std::path::Path;

// Import from organized modules
use ccr::Result;
use ccr::error::CcrError;
use ccr::types::{BurnRate, Cost, RemainingTime, StatuslineHookJson};
use ccr::utils::{get_claude_paths, get_git_branch, load_all_data, load_transcript_usage};

#[tokio::main]
async fn main() -> Result<()> {
    // Configure rayon thread pool for optimal performance
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_cpus::get())
        .thread_name(|i| format!("ccr-worker-{}", i))
        .build_global()
        .map_err(CcrError::ThreadPoolInit)?;

    // Force colored output even when not in a TTY
    colored::control::set_override(true);

    // Read input JSON directly from stdin using stream processing
    let hook_data: StatuslineHookJson = serde_json::from_reader(io::stdin())?;

    // Check Claude paths exist
    let claude_paths = get_claude_paths();
    if claude_paths.is_empty() {
        return Err(CcrError::ClaudePathNotFound);
    }

    // Load usage snapshot and context info
    let (usage_snapshot, git_branch, transcript_usage) = tokio::join!(
        load_all_data(&claude_paths, &hook_data.session_id),
        get_git_branch(Path::new(&hook_data.cwd)),
        load_transcript_usage(Path::new(&hook_data.transcript_path))
    );

    let lines_info_str = lines_info(&hook_data);

    let context_tokens = transcript_usage
        .as_ref()
        .map(ccr::ContextTokens::from_usage);

    let usage_snapshot = usage_snapshot?;

    // Calculate metrics from the snapshot
    let today_cost = usage_snapshot.today_cost();

    // Use API cost if available, otherwise calculate from usage data
    let session_cost = hook_data
        .cost
        .as_ref()
        .map(Cost::from)
        .unwrap_or_else(|| usage_snapshot.session_cost(&hook_data.session_id));

    // Calculate active block
    let (block_cost, burn_rate, remaining_time) = if let Some(block) = usage_snapshot.active_block()
    {
        (
            block.cost(),
            BurnRate::from_session_block(&block),
            RemainingTime::from_session_block(&block),
        )
    } else {
        (Cost::new(0.0), None, RemainingTime::new(0))
    };

    // Build and print status line
    println!(
        "{reset_color}{current_dir}{branch} 👤 {model}{output_style}{reset_color}{remaining} 💰 {today} today, {session} session{block}{burn_rate}{context}{lines}",
        reset_color = "\x1b[0m",
        current_dir = get_current_dir(&hook_data.cwd),
        branch = if let Some(branch) = git_branch {
            format!(" {}", branch.cyan())
        } else {
            String::new()
        },
        model = model_name(&hook_data.model.display_name),
        output_style = if let Some(style) = hook_data.output_style
            && style.name != "default"
        {
            format!(" [{}]", style.name.yellow())
        } else {
            String::new()
        },
        remaining = if remaining_time.has_remaining() {
            format!(" ⏰ {}", remaining_time.to_colored_string())
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
        lines = lines_info_str,
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

// Format lines added/removed
fn lines_info(hook_data: &StatuslineHookJson) -> String {
    if let Some(ref cost_info) = hook_data.cost {
        let mut parts = Vec::new();
        if cost_info.total_lines_added > 0 {
            parts.push(
                format!("+{}", cost_info.total_lines_added)
                    .green()
                    .to_string(),
            );
        }
        if cost_info.total_lines_removed > 0 {
            parts.push(
                format!("-{}", cost_info.total_lines_removed)
                    .red()
                    .to_string(),
            );
        }
        if !parts.is_empty() {
            format!(" ✏️ {}", parts.join(" "))
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}

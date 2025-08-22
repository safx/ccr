use ccr::types::SessionId;
use ccr::utils::{get_claude_paths, load_all_data};
use colored::Colorize;
use std::time::Instant;

#[tokio::main]
async fn main() -> ccr::Result<()> {
    println!("{}", "=== Data Loader Profiling ===".green().bold());

    // Setup
    let claude_paths = get_claude_paths();
    if claude_paths.is_empty() {
        println!("No Claude paths found");
        return Ok(());
    }

    let session_id = SessionId::from("test-profiling-session");

    // Warm up run
    println!("\n{}", "Warming up...".yellow());
    let _ = load_all_data(&claude_paths, &session_id).await?;

    // Profile runs
    const NUM_RUNS: usize = 5;
    let mut total_times = Vec::new();

    println!(
        "\n{}",
        format!("Running {} profiling iterations...", NUM_RUNS).cyan()
    );

    for run in 1..=NUM_RUNS {
        print!("Run {}/{}: ", run, NUM_RUNS);

        let start = Instant::now();
        let snapshot = load_all_data(&claude_paths, &session_id).await?;
        let duration = start.elapsed();

        total_times.push(duration.as_millis());

        println!(
            "{} - {} entries",
            format!("{}ms", duration.as_millis()).green(),
            snapshot.all_entries.len()
        );
    }

    // Statistics
    let avg_time = total_times.iter().sum::<u128>() / NUM_RUNS as u128;
    let min_time = total_times.iter().min().unwrap();
    let max_time = total_times.iter().max().unwrap();

    println!("\n{}", "=== Results ===".green().bold());
    println!("Average: {}ms", avg_time);
    println!("Min: {}ms", min_time);
    println!("Max: {}ms", max_time);

    Ok(())
}

use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let input = r#"{"session_id": "default__1736909592942__90409ed3-b637-492f-ac55-28bb9e837e8f", "cwd": "/Users/mac/src/_mydev/ccr", "transcript_path": "/tmp/test_transcript.jsonl", "model": {"display_name": "Claude Opus 4.1"}}"#;

    let total_start = Instant::now();

    // Parse input
    let t1 = Instant::now();
    let hook_data: ccr::types::StatuslineHookJson = serde_json::from_str(input)?;
    eprintln!("1. Parse JSON: {:?}", t1.elapsed());

    // Get paths
    let t2 = Instant::now();
    let paths = ccr::utils::get_claude_paths();
    eprintln!("2. Get paths: {:?}", t2.elapsed());

    // === MAIN BOTTLENECK: load_all_data ===
    let t3 = Instant::now();
    let snapshot = ccr::loader::load_all_data(&paths, &hook_data.session_id).await?;
    eprintln!("3. Load all data: {:?}", t3.elapsed());
    eprintln!("   Entries: {}", snapshot.all_entries.len());

    // Break down load_all_data timing
    let t4 = Instant::now();

    // Test directory scanning alone
    let mut file_count = 0;
    for path in &paths {
        let projects = path.join("projects");
        if projects.exists() {
            for project in std::fs::read_dir(&projects)? {
                let project = project?;
                if project.file_type()?.is_dir() {
                    for file in std::fs::read_dir(project.path())? {
                        let file = file?;
                        if file.file_name().to_string_lossy().ends_with(".jsonl") {
                            file_count += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!(
        "   - Directory scan (separate): {:?} for {} files",
        t4.elapsed(),
        file_count
    );

    // Cost calculations
    let t5 = Instant::now();
    let _today_cost = snapshot.calculate_today_cost();
    eprintln!("4. Calculate today cost: {:?}", t5.elapsed());

    let t6 = Instant::now();
    let _session_cost = snapshot.calculate_session_cost(&hook_data.session_id);
    eprintln!("5. Calculate session cost: {:?}", t6.elapsed());

    // Identify blocks
    let t7 = Instant::now();
    let blocks = ccr::session_blocks::identify_session_blocks(&snapshot.all_entries);
    eprintln!("6. Identify blocks: {:?}", t7.elapsed());
    eprintln!("   Blocks: {}", blocks.len());

    // Git branch
    let t8 = Instant::now();
    let _branch = ccr::utils::get_git_branch(std::path::Path::new(&hook_data.cwd)).await;
    eprintln!("7. Git branch: {:?}", t8.elapsed());

    // Context tokens
    let t9 = Instant::now();
    let _context =
        ccr::ContextTokens::from_transcript(std::path::Path::new(&hook_data.transcript_path)).await;
    eprintln!("8. Context tokens: {:?}", t9.elapsed());

    eprintln!("\n=== Total time: {:?} ===", total_start.elapsed());

    // Breakdown
    let load_time = t3.elapsed();
    let total_time = total_start.elapsed();
    let load_percent = (load_time.as_millis() as f64 / total_time.as_millis() as f64) * 100.0;
    eprintln!("\nload_all_data: {:.1}% of total", load_percent);

    // Memory stats
    eprintln!("\nMemory usage estimate:");
    eprintln!(
        "  - UsageEntry size: ~{} bytes",
        std::mem::size_of::<ccr::types::UsageEntry>()
    );
    eprintln!(
        "  - Total entries memory: ~{:.1} MB",
        (snapshot.all_entries.len() * std::mem::size_of::<ccr::types::UsageEntry>()) as f64
            / 1_048_576.0
    );

    Ok(())
}

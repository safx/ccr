use std::time::Instant;

#[tokio::main]
async fn main() -> ccr::Result<()> {
    let paths = ccr::utils::get_claude_paths();

    println!("=== Deep Performance Analysis ===\n");

    // 1. File I/O breakdown
    println!("1. File I/O Analysis:");
    let t1 = Instant::now();
    let mut total_size = 0u64;
    let mut file_paths = Vec::new();

    for path in &paths {
        let projects = path.join("projects");
        if projects.exists() {
            for project in std::fs::read_dir(&projects)? {
                let project = project?;
                if project.file_type()?.is_dir() {
                    for file in std::fs::read_dir(project.path())? {
                        let file = file?;
                        if file.file_name().to_string_lossy().ends_with(".jsonl") {
                            let metadata = file.metadata()?;
                            total_size += metadata.len();
                            file_paths.push(file.path());
                        }
                    }
                }
            }
        }
    }
    println!("   Directory scan: {:?}", t1.elapsed());
    println!(
        "   Files: {}, Total: {:.1} MB",
        file_paths.len(),
        total_size as f64 / 1_048_576.0
    );

    // 2. Sequential read test
    let t2 = Instant::now();
    let mut bytes_read = 0;
    for path in file_paths.iter().take(10) {
        if let Ok(content) = std::fs::read_to_string(path) {
            bytes_read += content.len();
        }
    }
    println!(
        "   Sequential read (10 files): {:?} for {:.1} MB",
        t2.elapsed(),
        bytes_read as f64 / 1_048_576.0
    );

    // 3. Parallel read test
    use rayon::prelude::*;
    let t3 = Instant::now();
    let bytes_parallel: usize = file_paths
        .par_iter()
        .take(10)
        .map(|path| std::fs::read_to_string(path).map(|s| s.len()).unwrap_or(0))
        .sum();
    println!(
        "   Parallel read (10 files): {:?} for {:.1} MB",
        t3.elapsed(),
        bytes_parallel as f64 / 1_048_576.0
    );

    // 4. JSON parsing speed
    println!("\n2. JSON Parsing Analysis:");
    let sample_json = r#"{"timestamp":"2024-01-01T00:00:00Z","model":"test","costUSD":0.1,"message":{"id":"msg_123","usage":{"input_tokens":100,"output_tokens":50}}}"#;

    let t4 = Instant::now();
    for _ in 0..100_000 {
        let _: ccr::types::UsageEntryData = serde_json::from_str(sample_json)?;
    }
    println!("   Parse 100k entries: {:?}", t4.elapsed());

    // 5. Deduplication overhead
    println!("\n3. Deduplication Analysis:");
    let t5 = Instant::now();
    let mut hash_set = std::collections::HashSet::with_capacity(100_000);
    for i in 0..100_000 {
        hash_set.insert(format!("msg_{}:req_{}", i, i));
    }
    println!("   HashSet 100k inserts: {:?}", t5.elapsed());

    // 6. Sorting analysis
    println!("\n4. Sorting Analysis:");
    let mut test_entries = Vec::with_capacity(100_000);
    for i in 0..100_000 {
        test_entries.push(ccr::types::UsageEntry::from_data(
            ccr::types::UsageEntryData {
                timestamp: Some(format!(
                    "2024-01-01T{:02}:{:02}:{:02}Z",
                    i / 3600,
                    (i / 60) % 60,
                    i % 60
                )),
                model: None,
                cost_usd: None,
                message: None,
                request_id: None,
            },
            format!("session-{}", i / 10000).into(),
        ));
    }

    let t6 = Instant::now();
    test_entries.sort_by(|a, b| {
        a.data
            .timestamp
            .as_deref()
            .cmp(&b.data.timestamp.as_deref())
    });
    println!("   Sort 100k entries: {:?}", t6.elapsed());

    // 7. Memory allocation patterns
    println!("\n5. Memory Allocation:");
    println!("   Vec pre-allocation sizes:");
    println!("     - all_entries: 100,000");
    println!("     - today_entries: 10,000");
    println!("     - processed_hashes: 100,000");

    // 8. Actual load_all_data with timing hooks
    println!("\n6. Actual load_all_data:");
    let t7 = Instant::now();
    use ccr::types::SessionId;
    let snapshot = ccr::utils::load_all_data(&paths, &SessionId::from("test")).await?;
    println!("   Total time: {:?}", t7.elapsed());
    println!("   Entries loaded: {}", snapshot.all_entries.len());

    // Bottleneck summary
    println!("\n=== BOTTLENECK SUMMARY ===");
    let read_time_est = (total_size as f64 / bytes_read as f64) * t2.elapsed().as_secs_f64();
    println!("1. File I/O: ~{:.0}ms (estimated)", read_time_est * 1000.0);
    println!(
        "2. JSON parsing: ~{:.0}ms (for {} entries)",
        (snapshot.all_entries.len() as f64 / 100_000.0) * t4.elapsed().as_millis() as f64,
        snapshot.all_entries.len()
    );
    println!(
        "3. Sorting: ~{:.0}ms",
        (snapshot.all_entries.len() as f64 / 100_000.0) * t6.elapsed().as_millis() as f64
    );
    println!("4. Deduplication: negligible");

    Ok(())
}

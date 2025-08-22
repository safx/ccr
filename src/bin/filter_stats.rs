use chrono::{Local, Utc};
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> ccr::Result<()> {
    // Get Claude data paths
    let home = env::var("HOME").map_err(|_| ccr::CcrError::EnvVarMissing {
        var: "HOME".to_string(),
    })?;
    let claude_paths = vec![
        PathBuf::from(format!("{}/.claude", home)),
        PathBuf::from(format!("{}/Library/Application Support/Claude", home)),
    ];

    // Calculate filter boundaries (same as in loader.rs)
    let today_start = Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_local_timezone(Local)
        .unwrap()
        .with_timezone(&Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // Get current session from stdin (like ccr does)
    let current_session = "current_session_example"; // Placeholder

    println!("=== Filter Statistics Analysis ===\n");
    println!("Today starts at: {}", today_start);
    println!("Current session: {}\n", current_session);

    let mut total_entries = 0;
    let mut today_entries = 0;
    let mut session_entries = 0;
    let mut filtered_entries = 0;
    let mut entries_by_day = std::collections::HashMap::new();

    for base_path in &claude_paths {
        let projects_path = base_path.join("projects");
        if !projects_path.exists() {
            continue;
        }

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
                    let session_from_file = file_name_str.trim_end_matches(".jsonl");
                    let path = file_entry.path();

                    if let Ok(contents) = fs::read_to_string(&path) {
                        for line in contents.lines() {
                            if line.trim().is_empty() {
                                continue;
                            }

                            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                                total_entries += 1;

                                if let Some(timestamp) =
                                    entry.get("timestamp").and_then(|v| v.as_str())
                                {
                                    // Extract day from timestamp
                                    if timestamp.len() >= 10 {
                                        let day = &timestamp[..10];
                                        *entries_by_day.entry(day.to_string()).or_insert(0) += 1;
                                    }

                                    // Check filtering conditions
                                    let is_today = timestamp >= today_start.as_str();
                                    let is_current_session = session_from_file == current_session;

                                    if is_today {
                                        today_entries += 1;
                                    }
                                    if is_current_session {
                                        session_entries += 1;
                                    }
                                    if is_today || is_current_session {
                                        filtered_entries += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Sort days and show recent activity
    let mut days: Vec<_> = entries_by_day.iter().collect();
    days.sort_by(|a, b| b.0.cmp(a.0));

    println!("=== Recent Activity (by day) ===");
    for (i, (day, count)) in days.iter().enumerate() {
        if i >= 7 {
            break;
        } // Show only last 7 days
        println!("{}: {:>6} entries", day, count);
    }

    println!("\n=== Filter Impact ===");
    println!("Total entries in files:     {:>8}", total_entries);
    println!(
        "Today's entries:            {:>8} ({:.1}%)",
        today_entries,
        today_entries as f64 / total_entries as f64 * 100.0
    );
    println!(
        "Current session entries:    {:>8} ({:.1}%)",
        session_entries,
        session_entries as f64 / total_entries as f64 * 100.0
    );
    println!(
        "Entries after filtering:    {:>8} ({:.1}%)",
        filtered_entries,
        filtered_entries as f64 / total_entries as f64 * 100.0
    );
    println!(
        "Entries filtered out:       {:>8} ({:.1}%)",
        total_entries - filtered_entries,
        (total_entries - filtered_entries) as f64 / total_entries as f64 * 100.0
    );

    let reduction_percent =
        (total_entries - filtered_entries) as f64 / total_entries as f64 * 100.0;
    println!("\n✨ Memory reduction: {:.1}%", reduction_percent);

    if reduction_percent > 50.0 {
        println!("🎉 Excellent optimization! More than half of entries are filtered out.");
    } else if reduction_percent > 20.0 {
        println!("👍 Good optimization! Significant memory savings achieved.");
    } else {
        println!("📊 Modest optimization. Most entries are from today or current session.");
    }

    Ok(())
}

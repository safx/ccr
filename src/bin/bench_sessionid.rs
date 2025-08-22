use ccr::types::SessionId;
use colored::Colorize;
use std::time::Instant;

fn bench_equality_same_arc(iterations: usize) -> u128 {
    let id1 = SessionId::from("test-session-12345");
    let id2 = id1.clone(); // Same Arc

    let start = Instant::now();
    let mut count = 0;
    for _ in 0..iterations {
        // Use black_box to prevent optimization
        if std::hint::black_box(&id1) == std::hint::black_box(&id2) {
            count += 1;
        }
    }
    let duration = start.elapsed().as_nanos() / iterations as u128;
    std::hint::black_box(count);
    duration
}

fn bench_equality_different_arc_same_content(iterations: usize) -> u128 {
    let id1 = SessionId::from("test-session-12345");
    let id2 = SessionId::from("test-session-12345"); // Different Arc, same content

    let start = Instant::now();
    let mut count = 0;
    for _ in 0..iterations {
        if std::hint::black_box(&id1) == std::hint::black_box(&id2) {
            count += 1;
        }
    }
    let duration = start.elapsed().as_nanos() / iterations as u128;
    std::hint::black_box(count);
    duration
}

fn bench_equality_different_content(iterations: usize) -> u128 {
    let id1 = SessionId::from("test-session-12345");
    let id2 = SessionId::from("test-session-67890");

    let start = Instant::now();
    let mut count = 0;
    for _ in 0..iterations {
        if std::hint::black_box(&id1) != std::hint::black_box(&id2) {
            count += 1;
        }
    }
    let duration = start.elapsed().as_nanos() / iterations as u128;
    std::hint::black_box(count);
    duration
}

fn bench_string_equality(iterations: usize) -> u128 {
    let s1 = "test-session-12345".to_string();
    let s2 = "test-session-12345".to_string();

    let start = Instant::now();
    let mut count = 0;
    for _ in 0..iterations {
        if std::hint::black_box(&s1) == std::hint::black_box(&s2) {
            count += 1;
        }
    }
    let duration = start.elapsed().as_nanos() / iterations as u128;
    std::hint::black_box(count);
    duration
}

fn main() {
    println!(
        "{}",
        "=== SessionId Performance Benchmark ===".green().bold()
    );

    const ITERATIONS: usize = 10_000_000;

    // Warm up
    println!("\n{}", "Warming up...".yellow());
    for _ in 0..1000 {
        let _ = bench_equality_same_arc(100);
        let _ = bench_equality_different_arc_same_content(100);
    }

    println!(
        "\n{}",
        format!("Running {} iterations per test...", ITERATIONS).cyan()
    );

    // Benchmark same Arc (pointer equality should be super fast)
    println!("\n{}", "1. Same Arc (clone):".green());
    let mut times = Vec::new();
    for run in 1..=5 {
        let time = bench_equality_same_arc(ITERATIONS);
        times.push(time);
        println!("  Run {}: {} ns/op", run, time);
    }
    let avg_same_arc = times.iter().sum::<u128>() / times.len() as u128;

    // Benchmark different Arc, same content
    println!("\n{}", "2. Different Arc, same content:".green());
    let mut times = Vec::new();
    for run in 1..=5 {
        let time = bench_equality_different_arc_same_content(ITERATIONS);
        times.push(time);
        println!("  Run {}: {} ns/op", run, time);
    }
    let avg_diff_arc_same = times.iter().sum::<u128>() / times.len() as u128;

    // Benchmark different content
    println!("\n{}", "3. Different content:".green());
    let mut times = Vec::new();
    for run in 1..=5 {
        let time = bench_equality_different_content(ITERATIONS);
        times.push(time);
        println!("  Run {}: {} ns/op", run, time);
    }
    let avg_diff_content = times.iter().sum::<u128>() / times.len() as u128;

    // Benchmark plain String equality for comparison
    println!("\n{}", "4. Plain String equality (baseline):".green());
    let mut times = Vec::new();
    for run in 1..=5 {
        let time = bench_string_equality(ITERATIONS);
        times.push(time);
        println!("  Run {}: {} ns/op", run, time);
    }
    let avg_string = times.iter().sum::<u128>() / times.len() as u128;

    // Results
    println!("\n{}", "=== Results ===".green().bold());
    println!("Same Arc (ptr_eq fast path):     {} ns/op", avg_same_arc);
    println!(
        "Different Arc, same content:     {} ns/op",
        avg_diff_arc_same
    );
    println!(
        "Different content:                {} ns/op",
        avg_diff_content
    );
    println!("Plain String comparison:          {} ns/op", avg_string);

    // Analysis
    println!("\n{}", "=== Analysis ===".green().bold());

    let speedup = avg_string as f64 / avg_same_arc as f64;
    println!(
        "Pointer equality speedup: {:.1}x faster than string comparison",
        speedup
    );

    if avg_same_arc < avg_diff_arc_same {
        let improvement =
            ((avg_diff_arc_same - avg_same_arc) as f64 / avg_diff_arc_same as f64) * 100.0;
        println!(
            "Optimization effectiveness: {:.1}% faster for cloned SessionIds",
            improvement
        );
    }

    // Real world scenario
    println!("\n{}", "=== Real World Impact ===".cyan());
    println!("In data_loader.rs:");
    println!("• Each JSONL file creates one SessionId");
    println!("• Each entry clones that SessionId (~2-3 entries/file)");
    println!(
        "• With {} files, we have ~{} SessionId comparisons",
        318,
        318 * 3
    );

    let real_world_savings = (avg_diff_arc_same - avg_same_arc) * 318 * 3;
    println!(
        "• Estimated savings: {} ns total ({} μs)",
        real_world_savings,
        real_world_savings / 1000
    );
}

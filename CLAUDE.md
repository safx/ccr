# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ccr is a Rust-based statusline hook for Claude Code that displays real-time usage statistics, costs, and session information. The tool processes JSONL usage data from Claude Code, performs deduplication, calculates costs based on token usage, and formats the output for the statusline.

## Common Development Commands

### Building
```bash
# Development build
cargo build

# Release build with optimizations
cargo build --release

# Install to ~/bin/
cp target/release/ccr ~/bin/
```

### Testing
```bash
# Run all tests
cargo test

# Run tests with cargo-nextest (faster, better output)
cargo nextest run

# Run a specific test
cargo test test_name
cargo nextest run test_name

# Run tests in a specific module
cargo test --lib module_name
```

### Code Quality
```bash
# Run clippy linter
cargo clippy

# Check for outdated dependencies
cargo outdated

# Format code
cargo fmt

# Check formatting without applying changes
cargo fmt --check
```

### Profiling & Benchmarking
```bash
# Basic profiling
./target/release/profile

# Detailed profiling breakdown
./target/release/profile_deep

# Benchmark with hyperfine
hyperfine './target/release/ccr' -i
```

### Testing the Hook
```bash
# Test with sample input
cat test_input.json | ./target/release/ccr

# Test with actual Claude Code integration
# Add to ~/.claude/settings.json and restart Claude Code
```

## Architecture & Key Components

### Core Data Flow
1. **Input Processing** (`src/bin/ccr.rs`): Receives JSON from stdin containing session info
2. **Data Loading** (`src/loader.rs`): Parallel loading of JSONL files from Claude Code data directories
3. **Deduplication** (`src/utils/dedup.rs`): Uses message_id:request_id pairs to eliminate duplicates
4. **Session Grouping** (`src/session_blocks.rs`): Groups activity into 5-hour blocks
5. **Cost Calculation** (`src/pricing/mod.rs`): Calculates costs using model-specific pricing
6. **Output Formatting** (`src/formatting/mod.rs`): Formats the final statusline string

### Key Algorithms

**Deduplication Strategy**: The system uses a HashSet of "message_id:request_id" strings to track processed entries. This handles the fact that Claude Code may write duplicate entries when resuming sessions.

**Session Block Identification**: Activity is grouped into blocks with 5-hour gaps. A new block starts when there's more than 5 hours between consecutive entries. This algorithm originates from the ccusage implementation.

**Parallel Processing**: Uses Rayon for CPU-bound parallel processing of JSONL files and Tokio for I/O-bound operations. The loader processes multiple project directories concurrently.

### Critical Paths

**Performance-Critical Path**: 
- `loader.rs::load_all_data()` → Must process potentially thousands of JSONL files quickly
- Uses parallel file processing with Rayon
- Maintains a shared deduplication HashSet with Arc<Mutex>

**Cost Calculation Path**:
- `pricing/mod.rs::calculate_cost()` → Handles four token types with model-specific pricing
- Must handle partial data gracefully (missing token types)

### Data Structures

**UsageEntry**: Core data structure parsed from JSONL, supports both nested and flattened formats for backward compatibility.

**MergedUsageSnapshot**: Aggregated view combining all usage data, session-specific data, and today's usage.

**TokenUsage**: Tracks four token types: input, output, cache_creation, cache_read.

## Testing Philosophy

Tests focus on:
- Deduplication logic correctness
- Cost calculation accuracy with various token combinations
- Session block boundary detection
- JSON parsing for both nested and flattened formats
- Timezone handling for "today" calculations

## Important Notes

- The code uses Local timezone for "today" calculations to match user expectations
- Deduplication is critical for accuracy as Claude Code may write duplicate entries
- The 5-hour session block algorithm is based on the original ccusage implementation
- Performance is critical as this runs on every Claude Code statusline update
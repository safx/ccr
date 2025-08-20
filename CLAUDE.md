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

# Build all binaries
cargo build --release --bins
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

# Run tests in a specific module (tests are in-module)
cargo test --lib types::cost::tests
cargo test --lib types::pricing::tests
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
# Basic profiling (requires release build)
./target/release/profile

# Detailed profiling breakdown
./target/release/profile_deep

# Benchmark with hyperfine
hyperfine './target/release/ccr' -i

# Filter statistics analysis
./target/release/filter_stats
```

### Testing the Hook
```bash
# Test with sample input
cat test_input.json | ./target/release/ccr

# Test with actual stdin JSON
echo '{"session_id":"test","cwd":"/tmp","transcript_path":"/dev/null","model":{"display_name":"claude-3-5-sonnet-20241022","max_output_tokens":8192}}' | ./target/release/ccr

# Test with actual Claude Code integration
# Add to ~/.claude/settings.json and restart Claude Code
```

## Architecture & Key Components

### Core Data Flow
1. **Input Processing** (`src/bin/ccr.rs`): Receives JSON from stdin containing session info
2. **Data Loading** (`src/utils/data_loader.rs`): Parallel loading of JSONL files from Claude Code data directories
3. **Deduplication** (inline in data_loader): Uses message_id:request_id pairs (UniqueHash) to eliminate duplicates
4. **Session Grouping** (`src/types/session.rs`): Groups activity into 5-hour blocks via MergedUsageSnapshot
5. **Cost Calculation** (`src/types/pricing.rs`): Calculates costs using model-specific pricing
6. **Output Formatting** (inline in ccr.rs): Formats the final statusline string

### Key Algorithms

**Deduplication Strategy**: The system uses a HashSet of "message_id:request_id" strings to track processed entries. This handles the fact that Claude Code may write duplicate entries when resuming sessions.

**Session Block Identification**: Activity is grouped into blocks with 5-hour gaps. A new block starts when there's more than 5 hours between consecutive entries. This algorithm originates from the ccusage implementation.

**Parallel Processing**: Uses Rayon for CPU-bound parallel processing of JSONL files and Tokio for I/O-bound operations. The loader processes multiple project directories concurrently.

### Critical Paths

**Performance-Critical Path**: 
- `data_loader.rs::load_all_data()` → Must process potentially thousands of JSONL files quickly
- Uses parallel file processing with Rayon
- Maintains a shared deduplication HashSet with Arc<Mutex>

**Cost Calculation Path**:
- `pricing.rs::ModelPricing::calculate_cost()` → Handles four token types with model-specific pricing
- Must handle partial data gracefully (missing token types)

### Data Structures

**UsageEntry**: Core data structure parsed from JSONL, supports both nested and flattened formats for backward compatibility.

**MergedUsageSnapshot**: Aggregated view combining all usage data, session-specific data, and today's usage.

**TokenUsage**: Tracks four token types: input, output, cache_creation, cache_read.

**NewType Pattern**: The codebase extensively uses NewType pattern for type safety:
- `Cost`: Handles monetary values with proper formatting
- `BurnRate`: Represents hourly cost rate
- `RemainingTime`: Calculates and formats time remaining
- `ContextTokens`: Manages context token counts and percentages
- `SessionId`, `MessageId`, `RequestId`: Type-safe ID wrappers

### Binary Tools

**Main Binary** (`ccr`): The statusline hook that processes stdin JSON and outputs formatted status

**Profiling Tools**:
- `profile`: Basic performance profiling with timing breakdown
- `profile_deep`: Detailed component-level profiling

**Utility Tools**:
- `filter_stats`: Analyzes usage data with various filters

## Testing Philosophy

Tests focus on:
- Deduplication logic correctness
- Cost calculation accuracy with various token combinations
- Session block boundary detection
- JSON parsing for both nested and flattened formats
- Timezone handling for "today" calculations
- NewType conversions and formatting

Note: Tests are implemented as inline `#[cfg(test)]` modules within each source file rather than separate test files.

## Important Notes

- The code uses Local timezone for "today" calculations to match user expectations
- Deduplication is critical for accuracy as Claude Code may write duplicate entries
- The 5-hour session block algorithm is based on the original ccusage implementation
- Performance is critical as this runs on every Claude Code statusline update
- Release builds use aggressive optimizations (LTO, single codegen unit, stripped symbols)
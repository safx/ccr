# ccr - Claude Code Usage StatusLine

A statusline hook for Claude Code that displays usage costs and session information.

## Setup

Add to `~/.claude/settings.json`:

```json
{
  "statusLine": {
    "type": "command",
    "command": "$HOME/bin/ccr"
  }
}
```

## Installation

```bash
# Build release binary
cargo build --release

# Copy to your bin directory
cp target/release/ccr ~/bin/
```

## What it displays

The statusline shows:
- Current session cost
- Hourly burn rate
- Remaining time at current rate
- Context usage percentage
- Active session blocks
- Current directory 
- Git branch (when in a git repository)
- Output style (when not default)

Example output:
```
ccr main 👤 Opus 4.1 [Learning] ⏰ 1h 11m left 💰 $55.58 today, $17.98 session, $55.58 block 🔥 $15.58/hr ⚖️ 24% (37,248 / 155,000)
```

## How it works

ccr reads Claude Code usage data from `~/.config/claude_code/projects/**/*.jsonl` files and:

1. Parses JSONL entries containing API usage information
2. Deduplicates entries using message_id:request_id pairs
3. Groups activity into 5-hour session blocks
4. Calculates costs based on token usage and model pricing
5. Outputs formatted statusline string to stdout

## Input format

Claude Code sends JSON via stdin:

```json
{
  "session_id": "session-123",
  "total_tokens": 1000,
  "cached_tokens": 200,
  "cost": 0.01,
  "cwd": "/path/to/project",
  "hourly_rate": 10.0,
  "remaining_minutes": 30,
  "context_percentage": 80,
  "transcript_path": null
}
```

## Profiling tools

Two binaries are included for performance analysis:

```bash
# Basic profiling
./target/release/profile

# Detailed breakdown
./target/release/profile_deep
```

## Project structure

```
src/
├── lib.rs                      # Library exports  
├── types/                      # Data structures and domain logic
│   ├── mod.rs                  # Module exports
│   ├── ids.rs                  # ID types (SessionId, MessageId, etc.)
│   ├── input.rs                # Input data structures
│   ├── pricing.rs              # Pricing models and calculations
│   ├── session.rs              # Session blocks and snapshots
│   ├── usage.rs                # Usage entry structures
│   ├── burn_rate.rs            # Burn rate calculation (NewType)
│   ├── context_tokens.rs       # Context token handling (NewType)
│   ├── cost.rs                 # Cost calculation and formatting (NewType)
│   └── remaining_time.rs       # Remaining time calculation (NewType)
├── utils/                      # Utility functions
│   ├── mod.rs                  # Module exports
│   ├── data_loader.rs          # Parallel JSONL file loading
│   ├── transcript_loader.rs    # Transcript file parsing
│   ├── git.rs                  # Git branch detection
│   └── paths.rs                # Claude Code path discovery
└── bin/
    ├── ccr.rs                  # Main statusline hook
    ├── filter_stats.rs         # Statistics filtering tool
    ├── profile.rs              # Performance profiling
    └── profile_deep.rs         # Detailed profiling
```

## Building from source

```bash
# Development
cargo build

# Release (with optimizations)
cargo build --release

# Run tests
cargo test

# Run with cargo-nextest (if installed)
cargo nextest run
```

## Testing

```bash
# Run all tests
cargo test

# Test with sample input
echo '{"session_id":"test","cwd":"/tmp","transcript_path":"/dev/null","model":{"display_name":"claude-3-5-sonnet-20241022","max_output_tokens":8192}}' | ./target/release/ccr
```

## Development History

1. Started with [ccusage](https://github.com/ryoppippi/ccusage) by ryoppippi
2. Converted ccusage to `other_langage/ccr_deno.ts` (standalone Deno TypeScript version)
3. Developed this Rust version based on ccr_deno.ts
4. Refactored to use NewType pattern and clean architecture principles

The core algorithms - session block identification, cost calculation, and deduplication logic - originate from the ccusage implementation.

## Acknowledgments

This implementation is heavily based on [ccusage](https://github.com/ryoppippi/ccusage) by ryoppippi. I'm grateful for the well-designed original implementation and for making it open source. The clear architecture and algorithms in ccusage made this Rust port possible. Thank you!
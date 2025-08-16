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

Example output:
```
ccr main 👤 Opus 4.1 ⏰ 1h 11m left 💰 $55.58 today, $17.98 session, $55.58 block 🔥 $15.58/hr ⚖️ 87,913 (43%)
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
├── lib.rs                 # Library exports
├── loader.rs              # Parallel file loading
├── session_blocks.rs      # 5-hour block grouping
├── pricing/               # Cost calculation
│   └── mod.rs
├── formatting/            # Display formatting
│   └── mod.rs
├── types/                 # Data structures
│   ├── mod.rs
│   ├── hook.rs
│   ├── pricing.rs
│   ├── session.rs
│   └── usage.rs
├── utils/                 # Utility functions
│   ├── mod.rs
│   ├── dedup.rs          # Entry deduplication
│   ├── context.rs        # Context percentage calculation
│   ├── git.rs            # Git branch detection
│   └── paths.rs          # Claude Code path discovery
└── bin/
    ├── ccr.rs            # Main statusline hook
    ├── profile.rs        # Performance profiling
    └── profile_deep.rs   # Detailed profiling
```

## Building from source

```bash
# Development
cargo build

# Release (with optimizations)
cargo build --release
```

## Testing

```bash
cargo test
```

## Development History

1. Started with [ccusage](https://github.com/ryoppippi/ccusage) by ryoppippi
2. Converted ccusage to `other_langage/ccr_deno.ts` (standalone Deno TypeScript version)
3. Developed this Rust version based on ccr_deno.ts

The core algorithms - session block identification, cost calculation, and deduplication logic - originate from the ccusage implementation.

## Acknowledgments

This implementation is heavily based on [ccusage](https://github.com/ryoppippi/ccusage) by ryoppippi. I'm grateful for the well-designed original implementation and for making it open source. The clear architecture and algorithms in ccusage made this Rust port possible. Thank you!
use anyhow::{Context, Result};
use chrono::{DateTime, Utc, Timelike};
use clap::Parser;
use colored::*;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use tokio::fs as async_fs;

// Re-use structures from lib.rs
use ccr::{ModelPricing, TokenUsage, UsageEntry, calculate_cost, is_duplicate};

// CLI arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to JSON input (reads from stdin if not provided)
    #[arg(short, long)]
    input: Option<PathBuf>,
}

// Hook input schema
#[derive(Debug, Deserialize)]
struct StatuslineHookJson {
    session_id: String,
    transcript_path: String,
    cwd: String,
    model: ModelInfo,
    workspace: Workspace,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    id: String,
    display_name: String,
}

#[derive(Debug, Deserialize)]
struct Workspace {
    current_dir: String,
    project_dir: String,
}

// Model pricing data
fn get_model_pricing_map() -> HashMap<String, ModelPricing> {
    let mut map = HashMap::new();
    
    map.insert("claude-opus-4-1-20250805".to_string(), ModelPricing {
        input_cost_per_token: Some(0.000015),
        output_cost_per_token: Some(0.000075),
        cache_creation_input_token_cost: Some(0.00001875),
        cache_read_input_token_cost: Some(0.0000015),
    });
    
    map.insert("claude-sonnet-4-20250514".to_string(), ModelPricing {
        input_cost_per_token: Some(0.000003),
        output_cost_per_token: Some(0.000015),
        cache_creation_input_token_cost: Some(0.00000375),
        cache_read_input_token_cost: Some(0.0000003),
    });
    
    map.insert("claude-3-opus-20240229".to_string(), ModelPricing {
        input_cost_per_token: Some(0.000015),
        output_cost_per_token: Some(0.000075),
        cache_creation_input_token_cost: Some(0.00001875),
        cache_read_input_token_cost: Some(0.0000015),
    });
    
    map.insert("claude-3.5-sonnet-20241022".to_string(), ModelPricing {
        input_cost_per_token: Some(0.000003),
        output_cost_per_token: Some(0.000015),
        cache_creation_input_token_cost: Some(0.00000375),
        cache_read_input_token_cost: Some(0.0000003),
    });
    
    map
}

fn get_model_pricing(model_name: &str) -> Option<ModelPricing> {
    let pricing_map = get_model_pricing_map();
    
    // Direct match
    if let Some(pricing) = pricing_map.get(model_name) {
        return Some(pricing.clone());
    }
    
    // Partial match
    for (key, pricing) in &pricing_map {
        if model_name.contains(key) || key.contains(model_name) {
            return Some(pricing.clone());
        }
    }
    
    // Default based on model type
    if model_name.to_lowercase().contains("opus") {
        return pricing_map.get("claude-opus-4-1-20250805").cloned();
    }
    if model_name.to_lowercase().contains("sonnet") {
        return pricing_map.get("claude-sonnet-4-20250514").cloned();
    }
    
    None
}

// Get Claude configuration directories
fn get_claude_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let home = home::home_dir().unwrap_or_default();
    
    // Check environment variable first
    if let Ok(custom_path) = env::var("CLAUDE_CONFIG_DIR") {
        for path in custom_path.split(',') {
            let path = path.trim();
            if !path.is_empty() {
                paths.push(PathBuf::from(path));
            }
        }
    } else {
        // Default paths
        paths.push(home.join(".config").join("claude"));
        paths.push(home.join(".claude"));
    }
    
    // Filter for existing directories
    paths.into_iter().filter(|p| p.exists() && p.is_dir()).collect()
}

// Format currency
fn format_currency(amount: f64) -> String {
    format!("${:.2}", amount)
}

// Format remaining time
fn format_remaining_time(remaining_minutes: i64) -> String {
    if remaining_minutes <= 0 {
        return "0m left".to_string();
    }
    
    let hours = remaining_minutes / 60;
    let mins = remaining_minutes % 60;
    
    if hours > 0 {
        format!("{}h {}m left", hours, mins)
    } else {
        format!("{}m left", mins)
    }
}

// Floor timestamp to hour (for block calculation)
fn floor_to_hour(dt: DateTime<Utc>) -> DateTime<Utc> {
    dt.date_naive()
        .and_hms_opt(dt.hour(), 0, 0)
        .unwrap()
        .and_utc()
}

// Calculate cost from entry
fn calculate_entry_cost(entry: &UsageEntry, _pricing_map: &HashMap<String, ModelPricing>) -> f64 {
    // Check for pre-calculated cost
    if let Some(cost) = entry.cost_usd {
        return cost;
    }
    
    // Calculate from usage data
    if let Some(message) = &entry.message {
        if let Some(usage) = &message.usage {
            let model_name = message.model.as_ref()
                .or(entry.model.as_ref());
            
            if let Some(model_name) = model_name {
                if let Some(pricing) = get_model_pricing(model_name) {
                    let tokens = TokenUsage {
                        input: usage.input_tokens,
                        output: usage.output_tokens,
                        cache_creation: usage.cache_creation_input_tokens,
                        cache_read: usage.cache_read_input_tokens,
                    };
                    return calculate_cost(&tokens, &pricing);
                }
            }
        }
    }
    
    0.0
}

// Load session usage by ID
async fn load_session_usage_by_id(session_id: &str) -> Result<Option<f64>> {
    let claude_paths = get_claude_paths();
    let mut total_cost = 0.0;
    let mut found = false;
    let mut processed_hashes = HashSet::new();
    let pricing_map = get_model_pricing_map();
    
    for base_path in claude_paths {
        let projects_path = base_path.join("projects");
        if !projects_path.exists() {
            continue;
        }
        
        let mut dir = async_fs::read_dir(&projects_path).await?;
        while let Some(entry) = dir.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            
            let session_file = entry.path().join(format!("{}.jsonl", session_id));
            if session_file.exists() {
                let content = async_fs::read_to_string(&session_file).await?;
                for line in content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    
                    if let Ok(entry) = serde_json::from_str::<UsageEntry>(line) {
                        if !is_duplicate(&entry, &mut processed_hashes) {
                            let cost = calculate_entry_cost(&entry, &pricing_map);
                            if cost > 0.0 {
                                total_cost += cost;
                                found = true;
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(if found { Some(total_cost) } else { None })
}

// Load today's usage data
async fn load_today_usage_data() -> Result<f64> {
    let claude_paths = get_claude_paths();
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let mut total_cost = 0.0;
    let mut processed_hashes = HashSet::new();
    let pricing_map = get_model_pricing_map();
    
    for base_path in claude_paths {
        let projects_path = base_path.join("projects");
        if !projects_path.exists() {
            continue;
        }
        
        let mut dir = async_fs::read_dir(&projects_path).await?;
        while let Some(entry) = dir.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            
            let mut session_dir = async_fs::read_dir(entry.path()).await?;
            while let Some(file) = session_dir.next_entry().await? {
                let file_name = file.file_name();
                let file_name_str = file_name.to_string_lossy();
                
                if !file_name_str.ends_with(".jsonl") {
                    continue;
                }
                
                let content = async_fs::read_to_string(file.path()).await?;
                for line in content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    
                    if let Ok(entry) = serde_json::from_str::<UsageEntry>(line) {
                        if let Some(timestamp) = &entry.timestamp {
                            if timestamp.starts_with(&today) {
                                if !is_duplicate(&entry, &mut processed_hashes) {
                                    let cost = calculate_entry_cost(&entry, &pricing_map);
                                    total_cost += cost;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(total_cost)
}

// Active block info
struct BlockInfo {
    block_cost: f64,
    burn_rate_per_hour: Option<f64>,
    remaining_minutes: i64,
}

// Load active block (5-hour window)
async fn load_active_block() -> Result<Option<BlockInfo>> {
    let claude_paths = get_claude_paths();
    let now = Utc::now();
    let five_hours = chrono::Duration::hours(5);
    
    let mut recent_entries = Vec::new();
    let mut block_start_time: Option<DateTime<Utc>> = None;
    let mut total_cost = 0.0;
    let mut processed_hashes = HashSet::new();
    let pricing_map = get_model_pricing_map();
    
    for base_path in claude_paths {
        let projects_path = base_path.join("projects");
        if !projects_path.exists() {
            continue;
        }
        
        let mut dir = async_fs::read_dir(&projects_path).await?;
        while let Some(entry) = dir.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            
            let mut session_dir = async_fs::read_dir(entry.path()).await?;
            while let Some(file) = session_dir.next_entry().await? {
                let file_name = file.file_name();
                let file_name_str = file_name.to_string_lossy();
                
                if !file_name_str.ends_with(".jsonl") {
                    continue;
                }
                
                let content = async_fs::read_to_string(file.path()).await?;
                for line in content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    
                    if let Ok(entry) = serde_json::from_str::<UsageEntry>(line) {
                        if let Some(timestamp_str) = &entry.timestamp {
                            if let Ok(entry_time) = timestamp_str.parse::<DateTime<Utc>>() {
                                let time_since = now.signed_duration_since(entry_time);
                                
                                if time_since <= five_hours {
                                    if !is_duplicate(&entry, &mut processed_hashes) {
                                        recent_entries.push(entry.clone());
                                        
                                        if block_start_time.is_none() || entry_time < block_start_time.unwrap() {
                                            block_start_time = Some(entry_time);
                                        }
                                        
                                        let cost = calculate_entry_cost(&entry, &pricing_map);
                                        total_cost += cost;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    if recent_entries.is_empty() || block_start_time.is_none() {
        return Ok(None);
    }
    
    // Floor block start time to hour
    let block_start = floor_to_hour(block_start_time.unwrap());
    let block_end = block_start + five_hours;
    let remaining = block_end.signed_duration_since(now);
    let remaining_minutes = remaining.num_minutes();
    
    // Calculate burn rate
    let elapsed_minutes = now.signed_duration_since(block_start).num_minutes() as f64;
    let burn_rate = if elapsed_minutes > 5.0 {
        Some((total_cost / elapsed_minutes) * 60.0)
    } else {
        None
    };
    
    Ok(Some(BlockInfo {
        block_cost: total_cost,
        burn_rate_per_hour: burn_rate,
        remaining_minutes,
    }))
}

// Get git branch
async fn get_git_branch(cwd: &Path) -> Option<String> {
    let head_path = cwd.join(".git").join("HEAD");
    
    if let Ok(content) = async_fs::read_to_string(&head_path).await {
        let trimmed = content.trim();
        
        // Parse ref format
        if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
            return Some(branch.to_string());
        }
        
        // Detached HEAD - return short hash
        if trimmed.len() >= 7 && !trimmed.starts_with("ref:") {
            return Some(trimmed[..7].to_string());
        }
    }
    
    None
}

// Calculate context tokens
async fn calculate_context_tokens(transcript_path: &Path) -> Option<String> {
    if let Ok(content) = async_fs::read_to_string(transcript_path).await {
        // Approximate: 1 token ≈ 4 characters
        let estimated_tokens = content.len() / 4;
        let max_tokens = 200_000;
        let percentage = (estimated_tokens * 100) / max_tokens;
        
        let percentage_str = if percentage < 50 {
            format!("{}%", percentage).green()
        } else if percentage < 80 {
            format!("{}%", percentage).yellow()
        } else {
            format!("{}%", percentage).red()
        };
        
        Some(format!("{} ({})", estimated_tokens.to_formatted_string(&Locale::en()), percentage_str))
    } else {
        None
    }
}

// Locale formatting helper
struct Locale;

impl Locale {
    const fn en() -> Self { Locale }
}

trait LocaleFormat {
    fn to_formatted_string(&self, locale: &Locale) -> String;
}

impl LocaleFormat for usize {
    fn to_formatted_string(&self, _locale: &Locale) -> String {
        let s = self.to_string();
        let mut result = String::new();
        let mut count = 0;
        
        for c in s.chars().rev() {
            if count == 3 {
                result.push(',');
                count = 0;
            }
            result.push(c);
            count += 1;
        }
        
        result.chars().rev().collect()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Read input JSON
    let input_json = if let Some(input_path) = args.input {
        fs::read_to_string(input_path)?
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        buffer
    };
    
    let hook_data: StatuslineHookJson = serde_json::from_str(&input_json)
        .context("Failed to parse input JSON")?;
    
    // Check Claude paths exist
    let claude_paths = get_claude_paths();
    if claude_paths.is_empty() {
        eprintln!("{} No Claude data directory found", "❌".red());
        std::process::exit(1);
    }
    
    // Load all data in parallel
    let (session_data, today_cost, block_data, context_info, git_branch) = tokio::join!(
        load_session_usage_by_id(&hook_data.session_id),
        load_today_usage_data(),
        load_active_block(),
        calculate_context_tokens(Path::new(&hook_data.transcript_path)),
        get_git_branch(Path::new(&hook_data.cwd))
    );
    
    // Handle results
    let session_cost = session_data?.unwrap_or(0.0);
    let today_cost = today_cost?;
    let block_info = block_data?;
    
    // Format output
    let current_dir = Path::new(&hook_data.cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&hook_data.cwd)
        .green();
    
    let branch_display = if let Some(branch) = git_branch {
        format!(" {}", branch.cyan())
    } else {
        String::new()
    };
    
    let model_name = &hook_data.model.display_name;
    let is_opus = model_name.to_lowercase().contains("opus");
    let colored_model = if is_opus {
        model_name.white()
    } else {
        model_name.yellow().bold()
    };
    
    let session_display = format_currency(session_cost);
    
    let (block_display, burn_rate_display, remaining_display) = if let Some(block) = block_info {
        let block_str = format!("{} block", format_currency(block.block_cost));
        
        let burn_str = if let Some(rate) = block.burn_rate_per_hour {
            let rate_str = format!("{}/hr", format_currency(rate));
            let colored_rate = if rate < 200.0 {
                rate_str.green()
            } else if rate < 400.0 {
                rate_str.yellow()
            } else {
                rate_str.red()
            };
            format!(" 🔥 {}", colored_rate)
        } else {
            String::new()
        };
        
        let remaining = if block.remaining_minutes > 0 {
            format!(" ⏰ {} ", format_remaining_time(block.remaining_minutes).magenta())
        } else {
            String::new()
        };
        
        (block_str, burn_str, remaining)
    } else {
        ("No active block".to_string(), String::new(), String::new())
    };
    
    let context_display = if let Some(ctx) = context_info {
        format!(" ⚖️ {}", ctx)
    } else {
        String::new()
    };
    
    // Build and print status line
    print!("\x1b[0m"); // Reset color
    print!("{}{} 👤 {}", current_dir, branch_display, colored_model);
    print!("\x1b[0m"); // Reset after model name
    print!("{}💰 {} today, {} session, {}{}{}", 
        remaining_display,
        format_currency(today_cost), 
        session_display,
        block_display,
        burn_rate_display,
        context_display
    );
    println!();
    
    Ok(())
}
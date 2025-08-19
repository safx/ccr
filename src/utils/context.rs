use crate::formatting::format_number_with_commas;
use crate::types::TranscriptMessage;
use colored::*;
use std::path::Path;
use tokio::fs as async_fs;

// Calculate context tokens from JSONL transcript
pub async fn calculate_context_tokens(transcript_path: &Path) -> Option<String> {
    // Try to read the file
    let Ok(content) = async_fs::read_to_string(transcript_path).await else {
        return None;
    };

    // Parse JSONL lines from last to first (most recent usage info)
    let lines: Vec<&str> = content.lines().rev().collect();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Try to parse as TranscriptMessage
        if let Ok(msg) = serde_json::from_str::<TranscriptMessage>(trimmed) {
            // Check if this is an assistant message with usage info
            if msg.message_type == "assistant"
                && let Some(message) = msg.message
                && let Some(usage) = message.usage
                && let Some(input_tokens) = usage.input_tokens
            {
                // Calculate total input tokens including cache
                let total_input = input_tokens
                    + usage.cache_creation_input_tokens.unwrap_or(0)
                    + usage.cache_read_input_tokens.unwrap_or(0);

                // Calculate percentage (capped at 100% for display)
                let max_tokens = 200_000;
                let percentage = ((total_input as usize * 100) / max_tokens).min(9999);

                let percentage_str = format!("{}%", percentage);
                let percentage_str = if percentage < 50 {
                    percentage_str.green()
                } else if percentage < 80 {
                    percentage_str.yellow()
                } else {
                    percentage_str.red()
                };

                // Format with thousands separator
                let formatted = format_number_with_commas(total_input as usize);

                return Some(format!("{} ({})", formatted, percentage_str));
            }
        }
    }

    // No valid usage information found
    None
}

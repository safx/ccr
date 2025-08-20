use crate::types::{TranscriptMessage, TranscriptUsage};
use std::path::Path;
use tokio::fs as async_fs;

/// Load the latest transcript usage from a transcript file
/// This function handles the I/O and parsing, returning just the usage data
pub async fn load_transcript_usage(transcript_path: &Path) -> Option<TranscriptUsage> {
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
                && usage.input_tokens.is_some()
            {
                return Some(usage);
            }
        }
    }

    // No valid usage information found
    None
}

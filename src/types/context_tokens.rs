use crate::types::TranscriptMessage;
use colored::*;
use std::env;
use std::fmt;
use std::path::Path;
use tokio::fs as async_fs;

/// Represents the context token usage for a session
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ContextTokens(u64);

impl ContextTokens {
    /// Create from raw token count
    pub fn new(tokens: u64) -> Self {
        ContextTokens(tokens)
    }

    /// Load context tokens from transcript file
    pub async fn from_transcript(transcript_path: &Path) -> Option<Self> {
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

                    return Some(ContextTokens(total_input));
                }
            }
        }

        // No valid usage information found
        None
    }

    /// Get raw token count
    pub fn value(&self) -> u64 {
        self.0
    }

    /// Calculate usage percentage and actual max tokens
    pub fn calculate_percentage(&self) -> (usize, usize) {
        let max_output_tokens = env::var("CLAUDE_CODE_MAX_OUTPUT_TOKENS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(32_000);

        let max_tokens = 200_000usize;
        let auto_compact_margin = 13_000usize;
        let actual_max_tokens = max_tokens
            .saturating_sub(max_output_tokens)
            .saturating_sub(auto_compact_margin);

        let percentage = if actual_max_tokens > 0 {
            ((self.0 as usize * 100) / actual_max_tokens).min(9999)
        } else {
            0
        };

        (percentage, actual_max_tokens)
    }

    /// Get formatted string with color coding for terminal output
    pub fn to_formatted_string(&self) -> String {
        let (percentage, actual_max_tokens) = self.calculate_percentage();
        let warning_margin = 20_000usize;
        let warning_threshold = actual_max_tokens.saturating_sub(warning_margin);

        let percentage_str = format!("{}%", percentage);
        let percentage_str = if percentage < 70 {
            percentage_str.green()
        } else if self.0 as usize <= warning_threshold {
            percentage_str.yellow()
        } else {
            percentage_str.red()
        };

        let formatted_total = Self::format_number(self.0 as usize);
        let formatted_max = Self::format_number(actual_max_tokens);

        format!(
            "{} ({} / {})",
            percentage_str, formatted_total, formatted_max
        )
    }

    /// Format a number with thousands separator (private helper)
    fn format_number(n: usize) -> String {
        let s = n.to_string();
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

impl fmt::Display for ContextTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} tokens", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_tokens_new() {
        let tokens = ContextTokens::new(1500);
        assert_eq!(tokens.value(), 1500);
    }

    #[test]
    fn test_context_tokens_display() {
        let tokens = ContextTokens::new(150000);
        assert_eq!(format!("{}", tokens), "150000 tokens");
    }

    #[test]
    fn test_context_tokens_percentage() {
        // This test depends on environment variables, so we just verify it doesn't panic
        let tokens = ContextTokens::new(50000);
        let (percentage, actual_max) = tokens.calculate_percentage();
        assert!(percentage <= 9999);
        assert!(actual_max > 0);
    }

    #[test]
    fn test_context_tokens_formatted_string() {
        let tokens = ContextTokens::new(50000);
        let formatted = tokens.to_formatted_string();
        // Check that the formatted string contains expected components
        assert!(formatted.contains("50,000"));
        assert!(formatted.contains("%"));
        assert!(formatted.contains("/"));
    }
}

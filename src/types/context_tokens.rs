use crate::types::{ContextWindow, TranscriptUsage};
use colored::Colorize;
use std::env;
use std::fmt;

/// Represents the context token usage for a session
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ContextTokens(u64);

impl ContextTokens {
    /// Create from raw token count
    pub fn new(tokens: u64) -> Self {
        ContextTokens(tokens)
    }

    /// Create from transcript usage data
    pub fn from_usage(usage: &TranscriptUsage) -> Self {
        // Calculate total input tokens including cache
        let total_input = usage.input_tokens.unwrap_or(0)
            + usage.cache_creation_input_tokens.unwrap_or(0)
            + usage.cache_read_input_tokens.unwrap_or(0);

        ContextTokens(total_input)
    }

    /// Create from API-provided context_window data
    pub fn from_context_window(ctx: &ContextWindow) -> Self {
        ContextTokens(ctx.total_input_tokens)
    }

    /// Calculate usage percentage and actual max tokens
    fn calculate_percentage(&self) -> (usize, usize) {
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

    /// Get formatted string using API-provided percentage and context window size
    pub fn to_formatted_string_with_api(
        &self,
        used_percentage: u8,
        context_window_size: u64,
    ) -> String {
        let percentage_str = format!("{}%", used_percentage);
        let percentage_str = if used_percentage < 70 {
            percentage_str.green()
        } else if used_percentage < 90 {
            percentage_str.yellow()
        } else {
            percentage_str.red()
        };

        let formatted_total = Self::format_number(self.0 as usize);
        let formatted_max = Self::format_number(context_window_size as usize);

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

    #[test]
    fn test_from_context_window() {
        let ctx = ContextWindow {
            total_input_tokens: 110530,
            total_output_tokens: 136335,
            context_window_size: 200000,
            current_usage: None,
            used_percentage: Some(55),
            remaining_percentage: Some(45),
        };
        let tokens = ContextTokens::from_context_window(&ctx);
        assert_eq!(format!("{}", tokens), "110530 tokens");
    }

    #[test]
    fn test_formatted_string_with_api_green() {
        let tokens = ContextTokens::new(100000);
        let formatted = tokens.to_formatted_string_with_api(50, 200000);
        // Should contain the API percentage (50%)
        assert!(formatted.contains("50%"));
        assert!(formatted.contains("100,000"));
        assert!(formatted.contains("200,000"));
    }

    #[test]
    fn test_formatted_string_with_api_yellow() {
        let tokens = ContextTokens::new(150000);
        let formatted = tokens.to_formatted_string_with_api(75, 200000);
        // Should contain the API percentage (75%)
        assert!(formatted.contains("75%"));
        assert!(formatted.contains("150,000"));
    }

    #[test]
    fn test_formatted_string_with_api_red() {
        let tokens = ContextTokens::new(180000);
        let formatted = tokens.to_formatted_string_with_api(95, 200000);
        // Should contain the API percentage (95%)
        assert!(formatted.contains("95%"));
        assert!(formatted.contains("180,000"));
    }
}

use super::ids::{ModelId, SessionId};
use serde::Deserialize;

// Input structure
#[derive(Debug, Deserialize)]
pub struct StatuslineHookJson {
    pub session_id: SessionId,
    pub cwd: String,
    pub transcript_path: String,
    pub model: Model,
    #[serde(default)]
    pub workspace: Option<Workspace>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub output_style: Option<OutputStyle>,
    #[serde(default)]
    pub cost: Option<SessionCost>,
    #[serde(default)]
    pub context_window: Option<ContextWindow>,
}

#[derive(Debug, Deserialize)]
pub struct Model {
    #[allow(dead_code)]
    pub id: Option<ModelId>,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct Workspace {
    pub current_dir: String,
    pub project_dir: String,
}

#[derive(Debug, Deserialize)]
pub struct OutputStyle {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionCost {
    pub total_cost_usd: f64,
    pub total_duration_ms: u64,
    pub total_api_duration_ms: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
}

/// Context window information from Claude Code API
#[derive(Debug, Deserialize)]
pub struct ContextWindow {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub context_window_size: u64,
    #[serde(default)]
    pub current_usage: Option<CurrentUsage>,
    #[serde(default)]
    pub used_percentage: Option<u8>,
    #[serde(default)]
    pub remaining_percentage: Option<u8>,
}

/// Current usage breakdown within the context window
#[derive(Debug, Deserialize)]
pub struct CurrentUsage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
}

// Transcript message structure for parsing JSONL
#[derive(Debug, Deserialize)]
pub struct TranscriptMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    #[serde(default)]
    pub message: Option<TranscriptMessageContent>,
}

#[derive(Debug, Deserialize)]
pub struct TranscriptMessageContent {
    #[serde(default)]
    pub usage: Option<TranscriptUsage>,
}

#[derive(Debug, Deserialize)]
pub struct TranscriptUsage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_window_deserialization() {
        let json = r#"{
            "total_input_tokens": 110530,
            "total_output_tokens": 136335,
            "context_window_size": 200000,
            "current_usage": {
                "input_tokens": 8,
                "output_tokens": 1,
                "cache_creation_input_tokens": 3923,
                "cache_read_input_tokens": 106229
            },
            "used_percentage": 55,
            "remaining_percentage": 45
        }"#;
        let ctx: ContextWindow = serde_json::from_str(json).expect("should parse");
        assert_eq!(ctx.total_input_tokens, 110530);
        assert_eq!(ctx.total_output_tokens, 136335);
        assert_eq!(ctx.context_window_size, 200000);
        assert_eq!(ctx.used_percentage, Some(55));
        assert_eq!(ctx.remaining_percentage, Some(45));

        let usage = ctx.current_usage.expect("should have current_usage");
        assert_eq!(usage.input_tokens, Some(8));
        assert_eq!(usage.output_tokens, Some(1));
        assert_eq!(usage.cache_creation_input_tokens, Some(3923));
        assert_eq!(usage.cache_read_input_tokens, Some(106229));
    }

    #[test]
    fn test_context_window_null_fields() {
        let json = r#"{
            "total_input_tokens": 0,
            "total_output_tokens": 0,
            "context_window_size": 200000,
            "current_usage": null,
            "used_percentage": null,
            "remaining_percentage": null
        }"#;
        let ctx: ContextWindow = serde_json::from_str(json).expect("should parse");
        assert_eq!(ctx.total_input_tokens, 0);
        assert_eq!(ctx.context_window_size, 200000);
        assert!(ctx.current_usage.is_none());
        assert!(ctx.used_percentage.is_none());
        assert!(ctx.remaining_percentage.is_none());
    }

    #[test]
    fn test_context_window_missing_optional_fields() {
        // Only required fields provided
        let json = r#"{
            "total_input_tokens": 50000,
            "total_output_tokens": 25000,
            "context_window_size": 200000
        }"#;
        let ctx: ContextWindow = serde_json::from_str(json).expect("should parse");
        assert_eq!(ctx.total_input_tokens, 50000);
        assert!(ctx.current_usage.is_none());
        assert!(ctx.used_percentage.is_none());
    }

    #[test]
    fn test_statusline_hook_with_context_window() {
        let json = r#"{
            "session_id": "17a7b2dd-0021-4824-bfc0-b9598daaa407",
            "transcript_path": "/tmp/test.jsonl",
            "cwd": "/tmp",
            "model": {
                "id": "claude-sonnet-4-5-20250929",
                "display_name": "Sonnet 4.5"
            },
            "context_window": {
                "total_input_tokens": 100000,
                "total_output_tokens": 50000,
                "context_window_size": 200000,
                "used_percentage": 50,
                "remaining_percentage": 50
            }
        }"#;
        let hook: StatuslineHookJson = serde_json::from_str(json).expect("should parse");
        assert!(hook.context_window.is_some());
        let ctx = hook.context_window.expect("should have context_window");
        assert_eq!(ctx.used_percentage, Some(50));
    }
}

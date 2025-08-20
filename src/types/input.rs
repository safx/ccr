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

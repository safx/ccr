use super::ids::{MessageId, ModelId, RequestId, SessionId};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEntry {
    pub timestamp: Option<String>,
    pub model: Option<ModelId>,
    #[serde(rename = "costUSD")]
    pub cost_usd: Option<f64>,
    pub message: Option<Message>,
    #[serde(rename = "requestId")]
    pub request_id: Option<RequestId>,
    // Additional fields for session blocks
    #[serde(skip)]
    pub message_id: Option<MessageId>,
    #[serde(skip)]
    pub message_model: Option<ModelId>,
    #[serde(skip)]
    pub message_usage: Option<Usage>,
    // Session ID from the file name (always set after parsing from JSONL)
    #[serde(skip)]
    pub session_id: SessionId,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub id: Option<MessageId>,
    pub model: Option<ModelId>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cache_creation_input_tokens: Option<u32>,
    pub cache_read_input_tokens: Option<u32>,
}

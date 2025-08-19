use super::ids::{MessageId, ModelId, RequestId, SessionId};
use serde::Deserialize;

// Pure data structure deserialized from JSON
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEntryData {
    pub timestamp: Option<String>,
    pub model: Option<ModelId>,
    #[serde(rename = "costUSD")]
    pub cost_usd: Option<f64>,
    pub message: Option<Message>,
    #[serde(rename = "requestId")]
    pub request_id: Option<RequestId>,
}

// Complete usage entry with session context
#[derive(Debug, Clone)]
pub struct UsageEntry {
    pub data: UsageEntryData,
    pub session_id: SessionId,
}

impl UsageEntry {
    pub fn from_data(data: UsageEntryData, session_id: SessionId) -> Self {
        Self { data, session_id }
    }
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

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEntry {
    pub timestamp: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "costUSD")]
    pub cost_usd: Option<f64>,
    pub message: Option<Message>,
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
    // Additional fields for session blocks
    #[serde(skip)]
    pub message_id: Option<String>,
    #[serde(skip)]
    pub message_model: Option<String>,
    #[serde(skip)]
    pub message_usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub id: Option<String>,
    pub model: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cache_creation_input_tokens: Option<u32>,
    pub cache_read_input_tokens: Option<u32>,
}

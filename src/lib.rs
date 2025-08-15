// TDD: Start with failing test for ModelPricing

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub mod session_blocks;

// Green phase: Minimal implementation to pass the test
#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricing {
    pub input_cost_per_token: Option<f64>,
    pub output_cost_per_token: Option<f64>,
    pub cache_creation_input_token_cost: Option<f64>,
    pub cache_read_input_token_cost: Option<f64>,
}

// Green phase: TokenUsage struct for calculate_cost
#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cache_creation_tokens: Option<u32>,
    pub cache_read_tokens: Option<u32>,
}

// Green phase: calculate_cost function
pub fn calculate_cost(tokens: &TokenUsage, pricing: &ModelPricing) -> f64 {
    let mut cost = 0.0;

    if let (Some(input), Some(price)) = (tokens.input_tokens, pricing.input_cost_per_token) {
        cost += input as f64 * price;
    }
    if let (Some(output), Some(price)) = (tokens.output_tokens, pricing.output_cost_per_token) {
        cost += output as f64 * price;
    }
    if let (Some(cache_creation), Some(price)) = (
        tokens.cache_creation_tokens,
        pricing.cache_creation_input_token_cost,
    ) {
        cost += cache_creation as f64 * price;
    }
    if let (Some(cache_read), Some(price)) = (
        tokens.cache_read_tokens,
        pricing.cache_read_input_token_cost,
    ) {
        cost += cache_read as f64 * price;
    }

    cost
}

// Green phase: UsageEntry struct for JSON parsing
#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    pub id: Option<String>,
    pub model: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Usage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cache_creation_input_tokens: Option<u32>,
    pub cache_read_input_tokens: Option<u32>,
}

// Green phase: Duplicate detection function
pub fn is_duplicate(entry: &UsageEntry, processed_hashes: &mut HashSet<String>) -> bool {
    if let (Some(message), Some(request_id)) = (&entry.message, &entry.request_id)
        && let Some(message_id) = &message.id
    {
        let unique_hash = format!("{}:{}", message_id, request_id);
        if processed_hashes.contains(&unique_hash) {
            return true;
        }
        processed_hashes.insert(unique_hash);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_pricing_fields() {
        // Red phase: Test will fail because ModelPricing doesn't exist yet
        let pricing = ModelPricing {
            input_cost_per_token: Some(0.000015),
            output_cost_per_token: Some(0.000075),
            cache_creation_input_token_cost: Some(0.00001875),
            cache_read_input_token_cost: Some(0.0000015),
        };

        assert_eq!(pricing.input_cost_per_token, Some(0.000015));
        assert_eq!(pricing.output_cost_per_token, Some(0.000075));
    }

    #[test]
    fn test_usage_entry_json_parsing() {
        // Red phase: Test for UsageEntry JSON deserialization
        let json_str = r#"{
            "timestamp": "2024-01-15T10:30:00Z",
            "model": "claude-opus-4-1-20250805",
            "costUSD": 0.123,
            "message": {
                "id": "msg_123",
                "model": "claude-opus-4-1-20250805",
                "usage": {
                    "input_tokens": 1000,
                    "output_tokens": 500,
                    "cache_creation_input_tokens": 200,
                    "cache_read_input_tokens": 300
                }
            },
            "requestId": "req_456"
        }"#;

        let entry: UsageEntry = serde_json::from_str(json_str).unwrap();

        assert_eq!(entry.timestamp, Some("2024-01-15T10:30:00Z".to_string()));
        assert_eq!(entry.model, Some("claude-opus-4-1-20250805".to_string()));
        assert_eq!(entry.cost_usd, Some(0.123));

        let message = entry.message.unwrap();
        assert_eq!(message.id, Some("msg_123".to_string()));
        assert_eq!(message.model, Some("claude-opus-4-1-20250805".to_string()));

        let usage = message.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(1000));
        assert_eq!(usage.output_tokens, Some(500));
        assert_eq!(usage.cache_creation_input_tokens, Some(200));
        assert_eq!(usage.cache_read_input_tokens, Some(300));
    }

    #[test]
    fn test_calculate_cost() {
        // Red phase: Test for calculate_cost function
        let pricing = ModelPricing {
            input_cost_per_token: Some(0.000015),
            output_cost_per_token: Some(0.000075),
            cache_creation_input_token_cost: Some(0.00001875),
            cache_read_input_token_cost: Some(0.0000015),
        };

        // Test with all token types
        let tokens = TokenUsage {
            input_tokens: Some(1000),
            output_tokens: Some(500),
            cache_creation_tokens: Some(200),
            cache_read_tokens: Some(300),
        };

        let cost = calculate_cost(&tokens, &pricing);

        // Expected: (1000 * 0.000015) + (500 * 0.000075) + (200 * 0.00001875) + (300 * 0.0000015)
        // = 0.015 + 0.0375 + 0.00375 + 0.00045 = 0.0567
        assert!((cost - 0.0567).abs() < 1e-10);

        // Test with partial tokens
        let tokens_partial = TokenUsage {
            input_tokens: Some(1000),
            output_tokens: Some(500),
            cache_creation_tokens: None,
            cache_read_tokens: None,
        };

        let cost_partial = calculate_cost(&tokens_partial, &pricing);
        assert!((cost_partial - 0.0525).abs() < 1e-10);
    }

    #[test]
    fn test_duplicate_detection() {
        // Red phase: Test for duplicate detection
        use std::collections::HashSet;

        let mut processed_hashes = HashSet::new();

        // First entry
        let entry1 = UsageEntry {
            timestamp: Some("2024-01-15T10:30:00Z".to_string()),
            model: None,
            cost_usd: Some(0.1),
            message: Some(Message {
                id: Some("msg_123".to_string()),
                model: None,
                usage: None,
            }),
            request_id: Some("req_456".to_string()),
            message_id: None,
            message_model: None,
            message_usage: None,
        };

        // Same message and request IDs (duplicate)
        let entry2 = UsageEntry {
            timestamp: Some("2024-01-15T10:30:01Z".to_string()),
            model: None,
            cost_usd: Some(0.2),
            message: Some(Message {
                id: Some("msg_123".to_string()),
                model: None,
                usage: None,
            }),
            request_id: Some("req_456".to_string()),
            message_id: None,
            message_model: None,
            message_usage: None,
        };

        // Different IDs (not a duplicate)
        let entry3 = UsageEntry {
            timestamp: Some("2024-01-15T10:30:02Z".to_string()),
            model: None,
            cost_usd: Some(0.3),
            message: Some(Message {
                id: Some("msg_789".to_string()),
                model: None,
                usage: None,
            }),
            request_id: Some("req_999".to_string()),
            message_id: None,
            message_model: None,
            message_usage: None,
        };

        // Test duplicate detection
        assert!(!is_duplicate(&entry1, &mut processed_hashes));
        assert!(is_duplicate(&entry2, &mut processed_hashes));
        assert!(!is_duplicate(&entry3, &mut processed_hashes));
    }
}

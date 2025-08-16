// Module declarations
pub mod formatting;
pub mod loader;
pub mod pricing;
pub mod session_blocks;
pub mod types;
pub mod utils;

// Re-export commonly used items for backward compatibility
pub use pricing::{MODEL_PRICING, calculate_cost};
pub use types::{
    MergedUsageSnapshot, Message, ModelPricing, SessionBlock, StatuslineHookJson, TokenUsage,
    Usage, UsageEntry, UsageSnapshot,
};
pub use utils::{create_entry_hash, is_duplicate};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_model_pricing_fields() {
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
    fn test_create_entry_hash() {
        // Test basic hash creation
        let hash1 = create_entry_hash("msg_123", "req_456");
        assert_eq!(hash1, "msg_123:req_456");

        // Test with different IDs
        let hash2 = create_entry_hash("msg_789", "req_999");
        assert_eq!(hash2, "msg_789:req_999");

        // Test uniqueness
        assert_ne!(hash1, hash2);

        // Test consistency (same inputs produce same output)
        let hash3 = create_entry_hash("msg_123", "req_456");
        assert_eq!(hash1, hash3);
    }

    #[test]
    fn test_duplicate_detection() {
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

// Module declarations
pub mod constants;
pub mod types;
pub mod utils;

// Re-export commonly used items for backward compatibility
pub use types::ids::ModelId;
pub use types::{
    BurnRate, ContextTokens, Cost, MergedUsageSnapshot, Message, ModelPricing, RemainingTime,
    SessionBlock, StatuslineHookJson, TokenUsage, UniqueHash, Usage, UsageEntry, UsageEntryData,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_model_pricing_fields() {
        let pricing = ModelPricing {
            input_cost_per_token: 0.000015,
            output_cost_per_token: 0.000075,
            cache_creation_input_token_cost: 0.00001875,
            cache_read_input_token_cost: 0.0000015,
        };

        assert_eq!(pricing.input_cost_per_token, 0.000015);
        assert_eq!(pricing.output_cost_per_token, 0.000075);
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

        let data: UsageEntryData = serde_json::from_str(json_str).unwrap();
        let entry = UsageEntry::from_data(data, "test-session".into());

        assert_eq!(
            entry.data.timestamp,
            Some("2024-01-15T10:30:00Z".to_string())
        );
        assert_eq!(entry.data.model, Some(ModelId::ClaudeOpus4_1_20250805));
        assert_eq!(entry.data.cost_usd, Some(0.123));

        let message = entry.data.message.unwrap();
        assert_eq!(message.id, Some("msg_123".into()));
        assert_eq!(message.model, Some(ModelId::ClaudeOpus4_1_20250805));

        let usage = message.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(1000));
        assert_eq!(usage.output_tokens, Some(500));
        assert_eq!(usage.cache_creation_input_tokens, Some(200));
        assert_eq!(usage.cache_read_input_tokens, Some(300));
    }

    #[test]
    fn test_unique_hash() {
        use crate::types::{MessageId, RequestId, UniqueHash};

        // Test basic hash creation
        let msg_id1 = MessageId::from("msg_123");
        let req_id1 = RequestId::from("req_456");
        let hash1 = UniqueHash::from_ids(&msg_id1, &req_id1);
        assert_eq!(hash1.as_str(), "msg_123:req_456");

        // Test with different IDs
        let msg_id2 = MessageId::from("msg_789");
        let req_id2 = RequestId::from("req_999");
        let hash2 = UniqueHash::from_ids(&msg_id2, &req_id2);
        assert_eq!(hash2.as_str(), "msg_789:req_999");

        // Test uniqueness
        assert_ne!(hash1, hash2);

        // Test consistency (same inputs produce same output)
        let hash3 = UniqueHash::from_ids(&msg_id1, &req_id1);
        assert_eq!(hash1, hash3);
    }

    // Helper function for testing duplicate detection
    fn is_duplicate(entry: &UsageEntry, processed_hashes: &mut HashSet<UniqueHash>) -> bool {
        if let (Some(message), Some(request_id)) = (&entry.data.message, &entry.data.request_id)
            && let Some(message_id) = &message.id
        {
            let unique_hash = UniqueHash::from_ids(message_id, request_id);
            if processed_hashes.contains(&unique_hash) {
                return true;
            }
            processed_hashes.insert(unique_hash);
        }
        false
    }

    #[test]
    fn test_duplicate_detection() {
        use crate::types::UniqueHash;
        let mut processed_hashes: HashSet<UniqueHash> = HashSet::new();

        // First entry
        let entry1 = UsageEntry::from_data(
            UsageEntryData {
                timestamp: Some("2024-01-15T10:30:00Z".to_string()),
                model: None,
                cost_usd: Some(0.1),
                message: Some(Message {
                    id: Some("msg_123".into()),
                    model: None,
                    usage: None,
                }),
                request_id: Some("req_456".into()),
            },
            "session-1".into(),
        );

        // Same message and request IDs (duplicate)
        let entry2 = UsageEntry::from_data(
            UsageEntryData {
                timestamp: Some("2024-01-15T10:30:01Z".to_string()),
                model: None,
                cost_usd: Some(0.2),
                message: Some(Message {
                    id: Some("msg_123".into()),
                    model: None,
                    usage: None,
                }),
                request_id: Some("req_456".into()),
            },
            "session-2".into(),
        );

        // Different IDs (not a duplicate)
        let entry3 = UsageEntry::from_data(
            UsageEntryData {
                timestamp: Some("2024-01-15T10:30:02Z".to_string()),
                model: None,
                cost_usd: Some(0.3),
                message: Some(Message {
                    id: Some("msg_789".into()),
                    model: None,
                    usage: None,
                }),
                request_id: Some("req_999".into()),
            },
            "session-3".into(),
        );

        // Test duplicate detection
        assert!(!is_duplicate(&entry1, &mut processed_hashes));
        assert!(is_duplicate(&entry2, &mut processed_hashes));
        assert!(!is_duplicate(&entry3, &mut processed_hashes));
    }
}

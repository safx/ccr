use crate::types::{ModelPricing, SessionBlock, UsageEntry, input::SessionCost};
use std::fmt;

/// A newtype wrapper for cost values in USD
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Cost(f64);

impl Cost {
    /// Create a new Cost from a raw value
    #[inline]
    pub fn new(value: f64) -> Self {
        Cost(value)
    }

    /// Create a Cost from an iterator of UsageEntry references
    pub fn from_entries<'a, I>(entries: I) -> Self
    where
        I: Iterator<Item = &'a UsageEntry>,
    {
        let total = entries.map(calculate_entry_cost).sum();
        Cost(total)
    }

    /// Create a Cost from a SessionBlock
    pub fn from_session_block(block: &SessionBlock) -> Self {
        match block {
            SessionBlock::Idle { .. } => Cost(0.0),
            SessionBlock::Active { entries, .. } | SessionBlock::Completed { entries, .. } => {
                Self::from_entries(entries.iter().map(|e| e.as_ref()))
            }
        }
    }

    /// Get the raw value
    #[inline]
    pub fn value(&self) -> f64 {
        self.0
    }

    /// Format as currency string (e.g., "$1.23")
    pub fn to_formatted_string(&self) -> String {
        // Handle negative zero case
        let formatted_value = if self.0.abs() < 0.005 { 0.00 } else { self.0 };
        format!("${:.2}", formatted_value)
    }

    /// Check if the cost is positive (greater than tolerance)
    #[inline]
    pub fn is_positive(&self) -> bool {
        self.0 > 0.005
    }
}

impl fmt::Display for Cost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_formatted_string())
    }
}

impl From<f64> for Cost {
    fn from(value: f64) -> Self {
        Cost(value)
    }
}

impl From<Cost> for f64 {
    fn from(cost: Cost) -> Self {
        cost.0
    }
}

impl From<&SessionCost> for Cost {
    fn from(session_cost: &SessionCost) -> Self {
        Cost(session_cost.total_cost_usd)
    }
}

/// Helper function to calculate token cost
#[inline]
fn calculate_token_cost(tokens: Option<u32>, cost_per_token: f64) -> f64 {
    tokens.unwrap_or(0) as f64 * cost_per_token
}

/// Calculate cost for a single entry (private helper function)
fn calculate_entry_cost(entry: &UsageEntry) -> f64 {
    // First check if there's a pre-calculated cost
    if let Some(cost) = entry.data.cost_usd {
        return cost;
    }

    // Otherwise calculate from token usage
    if let Some(message) = &entry.data.message
        && let Some(usage) = &message.usage
        && let Some(model_id) = message.model.as_ref().or(entry.data.model.as_ref())
    {
        let pricing = ModelPricing::from(model_id);

        // Common cost components
        let mut cost = calculate_token_cost(usage.input_tokens, pricing.input_cost_per_token)
            + calculate_token_cost(usage.output_tokens, pricing.output_cost_per_token)
            + calculate_token_cost(
                usage.cache_read_input_tokens,
                pricing.cache_read_input_token_cost,
            );

        // Add cache creation cost based on format
        if let Some(cache_creation) = &usage.cache_creation {
            // New format: calculate 5m and 1h cache separately with different prices
            cost += calculate_token_cost(
                cache_creation.ephemeral_5m_input_tokens,
                pricing.cache_creation_input_token_cost,
            );
            cost += calculate_token_cost(
                cache_creation.ephemeral_1h_input_tokens,
                pricing.cache_creation_1h_token_cost,
            );
        } else {
            // Old format: direct calculation
            cost += calculate_token_cost(
                usage.cache_creation_input_tokens,
                pricing.cache_creation_input_token_cost,
            );
        }

        return cost;
    }

    0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelId;
    use crate::types::{
        Message, MessageId, RequestId, SessionId, Usage, UsageEntryData, usage::CacheCreation,
    };
    use std::sync::Arc;

    // Helper function to create test UsageEntry with old format
    fn create_test_entry_old_format(
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
        cache_creation_tokens: Option<u32>,
        cache_read_tokens: Option<u32>,
        model: &str,
    ) -> UsageEntry {
        UsageEntry {
            data: UsageEntryData {
                timestamp: Some("2024-01-15T10:00:00.000Z".to_string()),
                model: Some(ModelId::from(model)),
                cost_usd: None,
                message: Some(Message {
                    id: Some(MessageId::from("msg-1")),
                    model: Some(ModelId::from(model)),
                    usage: Some(Usage {
                        input_tokens,
                        output_tokens,
                        cache_creation_input_tokens: cache_creation_tokens,
                        cache_read_input_tokens: cache_read_tokens,
                        cache_creation: None,
                        service_tier: None,
                    }),
                }),
                request_id: Some(RequestId::from("req-1")),
            },
            session_id: SessionId::from("test-session"),
        }
    }

    // Helper function to create test UsageEntry with new cache_creation format
    fn create_test_entry_new_format(
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
        cache_5m_tokens: Option<u32>,
        cache_1h_tokens: Option<u32>,
        cache_read_tokens: Option<u32>,
        model: &str,
    ) -> UsageEntry {
        UsageEntry {
            data: UsageEntryData {
                timestamp: Some("2024-01-15T10:00:00.000Z".to_string()),
                model: Some(ModelId::from(model)),
                cost_usd: None,
                message: Some(Message {
                    id: Some(MessageId::from("msg-1")),
                    model: Some(ModelId::from(model)),
                    usage: Some(Usage {
                        input_tokens,
                        output_tokens,
                        cache_creation_input_tokens: None,
                        cache_read_input_tokens: cache_read_tokens,
                        cache_creation: Some(CacheCreation {
                            ephemeral_5m_input_tokens: cache_5m_tokens,
                            ephemeral_1h_input_tokens: cache_1h_tokens,
                        }),
                        service_tier: None,
                    }),
                }),
                request_id: Some(RequestId::from("req-1")),
            },
            session_id: SessionId::from("test-session"),
        }
    }

    // Helper function to create test UsageEntry with pre-calculated cost
    fn create_test_entry_with_cost(cost_usd: f64) -> UsageEntry {
        UsageEntry {
            data: UsageEntryData {
                timestamp: Some("2024-01-15T10:00:00.000Z".to_string()),
                model: Some(ModelId::from("claude-3-5-sonnet-20241022")),
                cost_usd: Some(cost_usd),
                message: None,
                request_id: Some(RequestId::from("req-1")),
            },
            session_id: SessionId::from("test-session"),
        }
    }

    #[test]
    fn test_cost_formatting() {
        assert_eq!(Cost::new(1.234).to_formatted_string(), "$1.23");
        assert_eq!(Cost::new(0.0).to_formatted_string(), "$0.00");
        assert_eq!(Cost::new(-0.0).to_formatted_string(), "$0.00");
        assert_eq!(Cost::new(0.004).to_formatted_string(), "$0.00");
        assert_eq!(Cost::new(0.005).to_formatted_string(), "$0.01");
        assert_eq!(Cost::new(100.999).to_formatted_string(), "$101.00");
    }

    #[test]
    fn test_cost_zero_checks() {
        assert!(!Cost::new(0.0).is_positive());
        assert!(!Cost::new(0.005).is_positive());
        assert!(Cost::new(0.006).is_positive());
        assert!(Cost::new(1.0).is_positive());
    }

    #[test]
    fn test_cost_display() {
        let cost = Cost::new(42.42);
        assert_eq!(format!("{}", cost), "$42.42");
    }

    #[test]
    fn test_cost_conversions() {
        let cost = Cost::from(3.14);
        assert_eq!(cost.value(), 3.14);

        let value: f64 = cost.into();
        assert_eq!(value, 3.14);
    }

    #[test]
    fn test_cost_from_session_cost() {
        let session_cost = SessionCost {
            total_cost_usd: 12.34,
            total_duration_ms: 60000,
            total_api_duration_ms: 5000,
            total_lines_added: 100,
            total_lines_removed: 50,
        };

        let cost = Cost::from(&session_cost);
        assert_eq!(cost.value(), 12.34);
        assert_eq!(cost.to_formatted_string(), "$12.34");
    }

    #[test]
    fn test_calculate_entry_cost_with_precalculated() {
        let entry = create_test_entry_with_cost(5.67);
        let cost = calculate_entry_cost(&entry);
        assert_eq!(cost, 5.67);
    }

    #[test]
    fn test_calculate_entry_cost_old_format() {
        let entry = create_test_entry_old_format(
            Some(1000), // input_tokens
            Some(500),  // output_tokens
            Some(200),  // cache_creation_tokens
            Some(300),  // cache_read_tokens
            "claude-3-5-sonnet-20241022",
        );

        let cost = calculate_entry_cost(&entry);
        // Verify cost is calculated (exact value depends on pricing)
        assert!(cost > 0.0);
    }

    #[test]
    fn test_calculate_entry_cost_new_format_with_5m_cache() {
        let entry = create_test_entry_new_format(
            Some(1000), // input_tokens
            Some(500),  // output_tokens
            Some(200),  // cache_5m_tokens
            None,       // cache_1h_tokens
            Some(300),  // cache_read_tokens
            "claude-3-5-sonnet-20241022",
        );

        let cost = calculate_entry_cost(&entry);
        // Verify cost is calculated
        assert!(cost > 0.0);
    }

    #[test]
    fn test_calculate_entry_cost_new_format_with_1h_cache() {
        let entry = create_test_entry_new_format(
            Some(1000), // input_tokens
            Some(500),  // output_tokens
            None,       // cache_5m_tokens
            Some(400),  // cache_1h_tokens
            Some(300),  // cache_read_tokens
            "claude-3-5-sonnet-20241022",
        );

        let cost = calculate_entry_cost(&entry);
        // Verify cost is calculated
        assert!(cost > 0.0);
    }

    #[test]
    fn test_calculate_entry_cost_new_format_with_both_caches() {
        let entry = create_test_entry_new_format(
            Some(1000), // input_tokens
            Some(500),  // output_tokens
            Some(200),  // cache_5m_tokens
            Some(400),  // cache_1h_tokens
            Some(300),  // cache_read_tokens
            "claude-3-5-sonnet-20241022",
        );

        let cost = calculate_entry_cost(&entry);
        // Verify cost is calculated
        assert!(cost > 0.0);

        // Compare with only 5m cache - 1h cache should be cheaper
        let entry_5m_only = create_test_entry_new_format(
            Some(1000),
            Some(500),
            Some(600), // All cache as 5m
            None,
            Some(300),
            "claude-3-5-sonnet-20241022",
        );
        let cost_5m_only = calculate_entry_cost(&entry_5m_only);

        // 1h cache write is more expensive than 5m cache write (100% vs 25% markup)
        // This is correct: longer cache duration costs more to write
        assert!(cost > cost_5m_only);
    }

    #[test]
    fn test_calculate_entry_cost_with_missing_data() {
        // Entry with no message
        let entry_no_message = UsageEntry {
            data: UsageEntryData {
                timestamp: None,
                model: None,
                cost_usd: None,
                message: None,
                request_id: None,
            },
            session_id: SessionId::from("test-session"),
        };
        assert_eq!(calculate_entry_cost(&entry_no_message), 0.0);

        // Entry with message but no usage
        let entry_no_usage = UsageEntry {
            data: UsageEntryData {
                timestamp: None,
                model: Some(ModelId::from("claude-3-5-sonnet-20241022")),
                cost_usd: None,
                message: Some(Message {
                    id: None,
                    model: Some(ModelId::from("claude-3-5-sonnet-20241022")),
                    usage: None,
                }),
                request_id: None,
            },
            session_id: SessionId::from("test-session"),
        };
        assert_eq!(calculate_entry_cost(&entry_no_usage), 0.0);

        // Entry with usage but no model
        let entry_no_model = UsageEntry {
            data: UsageEntryData {
                timestamp: None,
                model: None,
                cost_usd: None,
                message: Some(Message {
                    id: None,
                    model: None,
                    usage: Some(Usage {
                        input_tokens: Some(100),
                        output_tokens: Some(50),
                        cache_creation_input_tokens: None,
                        cache_read_input_tokens: None,
                        cache_creation: None,
                        service_tier: None,
                    }),
                }),
                request_id: None,
            },
            session_id: SessionId::from("test-session"),
        };
        assert_eq!(calculate_entry_cost(&entry_no_model), 0.0);
    }

    #[test]
    fn test_calculate_entry_cost_with_all_none_tokens() {
        let entry = UsageEntry {
            data: UsageEntryData {
                timestamp: None,
                model: Some(ModelId::from("claude-3-5-sonnet-20241022")),
                cost_usd: None,
                message: Some(Message {
                    id: None,
                    model: Some(ModelId::from("claude-3-5-sonnet-20241022")),
                    usage: Some(Usage {
                        input_tokens: None,
                        output_tokens: None,
                        cache_creation_input_tokens: None,
                        cache_read_input_tokens: None,
                        cache_creation: None,
                        service_tier: None,
                    }),
                }),
                request_id: None,
            },
            session_id: SessionId::from("test-session"),
        };

        // Should handle None values as 0
        let cost = calculate_entry_cost(&entry);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_cost_from_entries() {
        let entries = vec![
            create_test_entry_with_cost(1.0),
            create_test_entry_with_cost(2.0),
            create_test_entry_with_cost(3.0),
        ];

        let cost = Cost::from_entries(entries.iter());
        assert_eq!(cost.value(), 6.0);
    }

    #[test]
    fn test_cost_from_entries_mixed_formats() {
        let entries = vec![
            create_test_entry_with_cost(1.0),
            create_test_entry_old_format(
                Some(100),
                Some(50),
                None,
                None,
                "claude-3-5-sonnet-20241022",
            ),
            create_test_entry_new_format(
                Some(100),
                Some(50),
                Some(20),
                None,
                None,
                "claude-3-5-sonnet-20241022",
            ),
        ];

        let cost = Cost::from_entries(entries.iter());
        // Should be > 1.0 due to the pre-calculated cost plus calculated costs
        assert!(cost.value() > 1.0);
    }

    #[test]
    fn test_cost_from_session_block_idle() {
        let block = SessionBlock::Idle {
            start_time: chrono::Utc::now(),
            end_time: chrono::Utc::now() + chrono::Duration::hours(1),
        };

        let cost = Cost::from_session_block(&block);
        assert_eq!(cost.value(), 0.0);
    }

    #[test]
    fn test_cost_from_session_block_active() {
        let entries = vec![
            Arc::new(create_test_entry_with_cost(1.5)),
            Arc::new(create_test_entry_with_cost(2.5)),
        ];

        let block = SessionBlock::Active {
            start_time: chrono::Utc::now(),
            entries,
        };

        let cost = Cost::from_session_block(&block);
        assert_eq!(cost.value(), 4.0);
    }

    #[test]
    fn test_cost_from_session_block_completed() {
        let entries = vec![
            Arc::new(create_test_entry_with_cost(3.0)),
            Arc::new(create_test_entry_with_cost(4.0)),
        ];

        let block = SessionBlock::Completed {
            start_time: chrono::Utc::now() - chrono::Duration::hours(2),
            entries,
        };

        let cost = Cost::from_session_block(&block);
        assert_eq!(cost.value(), 7.0);
    }

    #[test]
    fn test_different_model_pricing() {
        // Test with different models to ensure pricing varies
        let sonnet_entry = create_test_entry_old_format(
            Some(1000),
            Some(500),
            None,
            None,
            "claude-3-5-sonnet-20241022",
        );
        let opus_entry = create_test_entry_old_format(
            Some(1000),
            Some(500),
            None,
            None,
            "claude-3-opus-20240229",
        );
        let haiku_entry = create_test_entry_old_format(
            Some(1000),
            Some(500),
            None,
            None,
            "claude-3-haiku-20240307",
        );

        let sonnet_cost = calculate_entry_cost(&sonnet_entry);
        let opus_cost = calculate_entry_cost(&opus_entry);
        let haiku_cost = calculate_entry_cost(&haiku_entry);

        // Different models should have different costs
        assert!(sonnet_cost > 0.0);
        assert!(opus_cost > 0.0);
        assert!(haiku_cost > 0.0);

        // Opus is typically most expensive, Haiku least expensive
        assert_ne!(sonnet_cost, opus_cost);
        assert_ne!(sonnet_cost, haiku_cost);
        assert_ne!(opus_cost, haiku_cost);
    }

    #[test]
    fn test_cache_creation_pricing_difference() {
        // Verify that 1h cache is cheaper than 5m cache
        let entry_5m = create_test_entry_new_format(
            None,
            None,
            Some(1000), // 5m cache
            None,
            None,
            "claude-3-5-sonnet-20241022",
        );

        let entry_1h = create_test_entry_new_format(
            None,
            None,
            None,
            Some(1000), // 1h cache
            None,
            "claude-3-5-sonnet-20241022",
        );

        let cost_5m = calculate_entry_cost(&entry_5m);
        let cost_1h = calculate_entry_cost(&entry_1h);

        // 1h cache write costs more than 5m cache write (100% vs 25% markup)
        // This is by design: pay more upfront for longer cache retention
        assert!(cost_1h > cost_5m);
    }

    #[test]
    fn test_model_fallback_from_message_to_entry() {
        // Test that model can be taken from entry.data.model if message.model is None
        let entry = UsageEntry {
            data: UsageEntryData {
                timestamp: None,
                model: Some(ModelId::from("claude-3-5-sonnet-20241022")), // Model here
                cost_usd: None,
                message: Some(Message {
                    id: None,
                    model: None, // No model in message
                    usage: Some(Usage {
                        input_tokens: Some(100),
                        output_tokens: Some(50),
                        cache_creation_input_tokens: None,
                        cache_read_input_tokens: None,
                        cache_creation: None,
                        service_tier: None,
                    }),
                }),
                request_id: None,
            },
            session_id: SessionId::from("test-session"),
        };

        let cost = calculate_entry_cost(&entry);
        assert!(cost > 0.0); // Should still calculate cost using entry.data.model
    }
}

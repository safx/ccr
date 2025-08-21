use super::ids::ModelId;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricing {
    pub input_cost_per_token: f64,
    pub output_cost_per_token: f64,
    pub cache_creation_input_token_cost: f64,  // 5m cache write
    pub cache_read_input_token_cost: f64,        // cache hits/refreshes
    pub cache_creation_1h_token_cost: f64,       // 1h cache write
}

impl From<&ModelId> for ModelPricing {
    fn from(model_id: &ModelId) -> Self {
        match model_id {
            ModelId::ClaudeOpus4_1_20250805
            | ModelId::ClaudeOpus4_20250514
            | ModelId::Claude3Opus20240229 => ModelPricing {
                input_cost_per_token: 0.000015,        // $15/MTok
                output_cost_per_token: 0.000075,       // $75/MTok
                cache_creation_input_token_cost: 0.00001875,  // $18.75/MTok (5m cache)
                cache_read_input_token_cost: 0.0000015,       // $1.50/MTok
                cache_creation_1h_token_cost: 0.00003,        // $30/MTok (1h cache)
            },
            ModelId::ClaudeSonnet4_20250514 | ModelId::Claude3_5Sonnet20241022 => ModelPricing {
                input_cost_per_token: 0.000003,         // $3/MTok
                output_cost_per_token: 0.000015,        // $15/MTok
                cache_creation_input_token_cost: 0.00000375,  // $3.75/MTok (5m cache)
                cache_read_input_token_cost: 0.0000003,       // $0.30/MTok
                cache_creation_1h_token_cost: 0.000006,       // $6/MTok (1h cache)
            },
            ModelId::Other(s) => {
                // Fallback based on model name
                if s.to_lowercase().contains("opus") {
                    ModelPricing {
                        input_cost_per_token: 0.000015,
                        output_cost_per_token: 0.000075,
                        cache_creation_input_token_cost: 0.00001875,
                        cache_read_input_token_cost: 0.0000015,
                        cache_creation_1h_token_cost: 0.00003,
                    }
                } else if s.to_lowercase().contains("sonnet") {
                    ModelPricing {
                        input_cost_per_token: 0.000003,
                        output_cost_per_token: 0.000015,
                        cache_creation_input_token_cost: 0.00000375,
                        cache_read_input_token_cost: 0.0000003,
                        cache_creation_1h_token_cost: 0.000006,
                    }
                } else if s.to_lowercase().contains("haiku") {
                    // Haiku 3.5 pricing
                    ModelPricing {
                        input_cost_per_token: 0.0000008,      // $0.80/MTok
                        output_cost_per_token: 0.000004,      // $4/MTok
                        cache_creation_input_token_cost: 0.000001,    // $1/MTok (5m cache)
                        cache_read_input_token_cost: 0.00000008,      // $0.08/MTok
                        cache_creation_1h_token_cost: 0.0000016,      // $1.6/MTok (1h cache)
                    }
                } else {
                    // Unknown model - return zero pricing
                    ModelPricing {
                        input_cost_per_token: 0.0,
                        output_cost_per_token: 0.0,
                        cache_creation_input_token_cost: 0.0,
                        cache_read_input_token_cost: 0.0,
                        cache_creation_1h_token_cost: 0.0,
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_tokens: u32,
    pub cache_read_tokens: u32,
}

impl TokenUsage {
    /// Calculate the cost for this token usage given a pricing model
    pub fn calculate_cost(&self, pricing: &ModelPricing) -> f64 {
        self.input_tokens as f64 * pricing.input_cost_per_token
            + self.output_tokens as f64 * pricing.output_cost_per_token
            + self.cache_creation_tokens as f64 * pricing.cache_creation_input_token_cost
            + self.cache_read_tokens as f64 * pricing.cache_read_input_token_cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_cost() {
        let pricing = ModelPricing {
            input_cost_per_token: 0.000015,
            output_cost_per_token: 0.000075,
            cache_creation_input_token_cost: 0.00001875,
            cache_read_input_token_cost: 0.0000015,
            cache_creation_1h_token_cost: 0.00003,
        };

        // Test with all token types
        let tokens = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 200,
            cache_read_tokens: 300,
        };

        let cost = tokens.calculate_cost(&pricing);

        // Expected: (1000 * 0.000015) + (500 * 0.000075) + (200 * 0.00001875) + (300 * 0.0000015)
        // = 0.015 + 0.0375 + 0.00375 + 0.00045 = 0.0567
        assert!((cost - 0.0567).abs() < 1e-10);

        // Test with zero tokens
        let tokens_zero = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
        };

        let cost_zero = tokens_zero.calculate_cost(&pricing);
        assert!((cost_zero - 0.0525).abs() < 1e-10);
    }
}

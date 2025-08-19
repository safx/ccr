use super::ids::ModelId;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricing {
    pub input_cost_per_token: f64,
    pub output_cost_per_token: f64,
    pub cache_creation_input_token_cost: f64,
    pub cache_read_input_token_cost: f64,
}

impl From<&ModelId> for ModelPricing {
    fn from(model_id: &ModelId) -> Self {
        match model_id {
            ModelId::ClaudeOpus4_1_20250805
            | ModelId::ClaudeOpus4_20250514
            | ModelId::Claude3Opus20240229 => ModelPricing {
                input_cost_per_token: 0.000015,
                output_cost_per_token: 0.000075,
                cache_creation_input_token_cost: 0.00001875,
                cache_read_input_token_cost: 0.0000015,
            },
            ModelId::ClaudeSonnet4_20250514 | ModelId::Claude3_5Sonnet20241022 => ModelPricing {
                input_cost_per_token: 0.000003,
                output_cost_per_token: 0.000015,
                cache_creation_input_token_cost: 0.00000375,
                cache_read_input_token_cost: 0.0000003,
            },
            ModelId::Other(s) => {
                // Fallback based on model name
                if s.to_lowercase().contains("opus") {
                    ModelPricing {
                        input_cost_per_token: 0.000015,
                        output_cost_per_token: 0.000075,
                        cache_creation_input_token_cost: 0.00001875,
                        cache_read_input_token_cost: 0.0000015,
                    }
                } else if s.to_lowercase().contains("sonnet") {
                    ModelPricing {
                        input_cost_per_token: 0.000003,
                        output_cost_per_token: 0.000015,
                        cache_creation_input_token_cost: 0.00000375,
                        cache_read_input_token_cost: 0.0000003,
                    }
                } else {
                    // Unknown model - return zero pricing
                    ModelPricing {
                        input_cost_per_token: 0.0,
                        output_cost_per_token: 0.0,
                        cache_creation_input_token_cost: 0.0,
                        cache_read_input_token_cost: 0.0,
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

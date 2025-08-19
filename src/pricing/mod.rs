use crate::UsageEntry;
use crate::types::{ModelPricing, TokenUsage};

fn calculate_cost(tokens: &TokenUsage, pricing: &ModelPricing) -> f64 {
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

/// Calculate entry cost with pricing map
pub fn calculate_entry_cost(entry: &UsageEntry) -> f64 {
    if let Some(cost) = entry.cost_usd {
        return cost;
    }

    if let Some(message) = &entry.message
        && let Some(usage) = &message.usage
        && let Some(model_id) = message.model.as_ref().or(entry.model.as_ref())
    {
        let pricing = ModelPricing::from(model_id);
        let tokens = TokenUsage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_creation_tokens: usage.cache_creation_input_tokens,
            cache_read_tokens: usage.cache_read_input_tokens,
        };
        return calculate_cost(&tokens, &pricing);
    }

    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

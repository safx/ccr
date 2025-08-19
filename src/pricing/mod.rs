use crate::UsageEntry;
use crate::types::{ModelPricing, TokenUsage};

fn calculate_cost(tokens: &TokenUsage, pricing: &ModelPricing) -> f64 {
    tokens.input_tokens as f64 * pricing.input_cost_per_token
        + tokens.output_tokens as f64 * pricing.output_cost_per_token
        + tokens.cache_creation_tokens as f64 * pricing.cache_creation_input_token_cost
        + tokens.cache_read_tokens as f64 * pricing.cache_read_input_token_cost
}

/// Calculate entry cost with pricing map
fn calculate_entry_cost(entry: &UsageEntry) -> f64 {
    if let Some(cost) = entry.data.cost_usd {
        return cost;
    }

    if let Some(message) = &entry.data.message
        && let Some(usage) = &message.usage
        && let Some(model_id) = message.model.as_ref().or(entry.data.model.as_ref())
    {
        let pricing = ModelPricing::from(model_id);
        let tokens = TokenUsage {
            input_tokens: usage.input_tokens.unwrap_or(0),
            output_tokens: usage.output_tokens.unwrap_or(0),
            cache_creation_tokens: usage.cache_creation_input_tokens.unwrap_or(0),
            cache_read_tokens: usage.cache_read_input_tokens.unwrap_or(0),
        };
        return calculate_cost(&tokens, &pricing);
    }

    0.0
}

/// Calculate total cost for an iterator of entries
pub fn calculate_entry_costs<'a, I>(entries: I) -> f64
where
    I: Iterator<Item = &'a UsageEntry>,
{
    entries.map(calculate_entry_cost).sum()
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
        };

        // Test with all token types
        let tokens = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 200,
            cache_read_tokens: 300,
        };

        let cost = calculate_cost(&tokens, &pricing);

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

        let cost_zero = calculate_cost(&tokens_zero, &pricing);
        assert!((cost_zero - 0.0525).abs() < 1e-10);
    }
}

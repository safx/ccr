use crate::UsageEntry;
use crate::types::{ModelPricing, TokenUsage};

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

/// Calculate entry cost with pricing map
pub fn calculate_entry_cost(entry: &UsageEntry) -> f64 {
    if let Some(cost) = entry.cost_usd {
        return cost;
    }

    if let Some(message) = &entry.message
        && let Some(usage) = &message.usage
    {
        let model_id = message.model.as_ref().or(entry.model.as_ref());

        if let Some(model_id) = model_id {
            // Use From trait to get pricing from ModelId
            let pricing = ModelPricing::from(model_id);
            let tokens = TokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_tokens: usage.cache_creation_input_tokens,
                cache_read_tokens: usage.cache_read_input_tokens,
            };
            return calculate_cost(&tokens, &pricing);
        }
    }

    0.0
}

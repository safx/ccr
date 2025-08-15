use crate::types::{ModelPricing, TokenUsage};
use std::collections::HashMap;
use std::sync::LazyLock;

// Static model pricing data
pub static MODEL_PRICING: LazyLock<HashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut map = HashMap::with_capacity(4);

    map.insert(
        "claude-opus-4-1-20250805",
        ModelPricing {
            input_cost_per_token: Some(0.000015),
            output_cost_per_token: Some(0.000075),
            cache_creation_input_token_cost: Some(0.00001875),
            cache_read_input_token_cost: Some(0.0000015),
        },
    );

    map.insert(
        "claude-sonnet-4-20250514",
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            cache_creation_input_token_cost: Some(0.00000375),
            cache_read_input_token_cost: Some(0.0000003),
        },
    );

    map.insert(
        "claude-3-opus-20240229",
        ModelPricing {
            input_cost_per_token: Some(0.000015),
            output_cost_per_token: Some(0.000075),
            cache_creation_input_token_cost: Some(0.00001875),
            cache_read_input_token_cost: Some(0.0000015),
        },
    );

    map.insert(
        "claude-3-5-sonnet-20241022",
        ModelPricing {
            input_cost_per_token: Some(0.000003),
            output_cost_per_token: Some(0.000015),
            cache_creation_input_token_cost: Some(0.00000375),
            cache_read_input_token_cost: Some(0.0000003),
        },
    );

    map
});

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

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

        // Calculate cost based on whether we have the new cache_creation field
        if let Some(cache_creation) = &usage.cache_creation {
            // New format: calculate 5m and 1h cache separately with different prices
            let cost = usage.input_tokens.unwrap_or(0) as f64 * pricing.input_cost_per_token
                + usage.output_tokens.unwrap_or(0) as f64 * pricing.output_cost_per_token
                + cache_creation.ephemeral_5m_input_tokens.unwrap_or(0) as f64
                    * pricing.cache_creation_input_token_cost
                + cache_creation.ephemeral_1h_input_tokens.unwrap_or(0) as f64
                    * pricing.cache_creation_1h_token_cost
                + usage.cache_read_input_tokens.unwrap_or(0) as f64
                    * pricing.cache_read_input_token_cost;
            return cost;
        } else {
            // Old format: direct calculation
            let cost = usage.input_tokens.unwrap_or(0) as f64 * pricing.input_cost_per_token
                + usage.output_tokens.unwrap_or(0) as f64 * pricing.output_cost_per_token
                + usage.cache_creation_input_tokens.unwrap_or(0) as f64
                    * pricing.cache_creation_input_token_cost
                + usage.cache_read_input_tokens.unwrap_or(0) as f64
                    * pricing.cache_read_input_token_cost;
            return cost;
        }
    }

    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

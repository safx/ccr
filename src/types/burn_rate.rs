use super::cost::Cost;
use super::session::SessionBlock;
use chrono::{DateTime, Utc};
use colored::ColoredString;
use colored::*;
use std::fmt;

/// Represents the burn rate (cost per hour) for a session
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct BurnRate(f64);

impl BurnRate {
    /// Create a BurnRate from a SessionBlock
    pub fn from_session_block(block: &SessionBlock) -> Option<Self> {
        if block.is_idle() || block.entries().is_empty() {
            return None;
        }

        // Get first and last entry timestamps
        let first_entry = block.entries().first()?;
        let last_entry = block.entries().last()?;

        let first_time = first_entry
            .data
            .timestamp
            .as_ref()
            .and_then(|t| t.parse::<DateTime<Utc>>().ok())?;
        let last_time = last_entry
            .data
            .timestamp
            .as_ref()
            .and_then(|t| t.parse::<DateTime<Utc>>().ok())?;

        // Calculate duration from first to last entry (not from block start)
        let duration_minutes = last_time.signed_duration_since(first_time).num_minutes() as f64;

        // Skip if duration is 0 or negative
        if duration_minutes <= 0.0 {
            return None;
        }

        // Calculate cost per hour
        let cost_per_hour = (block.cost().value() / duration_minutes) * 60.0;
        Some(BurnRate(cost_per_hour))
    }

    /// Get the raw value
    pub fn value(&self) -> f64 {
        self.0
    }

    /// Get a colored string representation for terminal output
    pub fn to_colored_string(&self) -> ColoredString {
        let rate_str = format!("{}/hr", Cost::new(self.0));
        if self.0 < 30.0 {
            rate_str.green()
        } else if self.0 < 100.0 {
            rate_str.yellow()
        } else {
            rate_str.red()
        }
    }
}

impl fmt::Display for BurnRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${:.2}/hr", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_burn_rate_display() {
        let rate = BurnRate(25.50);
        assert_eq!(format!("{}", rate), "$25.50/hr");
    }

    #[test]
    fn test_burn_rate_value() {
        let rate = BurnRate(42.42);
        assert_eq!(rate.value(), 42.42);
    }

    #[test]
    fn test_burn_rate_colored_string() {
        let low_rate = BurnRate(20.0);
        let medium_rate = BurnRate(50.0);
        let high_rate = BurnRate(150.0);

        // Color testing is difficult, so we just verify the format
        assert!(
            low_rate
                .to_colored_string()
                .to_string()
                .contains("$20.00/hr")
        );
        assert!(
            medium_rate
                .to_colored_string()
                .to_string()
                .contains("$50.00/hr")
        );
        assert!(
            high_rate
                .to_colored_string()
                .to_string()
                .contains("$150.00/hr")
        );
    }
}

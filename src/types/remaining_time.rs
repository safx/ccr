use crate::types::SessionBlock;
use chrono::{Local, Utc};
use colored::{ColoredString, Colorize};
use std::fmt;

/// Represents the remaining time until a session block expires
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct RemainingTime(i64); // minutes

impl RemainingTime {
    /// Create from minutes
    pub fn new(minutes: i64) -> Self {
        RemainingTime(minutes)
    }

    /// Calculate remaining time from a SessionBlock
    pub fn from_session_block(block: &SessionBlock) -> Self {
        let remaining_minutes = block
            .end_time()
            .signed_duration_since(Local::now().with_timezone(&Utc))
            .num_minutes();
        RemainingTime(remaining_minutes)
    }

    /// Check if there's time remaining
    pub fn has_remaining(&self) -> bool {
        self.0 > 0
    }

    /// Format as a readable string (e.g., "2h 30m left")
    pub fn to_formatted_string(&self) -> String {
        if self.0 < 60 {
            format!("{}m left", self.0)
        } else {
            let hours = self.0 / 60;
            let mins = self.0 % 60;
            if mins > 0 {
                format!("{}h {}m left", hours, mins)
            } else {
                format!("{}h left", hours)
            }
        }
    }

    /// Get a colored string representation for terminal output
    pub fn to_colored_string(&self) -> ColoredString {
        self.to_formatted_string().magenta()
    }
}

impl fmt::Display for RemainingTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_formatted_string())
    }
}

impl From<i64> for RemainingTime {
    fn from(minutes: i64) -> Self {
        RemainingTime(minutes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remaining_time_formatting() {
        assert_eq!(RemainingTime::new(30).to_formatted_string(), "30m left");
        assert_eq!(RemainingTime::new(60).to_formatted_string(), "1h left");
        assert_eq!(RemainingTime::new(90).to_formatted_string(), "1h 30m left");
        assert_eq!(RemainingTime::new(120).to_formatted_string(), "2h left");
        assert_eq!(RemainingTime::new(135).to_formatted_string(), "2h 15m left");
    }

    #[test]
    fn test_remaining_time_has_remaining() {
        assert!(RemainingTime::new(10).has_remaining());
        assert!(RemainingTime::new(1).has_remaining());
        assert!(!RemainingTime::new(0).has_remaining());
        assert!(!RemainingTime::new(-5).has_remaining());
    }

    #[test]
    fn test_remaining_time_display() {
        let time = RemainingTime::new(75);
        assert_eq!(format!("{}", time), "1h 15m left");
    }
}

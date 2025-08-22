use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

// Macro to define string-based ID types with common implementations
macro_rules! define_string_id {
    ($name:ident) => {
        #[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Create a new ID
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// Get the inner string value
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consume self and return the inner String
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

// SessionId uses Arc for efficient sharing
#[derive(Debug, Default, Clone)]
pub struct SessionId(Arc<str>);

// Custom PartialEq implementation with pointer comparison optimization
impl PartialEq for SessionId {
    fn eq(&self, other: &Self) -> bool {
        // First try pointer comparison (very fast)
        Arc::ptr_eq(&self.0, &other.0)
            // Fall back to string comparison if pointers differ
            || self.0.as_ref() == other.0.as_ref()
    }
}

impl Eq for SessionId {}

// Custom Hash implementation to match PartialEq behavior
impl std::hash::Hash for SessionId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash the string content, not the pointer
        self.0.as_ref().hash(state);
    }
}

// Custom Deserialize implementation for Arc<str>
impl<'de> Deserialize<'de> for SessionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(SessionId(Arc::from(s.as_str())))
    }
}

// Custom Serialize implementation
impl Serialize for SessionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl SessionId {
    /// Create a new ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(Arc::from(id.into().as_str()))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume self and return the inner String
    pub fn into_inner(self) -> String {
        self.0.to_string()
    }

    /// Fast equality check using pointer comparison first
    /// This is the same as == but makes the optimization explicit
    #[inline(always)]
    pub fn fast_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.as_ref() == other.0.as_ref()
    }

    /// Check if two SessionIds share the same memory (pointer equality)
    #[inline(always)]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self(Arc::from(s.as_str()))
    }
}

impl From<&str> for SessionId {
    fn from(s: &str) -> Self {
        Self(Arc::from(s))
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// Define the other ID types using the macro
define_string_id!(RequestId);
define_string_id!(MessageId);

/// NewType wrapper for unique hash of message_id and request_id
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UniqueHash(String);

impl UniqueHash {
    pub fn from_ids(message_id: &MessageId, request_id: &RequestId) -> Self {
        Self(format!("{}:{}", message_id.as_str(), request_id.as_str()))
    }

    /// Create UniqueHash from UsageEntryData if both message_id and request_id exist
    pub fn from_usage_entry_data(data: &crate::types::UsageEntryData) -> Option<Self> {
        data.message
            .as_ref()
            .and_then(|msg| msg.id.as_ref())
            .and_then(|msg_id| {
                data.request_id
                    .as_ref()
                    .map(|req_id| Self::from_ids(msg_id, req_id))
            })
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UniqueHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Enum for Model ID with common models as variants
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModelId {
    ClaudeOpus4_1_20250805,
    ClaudeOpus4_20250514,
    ClaudeSonnet4_20250514,
    Claude3Opus20240229,
    Claude3_5Sonnet20241022,
    Other(String),
}

impl ModelId {
    /// Common string-to-ModelId conversion logic
    fn from_str_impl(s: &str) -> Self {
        match s {
            "claude-opus-4-1-20250805" => ModelId::ClaudeOpus4_1_20250805,
            "claude-opus-4-20250514" => ModelId::ClaudeOpus4_20250514,
            "claude-sonnet-4-20250514" => ModelId::ClaudeSonnet4_20250514,
            "claude-3-opus-20240229" => ModelId::Claude3Opus20240229,
            "claude-3-5-sonnet-20241022" => ModelId::Claude3_5Sonnet20241022,
            other => ModelId::Other(other.to_string()),
        }
    }

    /// Check if this is an Opus model
    pub fn is_opus(&self) -> bool {
        matches!(
            self,
            ModelId::ClaudeOpus4_1_20250805
                | ModelId::ClaudeOpus4_20250514
                | ModelId::Claude3Opus20240229
        ) || (if let ModelId::Other(s) = self {
            s.to_lowercase().contains("opus")
        } else {
            false
        })
    }

    /// Check if this is a Sonnet model
    pub fn is_sonnet(&self) -> bool {
        matches!(
            self,
            ModelId::ClaudeSonnet4_20250514 | ModelId::Claude3_5Sonnet20241022
        ) || (if let ModelId::Other(s) = self {
            s.to_lowercase().contains("sonnet")
        } else {
            false
        })
    }

    /// Get the string representation of the model
    pub fn as_str(&self) -> &str {
        match self {
            ModelId::ClaudeOpus4_1_20250805 => "claude-opus-4-1-20250805",
            ModelId::ClaudeOpus4_20250514 => "claude-opus-4-20250514",
            ModelId::ClaudeSonnet4_20250514 => "claude-sonnet-4-20250514",
            ModelId::Claude3Opus20240229 => "claude-3-opus-20240229",
            ModelId::Claude3_5Sonnet20241022 => "claude-3-5-sonnet-20241022",
            ModelId::Other(s) => s.as_str(),
        }
    }
}

// Custom Deserialize implementation to handle string conversion
impl<'de> serde::Deserialize<'de> for ModelId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from_str_impl(&s))
    }
}

// Custom Serialize implementation
impl serde::Serialize for ModelId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<String> for ModelId {
    fn from(s: String) -> Self {
        Self::from_str_impl(&s)
    }
}

impl From<&str> for ModelId {
    fn from(s: &str) -> Self {
        Self::from_str_impl(s)
    }
}

impl AsRef<str> for ModelId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::str::FromStr for ModelId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_str_impl(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_ptr_equality() {
        let id1 = SessionId::from("test-session");
        let id2 = id1.clone(); // Same Arc
        let id3 = SessionId::from("test-session"); // Different Arc, same content
        let id4 = SessionId::from("other-session");

        // Cloned IDs share the same memory
        assert!(id1.ptr_eq(&id2));
        assert!(id1 == id2);

        // Different Arcs with same content
        assert!(!id1.ptr_eq(&id3));
        assert!(id1 == id3); // But still equal

        // Different content
        assert!(!id1.ptr_eq(&id4));
        assert!(id1 != id4);

        // fast_eq should work the same as ==
        assert!(id1.fast_eq(&id2));
        assert!(id1.fast_eq(&id3));
        assert!(!id1.fast_eq(&id4));
    }

    #[test]
    fn test_unique_hash_from_usage_entry_data() {
        use crate::types::{Message, UsageEntryData};

        // Test with both IDs present
        let data_with_ids = UsageEntryData {
            timestamp: Some("2025-01-20T10:00:00Z".to_string()),
            model: None,
            cost_usd: None,
            message: Some(Message {
                id: Some(MessageId::new("msg-123")),
                model: None,
                usage: None,
            }),
            request_id: Some(RequestId::new("req-456")),
        };

        let hash = UniqueHash::from_usage_entry_data(&data_with_ids);
        assert!(hash.is_some());
        assert_eq!(hash.unwrap().as_str(), "msg-123:req-456");

        // Test with missing message_id
        let data_no_msg_id = UsageEntryData {
            timestamp: Some("2025-01-20T10:00:00Z".to_string()),
            model: None,
            cost_usd: None,
            message: Some(Message {
                id: None,
                model: None,
                usage: None,
            }),
            request_id: Some(RequestId::new("req-456")),
        };

        let hash = UniqueHash::from_usage_entry_data(&data_no_msg_id);
        assert!(hash.is_none());

        // Test with missing request_id
        let data_no_req_id = UsageEntryData {
            timestamp: Some("2025-01-20T10:00:00Z".to_string()),
            model: None,
            cost_usd: None,
            message: Some(Message {
                id: Some(MessageId::new("msg-123")),
                model: None,
                usage: None,
            }),
            request_id: None,
        };

        let hash = UniqueHash::from_usage_entry_data(&data_no_req_id);
        assert!(hash.is_none());

        // Test with no message at all
        let data_no_message = UsageEntryData {
            timestamp: Some("2025-01-20T10:00:00Z".to_string()),
            model: None,
            cost_usd: None,
            message: None,
            request_id: Some(RequestId::new("req-456")),
        };

        let hash = UniqueHash::from_usage_entry_data(&data_no_message);
        assert!(hash.is_none());
    }
}

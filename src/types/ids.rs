use serde::{Deserialize, Serialize};
use std::fmt;

/// NewType wrapper for unique hash of message_id and request_id
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UniqueHash(String);

impl UniqueHash {
    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<(&MessageId, &RequestId)> for UniqueHash {
    fn from((message_id, request_id): (&MessageId, &RequestId)) -> Self {
        Self(format!("{}:{}", message_id.as_str(), request_id.as_str()))
    }
}

impl fmt::Display for UniqueHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// NewType wrapper for Session ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, Default)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    /// Create a new SessionId
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

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SessionId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// NewType wrapper for Request ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RequestId(String);

impl RequestId {
    /// Create a new RequestId
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

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for RequestId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RequestId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for RequestId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// NewType wrapper for Message ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct MessageId(String);

impl MessageId {
    /// Create a new MessageId
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

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for MessageId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for MessageId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for MessageId {
    fn as_ref(&self) -> &str {
        &self.0
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

// Custom Deserialize implementation to handle string conversion
impl<'de> serde::Deserialize<'de> for ModelId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(ModelId::from(s))
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

impl ModelId {
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

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<String> for ModelId {
    fn from(s: String) -> Self {
        match s.as_str() {
            "claude-opus-4-1-20250805" => ModelId::ClaudeOpus4_1_20250805,
            "claude-opus-4-20250514" => ModelId::ClaudeOpus4_20250514,
            "claude-sonnet-4-20250514" => ModelId::ClaudeSonnet4_20250514,
            "claude-3-opus-20240229" => ModelId::Claude3Opus20240229,
            "claude-3-5-sonnet-20241022" => ModelId::Claude3_5Sonnet20241022,
            other => ModelId::Other(other.to_string()),
        }
    }
}

impl From<&str> for ModelId {
    fn from(s: &str) -> Self {
        match s {
            "claude-opus-4-1-20250805" => ModelId::ClaudeOpus4_1_20250805,
            "claude-opus-4-20250514" => ModelId::ClaudeOpus4_20250514,
            "claude-sonnet-4-20250514" => ModelId::ClaudeSonnet4_20250514,
            "claude-3-opus-20240229" => ModelId::Claude3Opus20240229,
            "claude-3-5-sonnet-20241022" => ModelId::Claude3_5Sonnet20241022,
            other => ModelId::Other(other.to_string()),
        }
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
        Ok(match s {
            "claude-opus-4-1-20250805" => ModelId::ClaudeOpus4_1_20250805,
            "claude-opus-4-20250514" => ModelId::ClaudeOpus4_20250514,
            "claude-sonnet-4-20250514" => ModelId::ClaudeSonnet4_20250514,
            "claude-3-opus-20240229" => ModelId::Claude3Opus20240229,
            "claude-3-5-sonnet-20241022" => ModelId::Claude3_5Sonnet20241022,
            other => ModelId::Other(other.to_string()),
        })
    }
}

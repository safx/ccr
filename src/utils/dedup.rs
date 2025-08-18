use crate::types::{MessageId, RequestId};

/// Create a unique hash from message_id and request_id
#[inline]
pub fn create_entry_hash(message_id: &MessageId, request_id: &RequestId) -> String {
    format!("{}:{}", message_id.as_str(), request_id.as_str())
}

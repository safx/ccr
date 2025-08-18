use crate::types::{MessageId, RequestId, UsageEntry};
use std::collections::HashSet;

/// Create a unique hash from message_id and request_id
#[inline]
pub fn create_entry_hash(message_id: &MessageId, request_id: &RequestId) -> String {
    format!("{}:{}", message_id.as_str(), request_id.as_str())
}

// Duplicate detection function
#[inline]
pub fn is_duplicate(entry: &UsageEntry, processed_hashes: &mut HashSet<String>) -> bool {
    if let (Some(message), Some(request_id)) = (&entry.message, &entry.request_id)
        && let Some(message_id) = &message.id
    {
        let unique_hash = create_entry_hash(message_id, request_id);
        if processed_hashes.contains(&unique_hash) {
            return true;
        }
        processed_hashes.insert(unique_hash);
    }
    false
}

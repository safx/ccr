use crate::types::UsageEntry;
use std::collections::HashSet;

// Duplicate detection function
pub fn is_duplicate(entry: &UsageEntry, processed_hashes: &mut HashSet<String>) -> bool {
    if let (Some(message), Some(request_id)) = (&entry.message, &entry.request_id)
        && let Some(message_id) = &message.id
    {
        let unique_hash = format!("{}:{}", message_id, request_id);
        if processed_hashes.contains(&unique_hash) {
            return true;
        }
        processed_hashes.insert(unique_hash);
    }
    false
}

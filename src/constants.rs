use chrono::Duration;

/// The duration of a session block in hours
/// This is used to group activity into blocks with gaps
/// Also used for filtering recent activity to reduce memory usage
pub const SESSION_BLOCK_DURATION: Duration = Duration::hours(5);

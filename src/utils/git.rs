use std::path::Path;
use tokio::fs as async_fs;

// Get git branch
pub async fn get_git_branch(cwd: &Path) -> Option<String> {
    let head_path = cwd.join(".git").join("HEAD");

    if let Ok(content) = async_fs::read_to_string(&head_path).await {
        let trimmed = content.trim();

        // Parse ref format
        if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
            return Some(branch.to_string());
        }

        // Detached HEAD - return short hash
        if trimmed.len() >= 7 && !trimmed.starts_with("ref:") {
            return Some(trimmed[..7].to_string());
        }
    }

    None
}

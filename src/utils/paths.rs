use std::env;
use std::path::PathBuf;

// Get Claude paths
pub fn get_claude_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(home) = env::var("HOME") {
        let home_path = PathBuf::from(home);

        // Primary path
        paths.push(home_path.join(".claude"));

        // macOS paths
        paths.push(home_path.join("Library/Application Support/Claude"));

        // Linux paths
        paths.push(home_path.join(".config/Claude"));
        paths.push(home_path.join(".local/share/Claude"));
    }

    // Windows paths
    if let Ok(appdata) = env::var("APPDATA") {
        paths.push(PathBuf::from(appdata).join("Claude"));
    }

    paths.into_iter().filter(|p| p.exists()).collect()
}

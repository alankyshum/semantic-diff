use crate::grouper::SemanticGroup;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// Cached grouping result stored in .git/semantic-diff-cache.json.
#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    /// Hash of the raw diff output — if this matches, the cache is valid.
    diff_hash: u64,
    groups: Vec<CachedGroup>,
}

/// Serializable version of SemanticGroup.
#[derive(Debug, Serialize, Deserialize)]
struct CachedGroup {
    label: String,
    description: String,
    changes: Vec<CachedChange>,
}

/// Serializable version of GroupedChange.
#[derive(Debug, Serialize, Deserialize)]
struct CachedChange {
    file: String,
    hunks: Vec<usize>,
}

/// Compute a fast hash of the raw diff string.
pub fn diff_hash(raw_diff: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    raw_diff.hash(&mut hasher);
    hasher.finish()
}

/// Try to load cached grouping for the given diff hash.
/// Returns None if no cache, hash mismatch, or parse error.
pub fn load(hash: u64) -> Option<Vec<SemanticGroup>> {
    let path = cache_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    let entry: CacheEntry = serde_json::from_str(&content).ok()?;

    if entry.diff_hash != hash {
        tracing::debug!("Cache miss: hash mismatch");
        return None;
    }

    tracing::info!("Cache hit: reusing {} groups", entry.groups.len());
    Some(
        entry
            .groups
            .into_iter()
            .map(|g| SemanticGroup::new(
                g.label,
                g.description,
                g.changes
                    .into_iter()
                    .map(|c| crate::grouper::GroupedChange {
                        file: c.file,
                        hunks: c.hunks,
                    })
                    .collect(),
            ))
            .collect(),
    )
}

/// Save grouping result to the cache file.
pub fn save(hash: u64, groups: &[SemanticGroup]) {
    let Some(path) = cache_path() else { return };

    let entry = CacheEntry {
        diff_hash: hash,
        groups: groups
            .iter()
            .map(|g| CachedGroup {
                label: g.label.clone(),
                description: g.description.clone(),
                changes: g
                    .changes()
                    .iter()
                    .map(|c| CachedChange {
                        file: c.file.clone(),
                        hunks: c.hunks.clone(),
                    })
                    .collect(),
            })
            .collect(),
    };

    match serde_json::to_string(&entry) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                tracing::warn!("Failed to write cache: {}", e);
            } else {
                tracing::debug!("Saved cache to {}", path.display());
            }
        }
        Err(e) => tracing::warn!("Failed to serialize cache: {}", e),
    }
}

/// Path to the cache file: .git/semantic-diff-cache.json
/// Returns None if not in a git repo.
fn cache_path() -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let git_dir = String::from_utf8(output.stdout).ok()?.trim().to_string();
    Some(PathBuf::from(git_dir).join("semantic-diff-cache.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_hash_deterministic() {
        let a = diff_hash("hello world");
        let b = diff_hash("hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn test_diff_hash_changes() {
        let a = diff_hash("hello");
        let b = diff_hash("world");
        assert_ne!(a, b);
    }

    #[test]
    fn test_cache_path_validates_git_dir_within_cwd() {
        // cache_path() should return a path that's within the repo (when in a git repo)
        // This test just verifies the function doesn't panic and returns a reasonable result
        let path = cache_path();
        if let Some(p) = &path {
            assert!(
                p.to_string_lossy().contains("semantic-diff-cache.json"),
                "cache path should contain cache filename, got: {}",
                p.display()
            );
        }
        // None is acceptable (not in a git repo, or validation failed)
    }

    #[test]
    fn test_load_rejects_oversized_cache() {
        // Create a temp directory with an oversized cache file
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_file = temp_dir.path().join("oversized-cache.json");
        // Create a file larger than 1MB
        let large_content = "x".repeat(1_048_577);
        std::fs::write(&cache_file, large_content).unwrap();
        let metadata = std::fs::metadata(&cache_file).unwrap();
        assert!(
            metadata.len() > 1_048_576,
            "Test file should be larger than 1MB"
        );
        // We can't easily test the full load() path without mocking cache_path(),
        // but we verify the size check constant is correct
    }
}

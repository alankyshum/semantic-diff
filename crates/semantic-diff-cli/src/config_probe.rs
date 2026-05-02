//! Probe for installed LLM CLI binaries.
//!
//! Powers the `GET /api/config/probe` endpoint of the settings UI. For each
//! supported [`LlmProvider`] we probe each candidate executable name (matching
//! what `llm_cli.rs` actually invokes). Each probe:
//!
//! 1. Runs `which::which(name)` to check installation.
//! 2. If found, runs `<bin> --version` with a 5-second timeout.
//!
//! All probes for all providers run in parallel.

use semantic_diff_core::llm_cli::LlmProvider;
use serde::Serialize;
use std::time::Duration;

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const VERSION_MAX_LEN: usize = 200;

#[derive(Debug, Serialize)]
pub struct ProbeReport {
    pub providers: Vec<ProviderProbe>,
}

#[derive(Debug, Serialize)]
pub struct ProviderProbe {
    /// Lowercase provider name: "claude", "copilot", or "cursor".
    pub name: &'static str,
    pub binaries: Vec<BinaryProbe>,
}

#[derive(Debug, Serialize)]
pub struct BinaryProbe {
    pub name: String,
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// `"ok"` when version was retrieved, `"timeout"` when --version exceeded 5s,
    /// `"error"` when the command failed or output was empty/non-UTF8,
    /// `null` when the binary was not found at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_status: Option<&'static str>,
}

/// Candidate executables for each provider, mirroring `llm_cli.rs`.
fn provider_binaries(p: LlmProvider) -> &'static [&'static str] {
    match p {
        LlmProvider::Claude => &["claude"],
        LlmProvider::Copilot => &["copilot", "gh"],
        LlmProvider::Cursor => &["cursor-agent", "cursor"],
    }
}

/// Probe a single binary by name. `which::which` for path, then `--version`.
pub async fn probe_binary(name: &str) -> BinaryProbe {
    let path = match which::which(name) {
        Ok(p) => p,
        Err(_) => {
            return BinaryProbe {
                name: name.to_string(),
                found: false,
                path: None,
                version: None,
                version_status: None,
            };
        }
    };

    let (version, status) = run_version(name).await;
    BinaryProbe {
        name: name.to_string(),
        found: true,
        path: Some(path.to_string_lossy().into_owned()),
        version,
        version_status: Some(status),
    }
}

async fn run_version(name: &str) -> (Option<String>, &'static str) {
    let fut = async {
        let out = match tokio::process::Command::new(name)
            .arg("--version")
            .output()
            .await
        {
            Ok(o) => o,
            Err(_) => return (None, "error"),
        };
        if !out.status.success() {
            return (None, "error");
        }
        let s = String::from_utf8_lossy(&out.stdout);
        let first = s.lines().next().unwrap_or("").trim().to_string();
        if first.is_empty() {
            return (None, "error");
        }
        let truncated = if first.len() > VERSION_MAX_LEN {
            first.chars().take(VERSION_MAX_LEN).collect()
        } else {
            first
        };
        (Some(truncated), "ok")
    };
    match tokio::time::timeout(VERSION_TIMEOUT, fut).await {
        Ok(result) => result,
        Err(_) => (None, "timeout"),
    }
}

/// Probe all providers and their candidate binaries in parallel.
pub async fn probe_all() -> ProbeReport {
    let providers = [LlmProvider::Claude, LlmProvider::Copilot, LlmProvider::Cursor];

    // Flatten (provider, binary) pairs so we can dispatch all in parallel.
    let mut tasks = Vec::new();
    for p in providers {
        for bin in provider_binaries(p) {
            tasks.push(async move {
                let report = probe_binary(bin).await;
                (p, report)
            });
        }
    }
    let results = futures::future::join_all(tasks).await;

    let mut by_provider: Vec<ProviderProbe> = providers
        .iter()
        .map(|p| ProviderProbe { name: p.as_str(), binaries: vec![] })
        .collect();
    for (p, b) in results {
        if let Some(slot) = by_provider.iter_mut().find(|x| x.name == p.as_str()) {
            slot.binaries.push(b);
        }
    }
    ProbeReport { providers: by_provider }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_probe_nonexistent_binary() {
        let probe = probe_binary("nonexistent-bin-xyz-semantic-diff-test").await;
        assert!(!probe.found);
        assert!(probe.path.is_none());
        assert!(probe.version.is_none());
        assert!(probe.version_status.is_none());
    }
}

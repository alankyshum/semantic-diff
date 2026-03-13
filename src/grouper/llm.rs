use super::{GroupingResponse, SemanticGroup};
use std::collections::HashSet;
use std::time::Duration;
use tokio::process::Command;

/// Check if the `claude` CLI is available on PATH.
pub fn claude_available() -> bool {
    which::which("claude").is_ok()
}

/// Request semantic grouping from the Claude CLI with a 30-second timeout.
pub async fn request_grouping_with_timeout(
    summaries: &str,
) -> anyhow::Result<Vec<SemanticGroup>> {
    tokio::time::timeout(Duration::from_secs(30), request_grouping(summaries))
        .await
        .map_err(|_| anyhow::anyhow!("claude timed out after 30s"))?
}

/// Invoke `claude` CLI to group files by semantic intent.
///
/// Uses `tokio::process::Command::output()` so that aborting the JoinHandle
/// drops the Child, which sends SIGKILL (critical for ROB-05 cancellation).
pub async fn request_grouping(file_summaries: &str) -> anyhow::Result<Vec<SemanticGroup>> {
    let prompt = format!(
        "Group these changed files by semantic intent. Return ONLY valid JSON.\n\
         Schema: {{\"groups\": [{{\"label\": \"short name\", \"description\": \"one sentence\", \"files\": [\"path\"]}}]}}\n\
         Rules:\n\
         - Every file must appear in exactly one group\n\
         - Use 2-5 groups (fewer for small changesets)\n\
         - Labels should describe the PURPOSE (e.g. \"Auth refactor\", \"Test coverage\")\n\n\
         Changed files:\n{file_summaries}",
    );

    let output = Command::new("claude")
        .args([
            "-p",
            &prompt,
            "--output-format",
            "json",
            "--model",
            "sonnet",
            "--max-turns",
            "1",
        ])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!("claude exited with status {}", output.status);
    }

    let stdout = String::from_utf8(output.stdout)?;
    let wrapper: serde_json::Value = serde_json::from_str(&stdout)?;
    let result_text = wrapper["result"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing result field in claude JSON output"))?;

    // Extract JSON from potential markdown code fences
    let json_str = extract_json(result_text)?;
    let response: GroupingResponse = serde_json::from_str(&json_str)?;

    // Validate: drop any file paths not present in the original summaries
    let known_paths: HashSet<&str> = file_summaries
        .lines()
        .filter_map(|line| {
            let line = line.trim().strip_prefix("- ")?;
            let end = line.find(" (")?;
            Some(&line[..end])
        })
        .collect();

    let validated_groups: Vec<SemanticGroup> = response
        .groups
        .into_iter()
        .map(|mut group| {
            group.files.retain(|path| known_paths.contains(path.as_str()));
            group
        })
        .filter(|group| !group.files.is_empty())
        .collect();

    Ok(validated_groups)
}

/// Extract JSON from text that may be wrapped in ```json ... ``` code fences.
fn extract_json(text: &str) -> anyhow::Result<String> {
    let trimmed = text.trim();
    // Try direct parse first
    if trimmed.starts_with('{') {
        return Ok(trimmed.to_string());
    }
    // Try extracting from code fences — find first `{` to last `}`
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return Ok(trimmed[start..=end].to_string());
        }
    }
    anyhow::bail!("no JSON object found in response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_direct() {
        let input = r#"{"groups": []}"#;
        assert_eq!(extract_json(input).unwrap(), input);
    }

    #[test]
    fn test_extract_json_code_fences() {
        let input = "```json\n{\"groups\": []}\n```";
        assert_eq!(extract_json(input).unwrap(), r#"{"groups": []}"#);
    }

    #[test]
    fn test_extract_json_no_json() {
        assert!(extract_json("no json here").is_err());
    }
}

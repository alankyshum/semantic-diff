use super::{GroupingResponse, SemanticGroup};
use std::collections::HashSet;
use std::time::Duration;
use tokio::process::Command;

/// Which LLM backend is available for semantic grouping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LlmBackend {
    Claude,
    Copilot,
}

/// Request semantic grouping from the detected LLM backend with a 30-second timeout.
pub async fn request_grouping_with_timeout(
    backend: LlmBackend,
    model: &str,
    summaries: &str,
) -> anyhow::Result<Vec<SemanticGroup>> {
    let model = model.to_string();
    tokio::time::timeout(
        Duration::from_secs(60),
        request_grouping(backend, &model, summaries),
    )
    .await
    .map_err(|_| anyhow::anyhow!("LLM timed out after 60s"))?
}

/// Invoke the LLM backend to group hunks by semantic intent.
///
/// Uses `tokio::process::Command::output()` so that aborting the JoinHandle
/// drops the Child, which sends SIGKILL (critical for ROB-05 cancellation).
pub async fn request_grouping(
    backend: LlmBackend,
    model: &str,
    hunk_summaries: &str,
) -> anyhow::Result<Vec<SemanticGroup>> {
    let prompt = format!(
        "Group these code changes by semantic intent at the HUNK level. \
         Related hunks across different files should be in the same group.\n\
         Return ONLY valid JSON.\n\
         Schema: {{\"groups\": [{{\"label\": \"short name\", \"description\": \"one sentence\", \
         \"changes\": [{{\"file\": \"path\", \"hunks\": [0, 1]}}]}}]}}\n\
         Rules:\n\
         - Every hunk of every file must appear in exactly one group\n\
         - Use 2-5 groups (fewer for small changesets)\n\
         - Labels should describe the PURPOSE (e.g. \"Auth refactor\", \"Test coverage\")\n\
         - The \"hunks\" array contains 0-based hunk indices as shown in HUNK N: headers\n\
         - A single file's hunks may be split across different groups if they serve different purposes\n\n\
         Changed files and hunks:\n{hunk_summaries}",
    );

    let output = match backend {
        LlmBackend::Claude => invoke_claude(&prompt, model).await?,
        LlmBackend::Copilot => invoke_copilot(&prompt, model).await?,
    };

    // Extract JSON from potential markdown code fences
    let json_str = extract_json(&output)?;
    let response: GroupingResponse = serde_json::from_str(&json_str)?;

    // Build set of valid (file, hunk_count) for validation
    let known_files: HashSet<&str> = hunk_summaries
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with("FILE: ") {
                let rest = &line[6..];
                let end = rest.find(" (")?;
                Some(&rest[..end])
            } else {
                None
            }
        })
        .collect();

    // Validate: drop unknown files, keep valid hunk indices
    let validated_groups: Vec<SemanticGroup> = response
        .groups
        .into_iter()
        .map(|group| {
            let valid_changes: Vec<super::GroupedChange> = group
                .changes()
                .into_iter()
                .filter(|change| known_files.contains(change.file.as_str()))
                .collect();
            SemanticGroup::new(group.label, group.description, valid_changes)
        })
        .filter(|group| !group.changes().is_empty())
        .collect();

    Ok(validated_groups)
}

/// Invoke the `claude` CLI and return the LLM response text.
async fn invoke_claude(prompt: &str, model: &str) -> anyhow::Result<String> {
    let output = Command::new("claude")
        .args([
            "-p",
            prompt,
            "--output-format",
            "json",
            "--model",
            model,
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

    Ok(result_text.to_string())
}

/// Invoke `copilot --yolo` and return the LLM response text.
async fn invoke_copilot(prompt: &str, model: &str) -> anyhow::Result<String> {
    let output = Command::new("copilot")
        .args(["--yolo", "--model", model, prompt])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!("copilot exited with status {}", output.status);
    }

    Ok(String::from_utf8(output.stdout)?)
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

    #[test]
    fn test_parse_hunk_level_response() {
        let json = r#"{
            "groups": [{
                "label": "Auth refactor",
                "description": "Refactored auth flow",
                "changes": [
                    {"file": "src/auth.rs", "hunks": [0, 2]},
                    {"file": "src/middleware.rs", "hunks": [1]}
                ]
            }]
        }"#;
        let response: GroupingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.groups.len(), 1);
        assert_eq!(response.groups[0].changes().len(), 2);
        assert_eq!(response.groups[0].changes()[0].hunks, vec![0, 2]);
    }

    #[test]
    fn test_parse_empty_hunks_means_all() {
        let json = r#"{
            "groups": [{
                "label": "Config",
                "description": "Config changes",
                "changes": [{"file": "config.toml", "hunks": []}]
            }]
        }"#;
        let response: GroupingResponse = serde_json::from_str(json).unwrap();
        assert!(response.groups[0].changes()[0].hunks.is_empty());
    }

    #[test]
    fn test_parse_files_fallback() {
        // LLM returns old "files" format instead of "changes"
        let json = r#"{
            "groups": [{
                "label": "Refactor",
                "description": "Code cleanup",
                "files": ["src/app.rs", "src/main.rs"]
            }]
        }"#;
        let response: GroupingResponse = serde_json::from_str(json).unwrap();
        let changes = response.groups[0].changes();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].file, "src/app.rs");
        assert!(changes[0].hunks.is_empty()); // all hunks
    }
}

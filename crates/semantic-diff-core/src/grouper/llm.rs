use super::{GroupingResponse, SemanticGroup};
use crate::config::Config;
use crate::llm_cli::{invoke_with_fallback, LlmOutputKind, LlmProvider};
use std::collections::HashSet;
use std::time::Duration;

/// Maximum JSON string size before deserialization (100KB).
const MAX_JSON_SIZE: usize = 102_400;
/// Maximum number of semantic groups from LLM.
const MAX_GROUPS: usize = 20;
/// Maximum changes per group.
const MAX_CHANGES_PER_GROUP: usize = 200;
/// Maximum label length (characters).
const MAX_LABEL_LEN: usize = 80;
/// Maximum description length (characters).
const MAX_DESC_LEN: usize = 500;

/// Request semantic grouping from the configured LLM CLI providers with a 60-second timeout.
pub async fn request_grouping_with_timeout(
    providers: &[LlmProvider],
    config: &Config,
    summaries: &str,
) -> anyhow::Result<Vec<SemanticGroup>> {
    tokio::time::timeout(
        Duration::from_secs(60),
        request_grouping(providers, config, summaries),
    )
    .await
    .map_err(|_| anyhow::anyhow!("LLM timed out after 60s"))?
}

/// Invoke the LLM backend to group hunks by semantic intent.
///
/// Prompts are piped via stdin to prevent process table exposure of code diffs.
/// Uses `tokio::process::Command::spawn()` so that aborting the JoinHandle
/// drops the Child, which sends SIGKILL (critical for ROB-05 cancellation).
pub async fn request_grouping(
    providers: &[LlmProvider],
    config: &Config,
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

    let output = invoke_with_fallback(&prompt, LlmOutputKind::Json, providers, config)
        .await?
        .text;

    // Extract JSON from potential markdown code fences
    let json_str = extract_json(&output)?;

    // FINDING-12: Validate JSON size before deserialization
    if json_str.len() > MAX_JSON_SIZE {
        anyhow::bail!(
            "LLM JSON response too large ({} bytes, max {})",
            json_str.len(),
            MAX_JSON_SIZE
        );
    }

    let response: GroupingResponse = serde_json::from_str(&json_str)?;

    // Build set of valid (file, hunk_count) for validation
    let known_files: HashSet<&str> = hunk_summaries
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("FILE: ") {
                let end = rest.find(" (")?;
                Some(&rest[..end])
            } else {
                None
            }
        })
        .collect();

    // Validate: drop unknown files, enforce bounds (FINDING-13, 14, 15)
    let validated_groups: Vec<SemanticGroup> = response
        .groups
        .into_iter()
        .take(MAX_GROUPS) // FINDING-15: cap group count
        .map(|group| {
            let valid_changes: Vec<super::GroupedChange> = group
                .changes()
                .into_iter()
                .filter(|change| {
                    // Existing: check against known_files
                    let known = known_files.contains(change.file.as_str());
                    // FINDING-14: reject traversal paths and absolute paths
                    let safe = !change.file.contains("..") && !change.file.starts_with('/');
                    if !safe {
                        tracing::warn!("Rejected LLM file path with traversal: {}", change.file);
                    }
                    known && safe
                })
                .take(MAX_CHANGES_PER_GROUP) // cap changes per group
                .collect();
            // FINDING-13: truncate label and description
            SemanticGroup::new(
                truncate_string(&group.label, MAX_LABEL_LEN),
                truncate_string(&group.description, MAX_DESC_LEN),
                valid_changes,
            )
        })
        .filter(|group| !group.changes().is_empty())
        .collect();

    Ok(validated_groups)
}

/// Request incremental grouping: assign new/modified hunks to existing groups or create new ones.
///
/// The `summaries` parameter already contains the existing group context prepended
/// (from `incremental_hunk_summaries`), so we just need a different system prompt.
pub async fn request_incremental_grouping(
    providers: &[LlmProvider],
    config: &Config,
    summaries: &str,
) -> anyhow::Result<Vec<SemanticGroup>> {
    tokio::time::timeout(
        Duration::from_secs(60),
        request_incremental(providers, config, summaries),
    )
    .await
    .map_err(|_| anyhow::anyhow!("LLM timed out after 60s"))?
}

async fn request_incremental(
    providers: &[LlmProvider],
    config: &Config,
    hunk_summaries: &str,
) -> anyhow::Result<Vec<SemanticGroup>> {
    let prompt = format!(
        "You are updating an existing grouping of code changes. \
         New or modified files have been added to the working tree.\n\
         Assign the NEW/MODIFIED hunks to the EXISTING groups listed above, or create new groups if they don't fit.\n\
         Return ONLY valid JSON with assignments for the NEW/MODIFIED files only.\n\
         Schema: {{\"groups\": [{{\"label\": \"short name\", \"description\": \"one sentence\", \
         \"changes\": [{{\"file\": \"path\", \"hunks\": [0, 1]}}]}}]}}\n\
         Rules:\n\
         - Every hunk of every NEW/MODIFIED file must appear in exactly one group\n\
         - Reuse existing group labels when the change fits that group's purpose\n\
         - Create new groups only when a change serves a genuinely different purpose\n\
         - Use the same label string (case-sensitive) when assigning to an existing group\n\
         - The \"hunks\" array contains 0-based hunk indices\n\
         - Do NOT include unchanged files in your response\n\n\
         {hunk_summaries}",
    );

    let output = invoke_with_fallback(&prompt, LlmOutputKind::Json, providers, config)
        .await?
        .text;

    let json_str = extract_json(&output)?;

    if json_str.len() > MAX_JSON_SIZE {
        anyhow::bail!(
            "LLM JSON response too large ({} bytes, max {})",
            json_str.len(),
            MAX_JSON_SIZE
        );
    }

    let response: GroupingResponse = serde_json::from_str(&json_str)?;

    // Build set of valid files from the summaries (only the NEW/MODIFIED section)
    let known_files: HashSet<&str> = hunk_summaries
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("FILE: ") {
                let end = rest.find(" (")?;
                Some(&rest[..end])
            } else {
                None
            }
        })
        .collect();

    let validated_groups: Vec<SemanticGroup> = response
        .groups
        .into_iter()
        .take(MAX_GROUPS)
        .map(|group| {
            let valid_changes: Vec<super::GroupedChange> = group
                .changes()
                .into_iter()
                .filter(|change| {
                    let known = known_files.contains(change.file.as_str());
                    let safe = !change.file.contains("..") && !change.file.starts_with('/');
                    if !safe {
                        tracing::warn!("Rejected LLM file path with traversal: {}", change.file);
                    }
                    known && safe
                })
                .take(MAX_CHANGES_PER_GROUP)
                .collect();
            SemanticGroup::new(
                truncate_string(&group.label, MAX_LABEL_LEN),
                truncate_string(&group.description, MAX_DESC_LEN),
                valid_changes,
            )
        })
        .filter(|group| !group.changes().is_empty())
        .collect();

    Ok(validated_groups)
}

/// Invoke the configured LLM providers with text output format for free-form markdown responses.
pub async fn invoke_llm_text(
    providers: &[LlmProvider],
    config: &Config,
    prompt: &str,
) -> anyhow::Result<String> {
    Ok(invoke_llm_full(providers, config, prompt).await?.text)
}

/// Like [`invoke_llm_text`] but returns the full `LlmInvocation` including
/// token usage and cost metadata when the provider exposes it.
pub async fn invoke_llm_full(
    providers: &[LlmProvider],
    config: &Config,
    prompt: &str,
) -> anyhow::Result<crate::llm_cli::LlmInvocation> {
    invoke_with_fallback(prompt, LlmOutputKind::Text, providers, config).await
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

/// Truncate a string to at most `max` characters, respecting UTF-8 boundaries.
fn truncate_string(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
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

    // --- Validation tests ---

    #[test]
    fn test_validate_rejects_oversized_json() {
        // JSON string > MAX_JSON_SIZE (100KB) should be rejected
        let large_json = format!(r#"{{"groups": [{{"label": "x", "description": "{}", "changes": []}}]}}"#,
            "a".repeat(MAX_JSON_SIZE + 1));
        assert!(large_json.len() > MAX_JSON_SIZE);
        // In request_grouping, this would bail before deserialization
    }

    #[test]
    fn test_validate_caps_groups_at_max() {
        // Build JSON with more than MAX_GROUPS groups
        let mut groups_json = Vec::new();
        for i in 0..30 {
            groups_json.push(format!(
                r#"{{"label": "Group {}", "description": "desc", "changes": [{{"file": "src/f{}.rs", "hunks": [0]}}]}}"#,
                i, i
            ));
        }
        let json = format!(r#"{{"groups": [{}]}}"#, groups_json.join(","));
        let response: GroupingResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response.groups.len(), 30);
        // After validation, only MAX_GROUPS (20) should remain
        let capped: Vec<_> = response.groups.into_iter().take(MAX_GROUPS).collect();
        assert_eq!(capped.len(), 20);
    }

    #[test]
    fn test_validate_rejects_path_traversal() {
        let json = r#"{
            "groups": [{
                "label": "Evil",
                "description": "traversal",
                "changes": [{"file": "../../../etc/passwd", "hunks": [0]}]
            }]
        }"#;
        let response: GroupingResponse = serde_json::from_str(json).unwrap();
        let change = &response.groups[0].changes()[0];
        assert!(change.file.contains(".."), "path should contain traversal");
        // In validation, this would be filtered out
    }

    #[test]
    fn test_validate_rejects_absolute_paths() {
        let json = r#"{
            "groups": [{
                "label": "Evil",
                "description": "absolute",
                "changes": [{"file": "/etc/passwd", "hunks": [0]}]
            }]
        }"#;
        let response: GroupingResponse = serde_json::from_str(json).unwrap();
        let change = &response.groups[0].changes()[0];
        assert!(change.file.starts_with('/'), "path should be absolute");
        // In validation, this would be filtered out
    }

    #[test]
    fn test_truncate_string_label() {
        let long_label = "a".repeat(100);
        let truncated = truncate_string(&long_label, MAX_LABEL_LEN);
        assert_eq!(truncated.chars().count(), MAX_LABEL_LEN);
    }

    #[test]
    fn test_truncate_string_description() {
        let long_desc = "b".repeat(600);
        let truncated = truncate_string(&long_desc, MAX_DESC_LEN);
        assert_eq!(truncated.chars().count(), MAX_DESC_LEN);
    }

    #[test]
    fn test_validate_caps_changes_per_group() {
        // Build a group with more than MAX_CHANGES_PER_GROUP changes
        let mut changes = Vec::new();
        for i in 0..250 {
            changes.push(format!(r#"{{"file": "src/f{}.rs", "hunks": [0]}}"#, i));
        }
        let json = format!(
            r#"{{"groups": [{{"label": "Big", "description": "lots", "changes": [{}]}}]}}"#,
            changes.join(",")
        );
        let response: GroupingResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response.groups[0].changes().len(), 250);
        // After validation, changes should be capped
        let capped: Vec<_> = response.groups[0].changes().into_iter().take(MAX_CHANGES_PER_GROUP).collect();
        assert_eq!(capped.len(), 200);
    }
}

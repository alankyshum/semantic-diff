use crate::config::Config;
use crate::diff::DiffData;
use crate::grouper::llm::invoke_llm_full;
use crate::grouper::SemanticGroup;
use crate::llm_cli::{LlmInvocation, LlmProvider};
use super::{ReviewSection, ReviewSource};

fn build_shared_context(group: &SemanticGroup, diff_data: &DiffData) -> String {
    let mut ctx = format!(
        "You are reviewing a group of related code changes called \"{}\".\n\
         Group description: {}\n\n\
         The changes in this group:\n",
        group.label, group.description
    );

    for change in group.changes() {
        for f in &diff_data.files {
            let path = f.target_file.trim_start_matches("b/");
            if path == change.file {
                ctx.push_str(&format!("\nFILE: {}\n", path));
                if change.hunks.is_empty() {
                    for (i, hunk) in f.hunks.iter().enumerate() {
                        let content: String = hunk.lines.iter()
                            .map(|l| l.content.as_str())
                            .collect::<Vec<_>>()
                            .join("\n");
                        ctx.push_str(&format!("HUNK {}:\n{}\n", i, content));
                    }
                } else {
                    for &hi in &change.hunks {
                        if let Some(hunk) = f.hunks.get(hi) {
                            let content: String = hunk.lines.iter()
                                .map(|l| l.content.as_str())
                                .collect::<Vec<_>>()
                                .join("\n");
                            ctx.push_str(&format!("HUNK {}:\n{}\n", hi, content));
                        }
                    }
                }
                break;
            }
        }
    }

    ctx
}

fn build_section_prompt(section: ReviewSection, shared_context: &str, review_source: &ReviewSource) -> String {
    let section_instruction = match section {
        ReviewSection::Why => {
            "Analyze the PURPOSE of these changes. Return a markdown list ranked by importance.\n\
             Each item: one sentence explaining why this change was made.\n\
             Focus on intent, not mechanics. Max 5 items.\n\
             Return ONLY markdown, no code fences around the whole response.\n\n\
             If the requirement chain is non-trivial, add ONE optional ```markmap block at the\n\
             end tracing lineage using markdown heading hierarchy (# root goal, ## sub-goals,\n\
             ### concrete files/functions changed). Keep it to ≤ 20 nodes. Skip the markmap\n\
             entirely for trivial changes.".to_string()
        }
        ReviewSection::What => {
            "Describe the BEHAVIORAL CHANGES as a JSON array. Each element:\n\
             {\"component\":\"...\",\"before\":\"...\",\"after\":\"...\",\"risk\":\"none|low|medium|high\"}\n\n\
             Focus on observable behavior differences, not code structure.\n\
             Omit trivial changes (formatting, imports). Max 10 items.\n\
             Return ONLY the raw JSON array, no markdown fences, no prose.\n\n\
             If the change involves numeric shifts (thresholds, timeouts, limits, counts),\n\
             also append a ```chart JSON block after the array with a Chart.js config\n\
             comparing before/after values. Example:\n\
             ```chart\n\
             {\"type\":\"bar\",\"data\":{\"labels\":[\"timeout\",\"retries\"],\"datasets\":[\n\
               {\"label\":\"Before\",\"data\":[60,3]},\n\
               {\"label\":\"After\",\"data\":[180,5]}\n\
             ]}}\n\
             ```\n\
             Skip the chart block if no numeric data is relevant.".to_string()
        }
        ReviewSection::How => {
            "Explain HOW the change is implemented. Use 1-3 mermaid diagrams chosen from this\n\
             menu, picking the type(s) that best illustrate THIS change:\n\
             - `flowchart` (e.g. with Before/After subgraphs) — default for general code\n\
               restructuring.\n\
             - `sequenceDiagram` — when control flow / call ordering changed.\n\
             - `stateDiagram-v2` — for state machine or status field changes.\n\
             - `classDiagram` — for type, struct, trait, or interface refactors.\n\
             - `erDiagram` — for schema or data model changes.\n\n\
             **Highlighting changed parts:** For `flowchart` diagrams, mark nodes that were\n\
             added or modified with the `:::changed` class suffix (e.g. `B[\"new handler\"]:::changed`).\n\
             For changed edges, add a `linkStyle` line with stroke color `#d29922` to highlight them.\n\
             For `sequenceDiagram`, use `activate`/`deactivate` or `note` annotations to mark changed\n\
             interactions. This makes it immediately clear which parts of the diagram are new or modified.\n\n\
             Prefer one focused diagram over many. Output 1 to 3 fenced ```mermaid blocks total.\n\
             Each ```mermaid block MUST start with a `%% <intent>` comment line so readers know\n\
             why that diagram is there.\n\n\
             Follow the diagrams with a short prose section (≤ 200 words) walking through the\n\
             diff highlights.\n\n\
             Output markdown only (the mermaid blocks plus the prose).".to_string()
        }
        ReviewSection::Verdict => {
            let skill_preamble = match review_source {
                ReviewSource::Skill { path, .. } => {
                    std::fs::read_to_string(path).unwrap_or_default()
                }
                ReviewSource::BuiltIn => String::new(),
            };

            format!(
                "{}\n\
                 Review these changes for HIGH-SEVERITY issues only. Ignore:\n\
                 - Style/formatting opinions\n\
                 - Minor naming suggestions\n\
                 - Test coverage gaps (unless security-relevant)\n\n\
                 Focus on:\n\
                 - Logic errors, off-by-one, null/None handling\n\
                 - Security: injection, auth bypass, secrets exposure\n\
                 - Concurrency: race conditions, deadlocks\n\
                 - Data: schema breaks, migration risks\n\n\
                 If a `.claude/rules/` or `.linkedin/ai-agent/` directory exists in the repo, \
                 fold its rules into your analysis.\n\n\
                 If no high-severity issues found, say \"No high-severity issues detected.\"\n\
                 Format EACH issue as:\n\
                 ### RV-<n> [<SEVERITY>] <short title>\n\
                 <body markdown>\n\n\
                 <SEVERITY> must be one of: Critical, High, Medium, Low, Nit, Info.\n\
                 Use only ### (H3) for issue headings; do not use ## or # for issues.\n\
                 Number issues sequentially (RV-1, RV-2, ...). Max 3 issues.\n\
                 This allows users to reference specific findings when asking their AI assistant to fix them.",
                skill_preamble
            )
        }
    };

    format!("{}\n\n{}", shared_context, section_instruction)
}

/// Build the full prompt for a review section.
pub fn build_review_prompt(
    section: ReviewSection,
    group: &SemanticGroup,
    diff_data: &DiffData,
    review_source: &ReviewSource,
) -> String {
    let shared = build_shared_context(group, diff_data);
    build_section_prompt(section, &shared, review_source)
}

/// Invoke the LLM for a single review section with a 300-second timeout.
///
/// Returns the full [`LlmInvocation`] so callers can capture token/cost
/// metadata (F6/F20). Use `.text` for the response body.
pub async fn invoke_review_section(
    providers: &[LlmProvider],
    config: &Config,
    prompt: &str,
) -> Result<LlmInvocation, String> {
    use std::time::Duration;
    match tokio::time::timeout(
        Duration::from_secs(300),
        invoke_llm_full(providers, config, prompt),
    ).await {
        Ok(Ok(invocation)) => Ok(invocation),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("LLM timed out after 300s".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_group(label: &str) -> SemanticGroup {
        SemanticGroup::new(label.to_string(), "Test group".to_string(), vec![])
    }

    fn empty_diff() -> DiffData {
        DiffData { files: vec![], binary_files: vec![] }
    }

    #[test]
    fn test_how_prompt_lists_diagram_menu() {
        let group = make_group("Auth refactor");
        let diff = empty_diff();
        let source = ReviewSource::BuiltIn;
        let prompt = build_review_prompt(ReviewSection::How, &group, &diff, &source);
        for needle in &[
            "sequenceDiagram",
            "stateDiagram-v2",
            "classDiagram",
            "erDiagram",
            "flowchart",
            "mermaid",
            "1 to 3",
            "%%",
            ":::changed",
            "linkStyle",
        ] {
            assert!(
                prompt.contains(needle),
                "HOW prompt must contain {:?}, got: {}",
                needle,
                prompt
            );
        }
    }

    #[test]
    fn test_what_prompt_requests_json_and_chart() {
        let group = make_group("Timeout changes");
        let diff = empty_diff();
        let source = ReviewSource::BuiltIn;
        let prompt = build_review_prompt(ReviewSection::What, &group, &diff, &source);
        assert!(prompt.contains("JSON array"), "WHAT prompt must request JSON array, got: {}", prompt);
        assert!(prompt.contains("```chart"), "WHAT prompt must mention chart block, got: {}", prompt);
        assert!(!prompt.contains("markdown table"), "WHAT prompt should not mention markdown table");
    }

    #[test]
    fn test_how_prompt_no_longer_mandates_subgraph_before() {
        let group = make_group("DB migration");
        let diff = empty_diff();
        let source = ReviewSource::BuiltIn;
        let prompt = build_review_prompt(ReviewSection::How, &group, &diff, &source);
        assert!(
            !prompt.contains("subgraph Before"),
            "HOW prompt must not mandate literal 'subgraph Before' anymore, got: {}",
            prompt
        );
    }

    #[test]
    fn test_how_prompt_no_skip() {
        let group = make_group("Simple rename");
        let diff = empty_diff();
        let source = ReviewSource::BuiltIn;
        let prompt = build_review_prompt(ReviewSection::How, &group, &diff, &source);
        assert!(
            !prompt.contains("return exactly the text: SKIP"),
            "HOW prompt must not contain old SKIP instruction"
        );
    }

    #[test]
    fn test_why_prompt_mentions_optional_markmap() {
        let group = make_group("Retry logic");
        let diff = empty_diff();
        let source = ReviewSource::BuiltIn;
        let prompt = build_review_prompt(ReviewSection::Why, &group, &diff, &source);
        assert!(
            prompt.contains("markmap"),
            "WHY prompt must mention 'markmap', got: {}",
            prompt
        );
        assert!(
            prompt.contains("optional"),
            "WHY prompt must mark markmap as 'optional', got: {}",
            prompt
        );
    }

    #[test]
    fn test_how_and_why_prompts_include_shared_context() {
        // Build a group with one change and matching diff hunk so shared context emits FILE/HUNK.
        use crate::diff::{DiffFile, DiffLine, Hunk, LineType};
        use crate::grouper::GroupedChange;

        let change = GroupedChange { file: "src/foo.rs".to_string(), hunks: vec![0] };
        let group = SemanticGroup::new(
            "Foo group".to_string(),
            "desc".to_string(),
            vec![change],
        );
        let hunk = Hunk {
            header: String::new(),
            source_start: 1,
            target_start: 1,
            lines: vec![DiffLine {
                line_type: LineType::Context,
                content: "fn foo() {}".to_string(),
                inline_segments: None,
            }],
        };
        let file = DiffFile {
            source_file: "a/src/foo.rs".to_string(),
            target_file: "b/src/foo.rs".to_string(),
            is_rename: false,
            is_untracked: false,
            hunks: vec![hunk],
            added_count: 0,
            removed_count: 0,
        };
        let diff = DiffData { files: vec![file], binary_files: vec![] };
        let source = ReviewSource::BuiltIn;

        let how = build_review_prompt(ReviewSection::How, &group, &diff, &source);
        let why = build_review_prompt(ReviewSection::Why, &group, &diff, &source);

        for prompt in &[&how, &why] {
            assert!(prompt.contains("FILE: src/foo.rs"), "missing FILE marker: {}", prompt);
            assert!(prompt.contains("HUNK 0:"), "missing HUNK marker: {}", prompt);
        }
    }

    #[test]
    fn test_verdict_prompt_with_skill_content() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "SKILL RULE: always check null").unwrap();
        let source = ReviewSource::Skill {
            name: "pr-review".to_string(),
            path: tmp.path().to_path_buf(),
        };
        let group = make_group("Feature");
        let diff = empty_diff();
        let prompt = build_review_prompt(ReviewSection::Verdict, &group, &diff, &source);
        assert!(
            prompt.contains("SKILL RULE: always check null"),
            "VERDICT prompt must embed skill content"
        );
    }
}

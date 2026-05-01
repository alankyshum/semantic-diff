use crate::config::Config;
use crate::diff::DiffData;
use crate::grouper::llm::invoke_llm_text;
use crate::grouper::SemanticGroup;
use crate::llm_cli::LlmProvider;
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
             Return ONLY markdown, no code fences around the whole response.".to_string()
        }
        ReviewSection::What => {
            "Describe the BEHAVIORAL CHANGES as a markdown table with columns:\n\
             | Component | Before | After | Risk |\n\n\
             Focus on observable behavior differences, not code structure.\n\
             Risk is one of: none, low, medium, high.\n\
             Omit trivial changes (formatting, imports). Max 10 rows.\n\
             Return ONLY the markdown table.".to_string()
        }
        ReviewSection::How => {
            // Updated prompt per plan §3.7: always emit Before/After Mermaid, never SKIP
            "Produce a Mermaid diagram contrasting BEFORE and AFTER for these changes.\n\
             REQUIRED format:\n\n\
             ```mermaid\n\
             flowchart LR\n\
                 subgraph Before\n\
                     ...nodes/edges showing existing flow...\n\
                 end\n\
                 subgraph After\n\
                     ...nodes/edges showing new flow...\n\
                 end\n\
             ```\n\n\
             Rules:\n\
             - Always include both subgraphs.\n\
             - Label nodes with concepts (e.g. \"Client\", \"Auth Service\", \"Cache\"), not code symbols.\n\
             - Label edges with outcome-focused short text.\n\
             - If the change does NOT alter data/control flow (pure rename, dep bump, docs), still emit\n\
               both subgraphs but make them identical and add a node \"(unchanged)\".\n\
             - Do NOT use SKIP. Always return a mermaid block.\n\
             Return ONLY the mermaid code block.".to_string()
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
                 Return markdown with ## headings per issue found. Max 3 issues.\n\
                 Prefix each issue heading with a bug number like RV-1, RV-2, etc.\n\
                 Example: ## RV-1: Potential null dereference in auth handler\n\
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

/// Invoke the LLM for a single review section with a 120-second timeout.
pub async fn invoke_review_section(
    providers: &[LlmProvider],
    config: &Config,
    prompt: &str,
) -> Result<String, String> {
    use std::time::Duration;
    match tokio::time::timeout(
        Duration::from_secs(120),
        invoke_llm_text(providers, config, prompt),
    ).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("LLM timed out after 120s".to_string()),
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
    fn test_how_prompt_always_has_subgraph_before() {
        let group = make_group("Auth refactor");
        let diff = empty_diff();
        let source = ReviewSource::BuiltIn;
        let prompt = build_review_prompt(ReviewSection::How, &group, &diff, &source);
        assert!(
            prompt.contains("subgraph Before"),
            "HOW prompt must always contain 'subgraph Before', got: {}",
            prompt
        );
    }

    #[test]
    fn test_how_prompt_always_has_subgraph_after() {
        let group = make_group("DB migration");
        let diff = empty_diff();
        let source = ReviewSource::BuiltIn;
        let prompt = build_review_prompt(ReviewSection::How, &group, &diff, &source);
        assert!(
            prompt.contains("subgraph After"),
            "HOW prompt must always contain 'subgraph After', got: {}",
            prompt
        );
    }

    #[test]
    fn test_how_prompt_no_skip() {
        let group = make_group("Simple rename");
        let diff = empty_diff();
        let source = ReviewSource::BuiltIn;
        let prompt = build_review_prompt(ReviewSection::How, &group, &diff, &source);
        // Verify SKIP instruction is gone
        assert!(
            !prompt.contains("return exactly the text: SKIP"),
            "HOW prompt must not contain old SKIP instruction"
        );
    }

    #[test]
    fn test_how_prompt_flowchart_lr() {
        let group = make_group("Retry logic");
        let diff = empty_diff();
        let source = ReviewSource::BuiltIn;
        let prompt = build_review_prompt(ReviewSection::How, &group, &diff, &source);
        assert!(
            prompt.contains("flowchart LR"),
            "HOW prompt must specify flowchart LR"
        );
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

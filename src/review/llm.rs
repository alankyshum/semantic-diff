use crate::grouper::llm::{invoke_llm_text, LlmBackend};
use crate::grouper::SemanticGroup;
use crate::diff::DiffData;
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
            "If these changes involve complex control flow, state transitions, or\n\
             architectural patterns, return a mermaid diagram showing the key logic.\n\
             Use flowchart TD or sequenceDiagram as appropriate.\n\n\
             If the changes are straightforward (simple CRUD, config changes, etc.),\n\
             return exactly the text: SKIP\n\n\
             Return ONLY the mermaid code block OR the word SKIP.".to_string()
        }
        ReviewSection::Verdict => {
            let skill_preamble = match review_source {
                ReviewSource::Skill { path, .. } => {
                    match std::fs::read_to_string(path) {
                        Ok(content) => content,
                        Err(_) => String::new(),
                    }
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
    backend: LlmBackend,
    model: &str,
    prompt: &str,
) -> Result<String, String> {
    use std::time::Duration;
    match tokio::time::timeout(
        Duration::from_secs(120),
        invoke_llm_text(backend, model, prompt),
    ).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("LLM timed out after 120s".to_string()),
    }
}

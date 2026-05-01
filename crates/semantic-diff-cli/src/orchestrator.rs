use anyhow::Result;
use semantic_diff_core::{
    config::Config,
    diff,
    grouper,
    llm_cli::LlmProvider,
    result::ResultDocument,
    review::{self, ReviewSection},
};
use std::path::{Path, PathBuf};
use tokio::sync::broadcast;
use tokio::task::JoinSet;

use crate::input::ResolvedInput;

/// Options for the orchestrator run.
pub struct RunOpts {
    pub output_dir: PathBuf,
    pub no_llm: bool,
    pub llm_providers: Vec<LlmProvider>,
    pub notifier: broadcast::Sender<String>,
}

/// Handle returned after orchestrator completes.
pub struct ResultHandle {
    pub id: String,
    pub path: PathBuf,
}

/// Run the full pipeline: parse → group → review → write result.json.
pub async fn run(input: ResolvedInput, opts: RunOpts, config: &Config) -> Result<ResultHandle> {
    let (diff_data, _combined) = diff::parse_with_untracked_paths(&input.diff, &input.untracked);
    let path = opts.output_dir.join("result.json");

    if diff_data.files.is_empty() && diff_data.binary_files.is_empty() {
        tracing::info!("Empty diff; writing result with no groups");
        let mut doc = ResultDocument::new(&input.diff, &diff_data, input.source, input.title);
        doc.mark_complete();
        doc.write_atomic(&path)?;
        let id = doc.id.clone();
        return Ok(ResultHandle { id, path });
    }

    let mut doc = ResultDocument::new(&input.diff, &diff_data, input.source.clone(), input.title.clone());
    doc.write_atomic(&path)?;

    if opts.no_llm {
        // Single synthetic group for all files
        let source = review::detect_review_skill();
        let synthetic_group = grouper::SemanticGroup::new(
            "All changes".to_string(),
            "All files in the diff".to_string(),
            diff_data.files.iter().map(|f| grouper::GroupedChange {
                file: f.target_file.trim_start_matches("b/").to_string(),
                hunks: vec![],
            }).collect(),
        );
        doc.set_groups(vec![synthetic_group], &source);
        // Mark all sections as skipped
        for group in doc.groups.clone() {
            for sec in ReviewSection::all() {
                doc.set_section(&group.id, sec, Err("skipped (--no-llm)".to_string()));
            }
        }
        doc.mark_complete();
        doc.write_atomic(&path)?;
        let id = doc.id.clone();
        return Ok(ResultHandle { id, path });
    }

    // Group the diff
    let summaries = grouper::hunk_summaries(&diff_data);
    let groups = match grouper::llm::request_grouping_with_timeout(&opts.llm_providers, config, &summaries).await {
        Ok(mut g) => {
            grouper::normalize_hunk_indices(&mut g, &diff_data);
            g
        }
        Err(e) => {
            tracing::warn!("Grouping failed: {}; using single synthetic group", e);
            vec![grouper::SemanticGroup::new(
                "All changes".to_string(),
                "Grouping failed, showing all files".to_string(),
                diff_data.files.iter().map(|f| grouper::GroupedChange {
                    file: f.target_file.trim_start_matches("b/").to_string(),
                    hunks: vec![],
                }).collect(),
            )]
        }
    };

    let review_source = review::detect_review_skill();
    doc.set_groups(groups.clone(), &review_source);
    doc.write_atomic(&path)?;
    let _ = opts.notifier.send("groups_ready".to_string());

    // Spawn review tasks for all groups × all sections
    let mut tasks: JoinSet<(String, ReviewSection, Result<String, String>)> = JoinSet::new();

    for (group_idx, group) in groups.iter().enumerate() {
        let group_id = format!("g{}", group_idx);
        for section in ReviewSection::all() {
            let prompt = review::llm::build_review_prompt(section, group, &diff_data, &review_source);
            let providers = opts.llm_providers.clone();
            let config = config.clone();
            let gid = group_id.clone();
            tasks.spawn(async move {
                let result = review::llm::invoke_review_section(&providers, &config, &prompt).await;
                (gid, section, result)
            });
        }
    }

    // Collect results and write after each section
    while let Some(task_result) = tasks.join_next().await {
        match task_result {
            Ok((gid, section, result)) => {
                doc.set_section(&gid, section, result);
                doc.write_atomic(&path)?;
                let _ = opts.notifier.send(gid);
            }
            Err(e) => {
                tracing::warn!("Review task panicked: {}", e);
            }
        }
    }

    doc.mark_complete();
    doc.write_atomic(&path)?;
    let _ = opts.notifier.send("complete".to_string());

    let id = doc.id.clone();
    Ok(ResultHandle { id, path })
}

/// Determine the base results directory, defaulting to ~/.local/share/semantic-diff/results/
pub fn default_results_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("semantic-diff")
        .join("results")
}

/// Determine the output directory for a result, defaulting to ~/.local/share/semantic-diff/results/<id>/
pub fn default_output_dir(id: &str) -> PathBuf {
    default_results_dir().join(id)
}

/// List all result.json files under the given results directory.
pub fn list_results(results_dir: &Path) -> Vec<PathBuf> {
    let mut results = vec![];
    if let Ok(entries) = std::fs::read_dir(results_dir) {
        for entry in entries.flatten() {
            let p = entry.path().join("result.json");
            if p.exists() {
                results.push(p);
            }
        }
    }
    // Sort by modification time, most recent first
    results.sort_by(|a, b| {
        let ta = std::fs::metadata(a).and_then(|m| m.modified()).ok();
        let tb = std::fs::metadata(b).and_then(|m| m.modified()).ok();
        tb.cmp(&ta)
    });
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_output_dir_is_results_dir_joined_with_id() {
        assert_eq!(default_output_dir("abc123"), default_results_dir().join("abc123"));
    }
}

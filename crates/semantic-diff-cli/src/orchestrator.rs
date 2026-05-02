use anyhow::Result;
use chrono::Utc;
use semantic_diff_core::{
    config::Config,
    diff,
    grouper,
    llm_cli::LlmProvider,
    result::{
        LlmInfo, PerSectionTiming, ResultDocument, RunMetadata, SkillFileInfo, TokenUsage,
        SCHEMA_VERSION,
    },
    review::{self, ReviewSection, ReviewSource},
};
use std::path::{Path, PathBuf};
use std::time::Instant;
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
    let started_at = Utc::now();
    let run_start = Instant::now();
    let cli_argv: Vec<String> = std::env::args().collect();
    let working_dir = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let (diff_data, _combined) = diff::parse_with_untracked_paths(&input.diff, &input.untracked);
    let path = opts.output_dir.join("result.json");

    // Build a baseline RunMetadata. Filled in incrementally.
    let mut metadata = RunMetadata {
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: SCHEMA_VERSION,
        started_at,
        completed_at: None,
        cli_argv,
        working_dir,
        llm: None,
        timings: Vec::new(),
        total_duration_ms: None,
        skill_files: Vec::new(),
        tokens: None,
    };

    if diff_data.files.is_empty() && diff_data.binary_files.is_empty() {
        tracing::info!("Empty diff; writing result with no groups");
        let mut doc = ResultDocument::new(&input.diff, &diff_data, input.source, input.title)
            .with_repo(input.repo);
        metadata.completed_at = Some(Utc::now());
        metadata.total_duration_ms = Some(run_start.elapsed().as_millis() as u64);
        doc.set_metadata(metadata);
        doc.mark_complete();
        doc.write_atomic(&path)?;
        let id = doc.id.clone();
        return Ok(ResultHandle { id, path });
    }

    let mut doc = ResultDocument::new(&input.diff, &diff_data, input.source.clone(), input.title.clone())
        .with_repo(input.repo.clone());
    doc.set_metadata(metadata.clone());
    doc.write_atomic(&path)?;

    if opts.no_llm {
        let source = review::detect_review_skill();
        // Hash skill files for provenance (F6).
        metadata.skill_files = collect_skill_files(&source);
        let synthetic_group = grouper::SemanticGroup::new(
            "All changes".to_string(),
            "All files in the diff".to_string(),
            diff_data.files.iter().map(|f| grouper::GroupedChange {
                file: f.target_file.trim_start_matches("b/").to_string(),
                hunks: vec![],
            }).collect(),
        );
        doc.set_groups(vec![synthetic_group], &source);
        for group in doc.groups.clone() {
            for sec in ReviewSection::all() {
                doc.set_section(&group.id, sec, Err("skipped (--no-llm)".to_string()));
            }
        }
        metadata.completed_at = Some(Utc::now());
        metadata.total_duration_ms = Some(run_start.elapsed().as_millis() as u64);
        doc.set_metadata(metadata);
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
    metadata.skill_files = collect_skill_files(&review_source);

    // Capture LLM provenance from the first preferred provider (best-effort).
    metadata.llm = capture_llm_info(&opts.llm_providers, config).await;
    doc.set_metadata(metadata.clone());

    doc.set_groups(groups.clone(), &review_source);
    doc.write_atomic(&path)?;
    let _ = opts.notifier.send("groups_ready".to_string());

    // Spawn review tasks for all groups × all sections.
    let mut tasks: JoinSet<(String, ReviewSection, Result<String, String>, u64)> = JoinSet::new();

    for (group_idx, group) in groups.iter().enumerate() {
        let group_id = format!("g{}", group_idx);
        for section in ReviewSection::all() {
            let prompt = review::llm::build_review_prompt(section, group, &diff_data, &review_source);
            let providers = opts.llm_providers.clone();
            let config = config.clone();
            let gid = group_id.clone();
            tasks.spawn(async move {
                let t0 = Instant::now();
                let result = review::llm::invoke_review_section(&providers, &config, &prompt).await;
                let dur = t0.elapsed().as_millis() as u64;
                (gid, section, result, dur)
            });
        }
    }

    while let Some(task_result) = tasks.join_next().await {
        match task_result {
            Ok((gid, section, result, dur)) => {
                doc.set_section(&gid, section, result);

                // Append timing entry (F6). cache_hit=false; orchestrator does
                // not currently observe disk-cache hits — TODO: thread signal
                // through invoke_review_section.
                metadata.timings.push(PerSectionTiming {
                    group_id: gid.clone(),
                    section: section.label().to_string(),
                    duration_ms: dur,
                    cache_hit: false,
                });
                doc.set_metadata(metadata.clone());

                doc.write_atomic(&path)?;
                let _ = opts.notifier.send(gid);
            }
            Err(e) => {
                tracing::warn!("Review task panicked: {}", e);
            }
        }
    }

    metadata.completed_at = Some(Utc::now());
    metadata.total_duration_ms = Some(run_start.elapsed().as_millis() as u64);
    // TODO(F20): aggregate from per-section invocations once `invoke_review_section`
    // returns the full `LlmInvocation` instead of just the response text.
    doc.set_metadata(metadata);
    doc.mark_complete();
    doc.write_atomic(&path)?;
    let _ = opts.notifier.send("complete".to_string());

    let id = doc.id.clone();
    Ok(ResultHandle { id, path })
}

/// Hash any skill files referenced by the review source.
fn collect_skill_files(source: &ReviewSource) -> Vec<SkillFileInfo> {
    match source {
        ReviewSource::BuiltIn => vec![],
        ReviewSource::Skill { name, path } => {
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(_) => return vec![],
            };
            let hash = blake3::hash(&bytes).to_hex().to_string();
            vec![SkillFileInfo {
                name: name.clone(),
                path: path.to_string_lossy().to_string(),
                hash_blake3: hash,
            }]
        }
    }
}

/// Best-effort LLM provenance capture for the first runnable provider in the
/// preference list. `cli_path` via `which`; `cli_version` via `<cli> --version`
/// with a 5s timeout.
async fn capture_llm_info(providers: &[LlmProvider], config: &Config) -> Option<LlmInfo> {
    use tokio::process::Command;
    use tokio::time::{Duration, timeout};

    if providers.is_empty() {
        return None;
    }

    let probe = |provider: LlmProvider| {
        let (cli_name, model) = match provider {
            LlmProvider::Claude => ("claude", Some(config.claude_model.clone())),
            LlmProvider::Copilot => ("copilot", Some(config.copilot_model.clone())),
            LlmProvider::Cursor => ("cursor-agent", None),
        };
        (provider, cli_name, model)
    };

    // First pass: pick the first provider with a binary on PATH.
    for &provider in providers {
        let (provider, cli_name, model) = probe(provider);
        if let Ok(path) = which::which(cli_name) {
            let cli_path = Some(path.to_string_lossy().to_string());
            let cli_version = {
                let fut = async {
                    let out = Command::new(cli_name).arg("--version").output().await.ok()?;
                    if !out.status.success() {
                        return None;
                    }
                    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if s.is_empty() { None } else { Some(s) }
                };
                timeout(Duration::from_secs(5), fut).await.ok().flatten()
            };
            return Some(LlmInfo {
                provider: provider.to_string(),
                model,
                cli_path,
                cli_version,
            });
        }
    }

    // Fallback: report the first preference even though no binary was found.
    let (provider, _cli_name, model) = probe(providers[0]);
    Some(LlmInfo {
        provider: provider.to_string(),
        model,
        cli_path: None,
        cli_version: None,
    })
}

// Currently unused but kept for future token aggregation.
#[allow(dead_code)]
fn aggregate_tokens(invocations: &[(Option<u64>, Option<u64>, Option<f64>)]) -> Option<TokenUsage> {
    let mut input_total: u64 = 0;
    let mut output_total: u64 = 0;
    let mut cost_total: f64 = 0.0;
    let mut any = false;
    for (i, o, c) in invocations {
        if let Some(v) = i { input_total += v; any = true; }
        if let Some(v) = o { output_total += v; any = true; }
        if let Some(v) = c { cost_total += v; any = true; }
    }
    if !any { return None; }
    Some(TokenUsage {
        input_tokens: if input_total > 0 { Some(input_total) } else { None },
        output_tokens: if output_total > 0 { Some(output_total) } else { None },
        cost_usd: if cost_total > 0.0 { Some(cost_total) } else { None },
    })
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

    #[test]
    fn aggregate_tokens_returns_none_for_all_none() {
        let xs = vec![(None, None, None), (None, None, None)];
        assert!(aggregate_tokens(&xs).is_none());
    }

    #[test]
    fn aggregate_tokens_sums_present_values() {
        let xs = vec![
            (Some(100), Some(50), Some(0.001)),
            (Some(200), None, Some(0.002)),
            (None, Some(75), None),
        ];
        let agg = aggregate_tokens(&xs).expect("some");
        assert_eq!(agg.input_tokens, Some(300));
        assert_eq!(agg.output_tokens, Some(125));
        assert!(agg.cost_usd.unwrap() - 0.003 < 1e-9);
    }
}

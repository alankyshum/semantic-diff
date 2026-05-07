use anyhow::Result;
use chrono::Utc;
use semantic_diff_core::{
    config::Config,
    diff,
    grouper,
    llm_cli::{LlmInvocation, LlmProvider},
    result::{
        LlmInfo, PerSectionTiming, ResultDocument, RunMetadata, SkillFileInfo, TokenUsage,
        SCHEMA_VERSION,
    },
    review::{self, CachedSection, ReviewSection, ReviewSource},
};
use std::collections::HashMap;
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
    /// When true, skip the on-disk review cache (force re-run all sections
    /// and overwrite any existing entry).
    pub no_cache: bool,
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

    // Per-group content hashes (string form, computed by `set_groups`). These
    // key the on-disk review cache so that re-running on identical base/target
    // commits replays cached LLM output instead of re-invoking providers.
    let group_hashes: Vec<String> = doc
        .groups
        .iter()
        .map(|g| g.content_hash.clone())
        .collect();

    // For each group, load any cached sections from disk (unless --no-cache).
    // Sections that hit the cache are written immediately to `doc` and skipped
    // from the spawn loop; the rest go to the LLM as usual.
    //
    // The cache hash is `GroupEntry.content_hash` (blake3-of-label+files+hunks)
    // and the entry is invalidated automatically when the review source or
    // skill body changes — see `review::load_sections_from_disk`.
    let mut cached_per_group: Vec<HashMap<String, CachedSection>> =
        vec![HashMap::new(); groups.len()];
    if !opts.no_cache {
        for (i, hash) in group_hashes.iter().enumerate() {
            if let Some(entries) = review::load_sections_from_disk(hash, &review_source) {
                cached_per_group[i] = entries;
            }
        }
    } else {
        // Force-refresh: drop any prior entries so a partial-write can't
        // resurrect stale content if the new run aborts early.
        for hash in &group_hashes {
            review::delete_review_from_disk(hash);
        }
    }

    // Spawn review tasks for all groups × all sections.
    let mut tasks: JoinSet<(String, ReviewSection, Result<LlmInvocation, String>, u64)> =
        JoinSet::new();

    for (group_idx, group) in groups.iter().enumerate() {
        let group_id = format!("g{}", group_idx);
        let cached = &cached_per_group[group_idx];
        for section in ReviewSection::all() {
            // Cache hit: replay the stored content synchronously.
            if let Some(entry) = cached.get(section.label()) {
                let (set_result, dur_ms) = match entry {
                    CachedSection::Ready(text) => (Ok(text.clone()), 0u64),
                    CachedSection::Skipped => (Err("skipped".to_string()), 0u64),
                };
                doc.set_section(&group_id, section, set_result);
                metadata.timings.push(PerSectionTiming {
                    group_id: group_id.clone(),
                    section: section.label().to_string(),
                    duration_ms: dur_ms,
                    cache_hit: true,
                    input_tokens: None,
                    output_tokens: None,
                    cost_usd: None,
                });
                continue;
            }

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

    // Persist the doc with any cache-hit sections already filled in, and
    // notify the SPA so cached content is visible immediately.
    doc.set_metadata(metadata.clone());
    doc.write_atomic(&path)?;
    if cached_per_group.iter().any(|m| !m.is_empty()) {
        let _ = opts.notifier.send("cached_sections_ready".to_string());
    }

    // Accumulator for per-section token/cost stats (F20).
    let mut token_accum: Vec<(Option<u64>, Option<u64>, Option<f64>)> = Vec::new();

    // Per-group section results destined for the on-disk cache. Seeded with
    // any entries already loaded from disk so a partial re-run (e.g. only
    // VERDICT was missing) still ends up with a complete cache file.
    let mut to_persist: Vec<HashMap<String, CachedSection>> = cached_per_group.clone();
    // Per-group "any section errored" flag — if set, we will NOT overwrite
    // the on-disk cache for that group (preserves prior good content).
    let mut group_had_error: Vec<bool> = vec![false; groups.len()];

    while let Some(task_result) = tasks.join_next().await {
        match task_result {
            Ok((gid, section, result, dur)) => {
                // Translate Result<LlmInvocation, String> into the
                // Result<String, String> that `set_section` expects, while
                // capturing token usage on the Ok path.
                let (set_result, tokens) = match result {
                    Ok(invocation) => {
                        let tokens =
                            (invocation.input_tokens, invocation.output_tokens, invocation.cost_usd);
                        // Run deterministic mermaid lint on HOW responses
                        // before persisting. Bad mermaid would otherwise reach
                        // the renderer and fail at view-time with cryptic
                        // YAML/parse errors.
                        let text = if matches!(section, ReviewSection::How) {
                            run_mermaid_lint_on_how(&gid, &invocation.text)
                        } else {
                            invocation.text
                        };
                        (Ok(text), Some(tokens))
                    }
                    Err(e) => (Err(e), None),
                };
                doc.set_section(&gid, section, set_result.clone());

                // Track for on-disk cache persistence below.
                if let Some(idx) = gid
                    .strip_prefix('g')
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    if idx < to_persist.len() {
                        match &set_result {
                            Ok(text) => {
                                to_persist[idx].insert(
                                    section.label().to_string(),
                                    CachedSection::Ready(text.clone()),
                                );
                            }
                            Err(_) => {
                                group_had_error[idx] = true;
                            }
                        }
                    }
                }

                // Append timing entry (F6/F20). cache_hit=false for live LLM
                // invocations; cached entries set this above before spawning.
                let (in_tok, out_tok, cost) = tokens.unwrap_or((None, None, None));
                metadata.timings.push(PerSectionTiming {
                    group_id: gid.clone(),
                    section: section.label().to_string(),
                    duration_ms: dur,
                    cache_hit: false,
                    input_tokens: in_tok,
                    output_tokens: out_tok,
                    cost_usd: cost,
                });
                if tokens.is_some() {
                    token_accum.push((in_tok, out_tok, cost));
                }
                doc.set_metadata(metadata.clone());

                doc.write_atomic(&path)?;
                let _ = opts.notifier.send(gid);
            }
            Err(e) => {
                tracing::warn!("Review task panicked: {}", e);
            }
        }
    }

    metadata.tokens = aggregate_tokens(&token_accum);
    metadata.completed_at = Some(Utc::now());
    metadata.total_duration_ms = Some(run_start.elapsed().as_millis() as u64);
    doc.set_metadata(metadata);
    doc.mark_complete();
    doc.write_atomic(&path)?;
    let _ = opts.notifier.send("complete".to_string());

    // Persist completed reviews to the on-disk cache. We only write entries
    // for groups whose every section reached a non-error terminal state, so
    // a transient provider failure won't poison the cache. If --no-cache was
    // set, we still write so subsequent runs benefit from this one's output.
    let expected = ReviewSection::all().len();
    for (idx, sections) in to_persist.iter().enumerate() {
        if group_had_error[idx] {
            continue;
        }
        if sections.len() < expected {
            continue;
        }
        if let Some(hash) = group_hashes.get(idx) {
            review::save_sections_to_disk(hash, &review_source, sections);
        }
    }

    let id = doc.id.clone();
    Ok(ResultHandle { id, path })
}

/// Collect skill file provenance for the result document. Delegates to
/// `review::skill_fingerprint` so this metadata stays bit-identical to what
/// the per-section disk cache stores for invalidation — previously these
/// two paths each hashed the file independently, which let them drift.
fn collect_skill_files(source: &ReviewSource) -> Vec<SkillFileInfo> {
    match review::skill_fingerprint(source) {
        Some(fp) => vec![SkillFileInfo {
            name: fp.name,
            path: fp.path,
            hash_blake3: fp.hash_blake3,
        }],
        None => vec![],
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

// Aggregate per-section token/cost into a single `TokenUsage` rollup (F20).
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

/// Run the deterministic mermaid linter on every fenced ```mermaid block in
/// a HOW-section response. Modifications are auto-applied; rejections (non-
/// mermaid prose dressed as mermaid) are downgraded to a leading warning
/// comment so the renderer's own fallback path can render the prose as
/// markdown without choking on YAML errors.
fn run_mermaid_lint_on_how(group_id: &str, raw: &str) -> String {
    // Defensive: a bug in the linter (e.g. byte-vs-char index slicing) must
    // not nuke an entire orchestrator run after we've already paid for the
    // LLM calls. catch_unwind isolates the panic and falls back to the raw
    // LLM text so the section still saves to the result document and the
    // on-disk review cache.
    let raw_owned = raw.to_string();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        review::lint_markdown_mermaid(&raw_owned)
    }));
    let (rewritten, results) = match result {
        Ok(v) => v,
        Err(_) => {
            tracing::error!(
                group = %group_id,
                "mermaid lint panicked; falling back to raw LLM text"
            );
            return raw_owned;
        }
    };
    for (i, r) in results.iter().enumerate() {
        if let Some(err) = &r.error {
            tracing::warn!(group=%group_id, block=%i, "mermaid lint rejected block: {err}");
        } else if r.modified {
            tracing::info!(
                group=%group_id, block=%i, fixes=?r.warnings,
                "mermaid lint auto-fixed block",
            );
        }
    }
    rewritten
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

mod app;
mod cache;
mod cli;
mod config;
mod diff;
mod event;
mod grouper;
mod highlight;
mod preview;
mod review;
mod signal;
mod theme;
mod ui;

use anyhow::Result;
use app::{App, Command, Message};
use clap::Parser;
use tokio::sync::mpsc;

/// Spawn review tasks for a SpawnReviewBatch command.
fn spawn_review_batch(
    cmd: Command,
    tx: &mpsc::Sender<Message>,
    app: &mut App,
) {
    if let Command::SpawnReviewBatch(cmds) = cmd {
        for cmd in cmds {
            if let Command::SpawnReviewSection { backend, model, section, prompt, group_content_hash } = cmd {
                let tx2 = tx.clone();
                let handle = tokio::spawn(async move {
                    let result = crate::review::llm::invoke_review_section(
                        backend, &model, &prompt,
                    ).await;
                    let _ = tx2.send(Message::ReviewSectionReady(group_content_hash, section, result)).await;
                });
                app.review_handles.insert((group_content_hash, section), handle);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Panic hook FIRST (ROB-01) — restore terminal and clean up PID file before printing panic info
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        signal::remove_pid_file();
        ratatui::restore();
        original_hook(info);
    }));

    // 2. Init logging to file (secure directory, not world-writable /tmp/)
    let log_path = signal::log_file_path();
    // Ensure parent directory exists (write_pid_file also does this, but log init comes first)
    if let Some(parent) = log_path.parent() {
        if !parent.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                let _ = std::fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(parent);
            }
            #[cfg(not(unix))]
            {
                let _ = std::fs::create_dir_all(parent);
            }
        }
    }
    let log_file = {
        let mut opts = std::fs::OpenOptions::new();
        opts.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        opts.open(&log_path)?
    };
    tracing_subscriber::fmt()
        .with_env_filter("semantic_diff=debug")
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    tracing::info!("semantic-diff starting");

    // 2b. Parse CLI arguments
    let cli = cli::Cli::parse();
    let git_diff_args = cli.git_diff_args();
    tracing::info!(?git_diff_args, "Git diff args");

    // 2c. Load config (creates default if missing), CLI --theme overrides config
    let mut config = config::load();
    if cli.theme.is_some() {
        config.theme_mode = cli.theme_mode();
    }
    tracing::info!(?config, "Loaded config");

    // 3. Run git diff with user-specified args (or default: unstaged changes)
    let output = std::process::Command::new("git")
        .args(&git_diff_args)
        .output()?;

    let raw_git_diff = String::from_utf8_lossy(&output.stdout);

    // 4. Parse diff, including untracked files
    let (diff_data, raw_diff) = diff::parse_with_untracked(&raw_git_diff);

    if diff_data.files.is_empty() && diff_data.binary_files.is_empty() {
        eprintln!("No changes detected");
        return Ok(());
    }
    tracing::info!(
        files = diff_data.files.len(),
        binary = diff_data.binary_files.len(),
        "Parsed diff"
    );

    // 5. Write PID file for external signal senders
    signal::write_pid_file()?;

    // 6. Set up async channel and app
    let (tx, mut rx) = mpsc::channel::<Message>(32);
    let mut app = App::new(diff_data, &config, git_diff_args);
    app.event_tx = Some(tx.clone());

    // 7. Spawn the async event loop (terminal events + SIGUSR1)
    tokio::spawn(event::event_loop(tx.clone()));

    // 7b. Trigger initial semantic grouping — check cache first, then LLM
    let diff_hash = cache::diff_hash(&raw_diff);
    if let Some(cached_groups) = cache::load(diff_hash) {
        let mut cached_groups = cached_groups;
        grouper::normalize_hunk_indices(&mut cached_groups, &app.diff_data);
        app.semantic_groups = Some(cached_groups);
        app.grouping_status = grouper::GroupingStatus::Done;
        tracing::info!("Using cached grouping");
        // Initialize incremental state from cache
        app.previous_head = cache::get_head_commit();
        app.previous_file_hashes = grouper::compute_all_file_hashes(&app.diff_data);
        // Spawn reviews for all groups
        if let Some(cmd) = app.spawn_all_reviews() {
            spawn_review_batch(cmd, &tx, &mut app);
        }
    } else if let Some(backend) = app.llm_backend {
        // Try incremental cache: same HEAD, previous groups + file hashes stored
        let current_head = cache::get_head_commit();
        let mut used_incremental = false;

        if let Some(ref head) = current_head {
            if let Some((prev_groups, prev_file_hashes)) = cache::load_incremental(head) {
                let new_hashes = grouper::compute_all_file_hashes(&app.diff_data);
                let delta = grouper::compute_diff_delta(&new_hashes, &prev_file_hashes);

                if !delta.has_changes() {
                    // Diff hasn't changed since last save — use cached groups
                    let mut groups = prev_groups;
                    grouper::normalize_hunk_indices(&mut groups, &app.diff_data);
                    app.semantic_groups = Some(groups);
                    app.grouping_status = grouper::GroupingStatus::Done;
                    app.previous_head = Some(head.clone());
                    app.previous_file_hashes = new_hashes;
                    tracing::info!("Incremental cache: no changes since last save");
                    if let Some(cmd) = app.spawn_all_reviews() {
                        spawn_review_batch(cmd, &tx, &mut app);
                    }
                    used_incremental = true;
                } else if delta.is_only_removals() {
                    // Only files removed — prune locally
                    let mut groups = prev_groups;
                    grouper::remove_files_from_groups(&mut groups, &delta.removed_files);
                    grouper::normalize_hunk_indices(&mut groups, &app.diff_data);
                    app.semantic_groups = Some(groups);
                    app.grouping_status = grouper::GroupingStatus::Done;
                    app.previous_head = Some(head.clone());
                    app.previous_file_hashes = new_hashes.clone();
                    cache::save_with_state(diff_hash, app.semantic_groups.as_ref().unwrap(), Some(head), &new_hashes);
                    tracing::info!("Incremental cache: pruned removed files");
                    if let Some(cmd) = app.spawn_all_reviews() {
                        spawn_review_batch(cmd, &tx, &mut app);
                    }
                    used_incremental = true;
                } else {
                    // New/modified files — spawn incremental grouping
                    let summaries = grouper::incremental_hunk_summaries(&app.diff_data, &delta, &prev_groups);
                    let model = app.llm_model.clone();
                    let head_clone = head.clone();
                    let tx2 = tx.clone();
                    tracing::info!(
                        new = delta.new_files.len(),
                        modified = delta.modified_files.len(),
                        removed = delta.removed_files.len(),
                        "Incremental grouping on startup"
                    );
                    // Store previous groups so IncrementalGroupingComplete can merge
                    app.semantic_groups = Some(prev_groups);
                    app.previous_head = Some(head.clone());
                    app.previous_file_hashes = prev_file_hashes;
                    let handle = tokio::spawn(async move {
                        match grouper::llm::request_incremental_grouping(backend, &model, &summaries).await {
                            Ok(groups) => {
                                let _ = tx2.send(Message::IncrementalGroupingComplete(
                                    groups, delta, new_hashes, diff_hash, head_clone,
                                )).await;
                            }
                            Err(e) => {
                                let _ = tx2.send(Message::GroupingFailed(e.to_string())).await;
                            }
                        }
                    });
                    app.grouping_handle = Some(handle);
                    app.grouping_status = grouper::GroupingStatus::Loading;
                    used_incremental = true;
                }
            }
        }

        if !used_incremental {
            // Full re-group: no incremental state available
            let summaries = grouper::hunk_summaries(&app.diff_data);
            let model = app.llm_model.clone();
            let tx2 = tx.clone();
            let handle = tokio::spawn(async move {
                match grouper::llm::request_grouping_with_timeout(backend, &model, &summaries).await {
                    Ok(groups) => {
                        let _ = tx2.send(Message::GroupingComplete(groups, diff_hash)).await;
                    }
                    Err(e) => {
                        let _ = tx2.send(Message::GroupingFailed(e.to_string())).await;
                    }
                }
            });
            app.grouping_handle = Some(handle);
            app.grouping_status = grouper::GroupingStatus::Loading;
        }
    }

    // 8. Init terminal and enter main loop
    let mut terminal = ratatui::init();

    let mut had_images_last_frame = false;

    loop {
        let mut pending_images = Vec::new();
        terminal.draw(|f| {
            app.ui_state.viewport_height = f.area().height.saturating_sub(1);
            pending_images = app.view(f);
        })?;

        let has_images = !pending_images.is_empty();

        if let preview::mermaid::ImageSupport::Supported(protocol) = &app.image_support {
            if has_images {
                ui::preview_view::flush_images(&pending_images, *protocol);
            } else if had_images_last_frame {
                // Switched away from preview or to a non-image file — clear stale images
                // and immediately redraw so the screen doesn't flash blank.
                ui::preview_view::clear_stale_images(*protocol, &mut terminal);
                terminal.draw(|f| {
                    app.ui_state.viewport_height = f.area().height.saturating_sub(1);
                    app.view(f);
                })?;
            }
        }

        had_images_last_frame = has_images;

        if let Some(msg) = rx.recv().await {
            if let Some(cmd) = app.update(msg) {
                match cmd {
                    Command::SpawnDiffParse { git_diff_args } => {
                        let tx2 = tx.clone();
                        tokio::spawn(async move {
                            let output = tokio::process::Command::new("git")
                                .args(&git_diff_args)
                                .output()
                                .await;
                            if let Ok(output) = output {
                                let raw_git = String::from_utf8_lossy(&output.stdout).to_string();
                                let untracked = crate::diff::untracked::discover_untracked_files_async().await;
                                let (data, combined) = crate::diff::parse_with_untracked_paths(&raw_git, &untracked);
                                let _ = tx2.send(Message::DiffParsed(data, combined)).await;
                            }
                        });
                    }
                    Command::SpawnGrouping { backend, model, summaries, diff_hash, .. } => {
                        let tx2 = tx.clone();
                        let handle = tokio::spawn(async move {
                            match crate::grouper::llm::request_grouping_with_timeout(
                                backend,
                                &model,
                                &summaries,
                            )
                            .await
                            {
                                Ok(groups) => {
                                    // Don't save here — GroupingComplete handler saves with full incremental state
                                    let _ = tx2.send(Message::GroupingComplete(groups, diff_hash)).await;
                                }
                                Err(e) => {
                                    let _ =
                                        tx2.send(Message::GroupingFailed(e.to_string())).await;
                                }
                            }
                        });
                        app.grouping_handle = Some(handle);
                    }
                    Command::SpawnIncrementalGrouping {
                        backend,
                        model,
                        summaries,
                        diff_hash,
                        head_commit,
                        file_hashes,
                        delta,
                    } => {
                        let tx2 = tx.clone();
                        let handle = tokio::spawn(async move {
                            match crate::grouper::llm::request_incremental_grouping(
                                backend,
                                &model,
                                &summaries,
                            )
                            .await
                            {
                                Ok(groups) => {
                                    let _ = tx2
                                        .send(Message::IncrementalGroupingComplete(
                                            groups,
                                            delta,
                                            file_hashes,
                                            diff_hash,
                                            head_commit,
                                        ))
                                        .await;
                                }
                                Err(e) => {
                                    let _ =
                                        tx2.send(Message::GroupingFailed(e.to_string())).await;
                                }
                            }
                        });
                        app.grouping_handle = Some(handle);
                    }
                    Command::SpawnReviewBatch(_) => {
                        spawn_review_batch(cmd, &tx, &mut app);
                    }
                    Command::SpawnReviewSection { .. } => {
                        // Individual spawn — handled within SpawnReviewBatch above
                        // This variant exists for type completeness but won't be sent alone in Phase 1
                    }
                    Command::CancelReview(hash) => {
                        let keys: Vec<_> = app.review_handles.keys()
                            .filter(|(h, _)| *h == hash)
                            .cloned()
                            .collect();
                        for key in keys {
                            if let Some(handle) = app.review_handles.remove(&key) {
                                handle.abort();
                            }
                        }
                    }
                    Command::Quit => break,
                }
            }
        } else {
            break; // channel closed
        }
    }

    // 9. Cleanup: remove PID file and restore terminal
    signal::remove_pid_file();
    ratatui::restore();

    Ok(())
}

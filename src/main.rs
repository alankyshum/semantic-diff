mod app;
mod diff;
mod event;
mod grouper;
mod highlight;
mod signal;
mod ui;

use anyhow::Result;
use app::{App, Command, Message};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Panic hook FIRST (ROB-01) — restore terminal and clean up PID file before printing panic info
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        signal::remove_pid_file();
        let _ = ratatui::restore();
        original_hook(info);
    }));

    // 2. Init logging to file
    let log_file = std::fs::File::create("/tmp/semantic-diff.log")?;
    tracing_subscriber::fmt()
        .with_env_filter("semantic_diff=debug")
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    tracing::info!("semantic-diff starting");

    // 3. Run git diff HEAD -M and capture output
    let output = std::process::Command::new("git")
        .args(["diff", "HEAD", "-M"])
        .output()?;

    let raw_diff = String::from_utf8_lossy(&output.stdout);

    if raw_diff.is_empty() {
        eprintln!("No changes detected");
        return Ok(());
    }

    // 4. Parse diff
    let diff_data = diff::parse(&raw_diff);
    tracing::info!(
        files = diff_data.files.len(),
        binary = diff_data.binary_files.len(),
        "Parsed diff"
    );

    // 5. Write PID file for external signal senders
    signal::write_pid_file()?;

    // 6. Set up async channel and app
    let (tx, mut rx) = mpsc::channel::<Message>(32);
    let mut app = App::new(diff_data);
    app.event_tx = Some(tx.clone());

    // 7. Spawn the async event loop (terminal events + SIGUSR1)
    tokio::spawn(event::event_loop(tx.clone()));

    // 7b. Trigger initial semantic grouping if claude is available
    if app.claude_available {
        let summaries = grouper::file_summaries(&app.diff_data);
        let tx2 = tx.clone();
        let handle = tokio::spawn(async move {
            match grouper::llm::request_grouping_with_timeout(&summaries).await {
                Ok(groups) => {
                    let _ = tx2.send(Message::GroupingComplete(groups)).await;
                }
                Err(e) => {
                    let _ = tx2.send(Message::GroupingFailed(e.to_string())).await;
                }
            }
        });
        app.grouping_handle = Some(handle);
        app.grouping_status = grouper::GroupingStatus::Loading;
    }

    // 8. Init terminal and enter main loop
    let mut terminal = ratatui::init();

    loop {
        terminal.draw(|f| {
            app.ui_state.viewport_height = f.area().height.saturating_sub(1);
            app.view(f);
        })?;

        if let Some(msg) = rx.recv().await {
            if let Some(cmd) = app.update(msg) {
                match cmd {
                    Command::SpawnDiffParse => {
                        let tx2 = tx.clone();
                        tokio::spawn(async move {
                            let output = tokio::process::Command::new("git")
                                .args(["diff", "HEAD", "-M"])
                                .output()
                                .await;
                            if let Ok(output) = output {
                                let raw = String::from_utf8_lossy(&output.stdout).to_string();
                                let data = crate::diff::parse(&raw);
                                let _ = tx2.send(Message::DiffParsed(data)).await;
                            }
                        });
                    }
                    Command::SpawnGrouping(summaries) => {
                        let tx2 = tx.clone();
                        let handle = tokio::spawn(async move {
                            match crate::grouper::llm::request_grouping_with_timeout(&summaries)
                                .await
                            {
                                Ok(groups) => {
                                    let _ = tx2.send(Message::GroupingComplete(groups)).await;
                                }
                                Err(e) => {
                                    let _ =
                                        tx2.send(Message::GroupingFailed(e.to_string())).await;
                                }
                            }
                        });
                        app.grouping_handle = Some(handle);
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

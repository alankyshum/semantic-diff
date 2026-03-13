mod app;
mod diff;
mod event;
mod highlight;
mod ui;

use anyhow::Result;
use app::App;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Panic hook FIRST (ROB-01) — restore terminal before printing panic info
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
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

    // 5. Init terminal and run app
    let mut terminal = ratatui::init();
    let result = App::new(diff_data).run(&mut terminal);

    // 6. Always restore terminal
    ratatui::restore();
    result
}

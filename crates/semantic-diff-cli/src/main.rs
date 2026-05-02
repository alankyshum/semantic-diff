use semantic_diff_cli::{cli, input, orchestrator, server};

use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing to stderr (no log file needed for web version)
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "semantic_diff_cli=info,semantic_diff_core=info".to_string()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = cli::Cli::parse_smart();

    // History mode: browse past saved reviews from the global results dir
    if cli.history {
        if !cli.git_args.is_empty() {
            anyhow::bail!("--history takes no positional args");
        }
        let results_dir = orchestrator::default_results_dir();
        let _ = std::fs::create_dir_all(&results_dir);

        let is_empty = std::fs::read_dir(&results_dir)
            .map(|mut it| it.next().is_none())
            .unwrap_or(true);

        let (tx, _rx) = broadcast::channel::<String>(32);
        let state = server::AppState {
            results_dir,
            notifier: tx,
            config: None,
            preregistered_notifiers: HashMap::new(),
        };
        let addr = server::start(state, cli.port).await?;
        let url = format!("http://{}:{}/", addr.ip(), addr.port());
        eprintln!("Browsing saved reviews at {url}");
        if is_empty {
            eprintln!("(no saved reviews yet — run `semantic-diff` on a diff first)");
        }
        if !cli.no_open {
            let _ = open::that(&url);
        }
        tokio::signal::ctrl_c().await?;
        return Ok(());
    }

    // Replay mode: serve an existing result.json without re-running review
    if let Some(result_path) = &cli.result {
        let content = std::fs::read_to_string(result_path)?;
        let doc: serde_json::Value = serde_json::from_str(&content)?;
        let id = doc["id"].as_str().unwrap_or("unknown").to_string();

        let results_dir = result_path.parent()
            .unwrap_or(std::path::Path::new("."))
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();

        let (tx, _rx) = broadcast::channel::<String>(32);
        let state = server::AppState {
            results_dir,
            notifier: tx,
            config: None,
            preregistered_notifiers: HashMap::new(),
        };
        let addr = server::start(state, cli.port).await?;
        let url = format!("http://{}:{}/r/{}", addr.ip(), addr.port(), id);
        eprintln!("Serving result at {}", url);
        if !cli.no_open {
            let _ = open::that(&url);
        }
        // Wait until Ctrl+C
        tokio::signal::ctrl_c().await?;
        return Ok(());
    }

    // Load config
    let mut config = semantic_diff_core::config::load();

    // Resolve input (determine where the diff comes from)
    let use_stdin = cli.use_stdin();
    let input = input::resolve_input(
        cli.diff.as_deref(),
        use_stdin,
        cli.pr.as_deref(),
        &cli.git_args,
        cli.title.as_deref(),
    ).await?;

    if input.diff.is_empty() || (input.diff.trim().is_empty() && input.untracked.is_empty()) {
        eprintln!("No changes detected");
        return Ok(());
    }

    // Determine output directory — we need the ID from the parsed diff first.
    // We'll use a temporary placeholder and compute it after parsing.
    // For now: compute preliminary output dir from hash.
    let preliminary_id = {
        let mut h = blake3::Hasher::new();
        h.update(input.diff.as_bytes());
        h.update(input.title.as_bytes());
        let hash = h.finalize();
        hash.to_hex()[..8].to_string()
    };

    let output_dir = cli.output.clone()
        .unwrap_or_else(|| orchestrator::default_output_dir(&preliminary_id));
    std::fs::create_dir_all(&output_dir)?;

    // Set up SSE notifier channel
    let (tx, _rx) = broadcast::channel::<String>(64);

    // Boot server before orchestration so the user can open the browser
    // and see the loading state as sections stream in.
    let results_dir = output_dir.parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();

    let state = server::AppState {
        results_dir,
        notifier: tx.clone(),
        config: None,
        preregistered_notifiers: {
            let mut m = HashMap::new();
            m.insert(preliminary_id.clone(), tx.clone());
            m
        },
    };
    let addr = server::start(state, cli.port).await?;
    let url = format!("http://{}:{}/r/{}", addr.ip(), addr.port(), preliminary_id);
    eprintln!("semantic-diff running at {}", url);
    eprintln!("Press Ctrl+C to stop.");

    if !cli.no_open {
        let _ = open::that(&url);
    }

    // Apply CLI override (highest precedence) for llm providers, unless --no-llm.
    // The flag has a default so we always pass Some(...); apply_overrides bakes precedence:
    // CLI > env (SEMANTIC_DIFF_LLM_PROVIDERS, deprecated) > file > default.
    let cli_override = if cli.no_llm {
        None
    } else {
        Some(semantic_diff_core::llm_cli::parse_provider_order(&cli.llm_providers)?)
    };
    config.apply_overrides(cli_override);
    tracing::info!(?config, "effective config");

    let llm_providers = if cli.no_llm {
        Vec::new()
    } else {
        config.llm_providers.clone()
    };

    // Run the orchestrator
    let opts = orchestrator::RunOpts {
        output_dir,
        no_llm: cli.no_llm,
        llm_providers,
        notifier: tx,
    };

    let handle = orchestrator::run(input, opts, &config).await?;
    eprintln!("Review complete. Result: {}", handle.path.display());

    // Keep server alive until Ctrl+C
    tokio::signal::ctrl_c().await?;
    eprintln!("Shutting down.");
    Ok(())
}

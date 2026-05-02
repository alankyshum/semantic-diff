//! gen-schema: write or verify `schemas/semantic-diff.schema.json`.
//!
//! Usage:
//!   gen-schema           # write the schema
//!   gen-schema --check   # exit non-zero if on-disk schema differs from generated
use std::path::PathBuf;

fn schema_path() -> PathBuf {
    // CARGO_MANIFEST_DIR = .../crates/semantic-diff-cli
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent() // crates/
        .and_then(|p| p.parent()) // workspace root
        .map(|p| p.join("schemas").join("semantic-diff.schema.json"))
        .expect("workspace root")
}

fn main() -> anyhow::Result<()> {
    let check = std::env::args().any(|a| a == "--check");
    let path = schema_path();
    let schema = semantic_diff_core::config::RawConfig::json_schema_value();
    let generated = format!("{}\n", serde_json::to_string_pretty(&schema)?);

    if check {
        let on_disk = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
        if on_disk != generated {
            eprintln!("schema drift detected at {}", path.display());
            eprintln!("run `cargo run -p semantic-diff-cli --bin gen-schema` to regenerate");
            std::process::exit(1);
        }
        println!("schema up to date: {}", path.display());
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &generated)?;
    println!("wrote {}", path.display());
    Ok(())
}

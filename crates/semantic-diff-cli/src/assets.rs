use include_dir::{include_dir, Dir};

/// Embedded SvelteKit build output.
/// Run `cd web && npm run build` before `cargo build` to populate this.
pub static WEB_ASSETS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../web/build");

use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let web_build = Path::new(&manifest_dir).join("../../web/build");

    if !web_build.join("index.html").exists() {
        println!("cargo:warning=Web UI not built. Run: cd web && npm install && npm run build");
        println!("cargo:warning=The binary will serve a placeholder page until web assets are built.");
    }

    // Re-run this script if web/build changes
    println!("cargo:rerun-if-changed=../../web/build");
}

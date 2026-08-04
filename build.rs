//! Writes `askama.toml` so Askama can find sigma-theme's templates.
//!
//! This crate's pages extend sigma-theme's `base.html`, which Askama resolves
//! from the filesystem at compile time. Nothing here can hardcode that path:
//! when sigma-theme resolves from git it lives in Cargo's checkout directory
//! under a rev-specific hash, and when a sibling checkout is patched in it lives
//! there instead. sigma-theme reports whichever it is through its `links`
//! metadata, so read it rather than guess.

use std::path::Path;

fn main() {
    let templates = std::env::var("DEP_SIGMA_THEME_TEMPLATES")
        .expect("sigma-theme publishes its template directory as links metadata");
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let config = format!("[general]\ndirs = [\"templates\", {templates:?}]\n");

    // Askama's derive reads this file, so write only on change: an unconditional
    // write would race parallel cargo invocations for no benefit.
    let path = Path::new(&manifest_dir).join("askama.toml");
    if std::fs::read_to_string(&path).is_ok_and(|current| current == config) {
        return;
    }
    std::fs::write(&path, config).expect("askama.toml must be writable");
}

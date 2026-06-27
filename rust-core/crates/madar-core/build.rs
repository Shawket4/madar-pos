//! Bakes `.env` values into the library at build time via `cargo:rustc-env`,
//! so `option_env!("MADAR_BASE_URL")` resolves at compile time and the host
//! apps never carry base-url/environment knowledge (per the rebuild brief).
//!
//! Looks for `.env` at the workspace root (../../.env) first, then crate-local.

use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let candidates = [
        manifest.join("../../.env"), // rust-core/.env (workspace root)
        manifest.join(".env"),       // crate-local override
    ];

    for path in candidates {
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        println!("cargo:rerun-if-changed={}", path.display());
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');
                // Only pass through our own namespaced keys.
                if key.starts_with("MADAR_") {
                    println!("cargo:rustc-env={key}={value}");
                }
            }
        }
    }
}

//! Standalone uniffi-bindgen entry point.
//!
//! Invoked in *library mode* against the compiled cdylib to emit Swift / Kotlin
//! bindings, e.g.:
//!
//!   cargo run --bin uniffi-bindgen -- generate \
//!     --library target/debug/libsufrix_core.dylib \
//!     --language swift --out-dir ../../bindings/swift
fn main() {
    uniffi::uniffi_bindgen_main()
}

#![no_main]
//! Coverage-guided fuzzing of the offline PIN verifier — it gatekeeps offline
//! login against a bundle a corrupt store or an attacker could feed it. It must
//! NEVER panic and must fail closed on any input. (Reached via the cfg(fuzzing)
//! `__fuzz` re-export, since the fn is pub(crate).)
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Split the raw BYTES (any index is valid — &str split_at would panic on a
    // non-char-boundary) and lossy-convert each half, so every byte sequence is
    // exercised, including invalid UTF-8 and control chars.
    let mid = data.len() / 2;
    let pin = String::from_utf8_lossy(&data[..mid]);
    let phc = String::from_utf8_lossy(&data[mid..]);
    // No assertion needed beyond "did not panic" — the harness fails on panic.
    let ok = madar_core::__fuzz::verify_offline_pin(pin.as_ref(), phc.as_ref());
    // A string with no '$' can never be a valid argon2 PHC → must reject.
    if !phc.contains('$') {
        assert!(!ok, "non-PHC string verified as a PIN");
    }
});

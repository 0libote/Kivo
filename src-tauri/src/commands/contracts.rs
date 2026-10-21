//! Surface validation for worker replies. The Rust<->TypeScript serialization
//! snapshot test that lived here was removed with the web frontend; the
//! native app owns its types in Rust.
//!
//! shortcut: if a generated cross-language contract returns, gate it behind
//! an env flag like KIVO_GENERATE_CONTRACTS did, with the fixture committed.

#[test]
fn worker_replies_require_the_flow_bar() {
    assert!(super::validate_worker_surface("flow-bar").is_ok());
    for label in ["settings", "onboarding", "writing-tools", "unknown"] {
        assert!(super::validate_worker_surface(label).is_err());
    }
}

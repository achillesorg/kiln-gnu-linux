//! Home for cross-crate integration tests (`tests/` directory) and shared test
//! fixtures. Kept as a workspace member so `cargo test --workspace` exercises
//! end-to-end behaviour across `glaze-common`, `glaze-service`, `glaze-session`,
//! `glaze-shell-core` and `glaze-backend`.

/// Version of the test-support harness.
pub const HARNESS_VERSION: &str = env!("CARGO_PKG_VERSION");

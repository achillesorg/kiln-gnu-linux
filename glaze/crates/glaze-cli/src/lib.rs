//! `glaze-cli` — the `glaze` command. Portable command parsing and formatting;
//! the socket transport to a running service is Unix-only.

pub mod args;
pub mod output;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_is_valid() {
        assert_eq!(super::VERSION, env!("CARGO_PKG_VERSION"));
    }
}

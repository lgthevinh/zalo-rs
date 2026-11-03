//! zalo-rs entry point.
//!
//! Protocol behaviour is specified in `docs/zalo-specs/`.

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Log level comes from RUST_LOG (e.g. `RUST_LOG=debug`), defaulting to info.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "zalo-rs starting");
}

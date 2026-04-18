//! `edge-agent` binary entry point.
//!
//! Populated in Phase 2.3.

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("edge-agent scaffold; implementation lands in Phase 2.3");
}

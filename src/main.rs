//! Windowsill: a thin, living strip that rests on top of the Windows taskbar.
//!
//! Nothing is drawn yet. The glass arrives in M1 (see the GitHub milestones).

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "windowsill=info".into()),
        )
        .init();

    tracing::info!("Windowsill {}: the sill is empty for now", env!("CARGO_PKG_VERSION"));
    Ok(())
}

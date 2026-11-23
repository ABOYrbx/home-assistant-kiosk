#![allow(unused_crate_dependencies)]
use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    Ok(())
}

#![forbid(unsafe_code)]

use eyre::{Context, Result};

fn main() -> Result<()> {
    color_eyre::install()?;
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    Ok(())
}
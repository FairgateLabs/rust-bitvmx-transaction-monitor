use anyhow::{Context, Ok, Result};
use clap::Parser;
use log::info;
use rust_bitcoin_tx_monitor::args::Args;

fn main() -> Result<()> {
    dotenv::dotenv().context("There was an error loading .env file")?;
    env_logger::init();

    let args = Args::parse();

    info!("{:#?}", args);

    Ok(())
}

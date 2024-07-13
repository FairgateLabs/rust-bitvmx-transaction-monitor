use anyhow::{Context, Ok, Result};
use clap::Parser;
use log::info;
use rust_bitcoin_tx_monitor::{args::Args, monitor::Monitor};

fn main() -> Result<()> {
    dotenv::dotenv().context("There was an error loading .env file")?;
    env_logger::init();

    let args = Args::parse();

    info!("{:#?}", args);

    let mut monitor = Monitor::new(args.bitcoin_indexer_db_url, args.operation_db_url)?;

    monitor.run()?;
    Ok(())
}

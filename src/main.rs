use anyhow::{Context, Ok, Result};
use clap::Parser;
use log::info;
use rust_bitcoin_tx_monitor::{
    args::Args,
    monitor::Monitor,
    stores::{bitcoin_store::BitcoinStore, bitvmx_store::BitvmxStore},
};
use std::env;

fn main() -> Result<()> {
    dotenv::dotenv().context("There was an error loading .env file")?;
    env_logger::init();

    let args = Args::parse();

    let bitcoin_indexer_db_url = args
        .bitcoin_indexer_db_url
        .or_else(|| env::var("BITCOIN_INDEXER_DB_URL").ok())
        .context("No Bitcoin indexer database URL provided")?;

    let bitvmx_file_path = args
        .bitvmx_file_path
        .or_else(|| env::var("BITVMX_FILE_PATH").ok())
        .context("No Bitvmx file path provided")?;

    let bitcoin_store = BitcoinStore::new(&bitcoin_indexer_db_url)?;
    let bitvmx_store = BitvmxStore::new(&bitvmx_file_path)?;

    let mut monitor = Monitor {
        bitcoin_store,
        bitvmx_store,
    };

    monitor.run()?;
    Ok(())
}

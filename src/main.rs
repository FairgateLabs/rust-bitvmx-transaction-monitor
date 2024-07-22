use anyhow::{Context, Result};
use clap::Parser;
use log::info;
use rust_bitcoin_tx_monitor::{
    args::Args,
    monitor::Monitor,
    stores::{
        bitcoin_store::{BitcoinApi, BitcoinStore},
        bitvmx_store::BitvmxStore,
    },
};
use std::{env, thread, time::Duration};

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
        is_running: false,
    };

    let mut prev_height = 0;
    let mut bitcoin_store = BitcoinStore::new(&bitcoin_indexer_db_url)?;

    loop {
        let current_height = bitcoin_store
            .get_block_count()
            .context("Failed to retrieve current block")?;

        if prev_height == current_height {
            info!("Waitting for a new block");
            thread::sleep(Duration::from_secs(1));
        } else {
            info!("New block found height: {}", current_height);
            prev_height = current_height;
        }

        monitor.run()?;
    }
}

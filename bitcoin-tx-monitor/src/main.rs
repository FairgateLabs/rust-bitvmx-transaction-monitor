use anyhow::{Context, Result};
use clap::Parser;
use log::{info, warn};
use rust_bitcoin_indexer::{
    bitcoin_client::{BitcoinClient, BitcoinClientApi},
    helper::define_height_to_sync,
    indexer::Indexer,
    store::{Store, StoreClient},
    types::BlockHeight,
};
use rust_bitcoin_tx_monitor::{args::Args, bitvmx_store::BitvmxStore, monitor::Monitor};
use std::{env, sync::Arc, thread, time::Duration};
fn main() -> Result<()> {
    let loaded = dotenv::dotenv();

    if loaded.is_err() {
        warn!("No .env file found");
    }

    env_logger::init();

    let args = Args::parse();

    let bitvmx_file_path = args
        .bitvmx_file_path
        .or_else(|| env::var("BITVMX_FILE_PATH").ok())
        .context("No Bitvmx file path provided")?;

    let db_file_path: String = args
        .db_file_path
        .or_else(|| env::var("DB_FILE_PATH").ok())
        .context("No Bitcoin database file path provided")?;

    let node_rpc_url: String = args
        .node_rpc_url
        .or_else(|| env::var("NODE_RPC_URL").ok())
        .context("No Bitcoin rpc url provided")?;

    let checkpoint_height: Option<u32> = get_checkpoint()?;

    let bitcoin_client = Arc::new(BitcoinClient::new(&node_rpc_url)?);
    let blockchain_height = bitcoin_client.get_best_block()? as BlockHeight;
    let network = bitcoin_client.get_blockchain_info()?;

    info!("Connected to chain {}", network);
    info!("Chain best block at {}H", blockchain_height);

    let store = Store::new(&db_file_path)?;
    let indexed_height = store.get_best_block_height()?;
    let mut height_to_sync =
        define_height_to_sync(checkpoint_height, blockchain_height, indexed_height)?;
    info!("Start synchronizing from {}H", height_to_sync);

    let indexer = Arc::new(Indexer::new(bitcoin_client, Arc::new(store))?);

    let bitvmx_store = Arc::new(BitvmxStore::new(&bitvmx_file_path)?);

    let monitor = Monitor {
        indexer_api: indexer.clone(),
        bitvmx_store,
    };

    let mut prev_height = 0;

    loop {
        height_to_sync = indexer.index_height(&height_to_sync)?;

        if prev_height == height_to_sync {
            info!("Waitting for a new block...");
            thread::sleep(Duration::from_secs(10));
        } else {
            prev_height = height_to_sync;
        }

        monitor.detect_instances()?;
    }
}

fn get_checkpoint() -> Result<Option<u32>, anyhow::Error> {
    let checkpoint = env::var("CHECKPOINT_HEIGHT");
    let mut checkpoint_height = None;

    if checkpoint.is_ok() {
        checkpoint_height = match checkpoint?.parse::<BlockHeight>() {
            Ok(checkpoint_height) => Some(checkpoint_height),
            Err(_) => {
                warn!("Checkpoint height must be a positive integer");
                None
            }
        };
    }

    Ok(checkpoint_height)
}

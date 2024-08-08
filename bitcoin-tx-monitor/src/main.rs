use anyhow::{Context, Ok, Result};
use bitcoin_indexer::{
    bitcoin_client::{BitcoinClient, BitcoinClientApi},
    helper::define_height_to_sync,
    indexer::Indexer,
    store::{Store, StoreClient},
    types::BlockHeight,
};
use bitcoin_tx_monitor::{args::Args, bitvmx_store::BitvmxStore, monitor::Monitor};
use clap::Parser;
use log::{info, warn};
use std::{env, sync::mpsc::channel, thread, time::Duration};
fn main() -> Result<()> {
    let (tx, rx) = channel();

    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel."))
        .expect("Error setting Ctrl-C handler");

    let envs = dotenv::dotenv();

    if envs.is_err() {
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

    let bitcoin_client = BitcoinClient::new(&node_rpc_url)?;
    let blockchain_height = bitcoin_client.get_best_block()? as BlockHeight;
    let network = bitcoin_client.get_blockchain_info()?;

    info!("Connected to chain {}", network);
    info!("Chain best block at {}H", blockchain_height);

    let store = Store::new(&db_file_path)?;
    let indexed_height = store.get_best_block_height()?;
    let mut height_to_sync =
        define_height_to_sync(checkpoint_height, blockchain_height, indexed_height)?;
    info!("Start synchronizing from {}H", height_to_sync);

    let indexer = Indexer::new(bitcoin_client, store)?;
    let bitvmx_store = BitvmxStore::new(&bitvmx_file_path)?;
    let monitor = Monitor::new(indexer, bitvmx_store);

    let mut prev_height = 0;

    loop {
        if rx.try_recv().is_ok() {
            info!("Stop Bitcoin transaction Monitor");
            break;
        }

        if prev_height == height_to_sync {
            info!("Waitting for a new block...");
            thread::sleep(Duration::from_secs(1));
        } else {
            prev_height = height_to_sync;
        }

        height_to_sync = monitor
            .detect_instances_at_height(height_to_sync)
            .context("Fail to detect instances")?;
    }

    Ok(())
}

fn get_checkpoint() -> Result<Option<u32>> {
    let checkpoint = env::var("CHECKPOINT_HEIGHT");

    if checkpoint.is_ok() {
        let checkpoint_height = checkpoint?.parse::<BlockHeight>();

        if checkpoint_height.is_err() {
            warn!("Checkpoint height must be a positive integer");
            return Ok(None);
        }

        return Ok(Some(checkpoint_height?));
    }

    Ok(None)
}

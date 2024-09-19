use anyhow::{Context, Ok, Result};
use bitcoin_indexer::{
    bitcoin_client::{BitcoinClient, BitcoinClientApi},
    types::BlockHeight,
};
use bitvmx_transaction_monitor::{
    args::Args, bitvmx_instances_example::get_bitvmx_instances_example, monitor::Monitor,
};
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

    let mut monitor = Monitor::new_with_paths(&node_rpc_url, &db_file_path, checkpoint_height)?;

    let bitvmx_instances = get_bitvmx_instances_example();
    monitor.save_instances_for_tracking(bitvmx_instances)?;

    let mut prev_height = 0;

    loop {
        if rx.try_recv().is_ok() {
            info!("Stop Bitcoin transaction Monitor");
            break;
        }

        if prev_height == monitor.get_current_height() && prev_height > 0 {
            info!("Waitting for a new block...");
            thread::sleep(Duration::from_secs(10));
        } else {
            prev_height = monitor.get_current_height();
        }

        monitor
            .detect_instances()
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

use anyhow::{Context, Ok, Result};
use bitvmx_bitcoin_rpc::{
    bitcoin_client::{BitcoinClient, BitcoinClientApi},
    types::BlockHeight,
};
use bitvmx_settings::settings;
use bitvmx_transaction_monitor::{
    bitvmx_instances_example::get_bitvmx_instances_example, config::ConfigMonitor, monitor::Monitor,
};
use std::{path::PathBuf, rc::Rc, sync::mpsc::channel, thread, time::Duration};
use storage_backend::storage::Storage;
use tracing::info;

fn main() -> Result<()> {
    let (tx, rx) = channel();

    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel."))
        .expect("Error setting Ctrl-C handler");

    let config = settings::load::<ConfigMonitor>()?;

    let log_level = match config.log_level {
        Some(level) => level.parse().unwrap_or(tracing::Level::INFO),
        None => tracing::Level::INFO,
    };

    tracing_subscriber::fmt().with_max_level(log_level).init();

    let bitcoin_client = BitcoinClient::new_from_config(&config.rpc)?;
    let blockchain_height = bitcoin_client.get_best_block()? as BlockHeight;
    let network = bitcoin_client.get_blockchain_info()?.chain;

    info!("Connected to chain {}", network);
    info!("Chain best block at {}H", blockchain_height);

    let storage = Rc::new(Storage::new_with_path(&PathBuf::from(config.db_file_path))?);
    let monitor = Monitor::new_with_paths(
        &config.rpc,
        storage,
        config.checkpoint_height,
        config.confirmation_threshold,
    )?;

    let bitvmx_instances = get_bitvmx_instances_example();
    monitor.save(bitvmx_instances)?;

    let mut prev_height = 0;

    loop {
        if rx.try_recv().is_ok() {
            info!("Stop Bitcoin transaction Monitor");
            break;
        }

        let current_height = monitor.get_current_height()?;

        if prev_height == current_height && prev_height > 0 {
            info!("Waitting for a new block...");
            thread::sleep(Duration::from_secs(10));
        } else {
            prev_height = current_height;
        }

        monitor.tick().context("Fail to detect instances")?;
    }

    Ok(())
}

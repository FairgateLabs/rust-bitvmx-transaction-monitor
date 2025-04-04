use anyhow::{Context, Ok, Result};
use bitvmx_bitcoin_rpc::{
    bitcoin_client::{BitcoinClient, BitcoinClientApi},
    types::BlockHeight,
};
use bitvmx_settings::settings;
use bitvmx_transaction_monitor::{
    config::ConfigMonitor,
    monitor::{Monitor, MonitorApi},
    types::TransactionMonitorType,
};
use std::{path::PathBuf, rc::Rc, sync::mpsc::channel, thread, time::Duration};
use storage_backend::storage::Storage;
use tracing::info;
use uuid::Uuid;

/// This integration test demonstrates how to set up and use the transaction monitor.
/// It initializes a Bitcoin client, connects to the blockchain, and sets up a monitor
/// to track transactions.
///
/// The test:
/// 1. Loads configuration from settings
/// 2. Initializes a Bitcoin client and connects to the blockchain
/// 3. Creates a storage backend for persisting monitoring data
/// 4. Initializes a transaction monitor with the Bitcoin client and storage
/// 5. Creates a group transaction monitor (which can track multiple transactions)
/// 6. Enters a loop that:
///    - Monitors the current blockchain height
///    - Waits for new blocks when no changes are detected
///
/// This test is marked as #[ignore] because it's meant to be run manually to understand
/// the behavior of the monitoring system rather than as part of automated testing.
#[test]
#[ignore]
fn test_pegin_tx_detection() -> Result<(), anyhow::Error> {
    let (tx, rx) = channel();
    info!("Setting Ctrl-C handler");
    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel."))
        .expect("Error setting Ctrl-C handler");

    let config = settings::load::<ConfigMonitor>()?;

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let bitcoin_client = BitcoinClient::new_from_config(&config.rpc)?;
    let blockchain_height = bitcoin_client.get_best_block()? as BlockHeight;
    info!("blockchain_height: {}", blockchain_height);
    let network = bitcoin_client.get_blockchain_info()?;

    info!("Connected to chain {:?}", network);
    info!("Chain best block at {}H", blockchain_height);

    let storage = Rc::new(Storage::new_with_path(&PathBuf::from(
        config.database.file_path,
    ))?);
    let monitor = Monitor::new_with_paths(
        &config.rpc,
        storage,
        Some(config.monitor.checkpoint_height),
        config.monitor.confirmation_threshold,
    )?;

    let group_monitor = TransactionMonitorType::GroupTransaction(Uuid::new_v4(), vec![]);
    monitor.monitor(group_monitor)?;

    let mut prev_height = 0;

    loop {
        if rx.try_recv().is_ok() {
            info!("Stop Bitcoin transaction Monitor");
            break;
        }

        let current_height = monitor.get_monitor_height()?;

        if prev_height == current_height && prev_height > 0 {
            info!("Waitting for a new block...");
            thread::sleep(Duration::from_secs(10));
        } else {
            prev_height = current_height;
        }

        monitor.tick()?;
    }

    Ok(())
}

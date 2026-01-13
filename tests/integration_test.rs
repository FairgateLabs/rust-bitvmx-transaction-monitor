use anyhow::{Ok, Result};
use bitcoind::{bitcoind::Bitcoind, config::BitcoindConfig};
use bitvmx_bitcoin_rpc::bitcoin_client::{BitcoinClient, BitcoinClientApi};
use bitvmx_settings::settings;
use bitvmx_transaction_monitor::{
    config::{MonitorConfig, MonitorSettingsConfig},
    monitor::{Monitor, MonitorApi},
    types::{AckMonitorNews, MonitorNews, TypesToMonitor},
};
use std::rc::Rc;
use storage_backend::{storage::Storage, storage_config::StorageConfig};
use tracing::info;
mod utils;

#[test]
// Integration test to detect a transaction monitor news.
// This test verifies that the monitor can correctly detect a transaction monitor news and new block monitor news.
// The test does the following:
// 1. Mines 130 blocks to the created wallet to ensure there are confirmed transactions and blocks to monitor.
// 2. Adds a Transaction to be monitored. That transaction is in block 90 (coinbase transaction).
// 3. Ticks the monitor 99 times, but does not expect news yet (asserts news is empty). This is because the transaction is not yet 11 confirmations.
// 4. After one more tick, expects exactly one news entry about the transaction reaching the required confirmation (11 confirmations).
// 5. Acknowledges the news. And then tick again, but does not expect news yet (asserts news is empty).
//   This is because the news was acknowledged and will not be returned again because monitor reach 11 confirmations. )
// 6. Adds a New Block monitor and ticks the monitor 40 times to simulate new blocks being discovered.
// 7. Expects exactly one news entry when the monitored block height is reached (block 130).
fn detect_transaction_monitor() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = settings::load_config_file::<MonitorConfig>(Some(
        "config/monitor_config.yaml".to_string(),
    ))?;

    let storage_config = StorageConfig::new("test_outputs".to_string(), None);

    let storage = Rc::new(Storage::new(&storage_config)?);

    let bitcoind_config = BitcoindConfig::new(
        "bitcoin-regtest".to_string(),
        "bitcoin/bitcoin:29.1".to_string(),
        None,
    );

    let bitcoind = Bitcoind::new(bitcoind_config, config.bitcoin.clone(), None);

    bitcoind.start()?;

    let bitcoin_client = BitcoinClient::new_from_config(&config.bitcoin)?;
    let wallet = bitcoin_client.init_wallet("test_wallet")?;

    info!("Mining 100 blocks to wallet");
    bitcoin_client.mine_blocks_to_address(130, &wallet)?;

    let monitor = Monitor::new_with_paths(
        &config.bitcoin,
        storage,
        Some(MonitorSettingsConfig::default()),
    )?;

    let block_info = bitcoin_client.get_block_by_height(&90)?.unwrap();

    let tx_id = block_info.txs[0].compute_txid();

    let txs_monitor = TypesToMonitor::Transactions(vec![tx_id], "Txid".to_string(), Some(11));
    monitor.monitor(txs_monitor)?;

    for _ in 0..99 {
        monitor.tick()?;
    }

    // Assert no news yet
    let news = monitor.get_news()?;
    assert_eq!(news.len(), 0);

    // Should find the transaction and send news (11 confirmations)
    monitor.tick()?;
    let news = monitor.get_news()?;
    assert_eq!(news.len(), 1);
    match &news[0] {
        MonitorNews::Transaction(txid, tx_status, _) => {
            assert_eq!(txid, &tx_id);
            assert_eq!(tx_status.confirmations, 11);
        }
        _ => panic!("Expected MonitorNews::Transaction"),
    }

    // Acknowledge the news
    monitor.ack_news(AckMonitorNews::Transaction(tx_id))?;

    // Add a new block monitor
    let best_block_monitor = TypesToMonitor::NewBlock;
    monitor.monitor(best_block_monitor)?;

    for _ in 0..40 {
        monitor.tick()?;
    }

    // Should find the new block and send news (block 130)
    let news = monitor.get_news()?;
    assert_eq!(news.len(), 1);
    match &news[0] {
        MonitorNews::NewBlock(height, _) => assert_eq!(height, &130),
        _ => panic!("Expected MonitorNews::NewBlock"),
    }

    utils::clear_output();
    bitcoind.stop()?;

    Ok(())
}

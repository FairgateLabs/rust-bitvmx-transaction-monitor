use anyhow::{Ok, Result};
use bitcoind::bitcoind::Bitcoind;
use bitvmx_bitcoin_rpc::bitcoin_client::{BitcoinClient, BitcoinClientApi};
use bitvmx_settings::settings;
use bitvmx_transaction_monitor::{
    config::MonitorConfig,
    monitor::{Monitor, MonitorApi},
    types::{MonitorNews, TypesToMonitor},
};
use std::rc::Rc;
use storage_backend::{storage::Storage, storage_config::StorageConfig};
use tracing::info;
mod utils;

/// This integration test demonstrates how to set up and use the transaction monitor.
/// It initializes a Bitcoin client, connects to the blockchain, and sets up a monitor
/// to track some transaction.
#[test]
#[ignore]
fn test_pegin_tx_detection() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = settings::load_config_file::<MonitorConfig>(Some(
        "config/monitor_config.yaml".to_string(),
    ))?;

    let storage_config = StorageConfig {
        path: "test_outputs".to_string(),
        encrypt: None,
    };

    let storage = Rc::new(Storage::new(&storage_config)?);

    let bitcoind = Bitcoind::new(
        "bitcoin-regtest",
        "bitcoin/bitcoin:29.1",
        config.bitcoin.clone(),
    );

    bitcoind.start()?;

    let bitcoin_client = BitcoinClient::new_from_config(&config.bitcoin)?;
    let wallet = bitcoin_client.init_wallet("test_wallet")?;

    info!("Mining 100 blocks to wallet");
    bitcoin_client.mine_blocks_to_address(100, &wallet)?;

    let monitor = Monitor::new_with_paths(&config.bitcoin, storage, config.settings)?;

    let block_info = bitcoin_client.get_block_by_height(&90)?.unwrap();

    let tx_id = block_info.txs[0].compute_txid();

    let txs_monitor = TypesToMonitor::Transactions(vec![tx_id], "Txid".to_string());
    monitor.monitor(txs_monitor)?;

    let best_block_monitor = TypesToMonitor::NewBlock;
    monitor.monitor(best_block_monitor)?;

    for _ in 0..100 {
        monitor.tick()?;
    }

    let news = monitor.get_news()?;

    assert_eq!(news.len(), 2);
    match &news[0] {
        MonitorNews::Transaction(txid, _, _) => assert_eq!(txid, &tx_id),
        _ => panic!("Expected MonitorNews::Transaction"),
    }
    assert!(matches!(news[1], MonitorNews::NewBlock(100, _)));

    utils::clear_output();
    bitcoind.stop()?;

    Ok(())
}

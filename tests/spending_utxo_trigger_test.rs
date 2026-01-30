use anyhow::Result;
use bitcoin::Amount;
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
use utils::generate_random_string;
mod utils;

/// Integration test for SpendingUTXOTransaction monitoring with confirmation triggers.
/// Test steps:
/// 1. Initialize bitcoind and launch it
/// 2. Instantiate the monitor
/// 3. Mine 200 blocks to mature coinbase outputs
/// 4. Wait for monitor to sync all blocks (tick until ready)
/// 5. Create transaction1 (funding tx)
/// 6. Add two monitors for the same UTXO (same txid, vout) but with different context strings (SpendingUTXOTransaction, confirmation trigger: Some(1))
/// 7. Mine 1 block, tick monitor (the UTXO is available, but not yet spent)
/// 8. Assert: No UTXO spending news is triggered yet
/// 9. Create transaction2 that spends transaction1's output
/// 10. Broadcast transaction2 to the network
/// 11. Mine 1 block, tick monitor, and check two spend news arrive for the monitored (txid, vout) with both contexts
/// 12. Mine another block, tick the monitor
/// 13. Assert: the monitor correctly reports 2 confirmations for the spending transaction (verifying confirmation trigger behavior) for both contexts
#[test]
fn test_spending_utxo_confirmation_trigger() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // 1) Setup bitcoind and start it
    let config = settings::load_config_file::<MonitorConfig>(Some(
        "config/monitor_config.yaml".to_string(),
    ))?;

    let path = format!("test_outputs/{}", generate_random_string());
    let storage_config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&storage_config)?);

    let bitcoind_config = BitcoindConfig::default();
    let bitcoind = Bitcoind::new(bitcoind_config, config.bitcoin.clone(), None);
    bitcoind.start()?;

    let bitcoin_client = BitcoinClient::new_from_config(&config.bitcoin)?;
    let wallet = bitcoin_client.init_wallet("test_wallet")?;

    // 2) Create the monitor instance
    let monitor = Monitor::new_with_paths(
        &config.bitcoin,
        storage,
        Some(MonitorSettingsConfig::default()),
    )?;

    // 3) Mine 200 blocks
    info!("Mining 100 blocks");
    bitcoin_client.mine_blocks_to_address(200, &wallet)?;

    // Sync the new blocks
    loop {
        monitor.tick()?;
        if monitor.is_ready()? {
            break;
        }
    }

    // 4) Create transaction1 that has a UTXO to spend
    // First, create a funding transaction to get a UTXO
    let funding_amount = Amount::from_sat(1_000_000); // 0.01 BTC
    let (transaction1, transaction1_vout) = bitcoin_client.fund_address(&wallet, funding_amount)?;
    let transaction1_txid = transaction1.compute_txid();

    info!(
        "Created transaction1 {} with vout {}",
        transaction1_txid, transaction1_vout
    );

    // Monitor transaction1's UTXO with SpendingUTXOTransaction and confirmation_trigger Some(1)
    monitor.monitor(TypesToMonitor::SpendingUTXOTransaction(
        transaction1_txid,
        transaction1_vout,
        "context_1".to_string(),
        Some(1),
    ))?;

    // Monitor the same transaction1's UTXO again with a different context
    monitor.monitor(TypesToMonitor::SpendingUTXOTransaction(
        transaction1_txid,
        transaction1_vout,
        "context_2".to_string(),
        Some(1),
    ))?;

    // Send transaction1 to the network (fund_address already sent it, but we need to ensure it's in mempool)
    // fund_address already sent it, so we just need to mine a block to confirm it
    info!(
        "Transaction1 {} already sent by fund_address",
        transaction1_txid
    );

    // 6) Mine 1 block and do 1 tick (this confirms transaction1)
    info!("Mining 1 block to confirm transaction1");
    bitcoin_client.mine_blocks_to_address(1, &wallet)?;
    monitor.tick()?;

    // 7) Check news should be empty (transaction1 is not spent yet)
    let news_after_first_block = monitor.get_news()?;
    info!(
        "News count after first block: {}",
        news_after_first_block.len()
    );
    assert_eq!(
        news_after_first_block.len(),
        0,
        "Expected no news after first block (transaction1 is not spent yet), but got {}",
        news_after_first_block.len()
    );

    // 8) Create and send transaction2 that consumes transaction1's output
    // Use utility function to create and send a spending transaction
    let spending_amount = Amount::from_sat(900_000); // Most of transaction1's output, leaving room for fees
    let (_transaction2, transaction2_txid) = utils::create_and_send_spending_transaction(
        &bitcoin_client,
        transaction1_txid,
        transaction1_vout,
        spending_amount,
    )?;
    info!(
        "Created and sent transaction2 {} that explicitly spends transaction1's UTXO",
        transaction2_txid
    );

    // 10) Mine 1 block and check that there should be a UTXO news
    info!("Mining 1 block to confirm transaction2");
    bitcoin_client.mine_blocks_to_address(1, &wallet)?;

    // Wait for indexer to sync the new block
    loop {
        monitor.tick()?;
        if monitor.is_ready()? {
            break;
        }
    }
    info!("Indexer synced after second block");

    let news_after_second_block = monitor.get_news()?;
    info!(
        "News count after second block: {}",
        news_after_second_block.len()
    );

    // Should have 2 news items (one for each context monitoring the same UTXO)
    assert_eq!(
        news_after_second_block.len(),
        2,
        "Expected 2 news after second block (one for each context monitoring transaction1's UTXO), but got {}",
        news_after_second_block.len()
    );

    // Verify both news items are for SpendingUTXOTransaction with 1 confirmation
    let mut found_first_context = false;
    let mut found_second_context = false;

    for news_item in &news_after_second_block {
        match news_item {
            MonitorNews::SpendingUTXOTransaction(txid, vout, tx_status, extra_data) => {
                assert_eq!(
                    *txid, transaction1_txid,
                    "Expected news for transaction1 txid {}, got {}",
                    transaction1_txid, txid
                );
                assert_eq!(
                    *vout, transaction1_vout,
                    "Expected news for transaction1 vout {}, got {}",
                    transaction1_vout, vout
                );
                assert_eq!(
                    tx_status.confirmations, 1,
                    "Expected 1 confirmation, got {}",
                    tx_status.confirmations
                );
                assert_eq!(
                    tx_status.tx_id, transaction2_txid,
                    "Expected spender tx_id {}, got {}",
                    transaction2_txid, tx_status.tx_id
                );

                if extra_data == "context_1" {
                    found_first_context = true;
                    info!(
                        "Received SpendingUTXOTransaction news for ({}, {}) with context '{}', spender {} and {} confirmations",
                        txid, vout, extra_data, tx_status.tx_id, tx_status.confirmations
                    );
                } else if extra_data == "context_2" {
                    found_second_context = true;
                    info!(
                        "Received SpendingUTXOTransaction news for ({}, {}) with context '{}', spender {} and {} confirmations",
                        txid, vout, extra_data, tx_status.tx_id, tx_status.confirmations
                    );
                }
            }
            _ => panic!(
                "Expected MonitorNews::SpendingUTXOTransaction, got {:?}",
                news_item
            ),
        }
    }

    assert!(
        found_first_context,
        "Expected to find news for first context 'context_1'"
    );
    assert!(
        found_second_context,
        "Expected to find news for second context 'context_2'"
    );

    // Acknowledge both news items (one for each context)
    monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        transaction1_txid,
        transaction1_vout,
    ))?;

    // 11) Mine 10 more blocks, do 10 ticks
    info!("Mining 10 more blocks");
    bitcoin_client.mine_blocks_to_address(10, &wallet)?;

    for _ in 0..10 {
        monitor.tick()?;
    }

    // 12) Check news again - should NOT have new news because trigger is Some(1) and already sent
    // With confirmation trigger Some(1), news should only be sent once at 1 confirmation
    // and not again at 2 confirmations
    let news_after_third_block = monitor.get_news()?;
    info!(
        "News count after third block: {}",
        news_after_third_block.len()
    );

    // Should have NO new news because trigger was already sent at 1 confirmation
    assert_eq!(
        news_after_third_block.len(),
        0,
        "Expected no news after third block (trigger Some(1) already sent at 1 confirmation), but got {}",
        news_after_third_block.len()
    );

    utils::clear_output();
    bitcoind.stop()?;

    Ok(())
}

use anyhow::Result;
use bitcoin::{Amount, Transaction};
use bitcoincore_rpc::{Client, RpcApi};
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

/// Integration test to verify SpendingUTXOTransaction monitoring with confirmation trigger.
/// This test:
/// 1. Sets up bitcoind and starts it
/// 2. Creates the monitor instance
/// 3. Mines 100 blocks
/// 4. Makes 100 ticks to sync
/// 5. Creates transaction1 with a UTXO to spend, monitors it with SpendingUTXOTransaction (trigger Some(1)), and sends it to network
/// 6. Mines 1 block and does 1 tick
/// 7. Checks that news should be empty (transaction1 is not spent yet)
/// 8. Creates transaction2 that consumes transaction1's output
/// 9. Sends transaction2 to the network
/// 10. Mines 1 block and checks that there should be a UTXO news
/// 11. Mines 1 more block, does 1 tick
/// 12. Checks confirmations for transaction2 is 2 (this is where there's a bug)
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

    // 3) Mine 100 blocks
    info!("Mining 100 blocks");
    bitcoin_client.mine_blocks_to_address(100, &wallet)?;

    info!(
        "Indexer is ready at height {}",
        monitor.get_monitor_height()?
    );

    // Generate more blocks to ensure funds are available
    info!("Generating additional blocks to ensure funds are available");
    bitcoin_client.mine_blocks_to_address(101, &wallet)?;

    // Sync the new blocks
    loop {
        monitor.tick()?;
        if monitor.is_ready()? {
            break;
        }
    }

    // 5) Create transaction1 that has a UTXO to spend
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
        "test_spending".to_string(),
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

    // 8) Create transaction2 that consumes transaction1's output
    // We'll use Bitcoin RPC to create a raw transaction that explicitly spends transaction1's UTXO

    // Get a new address to send to
    let recipient_address = bitcoin_client
        .client
        .get_new_address(None, Some(bitcoincore_rpc::json::AddressType::Bech32))?;

    // Create a raw transaction that spends transaction1's UTXO
    let spending_amount = Amount::from_sat(900_000); // Most of transaction1's output, leaving room for fees
    let inputs = vec![bitcoincore_rpc::json::CreateRawTransactionInput {
        txid: transaction1_txid,
        vout: transaction1_vout,
        sequence: None,
    }];

    let mut outputs = std::collections::HashMap::new();
    // Convert address to string for the output map
    let address_str = format!("{}", recipient_address.assume_checked());
    outputs.insert(address_str, spending_amount);

    let raw_tx = bitcoin_client
        .client
        .create_raw_transaction(&inputs, &outputs, None, None)?;

    // Sign the transaction with the wallet
    let signed_tx = bitcoin_client
        .client
        .sign_raw_transaction_with_wallet(&raw_tx, None, None)?;

    if !signed_tx.complete {
        return Err(anyhow::anyhow!(
            "Transaction signing incomplete: {:?}",
            signed_tx.errors
        ));
    }

    // Decode the signed transaction to get the txid
    // signed_tx.hex is Vec<u8>
    let transaction2: Transaction =
        bitcoin::consensus::Decodable::consensus_decode(&mut &signed_tx.hex[..])?;
    let transaction2_txid = transaction2.compute_txid();
    info!(
        "Created transaction2 {} that explicitly spends transaction1's UTXO",
        transaction2_txid
    );

    // 9) Send transaction2 to the network using send_raw_transaction
    // send_raw_transaction accepts Vec<u8> directly
    bitcoin_client.client.send_raw_transaction(&signed_tx.hex)?;
    info!("Sent transaction2 {} to the network", transaction2_txid);

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
    assert_eq!(
        news_after_second_block.len(),
        1,
        "Expected 1 news after second block (transaction2 spent transaction1's UTXO), but got {}",
        news_after_second_block.len()
    );

    // Verify the news is for SpendingUTXOTransaction with 1 confirmation
    match &news_after_second_block[0] {
        MonitorNews::SpendingUTXOTransaction(txid, vout, tx_status, _extra_data) => {
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
            info!(
                "Received SpendingUTXOTransaction news for ({}, {}) with spender {} and {} confirmations",
                txid, vout, tx_status.tx_id, tx_status.confirmations
            );
        }
        _ => panic!(
            "Expected MonitorNews::SpendingUTXOTransaction, got {:?}",
            news_after_second_block[0]
        ),
    }

    // Acknowledge the news
    monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        transaction1_txid,
        transaction1_vout,
    ))?;

    // 11) Mine 1 more block, do 1 tick
    info!("Mining 1 more block");
    bitcoin_client.mine_blocks_to_address(1, &wallet)?;

    // Wait for indexer to sync the new block
    loop {
        monitor.tick()?;
        if monitor.is_ready()? {
            break;
        }
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

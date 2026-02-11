use anyhow::Result;
use bitcoin::{Amount, Transaction, Txid};
use bitcoin_indexer::{config::IndexerSettings, indexer::Indexer, store::IndexerStore};
use bitcoincore_rpc::RpcApi;
use bitcoind::{bitcoind::Bitcoind, config::BitcoindConfig};
use bitvmx_bitcoin_rpc::bitcoin_client::{BitcoinClient, BitcoinClientApi};
use bitvmx_settings::settings;
use bitvmx_transaction_monitor::{
    config::MonitorConfig,
    monitor::{Monitor, MonitorApi},
    types::{AckMonitorNews, MonitorNews, TypesToMonitor},
};
use std::rc::Rc;
use storage_backend::storage::Storage;
use tracing::info;

use bitcoin::key::rand;
use rand::Rng;

pub fn generate_random_string() -> String {
    let mut rng = rand::thread_rng();
    (0..10).map(|_| rng.gen_range('a'..='z')).collect()
}

pub fn clear_output() {
    let _ = std::fs::remove_dir_all("test_outputs");
}

/// Creates and sends a funding transaction to an address.
/// Uses hardcoded wallet "test_wallet" and amount 1_000_000 satoshis (0.01 BTC).
/// Returns the transaction, its txid, and the vout index.
fn create_and_send_funding_transaction(
    bitcoin_client: &BitcoinClient,
) -> Result<(Transaction, Txid, u32)> {
    use bitcoin::Amount;
    let wallet_address = bitcoin_client.init_wallet("test_wallet")?;
    let funding_amount = Amount::from_sat(1_000_000); // 0.01 BTC
    let (transaction, vout) = bitcoin_client.fund_address(&wallet_address, funding_amount)?;
    let txid = transaction.compute_txid();
    Ok((transaction, txid, vout))
}

pub fn mine_blocks(bitcoin_client: &BitcoinClient, number_blocks: u64) -> Result<()> {
    info!("Mine {} blocks", number_blocks);
    let wallet = bitcoin_client.init_wallet("test_wallet")?;
    bitcoin_client.mine_blocks_to_address(number_blocks, &wallet)?;
    Ok(())
}

/// Creates and sends a transaction that spends a specific UTXO.
/// Returns the decoded transaction and its txid.
pub fn create_and_send_a_new_transaction(
    bitcoin_client: &BitcoinClient,
) -> Result<(Transaction, Txid)> {
    let amount = Amount::from_sat(900_000); // Most of the funding, leaving room for fees

    // Create a funding transaction to get a UTXO to spend
    let (_, funding_txid, funding_vout) = create_and_send_funding_transaction(bitcoin_client)?;

    mine_blocks(bitcoin_client, 1)?;

    // Get a new address to send to
    let recipient_address = bitcoin_client
        .client
        .get_new_address(None, Some(bitcoincore_rpc::json::AddressType::Bech32))?;

    // Create a raw transaction that spends the UTXO
    let inputs = vec![bitcoincore_rpc::json::CreateRawTransactionInput {
        txid: funding_txid,
        vout: funding_vout,
        sequence: None,
    }];

    let mut outputs = std::collections::HashMap::new();
    // Convert address to string for the output map
    let address_str = format!("{}", recipient_address.assume_checked());
    outputs.insert(address_str, amount);

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
    let transaction: Transaction =
        bitcoin::consensus::Decodable::consensus_decode(&mut &signed_tx.hex[..])?;
    let txid = transaction.compute_txid();

    info!("Sending Transaction({})", txid);
    // Send the transaction to the network
    bitcoin_client.client.send_raw_transaction(&signed_tx.hex)?;

    mine_blocks(bitcoin_client, 1)?;

    Ok((transaction, txid))
}

/// Creates a complete test setup with BitcoinClient, Monitor, and Bitcoind.
/// This function:
/// 1. Starts a bitcoind instance
/// 2. Creates a BitcoinClient
/// 3. Creates an IndexerStore and Indexer
/// 4. Mines initial blocks
/// 5. Creates a Monitor
/// 6. Syncs the Monitor
///
/// Returns the BitcoinClient, Monitor, and Bitcoind instance.
pub fn create_test_setup(
    max_monitoring_confirmations: u32,
) -> Result<(
    BitcoinClient,
    bitvmx_transaction_monitor::monitor::Monitor,
    Bitcoind,
)> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    use bitvmx_transaction_monitor::{
        config::MonitorSettings, monitor::Monitor, store::MonitorStore,
    };

    let config = settings::load_config_file::<MonitorConfig>(Some(
        "config/monitor_config.yaml".to_string(),
    ))?;

    let bitcoind_config = BitcoindConfig::default();

    let bitcoind = Bitcoind::new(bitcoind_config, config.bitcoin.clone(), None);

    bitcoind.start()?;

    let bitcoin_client = BitcoinClient::new_from_config(&config.bitcoin)?;

    mine_blocks(&bitcoin_client, 120)?;

    // Create storage
    let path = format!("test_outputs/{}", generate_random_string());
    let storage_config = storage_backend::storage_config::StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&storage_config)?);

    let indexer_settings = IndexerSettings::default();

    let indexer_store = IndexerStore::new(storage.clone(), indexer_settings.confirmation_threshold)
        .map_err(|e| anyhow::anyhow!("Failed to create IndexerStore: {}", e))?;

    let indexer = Indexer::new(
        bitcoin_client,
        Rc::new(indexer_store),
        Some(indexer_settings.clone()),
    )?;

    let store = MonitorStore::new(storage)?;
    let monitor_settings = MonitorSettings {
        max_monitoring_confirmations,
        indexer_settings: Some(indexer_settings),
    };

    let monitor = Monitor::new(indexer, store, monitor_settings)?;

    sync_monitor(&monitor)?;

    let bitcoin_client = BitcoinClient::new_from_config(&config.bitcoin)?;

    Ok((bitcoin_client, monitor, bitcoind))
}

pub fn sync_monitor(monitor: &Monitor) -> Result<()> {
    use tracing::info;

    info!("Syncing Monitor...");

    loop {
        monitor.tick()?;
        if monitor.is_ready()? {
            break;
        }
    }
    Ok(())
}

pub fn monitor_tx(monitor: &Monitor, tx_id: Txid, extra_data: &str) -> Result<()> {
    monitor.monitor(TypesToMonitor::Transactions(
        vec![tx_id],
        extra_data.to_string(),
        None,
    ))?;
    Ok(())
}

pub fn ack_tx_monitor(monitor: &Monitor, tx_id: Txid, extra_data: &str) -> Result<()> {
    monitor.ack_news(AckMonitorNews::Transaction(tx_id, extra_data.to_string()))?;
    Ok(())
}

pub fn assert_tx_news(
    news: &MonitorNews,
    tx_id: Txid,
    extra_data: &str,
    confirmations: u32,
) -> Result<()> {
    match news {
        MonitorNews::Transaction(id, _tx_status, context) => {
            assert_eq!(*id, tx_id);
            assert_eq!(context, &extra_data);
            assert_eq!(_tx_status.confirmations, confirmations);
        }
        _ => panic!("Expected Transaction news"),
    }
    Ok(())
}

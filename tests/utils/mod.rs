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

    info!("Mining 1 block");
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
/// 6. Syncs the indexer
///
/// Returns the BitcoinClient, Monitor (with indexer accessible via monitor.indexer), and Bitcoind instance.
/// The caller is responsible for stopping bitcoind when done.
pub fn create_test_setup() -> Result<(
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
    use tracing::info;

    const TEST_CONFIRMATION_THRESHOLD: u32 = 6;

    let config = settings::load_config_file::<MonitorConfig>(Some(
        "config/monitor_config.yaml".to_string(),
    ))?;

    let bitcoind_config = BitcoindConfig::default();

    let bitcoind = Bitcoind::new(bitcoind_config, config.bitcoin.clone(), None);

    bitcoind.start()?;

    let bitcoin_client = BitcoinClient::new_from_config(&config.bitcoin)?;
    let wallet = bitcoin_client.init_wallet("test_wallet")?;

    info!("Mining 120 blocks to provide spendable UTXOs...");
    bitcoin_client.mine_blocks_to_address(120 as u64, &wallet)?;

    // Create storage
    let path = format!("test_outputs/{}", generate_random_string());
    let storage_config = storage_backend::storage_config::StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&storage_config)?);

    let indexer_store = IndexerStore::new(storage.clone(), TEST_CONFIRMATION_THRESHOLD)
        .map_err(|e| anyhow::anyhow!("Failed to create IndexerStore: {}", e))?;

    let indexer_settings = IndexerSettings {
        confirmation_threshold: 6,
        ..Default::default()
    };

    let indexer = Indexer::new(
        bitcoin_client,
        Rc::new(indexer_store),
        Some(indexer_settings.clone()),
    )?;

    let store = MonitorStore::new(storage)?;
    let monitor_settings = MonitorSettings {
        max_monitoring_confirmations: 2,
        indexer_settings: Some(indexer_settings),
    };

    let monitor = Monitor::new(indexer, store, monitor_settings)?;

    sync_monitor(&monitor)?;

    // Create a new BitcoinClient for the caller since the indexer took ownership
    let bitcoin_client_for_caller = BitcoinClient::new_from_config(&config.bitcoin)?;

    // Return monitor with indexer accessible via monitor.indexer
    Ok((bitcoin_client_for_caller, monitor, bitcoind))
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

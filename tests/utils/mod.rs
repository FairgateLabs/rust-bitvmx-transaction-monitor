// This module contains utility functions used across multiple test files.
// The dead_code warnings are false positives - these functions are used in other test modules.
#![allow(dead_code)]

use anyhow::Result;
use bitcoin::{Amount, Transaction, Txid};
use bitcoin_indexer::{config::IndexerSettings, indexer::Indexer, store::IndexerStore};
use bitcoincore_rpc::RpcApi;
use bitcoind::{bitcoind::Bitcoind, config::BitcoindConfig};
use bitvmx_bitcoin_rpc::bitcoin_client::{BitcoinClient, BitcoinClientApi};
use bitvmx_settings::settings;
use bitvmx_transaction_monitor::{
    config::MonitorConfig,
    monitor::Monitor,
    types::{AckMonitorNews, MonitorNews, OutputPatternFilter, TypesToMonitor},
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
    use bitvmx_transaction_monitor::{
        config::MonitorSettings, monitor::Monitor, store::MonitorStore,
    };

    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

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

    let indexer_store = IndexerStore::new(storage.clone())
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

    let monitor = Monitor {
        indexer,
        store,
        settings: monitor_settings,
    };

    sync_monitor(&monitor)?;

    let bitcoin_client = BitcoinClient::new_from_config(&config.bitcoin)?;

    Ok((bitcoin_client, monitor, bitcoind))
}

/// Creates and sends a funding transaction to an address.
/// Uses hardcoded wallet "test_wallet" and amount 1_000_000 satoshis (0.01 BTC).
/// Returns the transaction, its txid, and the vout index.
pub fn create_and_send_funding_transaction(
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

/// Current best block height.
pub fn best_height(bitcoin_client: &BitcoinClient) -> Result<u32> {
    Ok(bitcoin_client.get_best_block()?)
}

/// Block hash at a given height on the active chain.
pub fn block_hash_at(bitcoin_client: &BitcoinClient, height: u32) -> Result<bitcoin::BlockHash> {
    Ok(bitcoin_client.client.get_block_hash(height as u64)?)
}

/// Invalidate a block, which reorgs it and every block above it off the active chain. Any transactions
/// they contained return to the mempool, ready to be re-mined into a new branch.
pub fn invalidate_block(bitcoin_client: &BitcoinClient, hash: &bitcoin::BlockHash) -> Result<()> {
    bitcoin_client.invalidate_block(hash)?;
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

/// Helper function to create and send a transaction that spends a specific UTXO.
/// Returns the transaction and its txid.
pub fn create_and_send_spending_transaction(
    bitcoin_client: &bitvmx_bitcoin_rpc::bitcoin_client::BitcoinClient,
    target_txid: Txid,
    target_vout: u32,
) -> Result<(bitcoin::Transaction, Txid)> {
    let spending_amount = Amount::from_sat(800_000);
    let change_amount = Amount::from_sat(100_000); // second output keeps tx above minimum relay size

    // Get a recipient address and a change address
    let recipient_address = bitcoin_client
        .client
        .get_new_address(None, Some(bitcoincore_rpc::json::AddressType::Bech32))?;
    let change_address = bitcoin_client
        .client
        .get_new_address(None, Some(bitcoincore_rpc::json::AddressType::Bech32))?;

    // Create a raw transaction that spends the UTXO with two outputs
    let inputs = vec![bitcoincore_rpc::json::CreateRawTransactionInput {
        txid: target_txid,
        vout: target_vout,
        sequence: None,
    }];

    let mut outputs = std::collections::HashMap::new();
    let address_str = format!("{}", recipient_address.assume_checked());
    let change_str = format!("{}", change_address.assume_checked());
    outputs.insert(address_str, spending_amount);
    outputs.insert(change_str, change_amount);

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

    // Decode the signed transaction
    let transaction: bitcoin::Transaction =
        bitcoin::consensus::Decodable::consensus_decode(&mut &signed_tx.hex[..])?;
    let txid = transaction.compute_txid();

    // Send the transaction to the network
    bitcoin_client.client.send_raw_transaction(&signed_tx.hex)?;

    Ok((transaction, txid))
}

/// Helper function to assert SpendingUTXOTransaction news.
pub fn assert_spending_utxo_news(
    news: &MonitorNews,
    target_txid: Txid,
    target_vout: u32,
    spender_txid: Txid,
    extra_data: &str,
    spending_txid: Txid,
    confirmations: u32,
) -> Result<()> {
    match news {
        MonitorNews::SpendingUTXOTransaction(tx_id, vout, tx_status, context) => {
            assert_eq!(*tx_id, target_txid, "Expected target txid {}", target_txid);
            assert_eq!(*vout, target_vout, "Expected vout {}", target_vout);
            assert_eq!(context, extra_data, "Expected extra_data {}", extra_data);
            assert_eq!(
                tx_status.confirmations, confirmations,
                "Expected {} confirmations, got {}",
                confirmations, tx_status.confirmations
            );
            assert_eq!(
                tx_status.tx.as_ref().unwrap().compute_txid(),
                spender_txid,
                "Expected spender txid {}, got {}",
                spender_txid,
                tx_status.tx.as_ref().unwrap().compute_txid()
            );
            assert_eq!(
                tx_status.tx.as_ref().unwrap().compute_txid(),
                spending_txid,
                "Expected spending txid {}, got {}",
                spending_txid,
                tx_status.tx.as_ref().unwrap().compute_txid()
            );
        }
        _ => panic!("Expected SpendingUTXOTransaction news, got {:?}", news),
    }
    Ok(())
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

pub fn monitor_tx(
    monitor: &Monitor,
    tx_id: Txid,
    extra_data: &str,
    confirmation_trigger: Option<u32>,
) -> Result<()> {
    monitor.monitor(
        TypesToMonitor::Transactions(vec![tx_id], extra_data.to_string(), confirmation_trigger),
        true,
    )?;
    Ok(())
}

pub fn monitor_spending_utxo(
    monitor: &Monitor,
    tx_id: Txid,
    vout: u32,
    extra_data: &str,
    confirmation_trigger: Option<u32>,
) -> Result<()> {
    monitor.monitor(
        TypesToMonitor::SpendingUTXOTransaction(
            tx_id,
            vout,
            extra_data.to_string(),
            confirmation_trigger,
        ),
        true,
    )?;
    Ok(())
}

pub fn monitor_output_pattern(
    monitor: &Monitor,
    filter: OutputPatternFilter,
    confirmation_trigger: Option<u32>,
) -> Result<()> {
    monitor.monitor(
        TypesToMonitor::OutputPattern(filter, confirmation_trigger),
        true,
    )?;
    Ok(())
}

pub fn ack_tx_monitor(monitor: &Monitor, tx_id: Txid, extra_data: &str) -> Result<()> {
    monitor.ack_news(AckMonitorNews::Transaction(tx_id, extra_data.to_string()))?;
    Ok(())
}
/// Helper function to acknowledge SpendingUTXOTransaction news.
pub fn ack_spending_utxo_monitor(
    monitor: &bitvmx_transaction_monitor::monitor::Monitor,
    target_txid: Txid,
    target_vout: u32,
    extra_data: &str,
) -> Result<()> {
    monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        target_txid,
        target_vout,
        extra_data.to_string(),
    ))?;
    Ok(())
}

pub fn ack_output_pattern_monitor(monitor: &Monitor, tx_id: Txid, tag: Vec<u8>) -> Result<()> {
    monitor.ack_news(AckMonitorNews::OutputPatternTransaction(tx_id, tag))?;
    Ok(())
}

/// Helper function to assert OutputPatternTransaction news.
pub fn assert_output_pattern_news(
    news: &MonitorNews,
    expected_txid: Txid,
    expected_tag: &[u8],
    confirmations: u32,
) -> Result<()> {
    match news {
        MonitorNews::OutputPatternTransaction(tx_id, tx_status, tag) => {
            assert_eq!(
                *tx_id, expected_txid,
                "Expected output pattern txid {}",
                expected_txid
            );
            assert_eq!(
                tag.as_slice(),
                expected_tag,
                "Expected tag {:?}, got {:?}",
                expected_tag,
                tag
            );
            assert_eq!(
                tx_status.confirmations, confirmations,
                "Expected {} confirmations, got {}",
                confirmations, tx_status.confirmations
            );
        }
        _ => panic!("Expected OutputPatternTransaction news, got {:?}", news),
    }
    Ok(())
}

/// Creates and sends a transaction matching a given output pattern filter.
/// The transaction has an OP_RETURN at `filter.output_index` whose data starts with `filter.tag`,
/// plus a change output to keep the transaction above the minimum relay size.
pub fn create_and_send_output_pattern_transaction(
    bitcoin_client: &BitcoinClient,
    filter: &OutputPatternFilter,
) -> Result<(Transaction, Txid)> {
    use bitcoin::{
        absolute::LockTime,
        consensus::Encodable,
        opcodes::all::OP_RETURN,
        script::{Builder, PushBytesBuf},
        OutPoint, ScriptBuf, Sequence, TxIn, TxOut, Witness,
    };

    let (_, funding_txid, funding_vout) = create_and_send_funding_transaction(bitcoin_client)?;

    let wallet_address = bitcoin_client.init_wallet("test_wallet")?;

    // Build an OP_RETURN output whose data starts with the filter tag
    let push_data =
        PushBytesBuf::try_from(filter.tag.clone()).expect("filter tag too large for script push");
    let op_return_output = TxOut {
        value: Amount::ZERO,
        script_pubkey: Builder::new()
            .push_opcode(OP_RETURN)
            .push_slice(&push_data)
            .into_script(),
    };

    // Change output ensures the transaction is above the minimum relay size
    let change_output = TxOut {
        value: Amount::from_sat(900_000),
        script_pubkey: wallet_address.script_pubkey(),
    };

    // Build outputs: place OP_RETURN at the required output_index,
    // fill preceding slots with dust, then append the change output.
    let mut outputs: Vec<TxOut> = Vec::new();
    for _ in 0..filter.output_index {
        outputs.push(TxOut {
            value: Amount::from_sat(1000),
            script_pubkey: wallet_address.script_pubkey(),
        });
    }
    outputs.push(op_return_output);
    outputs.push(change_output);

    let transaction = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: funding_txid,
                vout: funding_vout,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: outputs,
    };

    let mut encoded = Vec::new();
    transaction.consensus_encode(&mut encoded)?;

    let signed_tx = bitcoin_client
        .client
        .sign_raw_transaction_with_wallet(&encoded, None, None)?;

    if !signed_tx.complete {
        return Err(anyhow::anyhow!(
            "Transaction signing incomplete: {:?}",
            signed_tx.errors
        ));
    }

    let transaction: Transaction =
        bitcoin::consensus::Decodable::consensus_decode(&mut &signed_tx.hex[..])?;

    let txid = transaction.compute_txid();

    info!("Sending OutputPattern Transaction({})", txid);
    bitcoin_client.client.send_raw_transaction(&signed_tx.hex)?;

    mine_blocks(bitcoin_client, 1)?;

    Ok((transaction, txid))
}

pub fn assert_tx_news(
    news: &MonitorNews,
    tx_id: Txid,
    extra_data: &str,
    confirmations: u32,
) -> Result<()> {
    match news {
        MonitorNews::Transaction(n) => {
            assert_eq!(n.tx_id, tx_id);
            assert_eq!(n.context, extra_data);
            assert_eq!(n.status.confirmations, confirmations);
        }
        _ => panic!("Expected Transaction news"),
    }
    Ok(())
}

/// Like `assert_tx_news`, but also asserts the reorg-resend flag on the notification.
pub fn assert_tx_news_reorg(
    news: &MonitorNews,
    tx_id: Txid,
    extra_data: &str,
    confirmations: u32,
    expected_resent_due_to_reorg: bool,
) -> Result<()> {
    match news {
        MonitorNews::Transaction(n) => {
            assert_eq!(n.tx_id, tx_id);
            assert_eq!(n.context, extra_data);
            assert_eq!(n.status.confirmations, confirmations);
            assert_eq!(
                n.resent_due_to_reorg, expected_resent_due_to_reorg,
                "resent_due_to_reorg mismatch"
            );
        }
        _ => panic!("Expected Transaction news"),
    }
    Ok(())
}

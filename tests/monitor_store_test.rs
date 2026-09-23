use bitcoin::{absolute::LockTime, BlockHash, OutPoint, Transaction, Txid};
use bitvmx_transaction_monitor::{
    store::MonitorStore,
    types::{OutputPatternFilter, TransactionMonitor, TypesToMonitor},
};
use std::{rc::Rc, str::FromStr};
use storage_backend::{storage::Storage, storage_config::StorageConfig};
use utils::{clear_output, generate_random_string};
mod utils;

fn test_store() -> Result<MonitorStore, anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    Ok(MonitorStore::new(storage))
}

fn transaction(lock_time: u32) -> Transaction {
    Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(lock_time).unwrap(),
        input: vec![],
        output: vec![],
    }
}

fn filter() -> OutputPatternFilter {
    OutputPatternFilter {
        output_index: 0,
        tag: vec![0xde, 0xad],
        max_outputs: None,
    }
}

/// Every kind of monitor is stored, read back, and removed.
#[test]
fn test_monitor_store_save_get_remove() -> Result<(), anyhow::Error> {
    let store = test_store()?;

    assert!(store.get_transaction_monitors()?.is_empty());
    assert!(store.get_spending_utxo_monitors()?.is_empty());
    assert!(store.get_output_pattern_monitors()?.is_empty());
    assert!(!store.is_monitoring_new_block()?);

    let tx_id = transaction(1653195600).compute_txid();
    let utxo_tx_id = transaction(1653195602).compute_txid();

    // Transactions.
    let tx_monitor = TypesToMonitor::Transactions(vec![tx_id], String::new(), None);
    store.add_monitor(tx_monitor.clone(), false)?;

    let monitors = store.get_transaction_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert_eq!(monitors[0].tx_id, tx_id);

    store.remove_monitor(tx_monitor)?;
    assert!(store.get_transaction_monitors()?.is_empty());

    // Output patterns.
    let op_monitor = TypesToMonitor::OutputPattern(filter(), None);
    store.add_monitor(op_monitor.clone(), false)?;

    let monitors = store.get_output_pattern_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert_eq!(monitors[0].filter, filter());

    store.remove_monitor(op_monitor)?;
    assert!(store.get_output_pattern_monitors()?.is_empty());

    // Spending UTXOs.
    let outpoint = OutPoint::new(utxo_tx_id, 1);
    let utxo_monitor = TypesToMonitor::SpendingUTXOTransaction(outpoint, String::new(), None);
    store.add_monitor(utxo_monitor.clone(), false)?;

    let monitors = store.get_spending_utxo_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert_eq!(monitors[0].outpoint, outpoint);

    store.remove_monitor(utxo_monitor)?;
    assert!(store.get_spending_utxo_monitors()?.is_empty());

    // New blocks.
    store.add_monitor(TypesToMonitor::NewBlock, false)?;
    assert!(store.is_monitoring_new_block()?);

    store.remove_monitor(TypesToMonitor::NewBlock)?;
    assert!(!store.is_monitoring_new_block()?);

    clear_output();

    Ok(())
}

/// Removing one monitor leaves the others alone, and removing it twice is harmless.
#[test]
fn test_monitor_store_remove_monitor() -> Result<(), anyhow::Error> {
    let store = test_store()?;

    let tx_id = Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000")?;
    let tx_id_1 =
        Txid::from_str("0000000000000000000000000000000000000000000000000000000000000001")?;

    let utxo_monitor =
        TypesToMonitor::SpendingUTXOTransaction(OutPoint::new(tx_id, 1), String::new(), None);
    store.add_monitor(utxo_monitor.clone(), false)?;

    let tx_monitor = TypesToMonitor::Transactions(vec![tx_id_1], String::new(), None);
    store.add_monitor(tx_monitor.clone(), false)?;

    store.remove_monitor(utxo_monitor.clone())?;
    assert!(store.get_spending_utxo_monitors()?.is_empty());
    assert_eq!(store.get_transaction_monitors()?.len(), 1);

    // Removing something that is no longer there changes nothing.
    store.remove_monitor(utxo_monitor)?;
    assert_eq!(store.get_transaction_monitors()?[0].tx_id, tx_id_1);

    store.remove_monitor(tx_monitor)?;
    assert!(store.get_transaction_monitors()?.is_empty());

    clear_output();

    Ok(())
}

/// A monitor can be added again after it was removed.
#[test]
fn test_monitor_store_add_after_remove() -> Result<(), anyhow::Error> {
    let store = test_store()?;

    let tx_id = transaction(1653195600).compute_txid();

    let monitor = TypesToMonitor::Transactions(vec![tx_id], "context".to_string(), None);
    store.add_monitor(monitor.clone(), false)?;
    store.remove_monitor(monitor.clone())?;
    assert!(store.get_transaction_monitors()?.is_empty());

    store.add_monitor(monitor, false)?;
    let monitors = store.get_transaction_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert_eq!(monitors[0].entries.len(), 1);

    clear_output();

    Ok(())
}

/// One transaction monitored under several contexts keeps one entry per context, and monitoring the same context
/// again replaces that entry.
#[test]
fn test_multiple_entries_same_txid() -> Result<(), anyhow::Error> {
    let store = test_store()?;

    let tx_id = transaction(1653195600).compute_txid();

    for (context, trigger) in [("extra1", 1), ("extra2", 2), ("extra3", 3)] {
        store.add_monitor(
            TypesToMonitor::Transactions(vec![tx_id], context.to_string(), Some(trigger)),
            false,
        )?;
    }

    let monitors = store.get_transaction_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert_eq!(monitors[0].entries.len(), 3);

    let trigger_of = |monitors: &[TransactionMonitor], context: &str| {
        monitors[0]
            .entries
            .iter()
            .find(|e| e.entry.context == context)
            .and_then(|e| e.entry.confirmation_trigger)
    };

    assert_eq!(trigger_of(&monitors, "extra1"), Some(1));
    assert_eq!(trigger_of(&monitors, "extra2"), Some(2));
    assert_eq!(trigger_of(&monitors, "extra3"), Some(3));

    // The same context again replaces its entry, and leaves the other contexts untouched.
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id], "extra1".to_string(), Some(10)),
        false,
    )?;

    let monitors = store.get_transaction_monitors()?;
    assert_eq!(monitors[0].entries.len(), 3);
    assert_eq!(trigger_of(&monitors, "extra1"), Some(10));
    assert_eq!(trigger_of(&monitors, "extra2"), Some(2));

    // Removing one context leaves the others.
    store.remove_monitor(TypesToMonitor::Transactions(
        vec![tx_id],
        "extra2".to_string(),
        None,
    ))?;

    let monitors = store.get_transaction_monitors()?;
    assert_eq!(monitors[0].entries.len(), 2);
    assert_eq!(trigger_of(&monitors, "extra2"), None);

    clear_output();

    Ok(())
}

/// The same UTXO monitored under several contexts behaves the same way as a transaction.
#[test]
fn test_spending_utxo_multiple_entries() -> Result<(), anyhow::Error> {
    let store = test_store()?;

    let tx_id = transaction(1653195600).compute_txid();

    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(
            OutPoint::new(tx_id, 0),
            "extra1".to_string(),
            Some(1),
        ),
        false,
    )?;
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(
            OutPoint::new(tx_id, 0),
            "extra2".to_string(),
            Some(2),
        ),
        false,
    )?;
    // A different vout is a different target.
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(
            OutPoint::new(tx_id, 1),
            "extra1".to_string(),
            Some(1),
        ),
        false,
    )?;

    let monitors = store.get_spending_utxo_monitors()?;
    assert_eq!(monitors.len(), 2);

    let vout_zero = monitors.iter().find(|m| m.outpoint.vout == 0).unwrap();
    assert_eq!(vout_zero.entries.len(), 2);

    // The same context again replaces its entry.
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(
            OutPoint::new(tx_id, 0),
            "extra1".to_string(),
            Some(10),
        ),
        false,
    )?;

    let monitors = store.get_spending_utxo_monitors()?;
    let vout_zero = monitors.iter().find(|m| m.outpoint.vout == 0).unwrap();
    assert_eq!(vout_zero.entries.len(), 2);
    assert_eq!(
        vout_zero
            .entries
            .iter()
            .find(|e| e.context == "extra1")
            .and_then(|e| e.confirmation_trigger),
        Some(10)
    );

    // Removing one context leaves the other, and the target goes when its last context does.
    store.remove_monitor(TypesToMonitor::SpendingUTXOTransaction(
        OutPoint::new(tx_id, 0),
        "extra1".to_string(),
        None,
    ))?;
    store.remove_monitor(TypesToMonitor::SpendingUTXOTransaction(
        OutPoint::new(tx_id, 0),
        "extra2".to_string(),
        None,
    ))?;

    let monitors = store.get_spending_utxo_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert_eq!(monitors[0].outpoint.vout, 1);

    clear_output();

    Ok(())
}

/// The block a transaction was last notified in is stored per context, and monitoring the same context again keeps it.
#[test]
fn test_transaction_notified_block_hash() -> Result<(), anyhow::Error> {
    let store = test_store()?;

    let tx_id = transaction(1653195600).compute_txid();
    let block_hash =
        BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000abc")?;

    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id], "extra1".to_string(), Some(1)),
        false,
    )?;
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id], "extra2".to_string(), Some(2)),
        false,
    )?;

    assert_eq!(
        store.get_transaction_notified_block_hash(tx_id, "extra1")?,
        None
    );

    store.update_transaction_notified_block_hash(tx_id, "extra1", Some(block_hash))?;

    assert_eq!(
        store.get_transaction_notified_block_hash(tx_id, "extra1")?,
        Some(block_hash)
    );
    // Only the context that was notified carries it.
    assert_eq!(
        store.get_transaction_notified_block_hash(tx_id, "extra2")?,
        None
    );

    // Monitoring the same context again keeps the block it was notified in.
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id], "extra1".to_string(), Some(5)),
        false,
    )?;
    assert_eq!(
        store.get_transaction_notified_block_hash(tx_id, "extra1")?,
        Some(block_hash)
    );

    // A transaction or a context that is not monitored has none.
    let unknown_tx_id =
        Txid::from_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")?;
    assert_eq!(
        store.get_transaction_notified_block_hash(unknown_tx_id, "extra1")?,
        None
    );
    assert_eq!(
        store.get_transaction_notified_block_hash(tx_id, "unknown")?,
        None
    );

    clear_output();

    Ok(())
}

/// Removing an entry that does not exist leaves the stored monitors untouched.
#[test]
fn test_edge_cases_non_existent_entries() -> Result<(), anyhow::Error> {
    let store = test_store()?;

    let tx_id = transaction(1653195600).compute_txid();

    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id], "extra1".to_string(), None),
        false,
    )?;

    // A context that was never monitored.
    store.remove_monitor(TypesToMonitor::Transactions(
        vec![tx_id],
        "wrong_extra".to_string(),
        None,
    ))?;
    assert_eq!(store.get_transaction_monitors()?.len(), 1);

    // A transaction that was never monitored.
    let unknown_tx_id =
        Txid::from_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")?;
    store.remove_monitor(TypesToMonitor::Transactions(
        vec![unknown_tx_id],
        "extra1".to_string(),
        None,
    ))?;
    assert_eq!(store.get_transaction_monitors()?.len(), 1);

    // An output pattern that was never monitored.
    store.remove_monitor(TypesToMonitor::OutputPattern(filter(), None))?;
    assert!(store.get_output_pattern_monitors()?.is_empty());

    clear_output();

    Ok(())
}

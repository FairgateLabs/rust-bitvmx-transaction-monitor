use bitcoin::{absolute::LockTime, Transaction};
use bitvmx_transaction_monitor::store::{MonitorStore, MonitorStoreApi, TypesToMonitorStore};
use std::{path::PathBuf, rc::Rc};
use storage_backend::storage::Storage;
use utils::generate_random_string;
mod utils;

/// This test verifies the functionality of the MonitorStore implementation.
/// It tests the following operations:
/// 1. Saving different types of transaction monitors (SingleTransaction, GroupTransaction,
///    RskPeginTransaction, SpendingUTXOTransaction)
/// 2. Retrieving monitors based on block height filtering
/// 3. Removing monitors
#[test]
fn test_monitor_store_save_get_remove() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/address_test/{}", generate_random_string());
    let storage = Rc::new(Storage::new_with_path(&PathBuf::from(path))?);
    let store = MonitorStore::new(storage)?;

    // Verify initial state - no monitors
    let monitors = store.get_monitors(0)?;
    assert_eq!(monitors.len(), 0);

    let tx1 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };
    let tx2 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195601).unwrap(),
        input: vec![],
        output: vec![],
    };
    let tx3 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195602).unwrap(),
        input: vec![],
        output: vec![],
    };

    // Test get_monitors and save_monitor with all transaction types
    use bitvmx_transaction_monitor::types::TypesToMonitor;
    use uuid::Uuid;

    // 1. Test SingleTransaction
    let single_tx_monitor = TypesToMonitor::Transactions(vec![tx1.compute_txid()], String::new());

    store.save_monitor(single_tx_monitor.clone(), 100)?;
    let monitors = store.get_monitors(0)?;
    assert_eq!(monitors.len(), 0);
    let monitors = store.get_monitors(100)?;
    assert!(matches!(
        monitors[0],
        TypesToMonitorStore::Transaction(tx_id, _) if tx_id == tx1.compute_txid()
    ));

    store.remove_monitor(single_tx_monitor.clone())?;

    let monitors = store.get_monitors(100)?;
    assert_eq!(monitors.len(), 0);

    // 2. Test GroupTransaction
    let group_id = Uuid::new_v4();
    let group_tx_monitor =
        TypesToMonitor::Transactions(vec![tx2.compute_txid()], group_id.to_string());
    store.save_monitor(group_tx_monitor.clone(), 200)?;
    let monitors = store.get_monitors(200)?;
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::Transaction(tx_id, _) if tx_id == tx2.compute_txid()
    ));
    store.remove_monitor(group_tx_monitor.clone())?;
    let monitors = store.get_monitors(200)?;
    assert_eq!(monitors.len(), 0);

    // 3. Test RskPeginTransaction
    let rsk_monitor = TypesToMonitor::RskPeginTransaction;
    store.save_monitor(rsk_monitor.clone(), 300)?;
    let monitors = store.get_monitors(300)?;
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::RskPeginTransaction
    ));
    store.remove_monitor(rsk_monitor.clone())?;
    let monitors = store.get_monitors(300)?;
    assert_eq!(monitors.len(), 0);

    // 4. Test SpendingUTXOTransaction
    let utxo_monitor =
        TypesToMonitor::SpendingUTXOTransaction(tx3.compute_txid(), 1, String::new());
    store.save_monitor(utxo_monitor.clone(), 400)?;
    let monitors = store.get_monitors(400)?;
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::SpendingUTXOTransaction(tx_id, utxo_index, _)
            if tx_id == tx3.compute_txid() && utxo_index == 1
    ));
    store.remove_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors(400)?;
    assert_eq!(monitors.len(), 0);

    Ok(())
}

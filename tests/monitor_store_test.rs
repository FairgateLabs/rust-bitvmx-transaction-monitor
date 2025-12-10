use bitcoin::{absolute::LockTime, Transaction, Txid};
use bitvmx_transaction_monitor::{
    store::{MonitorStore, MonitorStoreApi, TypesToMonitorStore},
    types::TypesToMonitor,
};
use std::{rc::Rc, str::FromStr};
use storage_backend::{storage::Storage, storage_config::StorageConfig};
use utils::{clear_output, generate_random_string};
mod utils;

/// This test verifies the functionality of the MonitorStore implementation.
/// It tests the following operations:
/// 1. Saving different types of transaction monitors (Transactions,
///    RskPeginTransaction, SpendingUTXOTransaction, NewBlock)
/// 2. Removing monitors
#[test]
fn test_monitor_store_save_get_remove() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    // Verify initial state - no monitors
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    let tx1 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
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

    // 1. Test One Transaction
    let one_tx_monitor = TypesToMonitor::Transactions(vec![tx1.compute_txid()], String::new());

    store.add_monitor(one_tx_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert!(matches!(
        monitors[0],
        TypesToMonitorStore::Transaction(tx_id, _) if tx_id == tx1.compute_txid()
    ));

    store.deactivate_monitor(one_tx_monitor.clone())?;

    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    // 3. Test RskPeginTransaction
    let rsk_monitor = TypesToMonitor::RskPeginTransaction;
    store.add_monitor(rsk_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::RskPeginTransaction
    ));
    store.deactivate_monitor(rsk_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    // 4. Test SpendingUTXOTransaction
    let utxo_monitor =
        TypesToMonitor::SpendingUTXOTransaction(tx3.compute_txid(), 1, String::new());
    store.add_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::SpendingUTXOTransaction(tx_id, utxo_index, _, _)
            if tx_id == tx3.compute_txid() && utxo_index == 1
    ));
    store.deactivate_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    clear_output();

    Ok(())
}

#[test]
fn test_monitor_store_cancel_monitor() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    let tx_id = Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000")?;
    let tx_id_1 =
        Txid::from_str("0000000000000000000000000000000000000000000000000000000000000001")?;

    let utxo_monitor = TypesToMonitor::SpendingUTXOTransaction(tx_id, 1, String::new());
    store.add_monitor(utxo_monitor.clone())?;

    let tx_monitor = TypesToMonitor::Transactions(vec![tx_id_1], String::new());
    store.add_monitor(tx_monitor.clone())?;

    // Cancel utxo monitor
    store.cancel_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;

    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::Transaction(tx, _) if tx == tx_id_1
    ));

    // Cancel utxo monitor again
    store.cancel_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::Transaction(tx, _) if tx == tx_id_1
    ));

    store.cancel_monitor(tx_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    Ok(())
}

#[test]
fn test_monitor_store_cancel_deactivated_transaction_monitor() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    let tx_id_active =
        Txid::from_str("1000000000000000000000000000000000000000000000000000000000000000")?;
    let tx_id_inactive =
        Txid::from_str("2000000000000000000000000000000000000000000000000000000000000000")?;

    let active_monitor = TypesToMonitor::Transactions(vec![tx_id_active], String::new());
    store.add_monitor(active_monitor.clone())?;

    let inactive_monitor = TypesToMonitor::Transactions(vec![tx_id_inactive], String::new());
    store.add_monitor(inactive_monitor.clone())?;

    store.cancel_monitor(inactive_monitor.clone())?;

    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0],
        TypesToMonitorStore::Transaction(tx, _) if tx == tx_id_active
    ));

    store.cancel_monitor(active_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert!(monitors.is_empty());

    Ok(())
}

#[test]
fn test_update_spending_utxo_monitor() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    let tx_id = Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000")?;
    let spender_tx_id_1 =
        Txid::from_str("1000000000000000000000000000000000000000000000000000000000000000")?;
    let spender_tx_id_2 =
        Txid::from_str("2000000000000000000000000000000000000000000000000000000000000000")?;

    // Add a SpendingUTXOTransaction monitor
    let utxo_monitor = TypesToMonitor::SpendingUTXOTransaction(tx_id, 1, String::new());
    store.add_monitor(utxo_monitor.clone())?;

    // Verify initial state - spending tx_id should be None
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::SpendingUTXOTransaction(t, u, _, None)
            if t == tx_id && u == 1
    ));

    // Update with a spending transaction ID
    store.update_spending_utxo_monitor((tx_id, 1, Some(spender_tx_id_1)))?;

    // Verify the spending tx_id was updated
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::SpendingUTXOTransaction(t, u, _, Some(stx))
            if t == tx_id && u == 1 && stx == spender_tx_id_1
    ));

    // Update with a different spending transaction ID
    store.update_spending_utxo_monitor((tx_id, 1, Some(spender_tx_id_2)))?;

    // Verify the spending tx_id was updated again
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::SpendingUTXOTransaction(t, u, _, Some(stx))
            if t == tx_id && u == 1 && stx == spender_tx_id_2
    ));

    // Update with None (orphan case)
    store.update_spending_utxo_monitor((tx_id, 1, None))?;

    // Verify the spending tx_id was set back to None
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::SpendingUTXOTransaction(t, u, _, None)
            if t == tx_id && u == 1
    ));

    clear_output();

    Ok(())
}

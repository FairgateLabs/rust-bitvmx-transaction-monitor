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
    let one_tx_monitor =
        TypesToMonitor::Transactions(vec![tx1.compute_txid()], String::new(), None);

    store.add_monitor(one_tx_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert!(matches!(
        monitors[0],
        TypesToMonitorStore::Transaction(tx_id, _, _) if tx_id == tx1.compute_txid()
    ));

    store.deactivate_monitor(one_tx_monitor.clone())?;

    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    // 3. Test RskPeginTransaction
    let rsk_monitor = TypesToMonitor::RskPegin(None);
    store.add_monitor(rsk_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::RskPegin(_)
    ));
    store.deactivate_monitor(rsk_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    // 4. Test SpendingUTXOTransaction
    let utxo_monitor =
        TypesToMonitor::SpendingUTXOTransaction(tx3.compute_txid(), 1, String::new(), None);
    store.add_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::SpendingUTXOTransaction(tx_id, utxo_index, _, _, _)
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

    let utxo_monitor = TypesToMonitor::SpendingUTXOTransaction(tx_id, 1, String::new(), None);
    store.add_monitor(utxo_monitor.clone())?;

    let tx_monitor = TypesToMonitor::Transactions(vec![tx_id_1], String::new(), None);
    store.add_monitor(tx_monitor.clone())?;

    // Cancel utxo monitor
    store.cancel_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;

    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::Transaction(tx, _, _) if tx == tx_id_1
    ));

    // Cancel utxo monitor again
    store.cancel_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::Transaction(tx, _, _) if tx == tx_id_1
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

    let active_monitor = TypesToMonitor::Transactions(vec![tx_id_active], String::new(), None);
    store.add_monitor(active_monitor.clone())?;

    let inactive_monitor = TypesToMonitor::Transactions(vec![tx_id_inactive], String::new(), None);
    store.add_monitor(inactive_monitor.clone())?;

    store.cancel_monitor(inactive_monitor.clone())?;

    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0],
        TypesToMonitorStore::Transaction(tx, _, _) if tx == tx_id_active
    ));

    store.cancel_monitor(active_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert!(monitors.is_empty());

    Ok(())
}

/// This test verifies that active and inactive monitors are stored in separate keys.
/// It tests:
/// 1. Active monitors are returned by get_monitors
/// 2. Inactive monitors are not returned by get_monitors
/// 3. Deactivating moves monitors from active to inactive
/// 4. Reactivating (adding a deactivated monitor) moves it back to active
/// 5. Multiple monitors can coexist in active and inactive states
#[test]
fn test_active_inactive_monitor_separation() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

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

    let tx_id1 = tx1.compute_txid();
    let tx_id2 = tx2.compute_txid();
    let tx_id3 = tx3.compute_txid();

    // Add three transaction monitors
    store.add_monitor(TypesToMonitor::Transactions(
        vec![tx_id1],
        "extra1".to_string(),
        None,
    ))?;
    store.add_monitor(TypesToMonitor::Transactions(
        vec![tx_id2],
        "extra2".to_string(),
        None,
    ))?;
    store.add_monitor(TypesToMonitor::Transactions(
        vec![tx_id3],
        "extra3".to_string(),
        None,
    ))?;

    // All three should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id1)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id2)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id3)));

    // Deactivate tx_id2
    store.deactivate_monitor(TypesToMonitor::Transactions(
        vec![tx_id2],
        String::new(),
        None,
    ))?;

    // Only tx_id1 and tx_id3 should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id1)));
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id2)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id3)));

    // Deactivate tx_id1 as well
    store.deactivate_monitor(TypesToMonitor::Transactions(
        vec![tx_id1],
        String::new(),
        None,
    ))?;

    // Only tx_id3 should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id1)));
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id2)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id3)));

    // Reactivate tx_id2 (add it again)
    store.add_monitor(TypesToMonitor::Transactions(
        vec![tx_id2],
        "extra2_reactivated".to_string(),
        None,
    ))?;

    // tx_id2 and tx_id3 should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id1)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id2)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id3)));

    // Cancel tx_id2 (should remove from both active and inactive)
    store.cancel_monitor(TypesToMonitor::Transactions(
        vec![tx_id2],
        String::new(),
        None,
    ))?;

    // Only tx_id3 should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id3)));

    // Reactivate tx_id1
    store.add_monitor(TypesToMonitor::Transactions(
        vec![tx_id1],
        "extra1_reactivated".to_string(),
        None,
    ))?;

    // tx_id1 and tx_id3 should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id1)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id3)));

    clear_output();

    Ok(())
}

/// This test verifies active/inactive separation for RskPeginTransaction and NewBlock monitors
#[test]
fn test_active_inactive_boolean_monitors() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    // Test RskPeginTransaction
    store.add_monitor(TypesToMonitor::RskPegin(None))?;
    let monitors = store.get_monitors()?;
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_))));

    store.deactivate_monitor(TypesToMonitor::RskPegin(None))?;
    let monitors = store.get_monitors()?;
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_))));

    // Reactivate
    store.add_monitor(TypesToMonitor::RskPegin(None))?;
    let monitors = store.get_monitors()?;
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_))));

    // Cancel
    store.cancel_monitor(TypesToMonitor::RskPegin(None))?;
    let monitors = store.get_monitors()?;
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_))));

    // Test NewBlock
    store.add_monitor(TypesToMonitor::NewBlock)?;
    let monitors = store.get_monitors()?;
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::NewBlock)));

    store.deactivate_monitor(TypesToMonitor::NewBlock)?;
    let monitors = store.get_monitors()?;
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::NewBlock)));

    // Reactivate
    store.add_monitor(TypesToMonitor::NewBlock)?;
    let monitors = store.get_monitors()?;
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::NewBlock)));

    // Cancel
    store.cancel_monitor(TypesToMonitor::NewBlock)?;
    let monitors = store.get_monitors()?;
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::NewBlock)));

    clear_output();

    Ok(())
}

/// This test verifies active/inactive separation for SpendingUTXOTransaction monitors
#[test]
fn test_active_inactive_spending_utxo_monitors() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

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

    let tx_id1 = tx1.compute_txid();
    let tx_id2 = tx2.compute_txid();

    // Add two UTXO monitors
    store.add_monitor(TypesToMonitor::SpendingUTXOTransaction(
        tx_id1,
        0,
        "extra1".to_string(),
        None,
    ))?;
    store.add_monitor(TypesToMonitor::SpendingUTXOTransaction(
        tx_id1,
        1,
        "extra2".to_string(),
        None,
    ))?;
    store.add_monitor(TypesToMonitor::SpendingUTXOTransaction(
        tx_id2,
        0,
        "extra3".to_string(),
        None,
    ))?;

    // All three should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3);
    assert!(monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, idx, _, _, _) if *id == tx_id1 && *idx == 0)));
    assert!(monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, idx, _, _, _) if *id == tx_id1 && *idx == 1)));
    assert!(monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, idx, _, _, _) if *id == tx_id2 && *idx == 0)));

    // Deactivate one
    store.deactivate_monitor(TypesToMonitor::SpendingUTXOTransaction(
        tx_id1,
        0,
        String::new(),
        None,
    ))?;

    // Two should remain active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);
    assert!(!monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, idx, _, _, _) if *id == tx_id1 && *idx == 0)));
    assert!(monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, idx, _, _, _) if *id == tx_id1 && *idx == 1)));
    assert!(monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, idx, _, _, _) if *id == tx_id2 && *idx == 0)));

    // Reactivate
    store.add_monitor(TypesToMonitor::SpendingUTXOTransaction(
        tx_id1,
        0,
        "extra1_reactivated".to_string(),
        None,
    ))?;

    // All three should be active again
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3);

    // Cancel one
    store.cancel_monitor(TypesToMonitor::SpendingUTXOTransaction(
        tx_id1,
        0,
        String::new(),
        None,
    ))?;

    // Two should remain
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);

    clear_output();

    Ok(())
}

/// This test specifically verifies reactivating monitors after they have been deactivated.
/// It tests that calling add_monitor on a previously deactivated monitor reactivates it.
#[test]
fn test_reactivate_monitor() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

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

    let tx_id1 = tx1.compute_txid();
    let tx_id2 = tx2.compute_txid();

    // Test reactivating Transactions monitor
    let tx_monitor = TypesToMonitor::Transactions(vec![tx_id1], "extra1".to_string(), None);
    store.add_monitor(tx_monitor.clone())?;

    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id1)));

    // Deactivate
    store.deactivate_monitor(tx_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    // Reactivate by calling add_monitor again
    store.add_monitor(TypesToMonitor::Transactions(
        vec![tx_id1],
        "extra1_reactivated".to_string(),
        None,
    ))?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _) if *id == tx_id1)));

    // Test reactivating RskPeginTransaction monitor
    store.add_monitor(TypesToMonitor::RskPegin(None))?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2); // tx_id1 + RskPeginTransaction
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_))));

    store.deactivate_monitor(TypesToMonitor::RskPegin(None))?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1); // Only tx_id1

    // Reactivate
    store.add_monitor(TypesToMonitor::RskPegin(None))?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_))));

    // Test reactivating SpendingUTXOTransaction monitor
    let utxo_monitor =
        TypesToMonitor::SpendingUTXOTransaction(tx_id2, 0, "extra2".to_string(), None);
    store.add_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3); // tx_id1 + RskPeginTransaction + utxo

    store.deactivate_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2); // tx_id1 + RskPeginTransaction

    // Reactivate
    store.add_monitor(TypesToMonitor::SpendingUTXOTransaction(
        tx_id2,
        0,
        "extra2_reactivated".to_string(),
        None,
    ))?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3);
    assert!(monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, idx, _, _, _) if *id == tx_id2 && *idx == 0)));

    // Test reactivating NewBlock monitor
    store.add_monitor(TypesToMonitor::NewBlock)?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 4); // All monitors

    store.deactivate_monitor(TypesToMonitor::NewBlock)?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3); // Without NewBlock

    // Reactivate
    store.add_monitor(TypesToMonitor::NewBlock)?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 4);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::NewBlock)));

    clear_output();

    Ok(())
}

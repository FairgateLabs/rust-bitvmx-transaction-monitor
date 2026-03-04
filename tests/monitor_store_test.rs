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

    store.add_monitor(one_tx_monitor.clone(), false)?;
    let monitors = store.get_monitors()?;
    assert!(matches!(
        monitors[0],
        TypesToMonitorStore::Transaction(tx_id, _, _, _) if tx_id == tx1.compute_txid()
    ));

    store.deactivate_monitor(one_tx_monitor.clone())?;

    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    // 3. Test RskPeginTransaction
    let rsk_monitor = TypesToMonitor::RskPegin(None);
    store.add_monitor(rsk_monitor.clone(), false)?;
    let monitors = store.get_monitors()?;
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::RskPegin(_, _)
    ));
    store.deactivate_monitor(rsk_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    // 4. Test SpendingUTXOTransaction
    let utxo_monitor =
        TypesToMonitor::SpendingUTXOTransaction(tx3.compute_txid(), 1, String::new(), None);
    store.add_monitor(utxo_monitor.clone(), false)?;
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
    store.add_monitor(utxo_monitor.clone(), false)?;

    let tx_monitor = TypesToMonitor::Transactions(vec![tx_id_1], String::new(), None);
    store.add_monitor(tx_monitor.clone(), false)?;

    // Cancel utxo monitor
    store.cancel_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;

    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::Transaction(tx, _, _, _) if tx == tx_id_1
    ));

    // Cancel utxo monitor again
    store.cancel_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::Transaction(tx, _, _, _) if tx == tx_id_1
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
    store.add_monitor(active_monitor.clone(), false)?;

    let inactive_monitor = TypesToMonitor::Transactions(vec![tx_id_inactive], String::new(), None);
    store.add_monitor(inactive_monitor.clone(), false)?;

    store.cancel_monitor(inactive_monitor.clone())?;

    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0],
        TypesToMonitorStore::Transaction(tx, _, _, _) if tx == tx_id_active
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
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra1".to_string(), None),
        false,
    )?;
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id2], "extra2".to_string(), None),
        false,
    )?;
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id3], "extra3".to_string(), None),
        false,
    )?;

    // All three should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id1)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id2)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id3)));

    // Deactivate tx_id2 (using the same extra_data that was used when adding)
    store.deactivate_monitor(TypesToMonitor::Transactions(
        vec![tx_id2],
        "extra2".to_string(),
        None,
    ))?;

    // Only tx_id1 and tx_id3 should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id1)));
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id2)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id3)));

    // Deactivate tx_id1 as well (using the same extra_data that was used when adding)
    store.deactivate_monitor(TypesToMonitor::Transactions(
        vec![tx_id1],
        "extra1".to_string(),
        None,
    ))?;

    // Only tx_id3 should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id1)));
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id2)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id3)));

    // Reactivate tx_id2 (add it again)
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id2], "extra2_reactivated".to_string(), None),
        false,
    )?;

    // tx_id2 and tx_id3 should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id1)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id2)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id3)));

    // Cancel tx_id2 (should remove from both active and inactive)
    // Cancel the reactivated entry with "extra2_reactivated"
    store.cancel_monitor(TypesToMonitor::Transactions(
        vec![tx_id2],
        "extra2_reactivated".to_string(),
        None,
    ))?;

    // Only tx_id3 should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id3)));

    // Reactivate tx_id1
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra1_reactivated".to_string(), None),
        false,
    )?;

    // tx_id1 and tx_id3 should be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id1)));
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id3)));

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
    store.add_monitor(TypesToMonitor::RskPegin(None), false)?;
    let monitors = store.get_monitors()?;
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_, _))));

    store.deactivate_monitor(TypesToMonitor::RskPegin(None))?;
    let monitors = store.get_monitors()?;
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_, _))));

    // Reactivate
    store.add_monitor(TypesToMonitor::RskPegin(None), false)?;
    let monitors = store.get_monitors()?;
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_, _))));

    // Cancel
    store.cancel_monitor(TypesToMonitor::RskPegin(None))?;
    let monitors = store.get_monitors()?;
    assert!(!monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_, _))));

    // Test NewBlock
    store.add_monitor(TypesToMonitor::NewBlock, false)?;
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
    store.add_monitor(TypesToMonitor::NewBlock, false)?;
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
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(tx_id1, 0, "extra1".to_string(), None),
        false,
    )?;
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(tx_id1, 1, "extra2".to_string(), None),
        false,
    )?;
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(tx_id2, 0, "extra3".to_string(), None),
        false,
    )?;

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
        "extra1".to_string(),
        None,
    ))?;

    // Two should remain active
    let monitors = store.get_monitors()?;

    assert_eq!(monitors.len(), 2);
    assert!(!monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, idx, _, _, _) if *id == tx_id1 && *idx == 0)));
    assert!(monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, idx, _, _, _) if *id == tx_id1 && *idx == 1)));
    assert!(monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, idx, _, _, _) if *id == tx_id2 && *idx == 0)));

    // Reactivate
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(tx_id1, 0, "extra1_reactivated".to_string(), None),
        false,
    )?;

    // All three should be active again
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3);

    // Cancel one monitor
    store.cancel_monitor(TypesToMonitor::SpendingUTXOTransaction(
        tx_id1,
        1,
        "extra2".to_string(),
        None,
    ))?;

    // Two should remain
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);

    store.cancel_monitor(TypesToMonitor::SpendingUTXOTransaction(
        tx_id1,
        0,
        "extra1_reactivated".to_string(),
        None,
    ))?;

    store.cancel_monitor(TypesToMonitor::SpendingUTXOTransaction(
        tx_id2,
        0,
        "extra3".to_string(),
        None,
    ))?;

    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

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
    store.add_monitor(tx_monitor.clone(), false)?;

    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id1)));

    // Deactivate
    store.deactivate_monitor(tx_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    // Reactivate by calling add_monitor again
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra1_reactivated".to_string(), None),
        false,
    )?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::Transaction(id, _, _, _) if *id == tx_id1)));

    // Test reactivating RskPeginTransaction monitor
    store.add_monitor(TypesToMonitor::RskPegin(None), false)?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2); // tx_id1 + RskPeginTransaction
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_, _))));

    store.deactivate_monitor(TypesToMonitor::RskPegin(None))?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1); // Only tx_id1

    // Reactivate
    store.add_monitor(TypesToMonitor::RskPegin(None), false)?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::RskPegin(_, _))));

    // Test reactivating SpendingUTXOTransaction monitor
    let utxo_monitor =
        TypesToMonitor::SpendingUTXOTransaction(tx_id2, 0, "extra2".to_string(), None);
    store.add_monitor(utxo_monitor.clone(), false)?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3); // tx_id1 + RskPeginTransaction + utxo

    store.deactivate_monitor(utxo_monitor.clone())?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2); // tx_id1 + RskPeginTransaction

    // Reactivate
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(tx_id2, 0, "extra2_reactivated".to_string(), None),
        false,
    )?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3);
    assert!(monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, idx, _, _, _) if *id == tx_id2 && *idx == 0)));

    // Test reactivating NewBlock monitor
    store.add_monitor(TypesToMonitor::NewBlock, false)?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 4); // All monitors

    store.deactivate_monitor(TypesToMonitor::NewBlock)?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3); // Without NewBlock

    // Reactivate
    store.add_monitor(TypesToMonitor::NewBlock, false)?;
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 4);
    assert!(monitors
        .iter()
        .any(|m| matches!(m, TypesToMonitorStore::NewBlock)));

    clear_output();

    Ok(())
}

/// This test verifies that multiple entries can be added for the same txid with different extra_data
#[test]
fn test_multiple_entries_same_txid() -> Result<(), anyhow::Error> {
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

    let tx_id1 = tx1.compute_txid();

    // Add same txid with different extra_data values
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra1".to_string(), Some(1)),
        false,
    )?;
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra2".to_string(), Some(2)),
        false,
    )?;
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra3".to_string(), Some(3)),
        false,
    )?;

    // All three entries should be present
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3);

    let tx_monitors: Vec<_> = monitors
        .iter()
        .filter_map(|m| match m {
            TypesToMonitorStore::Transaction(id, extra, conf, _) if *id == tx_id1 => {
                Some((extra.clone(), *conf))
            }
            _ => None,
        })
        .collect();

    assert_eq!(tx_monitors.len(), 3);
    assert!(tx_monitors
        .iter()
        .any(|(e, c)| e == "extra1" && *c == Some(1)));
    assert!(tx_monitors
        .iter()
        .any(|(e, c)| e == "extra2" && *c == Some(2)));
    assert!(tx_monitors
        .iter()
        .any(|(e, c)| e == "extra3" && *c == Some(3)));

    // Update existing entry with same extra_data should update confirmation_trigger
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra1".to_string(), Some(10)),
        false,
    )?;

    let monitors = store.get_monitors()?;
    let tx_monitors: Vec<_> = monitors
        .iter()
        .filter_map(|m| match m {
            TypesToMonitorStore::Transaction(id, extra, conf, _) if *id == tx_id1 => {
                Some((extra.clone(), *conf))
            }
            _ => None,
        })
        .collect();

    // Should still have 3 entries, but extra1 should have updated confirmation
    assert_eq!(tx_monitors.len(), 3);
    assert!(tx_monitors
        .iter()
        .any(|(e, c)| e == "extra1" && *c == Some(10)));
    assert!(tx_monitors
        .iter()
        .any(|(e, c)| e == "extra2" && *c == Some(2)));
    assert!(tx_monitors
        .iter()
        .any(|(e, c)| e == "extra3" && *c == Some(3)));

    clear_output();
    Ok(())
}

/// This test verifies get_transaction_trigger_sent and update_transaction_trigger_sent
#[test]
fn test_transaction_trigger_sent() -> Result<(), anyhow::Error> {
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

    let tx_id1 = tx1.compute_txid();

    // Add monitor with extra_data
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra1".to_string(), Some(1)),
        false,
    )?;

    // Initially trigger_sent should be false
    let trigger_sent = store.get_transaction_trigger_sent(tx_id1, "extra1")?;
    assert_eq!(trigger_sent, false);

    // Update trigger_sent to true
    store.update_transaction_trigger_sent(tx_id1, "extra1", true)?;
    let trigger_sent = store.get_transaction_trigger_sent(tx_id1, "extra1")?;
    assert_eq!(trigger_sent, true);

    // Add another entry with different extra_data
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra2".to_string(), Some(2)),
        false,
    )?;

    // extra2 should have trigger_sent = false
    let trigger_sent = store.get_transaction_trigger_sent(tx_id1, "extra2")?;
    assert_eq!(trigger_sent, false);

    // extra1 should still be true
    let trigger_sent = store.get_transaction_trigger_sent(tx_id1, "extra1")?;
    assert_eq!(trigger_sent, true);

    // Update extra2 trigger_sent
    store.update_transaction_trigger_sent(tx_id1, "extra2", true)?;
    let trigger_sent = store.get_transaction_trigger_sent(tx_id1, "extra2")?;
    assert_eq!(trigger_sent, true);

    // Test error case - non-existent txid
    let non_existent_txid =
        Txid::from_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")?;
    let result = store.get_transaction_trigger_sent(non_existent_txid, "extra1");
    assert!(result.is_err());

    // Test error case - non-existent extra_data
    let result = store.get_transaction_trigger_sent(tx_id1, "non_existent");
    assert!(result.is_err());

    clear_output();
    Ok(())
}

/// This test verifies update_spending_utxo_monitor and multiple entries for same (txid, vout)
#[test]
fn test_spending_utxo_multiple_entries_and_update() -> Result<(), anyhow::Error> {
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

    // Add same (txid, vout) with different extra_data values
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(tx_id1, 0, "extra1".to_string(), Some(1)),
        false,
    )?;
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(tx_id1, 0, "extra2".to_string(), Some(2)),
        false,
    )?;

    // Both entries should be present
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);

    // Update spender_tx_id for all entries of (tx_id1, 0)
    store.update_spending_utxo_monitor((tx_id1, 0, Some(tx_id2)))?;

    // Verify both entries still exist
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);

    // Update existing entry with same extra_data should preserve spender_tx_id
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(tx_id1, 0, "extra1".to_string(), Some(10)),
        false,
    )?;

    // Verify both entries still exist and confirmation trigger is updated
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);
    assert!(monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, vout, extra, conf, _) if *id == tx_id1 && *vout == 0 && *extra == "extra1" && *conf == Some(10))));
    assert!(monitors.iter().any(|m| matches!(m, TypesToMonitorStore::SpendingUTXOTransaction(id, vout, extra, conf, _) if *id == tx_id1 && *vout == 0 && *extra == "extra2" && *conf == Some(2))));

    // Should still have 2 entries (extra1 updated, extra2 unchanged)
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 2);

    // Add new entry with different extra_data should have spender_tx_id = None initially
    store.add_monitor(
        TypesToMonitor::SpendingUTXOTransaction(tx_id1, 0, "extra3".to_string(), Some(3)),
        false,
    )?;

    // Now should have 3 entries
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 3);

    clear_output();
    Ok(())
}

/// This test verifies edge cases: deactivating/canceling non-existent entries
#[test]
fn test_edge_cases_non_existent_entries() -> Result<(), anyhow::Error> {
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

    let tx_id1 = tx1.compute_txid();

    // Add a monitor
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra1".to_string(), None),
        false,
    )?;

    // Try to deactivate with wrong extra_data - should not fail, just do nothing
    store.deactivate_monitor(TypesToMonitor::Transactions(
        vec![tx_id1],
        "wrong_extra".to_string(),
        None,
    ))?;

    // Monitor should still be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);

    // Try to cancel with wrong extra_data - should not fail, just do nothing
    store.cancel_monitor(TypesToMonitor::Transactions(
        vec![tx_id1],
        "wrong_extra".to_string(),
        None,
    ))?;

    // Monitor should still be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);

    // Try to deactivate/cancel non-existent txid - should not fail
    let non_existent_txid =
        Txid::from_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")?;
    store.deactivate_monitor(TypesToMonitor::Transactions(
        vec![non_existent_txid],
        "extra1".to_string(),
        None,
    ))?;
    store.cancel_monitor(TypesToMonitor::Transactions(
        vec![non_existent_txid],
        "extra1".to_string(),
        None,
    ))?;

    // Original monitor should still be active
    let monitors = store.get_monitors()?;
    assert_eq!(monitors.len(), 1);

    clear_output();
    Ok(())
}

/// This test verifies that when updating an existing entry, trigger_sent is reset to false
#[test]
fn test_update_entry_resets_trigger_sent() -> Result<(), anyhow::Error> {
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

    let tx_id1 = tx1.compute_txid();

    // Add monitor
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra1".to_string(), Some(1)),
        false,
    )?;

    // Set trigger_sent to true
    store.update_transaction_trigger_sent(tx_id1, "extra1", true)?;
    assert_eq!(store.get_transaction_trigger_sent(tx_id1, "extra1")?, true);

    // Update the entry with same extra_data - should reset trigger_sent to false
    store.add_monitor(
        TypesToMonitor::Transactions(vec![tx_id1], "extra1".to_string(), Some(10)),
        false,
    )?;

    // trigger_sent should be reset to false
    assert_eq!(store.get_transaction_trigger_sent(tx_id1, "extra1")?, false);

    clear_output();
    Ok(())
}

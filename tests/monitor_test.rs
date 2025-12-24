use bitcoin::{absolute::LockTime, BlockHash, Transaction};
use bitcoin_indexer::{
    indexer::MockIndexerApi,
    types::{FullBlock, TransactionInfo},
};
use bitvmx_transaction_monitor::{
    config::{MonitorSettings, MonitorSettingsConfig},
    monitor::Monitor,
    store::{MonitorStore, MonitorStoreApi, TypesToMonitorStore},
    types::{AckMonitorNews, MonitorNews, TypesToMonitor},
};
use mockall::predicate::*;
use std::{rc::Rc, str::FromStr};
use storage_backend::{storage::Storage, storage_config::StorageConfig};
use utils::{clear_output, generate_random_string};
mod utils;

#[test]
fn no_monitors() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    let best_block_100 = FullBlock {
        height: 100,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap(),
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap(),
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    let best_block_100_clone = best_block_100.clone();

    mock_indexer
        .expect_get_block_by_height()
        .returning(move |_| Ok(Some(best_block_100_clone.clone())));

    mock_indexer.expect_tick().returning(move || Ok(()));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(best_block_100.clone())));

    let monitor = Monitor::new(
        mock_indexer,
        store,
        MonitorSettings::from(MonitorSettingsConfig::default()),
    )?;
    monitor.tick()?;

    clear_output();

    Ok(())
}

#[test]
fn monitor_txs_detected() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    let block_height_200 = 200;
    let block_200 = FullBlock {
        height: block_height_200,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000011",
        )
        .unwrap(),
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap(),
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    let block_0 = FullBlock {
        height: 0,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap(),
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000022",
        )
        .unwrap(),
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    let block_200_clone = block_200.clone();

    mock_indexer
        .expect_get_block_by_height()
        .with(eq(0))
        .returning(move |_| Ok(Some(block_0.clone())));

    mock_indexer
        .expect_get_block_by_height()
        .with(eq(block_height_200))
        .returning(move |_| Ok(Some(block_200_clone.clone())));

    let tx_to_seen = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195601).unwrap(),
        input: vec![],
        output: vec![],
    };

    let tx_id = tx.compute_txid();
    let tx_id_2 = tx_to_seen.compute_txid();

    let tx_to_seen_info = TransactionInfo {
        tx: tx_to_seen.clone(),
        block_info: block_200.clone(),
        confirmations: 1,
    };

    let tx_info = TransactionInfo {
        tx: tx.clone(),
        block_info: block_200.clone(),
        confirmations: 1,
    };

    mock_indexer.expect_tick().returning(move || Ok(()));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(block_200.clone())));

    mock_indexer
        .expect_get_tx()
        .with(eq(tx_id_2))
        .returning(move |_| Ok(Some(tx_to_seen_info.clone())));

    mock_indexer
        .expect_get_tx()
        .with(eq(tx_id))
        .returning(move |_| Ok(Some(tx_info.clone())));

    let monitor = Monitor::new(
        mock_indexer,
        store,
        MonitorSettings::from(MonitorSettingsConfig::default()),
    )?;

    monitor.save_monitor(TypesToMonitor::Transactions(
        vec![tx_id],
        "test".to_string(),
        None,
    ))?;
    monitor.save_monitor(TypesToMonitor::Transactions(
        vec![tx_id_2],
        "test 2".to_string(),
        None,
    ))?;

    monitor.tick()?;

    // Verify news
    let news = monitor.get_news()?;
    assert_eq!(news.len(), 2);

    match &news[0] {
        MonitorNews::Transaction(id, _, _) => assert_eq!(*id, tx_id),
        _ => panic!("Expected Transaction news"),
    }
    match &news[1] {
        MonitorNews::Transaction(id, _, _) => assert_eq!(*id, tx_id_2),
        _ => panic!("Expected Transaction news"),
    }

    // Acknowledge the news
    monitor.ack_news(AckMonitorNews::Transaction(tx_id))?;
    monitor.ack_news(AckMonitorNews::Transaction(tx_id_2))?;

    // Verify news are gone after acknowledgment
    let news_after_ack = monitor.get_news()?;
    assert_eq!(
        news_after_ack.len(),
        0,
        "Expected no news after acknowledgment"
    );

    clear_output();

    Ok(())
}

#[test]
fn test_monitor_deactivation_after_100_confirmations() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    let tx_id = tx.compute_txid();

    let block_info = FullBlock {
        height: 100,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )?,
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000002",
        )?,
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    let block_0 = FullBlock {
        height: 0,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000003",
        )?,
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000004",
        )?,
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    let block_200 = FullBlock {
        height: 200,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000005",
        )
        .unwrap(),
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000006",
        )
        .unwrap(),
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    let tx_info = TransactionInfo {
        tx: tx.clone(),
        block_info,
        confirmations: 101, // More than 100 confirmations
    };

    mock_indexer
        .expect_get_tx()
        .with(eq(tx_id))
        .times(1)
        .returning(move |_| Ok(Some(tx_info.clone())));

    let block_200_clone = block_200.clone();
    let block_200_clone_1 = block_200.clone();

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(block_200_clone.clone())));

    mock_indexer
        .expect_get_block_by_height()
        .with(eq(0))
        .returning(move |_| Ok(Some(block_0.clone())));

    mock_indexer
        .expect_get_block_by_height()
        .returning(move |_| Ok(Some(block_200_clone_1.clone())));

    mock_indexer.expect_tick().returning(move || Ok(()));

    let monitor = Monitor::new(
        mock_indexer,
        store,
        MonitorSettings::from(MonitorSettingsConfig::default()),
    )?;

    monitor.save_monitor(TypesToMonitor::Transactions(
        vec![tx_id],
        "test".to_string(),
        None,
    ))?;

    monitor.tick()?;

    // Verify monitor was deactivated
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    clear_output();

    Ok(())
}

#[test]
fn test_inactive_monitors_are_skipped() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    let tx_id = tx.compute_txid();
    store.add_monitor(TypesToMonitor::Transactions(
        vec![tx_id],
        String::new(),
        None,
    ))?;
    store.deactivate_monitor(TypesToMonitor::Transactions(
        vec![tx_id],
        String::new(),
        None,
    ))?;

    let full_block = FullBlock {
        height: 200,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap(),
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap(),
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    let full_block_clone = full_block.clone();

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(full_block.clone())));

    mock_indexer
        .expect_get_block_by_height()
        .returning(move |_| Ok(Some(full_block_clone.clone())));

    mock_indexer.expect_tick().returning(move || Ok(()));

    let monitor = Monitor::new(
        mock_indexer,
        store,
        MonitorSettings::from(MonitorSettingsConfig::default()),
    )?;
    monitor.tick()?;

    // Verify no news was produced
    let news = monitor.get_news()?;
    assert_eq!(news.len(), 0);

    clear_output();

    Ok(())
}

#[test]
fn test_rsk_pegin_monitor_not_deactivated() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    let full_block = FullBlock {
        height: 200,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap(),
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap(),
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    let full_block_clone = full_block.clone();

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(full_block.clone())));

    let full_block_clone = full_block_clone.clone();

    mock_indexer
        .expect_get_block_by_height()
        .returning(move |_| Ok(Some(full_block_clone.clone())));

    mock_indexer.expect_tick().returning(move || Ok(()));

    let monitor = Monitor::new(
        mock_indexer,
        store,
        MonitorSettings::from(MonitorSettingsConfig::default()),
    )?;
    monitor.save_monitor(TypesToMonitor::RskPeginTransaction(None))?;
    monitor.tick()?;

    // Verify monitor is still active
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0],
        TypesToMonitorStore::RskPeginTransaction(_)
    ));

    clear_output();

    Ok(())
}

#[test]
fn test_best_block_news() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    // Simulate the monitor's current height is 199, but the best block is 200
    // so a new block should be detected.
    let monitor_height: u32 = 199;
    {
        let store_ref = &store;
        store_ref.update_monitor_height(monitor_height)?;
    }
    let block_200 = FullBlock {
        height: 200,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap(),
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000022",
        )
        .unwrap(),
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    let block_199 = FullBlock {
        height: 199,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap(),
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000002",
        )
        .unwrap(),
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    let block_200_clone = block_200.clone();
    let block_200_clone_1 = block_200.clone();
    let block_200_clone_2 = block_200.clone();

    mock_indexer
        .expect_get_block_by_height()
        .with(eq(199))
        .returning(move |_| Ok(Some(block_199.clone())));

    mock_indexer
        .expect_get_block_by_height()
        .with(eq(200))
        .returning(move |_| Ok(Some(block_200_clone.clone())));

    mock_indexer
        .expect_get_best_block()
        .times(3)
        .returning(move || Ok(Some(block_200_clone_1.clone())));

    mock_indexer.expect_tick().returning(move || Ok(()));

    let monitor = Monitor::new(
        mock_indexer,
        store,
        MonitorSettings::from(MonitorSettingsConfig::default()),
    )?;
    monitor.save_monitor(TypesToMonitor::NewBlock)?;
    monitor.tick()?;

    // After tick, NewBlock news should be present
    let news = monitor.store.get_news()?;
    assert_eq!(news.len(), 1);
    assert!(matches!(
        news[0],
        bitvmx_transaction_monitor::store::MonitoredTypes::NewBlock(hash) if hash == block_200_clone_2.hash
    ));

    // Acknowledge the news and verify it's gone
    monitor.ack_news(AckMonitorNews::NewBlock)?;
    let news = monitor.store.get_news()?;
    assert_eq!(news.len(), 0);

    monitor.tick()?;

    // After tick, NewBlock news should not be present because it was already acknowledged
    let news = monitor.store.get_news()?;
    assert_eq!(news.len(), 0);

    // Check if there's any pending work initially; it should be false
    let is_pending_work = monitor.store.has_pending_work()?;
    assert!(!is_pending_work);

    // Save a new monitor for NewBlock and check again for pending work; should still be false
    monitor.save_monitor(TypesToMonitor::NewBlock)?;

    let is_pending_work = monitor.store.has_pending_work()?;
    assert!(!is_pending_work);

    // Create a new transaction and compute its txid
    let tx_id = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    }
    .compute_txid();

    // Save a monitor for the transaction and set a description "test"
    monitor.save_monitor(TypesToMonitor::Transactions(
        vec![tx_id],
        "test".to_string(),
        None,
    ))?;

    // Check if there's pending work after saving the transaction monitor; it should be true
    let is_pending_work = monitor.store.has_pending_work()?;
    assert!(is_pending_work);

    clear_output();

    Ok(())
}

#[test]
fn test_spending_utxo_monitor_orphan_handling() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    let target_tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    let target_tx_id = target_tx.compute_txid();
    let target_utxo_index = 0u32;

    // Create a spending transaction
    let spending_tx1 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195601).unwrap(),
        input: vec![bitcoin::TxIn {
            previous_output: bitcoin::OutPoint {
                txid: target_tx_id,
                vout: target_utxo_index,
            },
            script_sig: bitcoin::ScriptBuf::new(),
            sequence: bitcoin::Sequence::MAX,
            witness: bitcoin::Witness::new(),
        }],
        output: vec![],
    };

    let spending_tx2 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195601).unwrap(),
        input: vec![bitcoin::TxIn {
            previous_output: bitcoin::OutPoint {
                txid: target_tx_id,
                vout: target_utxo_index,
            },
            script_sig: bitcoin::ScriptBuf::new(),
            sequence: bitcoin::Sequence::MAX,
            witness: bitcoin::Witness::new(),
        }],
        output: vec![],
    };

    let spending_tx1_id = spending_tx1.compute_txid();

    let block_100 = FullBlock {
        height: 100,
        hash: BlockHash::from_str(
            "1000000000000000000000000000000000000000000000000000000000000001",
        )?,
        prev_hash: BlockHash::from_str(
            "2000000000000000000000000000000000000000000000000000000000000000",
        )?,
        txs: vec![spending_tx1.clone()],
        orphan: false,
        estimated_fee_rate: 0,
    };

    // Block 100 has the spending transaction tx1
    let block_101 = FullBlock {
        height: 101,
        hash: BlockHash::from_str(
            "1000000000000000000000000000000000000000000000000000000000000002",
        )?,
        prev_hash: BlockHash::from_str(
            "2000000000000000000000000000000000000000000000000000000000000001",
        )?,
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    // Block 100 is a new block making a reorg with block 100 , and has another spending transaction tx2
    let block_100_reorg = FullBlock {
        height: 100,
        hash: BlockHash::from_str(
            "1000000000000000000000000000000000000000000000000000000000000003",
        )?,
        prev_hash: BlockHash::from_str(
            "2000000000000000000000000000000000000000000000000000000000000002",
        )?,
        txs: vec![spending_tx2.clone()],
        orphan: false,
        estimated_fee_rate: 0,
    };

    // Create transaction info for the spending transaction (orphan)
    let spending_tx1 = TransactionInfo {
        tx: spending_tx1.clone(),
        block_info: block_100.clone(),
        confirmations: 1,
    };

    let spending_tx2 = TransactionInfo {
        tx: spending_tx2.clone(),
        block_info: block_100_reorg.clone(),
        confirmations: 1,
    };

    let spending_tx2_id = spending_tx2.tx.compute_txid();

    let spending_tx1_clone = spending_tx1.clone();
    let mut spending_tx1_clone_2 = spending_tx1_clone.clone();
    let spending_tx2_clone = spending_tx2.clone();
    let spending_tx2_clone_2 = spending_tx2_clone.clone();

    mock_indexer.expect_tick().returning(move || Ok(()));

    // Set up expectations
    let block_100_clone = block_100.clone();

    // Each tick in the monitor uses 2 get_best_block call but if there is pending work, it will use 1 get_best_block call
    mock_indexer
        .expect_get_best_block()
        .times(1)
        .returning(move || Ok(Some(block_100.clone())));

    mock_indexer
        .expect_get_block_by_height()
        .with(eq(100))
        .returning(move |_| Ok(Some(block_100_clone.clone())));

    let block_101_clone = block_101.clone();

    // Each tick in the monitor uses 2 get_best_block call but if there is pending work, it will use 1 get_best_block call
    mock_indexer
        .expect_get_best_block()
        .times(2)
        .returning(move || Ok(Some(block_101.clone())));

    mock_indexer
        .expect_get_block_by_height()
        .with(eq(101))
        .returning(move |_| Ok(Some(block_101_clone.clone())));

    // Each tick in the monitor uses 2 get_best_block call but if there is pending work, it will use 1 get_best_block call
    mock_indexer
        .expect_get_best_block()
        .times(2)
        .returning(move || Ok(Some(block_100_reorg.clone())));

    // Expect get_tx to be called for the spending transaction
    mock_indexer
        .expect_get_tx()
        .with(eq(spending_tx1_id))
        .times(2)
        .returning(move |_| Ok(Some(spending_tx1_clone.clone())));

    mock_indexer
        .expect_get_tx()
        .with(eq(spending_tx1_id))
        .times(1)
        .returning(move |_| Ok(Some(spending_tx2.clone())));

    spending_tx1_clone_2.confirmations = 2;

    mock_indexer
        .expect_get_tx()
        .with(eq(spending_tx1_id))
        .times(1)
        .returning(move |_| Ok(Some(spending_tx1_clone_2.clone())));

    mock_indexer
        .expect_get_tx()
        .with(eq(spending_tx2_id))
        .times(2)
        .returning(move |_| Ok(Some(spending_tx2_clone.clone())));

    let monitor = Monitor::new(
        mock_indexer,
        store,
        MonitorSettings::from(MonitorSettingsConfig::default()),
    )?;

    // Add the SpendingUTXOTransaction monitor
    monitor.save_monitor(TypesToMonitor::SpendingUTXOTransaction(
        target_tx_id,
        target_utxo_index,
        String::new(),
        None,
    ))?;

    // First tick - should detect the spending transaction
    monitor.tick()?;

    let news = monitor.get_news()?;
    assert_eq!(news.len(), 1);

    assert!(matches!(
        news[0].clone(),
        MonitorNews::SpendingUTXOTransaction(t, u, tx_status, _)
            if t == target_tx_id && u == target_utxo_index && tx_status.tx_id == spending_tx1.tx.compute_txid() && tx_status.confirmations == 1
    ));

    monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        target_tx_id,
        target_utxo_index,
    ))?;

    // Second tick - should confirm the spending transaction (2 confirmations)
    monitor.tick()?;

    let news = monitor.get_news()?;
    assert_eq!(news.len(), 1);

    assert!(matches!(
        news[0].clone(),
        MonitorNews::SpendingUTXOTransaction(t, u, tx_status, _)
            if t == target_tx_id && u == target_utxo_index && tx_status.tx_id == spending_tx1.tx.compute_txid() && tx_status.confirmations == 2
    ));

    monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        target_tx_id,
        target_utxo_index,
    ))?;

    // Third tick - Reorg with block 100, and should detect the new spending transaction tx2
    monitor.tick()?;

    let news = monitor.get_news()?;

    assert_eq!(news.len(), 1);
    assert!(matches!(
        news[0].clone(),
        MonitorNews::SpendingUTXOTransaction(t, u, tx_status, _)
            if t == target_tx_id && u == target_utxo_index && tx_status.tx_id == spending_tx2_clone_2.tx.compute_txid() && tx_status.confirmations == 1
    ));

    clear_output();

    Ok(())
}

// This test verifies that a SpendingUTXOTransaction monitor is correctly deactivated after 3 confirmations.
// It also checks that a SpendingUTXOTransaction notification is created and the monitor is removed properly.

#[test]
fn test_spending_utxo_monitor_deactivation_after_max_confirmations() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    let target_tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    let target_tx_id = target_tx.compute_txid();
    let target_utxo_index = 0u32;

    // Create a spending transaction
    let spending_tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195601).unwrap(),
        input: vec![bitcoin::TxIn {
            previous_output: bitcoin::OutPoint {
                txid: target_tx_id,
                vout: target_utxo_index,
            },
            script_sig: bitcoin::ScriptBuf::new(),
            sequence: bitcoin::Sequence::MAX,
            witness: bitcoin::Witness::new(),
        }],
        output: vec![],
    };

    let spending_tx_id = spending_tx.compute_txid();

    // Create a block at height 100 containing the spending transaction
    let block_with_spending_tx = FullBlock {
        height: 100,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )?,
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )?,
        txs: vec![spending_tx.clone()],
        orphan: false,
        estimated_fee_rate: 0,
    };

    // Create a block at height 101 for further confirmations
    let block_101 = FullBlock {
        height: 101,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000002",
        )?,
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )?,
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    // Create a block at height 102 for the final confirmation count
    let block_102 = FullBlock {
        height: 102,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000003",
        )?,
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000002",
        )?,
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    // Transaction info for the spending transaction at each confirmation level
    let spending_tx_info_at_100 = TransactionInfo {
        tx: spending_tx.clone(),
        block_info: block_102.clone(),
        confirmations: 1,
    };

    let spending_tx_info_at_101 = TransactionInfo {
        tx: spending_tx.clone(),
        block_info: block_102.clone(),
        confirmations: 2,
    };

    let spending_tx_info_at_102 = TransactionInfo {
        tx: spending_tx.clone(),
        block_info: block_102.clone(),
        confirmations: 3,
    };

    // Set expectations for each tick: block 100, then 101, then 102
    let best_block_100_clone_1 = block_with_spending_tx.clone();
    let best_block_100_clone_2 = block_with_spending_tx.clone();
    let best_block_101_clone = block_101.clone();
    let best_block_101_clone_2 = block_101.clone();
    let best_block_102_clone = block_102.clone();
    let best_block_102_clone_2 = block_102.clone();

    // Each tick in the monitor uses 1 get_best_block call but if there is pending work, it will use 2 get_best_block calls
    mock_indexer
        .expect_get_best_block()
        .times(1)
        .returning(move || Ok(Some(best_block_100_clone_1.clone())));

    mock_indexer
        .expect_get_block_by_height()
        .with(eq(100))
        .returning(move |_| Ok(Some(best_block_100_clone_2.clone())));

    // Each tick in the monitor uses 2 get_best_block calls
    mock_indexer
        .expect_get_best_block()
        .times(2)
        .returning(move || Ok(Some(best_block_101_clone.clone())));

    mock_indexer
        .expect_get_block_by_height()
        .with(eq(101))
        .returning(move |_| Ok(Some(best_block_101_clone_2.clone())));

    // Each tick in the monitor uses 2 get_best_block calls
    mock_indexer
        .expect_get_best_block()
        .times(2)
        .returning(move || Ok(Some(best_block_102_clone.clone())));

    mock_indexer
        .expect_get_block_by_height()
        .with(eq(102))
        .returning(move |_| Ok(Some(best_block_102_clone_2.clone())));

    mock_indexer.expect_tick().returning(move || Ok(()));

    mock_indexer
        .expect_get_tx()
        .with(eq(spending_tx_id))
        .times(2)
        .returning(move |_| Ok(Some(spending_tx_info_at_100.clone())));

    mock_indexer
        .expect_get_tx()
        .with(eq(spending_tx_id))
        .times(2)
        .returning(move |_| Ok(Some(spending_tx_info_at_101.clone())));

    mock_indexer
        .expect_get_tx()
        .with(eq(spending_tx_id))
        .times(1)
        .returning(move |_| Ok(Some(spending_tx_info_at_102.clone())));

    let mut settings = MonitorSettings::from(MonitorSettingsConfig::default());
    settings.max_monitoring_confirmations = 3;

    let monitor = Monitor::new(mock_indexer, store, settings)?;

    // Add the SpendingUTXOTransaction monitor
    monitor.save_monitor(TypesToMonitor::SpendingUTXOTransaction(
        target_tx_id,
        target_utxo_index,
        String::new(),
        None,
    ))?;

    // Ensure the monitor is initially active
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 1);

    // First tick: detect and save the spending transaction
    monitor.tick()?;

    // Confirm the monitor still tracks the spending transaction
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::SpendingUTXOTransaction(t, u, _, Some(stx), _)
            if t == target_tx_id && u == target_utxo_index && stx == spending_tx_id
    ));

    let news = monitor.get_news()?;
    assert_eq!(news.len(), 1);
    assert!(matches!(
        news[0].clone(),
        MonitorNews::SpendingUTXOTransaction(t, u, _, _)
            if t == target_tx_id && u == target_utxo_index
    ));

    monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        target_tx_id,
        target_utxo_index,
    ))?;

    // Second tick: confirmations reach the threshold; the monitor should be dequeued
    monitor.tick()?;

    // Check that news was created as expected
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 1);

    let news = monitor.get_news()?;
    assert_eq!(news.len(), 1);
    assert!(matches!(
        news[0].clone(),
        MonitorNews::SpendingUTXOTransaction(t, u, _, _)
            if t == target_tx_id && u == target_utxo_index
    ));

    monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        target_tx_id,
        target_utxo_index,
    ))?;

    monitor.tick()?;

    // Verify that the monitor is now deactivated
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    let news = monitor.get_news()?;
    assert_eq!(news.len(), 0);

    Ok(())
}

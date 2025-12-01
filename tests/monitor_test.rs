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

    let block_200_clone = block_200.clone();

    mock_indexer
        .expect_get_block_by_height()
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
    ))?;
    monitor.save_monitor(TypesToMonitor::Transactions(
        vec![tx_id_2],
        "test 2".to_string(),
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
            "0000000000000000000000000000000000000000000000000000000000000000",
        )?,
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )?,
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
        .returning(move || Ok(Some(full_block_clone.clone())));

    let full_block_clone = full_block.clone();

    mock_indexer
        .expect_get_block_by_height()
        .returning(move |_| Ok(Some(full_block_clone.clone())));

    mock_indexer.expect_tick().returning(move || Ok(()));

    let monitor = Monitor::new(
        mock_indexer,
        store,
        MonitorSettings::from(MonitorSettingsConfig::default()),
    )?;

    monitor.save_monitor(TypesToMonitor::Transactions(
        vec![tx_id],
        "test".to_string(),
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
    store.add_monitor(TypesToMonitor::Transactions(vec![tx_id], String::new()))?;
    store.deactivate_monitor(TypesToMonitor::Transactions(vec![tx_id], String::new()))?;

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
    monitor.save_monitor(TypesToMonitor::RskPeginTransaction)?;
    monitor.tick()?;

    // Verify monitor is still active
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0],
        TypesToMonitorStore::RskPeginTransaction
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
    let mut full_block = FullBlock {
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

    let full_block_200 = full_block.clone();
    let full_block_200_clone = full_block.clone();
    let full_block_200_clone_2 = full_block.clone();

    mock_indexer
        .expect_get_block_by_height()
        .returning(move |_| Ok(Some(full_block_200_clone.clone())));

    mock_indexer
        .expect_get_best_block()
        .times(1)
        .returning(move || Ok(Some(full_block_200.clone())));

    full_block.height = 201;

    mock_indexer
        .expect_get_best_block()
        .times(1)
        .returning(move || Ok(Some(full_block.clone())));

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
        bitvmx_transaction_monitor::store::MonitoredTypes::NewBlock(hash) if hash == full_block_200_clone_2.hash
    ));

    // Acknowledge the news and verify it's gone
    monitor.ack_news(AckMonitorNews::NewBlock)?;
    let news = monitor.store.get_news()?;
    assert_eq!(news.len(), 0);

    monitor.tick()?;

    // After tick, NewBlock news should not be present because it was already acknowledged
    let news = monitor.store.get_news()?;
    assert_eq!(news.len(), 0);

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

    // Create an orphan block containing the spending transaction
    let orphan_block = FullBlock {
        height: 100,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )?,
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )?,
        txs: vec![spending_tx.clone()],
        orphan: true,
        estimated_fee_rate: 0,
    };

    // Create transaction info for the spending transaction (orphan)
    let spending_tx_info = TransactionInfo {
        tx: spending_tx.clone(),
        block_info: orphan_block.clone(),
        confirmations: 0,
    };

    let best_block = FullBlock {
        height: 200,
        hash: BlockHash::from_str(
            "1000000000000000000000000000000000000000000000000000000000000000",
        )?,
        prev_hash: BlockHash::from_str(
            "2000000000000000000000000000000000000000000000000000000000000000",
        )?,
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    // Set up expectations
    let best_block_clone_1 = best_block.clone();
    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(best_block_clone_1.clone())));

    let best_block_clone_2 = best_block.clone();
    mock_indexer
        .expect_get_block_by_height()
        .returning(move |_| Ok(Some(best_block_clone_2.clone())));

    mock_indexer.expect_tick().returning(move || Ok(()));

    // Expect get_tx to be called for the spending transaction
    mock_indexer
        .expect_get_tx()
        .with(eq(spending_tx_id))
        .returning(move |_| Ok(Some(spending_tx_info.clone())));

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
    ))?;

    // First, manually set a spending transaction ID to simulate it was found before
    monitor.store.update_spending_utxo_monitor((
        target_tx_id,
        target_utxo_index,
        Some(spending_tx_id),
    ))?;

    // Verify the monitor has the spending tx_id
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::SpendingUTXOTransaction(t, u, _, Some(stx))
            if t == target_tx_id && u == target_utxo_index && stx == spending_tx_id
    ));

    // Run tick - should detect orphan and update monitor to None
    monitor.tick()?;

    // Verify the monitor's spending tx_id was set to None due to orphan
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::SpendingUTXOTransaction(t, u, _, None)
            if t == target_tx_id && u == target_utxo_index
    ));

    // Verify news was created for the orphan transaction
    let news = monitor.store.get_news()?;
    assert_eq!(news.len(), 1);
    assert!(matches!(
        news[0].clone(),
        bitvmx_transaction_monitor::store::MonitoredTypes::SpendingUTXOTransaction(t, u, _, stx)
            if t == target_tx_id && u == target_utxo_index && stx == spending_tx_id
    ));

    clear_output();

    Ok(())
}

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
    let block_with_spending = FullBlock {
        height: 100,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )?,
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )?,
        txs: vec![spending_tx.clone()],
        orphan: false,
        estimated_fee_rate: 0,
    };

    // Create transaction info for the spending transaction at block 100
    let spending_tx_info_at_100 = TransactionInfo {
        tx: spending_tx.clone(),
        block_info: block_with_spending.clone(),
        confirmations: 1,
    };

    // Create transaction info for the spending transaction when best block is 200
    let spending_tx_info_at_200 = TransactionInfo {
        tx: spending_tx.clone(),
        block_info: block_with_spending.clone(),
        confirmations: 101, // 200 - 100 + 1 = 101 confirmations
    };

    // First best block at height 100 (when spending tx is found)
    let best_block_100 = FullBlock {
        height: 100,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )?,
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )?,
        txs: vec![spending_tx.clone()],
        orphan: false,
        estimated_fee_rate: 0,
    };

    // Second best block at height 200 (100 blocks later, should trigger deactivation)
    let best_block_200 = FullBlock {
        height: 200,
        hash: BlockHash::from_str(
            "1000000000000000000000000000000000000000000000000000000000000000",
        )?,
        prev_hash: BlockHash::from_str(
            "2000000000000000000000000000000000000000000000000000000000000000",
        )?,
        txs: vec![],
        orphan: false,
        estimated_fee_rate: 0,
    };

    // Set up expectations for first tick (block 100)
    let best_block_100_clone_1 = best_block_100.clone();

    // Set up expectations for second tick (block 200)
    let best_block_200_clone_1 = best_block_200.clone();

    // First call to get_best_block returns block 100
    mock_indexer
        .expect_get_best_block()
        .times(1)
        .returning(move || Ok(Some(best_block_100_clone_1.clone())));

    // Second call to get_best_block returns block 200
    mock_indexer
        .expect_get_best_block()
        .times(1)
        .returning(move || Ok(Some(best_block_200_clone_1.clone())));

    mock_indexer
        .expect_tick()
        .times(2)
        .returning(move || Ok(()));

    // First call to get_tx returns info at block 100 (when spending tx is first detected)
    mock_indexer
        .expect_get_tx()
        .with(eq(spending_tx_id))
        .times(1)
        .returning(move |_| Ok(Some(spending_tx_info_at_100.clone())));

    // Second call to get_tx returns info at block 200 (when checking confirmations)
    mock_indexer
        .expect_get_tx()
        .with(eq(spending_tx_id))
        .times(1)
        .returning(move |_| Ok(Some(spending_tx_info_at_200.clone())));

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
    ))?;

    // Verify the monitor is active
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 1);

    // First tick: block 100 - spending transaction is detected and saved
    monitor.tick()?;

    // Verify the monitor is still active and has the spending tx_id
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 1);
    assert!(matches!(
        monitors[0].clone(),
        TypesToMonitorStore::SpendingUTXOTransaction(t, u, _, Some(stx))
            if t == target_tx_id && u == target_utxo_index && stx == spending_tx_id
    ));

    // Second tick: block 200 - 100 blocks have passed, confirmations >= max_monitoring_confirmations
    // The monitor should be deactivated
    monitor.tick()?;

    // Verify the monitor was deactivated
    let monitors = monitor.store.get_monitors()?;
    assert_eq!(monitors.len(), 0);

    clear_output();

    Ok(())
}

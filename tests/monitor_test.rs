use bitcoin::{absolute::LockTime, BlockHash, Transaction};
use bitcoin_indexer::{
    indexer::MockIndexerApi,
    types::{FullBlock, TransactionInfo},
};
use bitvmx_transaction_monitor::{
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
    let path = format!("test_outputs/no_monitors/{}", generate_random_string());
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
    };

    mock_indexer.expect_tick().returning(move || Ok(()));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(best_block_100.clone())));

    let monitor = Monitor::new(mock_indexer, store, 6)?;
    monitor.tick()?;

    clear_output();

    Ok(())
}

#[test]
fn monitor_txs_detected() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let path = format!(
        "test_outputs/monitor_tx_detected/{}",
        generate_random_string()
    );
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
    };

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

    let monitor = Monitor::new(mock_indexer, store, 6)?;

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
    let path = format!(
        "test_outputs/monitor_deactivation/{}",
        generate_random_string()
    );
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

    mock_indexer.expect_get_best_block().returning(move || {
        Ok(Some(FullBlock {
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
        }))
    });

    mock_indexer.expect_tick().returning(move || Ok(()));

    let monitor = Monitor::new(mock_indexer, store, 6)?;

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
    let path = format!(
        "test_outputs/inactive_monitors/{}",
        generate_random_string()
    );
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

    mock_indexer.expect_get_best_block().returning(move || {
        Ok(Some(FullBlock {
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
        }))
    });

    mock_indexer.expect_tick().returning(move || Ok(()));

    let monitor = Monitor::new(mock_indexer, store, 6)?;
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
    let path = format!(
        "test_outputs/rsk_pegin_monitor/{}",
        generate_random_string()
    );
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    mock_indexer.expect_get_best_block().returning(move || {
        Ok(Some(FullBlock {
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
        }))
    });

    mock_indexer.expect_tick().returning(move || Ok(()));

    let monitor = Monitor::new(mock_indexer, store, 6)?;
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
    let path = format!(
        "test_outputs/new_block_monitor/{}",
        generate_random_string()
    );
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
    };

    let full_block_200 = full_block.clone();

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

    let monitor = Monitor::new(mock_indexer, store, 6)?;
    monitor.save_monitor(TypesToMonitor::NewBlock)?;
    monitor.tick()?;

    // After tick, NewBlock news should be present
    let news = monitor.store.get_news()?;
    assert_eq!(news.len(), 1);
    assert!(matches!(
        news[0],
        bitvmx_transaction_monitor::store::MonitoredTypes::NewBlock
    ));

    // Acknowledge the news and verify it's gone
    monitor.ack_news(AckMonitorNews::NewBlock)?;
    let news = monitor.store.get_news()?;
    assert_eq!(news.len(), 0);

    monitor.tick()?;

    // After tick, NewBlock news should be present
    let news = monitor.store.get_news()?;
    assert_eq!(news.len(), 1);
    assert!(matches!(
        news[0],
        bitvmx_transaction_monitor::store::MonitoredTypes::NewBlock
    ));

    clear_output();

    Ok(())
}

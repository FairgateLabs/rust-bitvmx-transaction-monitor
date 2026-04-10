use bitcoin::{absolute::LockTime, BlockHash, Transaction};
use bitvmx_transaction_monitor::{
    store::{MonitorStore, MonitorStoreApi, MonitoredTypes},
    types::AckMonitorNews,
};
use std::{rc::Rc, str::FromStr};
use storage_backend::{storage::Storage, storage_config::StorageConfig};
use utils::{clear_output, generate_random_string};
use uuid::Uuid;
mod utils;

/// Test the news functionality of the MonitorStore
/// This test verifies:
/// 1. Initial state - store starts with no news
/// 2. Transactions News
///    - Can add transactions to news
///    - Can acknowledge and remove it
/// 3. Spending UTXO Transaction News
///    - Can add a spending UTXO transaction
///    - Can acknowledge and remove it
/// 4. New Block News
///    - Can add a new block notification
///    - Can acknowledge and remove it
/// 5. Output Pattern Transaction News
///    - Can add an output pattern transaction
///    - Can acknowledge and remove it
///
#[test]
fn news_test() -> Result<(), anyhow::Error> {
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

    // No news for now
    let news = store.get_news()?;
    assert_eq!(news, vec![]);

    let block_hash =
        BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000")?;

    let block_hash_1 =
        BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000001")?;

    // Test one transaction news
    let tx_news = MonitoredTypes::Transaction(tx.compute_txid(), "Context_1".to_string());
    store.update_news(tx_news.clone(), block_hash)?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    // Make ack to that news
    store.ack_news(AckMonitorNews::Transaction(
        tx.compute_txid(),
        "Context_1".to_string(),
    ))?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Update the existing news with same block hash
    let txs_news = MonitoredTypes::Transaction(tx.compute_txid(), "Context_1".to_string());
    store.update_news(txs_news.clone(), block_hash)?;

    // Verify we have a No news because for this block hash we already have an ack
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    store.update_news(txs_news.clone(), block_hash_1)?;

    // Verify we have a new news
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0], txs_news);

    // Make ack to that news and verify we have no news
    store.ack_news(AckMonitorNews::Transaction(
        tx.compute_txid(),
        "Context_1".to_string(),
    ))?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // // Test spending UTXO transaction news
    // let spending_tx_news = MonitoredTypes::SpendingUTXOTransaction(
    //     tx.compute_txid(),
    //     0,
    //     tx.compute_txid(),
    //     String::new(),
    // );
    // store.update_news(spending_tx_news.clone(), block_hash)?;
    // let news = store.get_news()?;
    // assert_eq!(news.len(), 1);
    // assert_eq!(news[0], spending_tx_news);

    // store.ack_news(AckMonitorNews::SpendingUTXOTransaction(
    //     tx.compute_txid(),
    //     0,
    // ))?;
    // let news = store.get_news()?;
    // assert_eq!(news.len(), 0);

    // // Test new block news
    // let block_news = MonitoredTypes::NewBlock;
    // store.update_news(block_news.clone(), block_hash)?;
    // let news = store.get_news()?;
    // assert_eq!(news.len(), 1);
    // assert_eq!(news[0], block_news);

    // store.ack_news(AckMonitorNews::NewBlock)?;
    // let news = store.get_news()?;
    // assert_eq!(news.len(), 0);

    // Test output pattern transaction news
    let tag = vec![0xde, 0xad];
    let op_news = MonitoredTypes::OutputPatternTransaction(tx.compute_txid(), tag.clone());
    store.update_news(op_news.clone(), block_hash)?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0], op_news);
    store.ack_news(AckMonitorNews::OutputPatternTransaction(
        tx.compute_txid(),
        tag.clone(),
    ))?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    clear_output();

    Ok(())
}

#[test]
fn test_duplicate_news() -> Result<(), anyhow::Error> {
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

    let block_hash =
        BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000")?;

    let block_hash_1 =
        BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000001")?;

    // Test duplicate transaction news
    let tx_news = MonitoredTypes::Transaction(tx.compute_txid(), String::new());
    store.update_news(tx_news.clone(), block_hash)?;
    store.update_news(tx_news.clone(), block_hash)?; // Try adding same tx again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should still only have 1 entry
    assert_eq!(news[0], tx_news);
    store.ack_news(AckMonitorNews::Transaction(
        tx.compute_txid(),
        String::new(),
    ))?;

    // Test duplicate group transaction news
    let context_data = Uuid::new_v4();
    let monitored_tx = MonitoredTypes::Transaction(tx.compute_txid(), context_data.to_string());
    store.update_news(monitored_tx.clone(), block_hash_1)?;
    store.update_news(monitored_tx.clone(), block_hash_1)?; // Try adding same group tx again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should have only group tx
    assert!(news.contains(&monitored_tx));
    store.ack_news(AckMonitorNews::Transaction(
        tx.compute_txid(),
        context_data.to_string(),
    ))?;

    // Test duplicate spending UTXO transaction news
    let spending_tx_news = MonitoredTypes::SpendingUTXOTransaction(
        tx.compute_txid(),
        0,
        String::new(),
        tx.compute_txid(),
    );
    store.update_news(spending_tx_news.clone(), block_hash)?;
    store.update_news(spending_tx_news.clone(), block_hash)?; // Try adding same spending tx again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should have only spending tx
    assert!(news.contains(&spending_tx_news));
    store.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        tx.compute_txid(),
        0,
        String::new(),
    ))?;

    // Test duplicate new block news
    let block_news = MonitoredTypes::NewBlock(block_hash);
    store.update_news(block_news.clone(), block_hash)?;
    store.update_news(block_news.clone(), block_hash)?; // Try adding same block news again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should have only block news
    assert!(news.contains(&block_news));
    store.ack_news(AckMonitorNews::NewBlock)?;

    // Test duplicate output pattern transaction news
    let tag = vec![0xde, 0xad];
    let op_news = MonitoredTypes::OutputPatternTransaction(tx.compute_txid(), tag.clone());
    store.update_news(op_news.clone(), block_hash)?;
    store.update_news(op_news.clone(), block_hash)?; // Try adding same entry again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should still only have 1 entry
    assert!(news.contains(&op_news));
    store.ack_news(AckMonitorNews::OutputPatternTransaction(
        tx.compute_txid(),
        tag.clone(),
    ))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0); // Should have no news after all acknowledgements

    clear_output();

    Ok(())
}

#[test]
fn test_multiple_transactions_per_type() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Rc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;

    // Create 3 different transactions
    let tx1 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };
    let tx2 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195700).unwrap(),
        input: vec![],
        output: vec![],
    };
    let tx3 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195800).unwrap(),
        input: vec![],
        output: vec![],
    };

    // Test multiple transactions
    let monitor_tx1 = MonitoredTypes::Transaction(tx1.compute_txid(), String::new());
    let monitor_tx2 = MonitoredTypes::Transaction(tx2.compute_txid(), String::new());
    let monitor_tx3 = MonitoredTypes::Transaction(tx3.compute_txid(), String::new());

    let block_hash =
        BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000")?;

    let block_hash_1 =
        BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000001")?;

    store.update_news(monitor_tx1.clone(), block_hash)?;
    store.update_news(monitor_tx2.clone(), block_hash)?;
    store.update_news(monitor_tx3.clone(), block_hash)?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&monitor_tx1));
    assert!(news.contains(&monitor_tx2));
    assert!(news.contains(&monitor_tx3));

    store.ack_news(AckMonitorNews::Transaction(
        tx1.compute_txid(),
        String::new(),
    ))?;
    store.ack_news(AckMonitorNews::Transaction(
        tx2.compute_txid(),
        String::new(),
    ))?;
    store.ack_news(AckMonitorNews::Transaction(
        tx3.compute_txid(),
        String::new(),
    ))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test multiple group transactions
    let context_data1 = Uuid::new_v4();
    let context_data2 = Uuid::new_v4();
    let context_data3 = Uuid::new_v4();

    let monitored_tx1 = MonitoredTypes::Transaction(tx1.compute_txid(), context_data1.to_string());
    let monitored_tx2 = MonitoredTypes::Transaction(tx2.compute_txid(), context_data2.to_string());
    let monitored_tx3 = MonitoredTypes::Transaction(tx3.compute_txid(), context_data3.to_string());

    store.update_news(monitored_tx1.clone(), block_hash_1)?;
    store.update_news(monitored_tx2.clone(), block_hash_1)?;
    store.update_news(monitored_tx3.clone(), block_hash_1)?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&monitored_tx1));
    assert!(news.contains(&monitored_tx2));
    assert!(news.contains(&monitored_tx3));

    store.ack_news(AckMonitorNews::Transaction(
        tx1.compute_txid(),
        context_data1.to_string(),
    ))?;
    store.ack_news(AckMonitorNews::Transaction(
        tx2.compute_txid(),
        context_data2.to_string(),
    ))?;
    store.ack_news(AckMonitorNews::Transaction(
        tx3.compute_txid(),
        context_data3.to_string(),
    ))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test multiple spending UTXO transactions
    let spending_tx1 = MonitoredTypes::SpendingUTXOTransaction(
        tx1.compute_txid(),
        0,
        String::new(),
        tx1.compute_txid(),
    );
    let spending_tx2 = MonitoredTypes::SpendingUTXOTransaction(
        tx2.compute_txid(),
        1,
        String::new(),
        tx1.compute_txid(),
    );
    let spending_tx3 = MonitoredTypes::SpendingUTXOTransaction(
        tx3.compute_txid(),
        2,
        String::new(),
        tx1.compute_txid(),
    );

    store.update_news(spending_tx1.clone(), block_hash)?;
    store.update_news(spending_tx2.clone(), block_hash)?;
    store.update_news(spending_tx3.clone(), block_hash)?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&spending_tx1));
    assert!(news.contains(&spending_tx2));
    assert!(news.contains(&spending_tx3));

    store.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        tx1.compute_txid(),
        0,
        String::new(),
    ))?;
    store.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        tx2.compute_txid(),
        1,
        String::new(),
    ))?;
    store.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        tx3.compute_txid(),
        2,
        String::new(),
    ))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test multiple new block notifications
    let block_news1 = MonitoredTypes::NewBlock(block_hash);
    store.update_news(block_news1.clone(), block_hash)?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    assert!(news.contains(&block_news1));

    store.ack_news(AckMonitorNews::NewBlock)?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test multiple output pattern transactions (different txid+tag combinations)
    let tag1 = vec![0xde, 0xad];
    let tag2 = vec![0xbe, 0xef];
    let op_news1 = MonitoredTypes::OutputPatternTransaction(tx1.compute_txid(), tag1.clone());
    let op_news2 = MonitoredTypes::OutputPatternTransaction(tx2.compute_txid(), tag1.clone());
    let op_news3 = MonitoredTypes::OutputPatternTransaction(tx1.compute_txid(), tag2.clone());

    store.update_news(op_news1.clone(), block_hash)?;
    store.update_news(op_news2.clone(), block_hash)?;
    store.update_news(op_news3.clone(), block_hash)?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&op_news1));
    assert!(news.contains(&op_news2));
    assert!(news.contains(&op_news3));

    store.ack_news(AckMonitorNews::OutputPatternTransaction(
        tx1.compute_txid(),
        tag1.clone(),
    ))?;
    store.ack_news(AckMonitorNews::OutputPatternTransaction(
        tx2.compute_txid(),
        tag1.clone(),
    ))?;
    store.ack_news(AckMonitorNews::OutputPatternTransaction(
        tx1.compute_txid(),
        tag2.clone(),
    ))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    clear_output();

    Ok(())
}

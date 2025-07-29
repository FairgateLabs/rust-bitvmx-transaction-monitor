use bitcoin::{absolute::LockTime, Transaction};
use bitvmx_transaction_monitor::{
    store::{MonitorStore, MonitorStoreApi, MonitoredTypes},
    types::AckMonitorNews,
};
use std::sync::Arc;
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
/// 3. RSK Pegin Transaction News
///    - Can add an RSK pegin transaction
///    - Can acknowledge and remove it
/// 4. Spending UTXO Transaction News
///    - Can add a spending UTXO transaction
///    - Can acknowledge and remove it
/// 5. New Block News
///    - Can add a new block notification
///    - Can acknowledge and remove it
///
#[test]
fn news_test() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Arc::new(Storage::new(&config)?);
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

    // Test one transaction news
    let tx_news = MonitoredTypes::Transaction(tx.compute_txid(), String::new());
    store.update_news(tx_news.clone())?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    store.ack_news(AckMonitorNews::Transaction(tx.compute_txid()))?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test vector of transactions news
    let context_data = Uuid::new_v4();
    let txs_news = MonitoredTypes::Transaction(tx.compute_txid(), context_data.to_string());
    store.update_news(txs_news.clone())?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0], txs_news);

    store.ack_news(AckMonitorNews::Transaction(tx.compute_txid()))?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test RSK pegin transaction news
    let rsk_tx_news = MonitoredTypes::RskPeginTransaction(tx.compute_txid());
    store.update_news(rsk_tx_news.clone())?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0], rsk_tx_news);

    store.ack_news(AckMonitorNews::RskPeginTransaction(tx.compute_txid()))?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test spending UTXO transaction news
    let spending_tx_news = MonitoredTypes::SpendingUTXOTransaction(
        tx.compute_txid(),
        0,
        tx.compute_txid(),
        String::new(),
    );
    store.update_news(spending_tx_news.clone())?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0], spending_tx_news);

    store.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        tx.compute_txid(),
        0,
    ))?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test new block news
    let block_news = MonitoredTypes::NewBlock;
    store.update_news(block_news.clone())?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0], block_news);

    store.ack_news(AckMonitorNews::NewBlock)?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    clear_output();

    Ok(())
}

#[test]
fn test_duplicate_news() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Arc::new(Storage::new(&config)?);
    let store = MonitorStore::new(storage)?;
    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    // Test duplicate transaction news
    let tx_news = MonitoredTypes::Transaction(tx.compute_txid(), String::new());
    store.update_news(tx_news.clone())?;
    store.update_news(tx_news.clone())?; // Try adding same tx again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should still only have 1 entry
    assert_eq!(news[0], tx_news);
    store.ack_news(AckMonitorNews::Transaction(tx.compute_txid()))?;

    // Test duplicate group transaction news
    let context_data = Uuid::new_v4();
    let monitored_tx = MonitoredTypes::Transaction(tx.compute_txid(), context_data.to_string());
    store.update_news(monitored_tx.clone())?;
    store.update_news(monitored_tx.clone())?; // Try adding same group tx again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should have only group tx
    assert!(news.contains(&monitored_tx));
    store.ack_news(AckMonitorNews::Transaction(tx.compute_txid()))?;

    // Test duplicate RSK pegin transaction news
    let rsk_tx_news = MonitoredTypes::RskPeginTransaction(tx.compute_txid());
    store.update_news(rsk_tx_news.clone())?;
    store.update_news(rsk_tx_news.clone())?; // Try adding same RSK tx again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should have only RSK tx
    assert!(news.contains(&rsk_tx_news));
    store.ack_news(AckMonitorNews::RskPeginTransaction(tx.compute_txid()))?;

    // Test duplicate spending UTXO transaction news
    let spending_tx_news = MonitoredTypes::SpendingUTXOTransaction(
        tx.compute_txid(),
        0,
        tx.compute_txid(),
        String::new(),
    );
    store.update_news(spending_tx_news.clone())?;
    store.update_news(spending_tx_news.clone())?; // Try adding same spending tx again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should have only spending tx
    assert!(news.contains(&spending_tx_news));
    store.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        tx.compute_txid(),
        0,
    ))?;

    // Test duplicate new block news
    let block_news = MonitoredTypes::NewBlock;
    store.update_news(block_news.clone())?;
    store.update_news(block_news.clone())?; // Try adding same block news again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should have only block news
    assert!(news.contains(&block_news));
    store.ack_news(AckMonitorNews::NewBlock)?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0); // Should have no news after all acknowledgements

    clear_output();

    Ok(())
}

#[test]
fn test_multiple_transactions_per_type() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/{}", generate_random_string());
    let config = StorageConfig::new(path, None);
    let storage = Arc::new(Storage::new(&config)?);
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

    store.update_news(monitor_tx1.clone())?;
    store.update_news(monitor_tx2.clone())?;
    store.update_news(monitor_tx3.clone())?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&monitor_tx1));
    assert!(news.contains(&monitor_tx2));
    assert!(news.contains(&monitor_tx3));

    store.ack_news(AckMonitorNews::Transaction(tx1.compute_txid()))?;
    store.ack_news(AckMonitorNews::Transaction(tx2.compute_txid()))?;
    store.ack_news(AckMonitorNews::Transaction(tx3.compute_txid()))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test multiple group transactions
    let context_data1 = Uuid::new_v4();
    let context_data2 = Uuid::new_v4();
    let context_data3 = Uuid::new_v4();

    let monitored_tx1 = MonitoredTypes::Transaction(tx1.compute_txid(), context_data1.to_string());
    let monitored_tx2 = MonitoredTypes::Transaction(tx2.compute_txid(), context_data2.to_string());
    let monitored_tx3 = MonitoredTypes::Transaction(tx3.compute_txid(), context_data3.to_string());

    store.update_news(monitored_tx1.clone())?;
    store.update_news(monitored_tx2.clone())?;
    store.update_news(monitored_tx3.clone())?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&monitored_tx1));
    assert!(news.contains(&monitored_tx2));
    assert!(news.contains(&monitored_tx3));

    store.ack_news(AckMonitorNews::Transaction(tx1.compute_txid()))?;
    store.ack_news(AckMonitorNews::Transaction(tx2.compute_txid()))?;
    store.ack_news(AckMonitorNews::Transaction(tx3.compute_txid()))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test multiple RSK pegin transactions
    let rsk_tx1 = MonitoredTypes::RskPeginTransaction(tx1.compute_txid());
    let rsk_tx2 = MonitoredTypes::RskPeginTransaction(tx2.compute_txid());
    let rsk_tx3 = MonitoredTypes::RskPeginTransaction(tx3.compute_txid());

    store.update_news(rsk_tx1.clone())?;
    store.update_news(rsk_tx2.clone())?;
    store.update_news(rsk_tx3.clone())?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&rsk_tx1));
    assert!(news.contains(&rsk_tx2));
    assert!(news.contains(&rsk_tx3));

    store.ack_news(AckMonitorNews::RskPeginTransaction(tx1.compute_txid()))?;
    store.ack_news(AckMonitorNews::RskPeginTransaction(tx2.compute_txid()))?;
    store.ack_news(AckMonitorNews::RskPeginTransaction(tx3.compute_txid()))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test multiple spending UTXO transactions
    let spending_tx1 = MonitoredTypes::SpendingUTXOTransaction(
        tx1.compute_txid(),
        0,
        tx1.compute_txid(),
        String::new(),
    );
    let spending_tx2 = MonitoredTypes::SpendingUTXOTransaction(
        tx2.compute_txid(),
        1,
        tx1.compute_txid(),
        String::new(),
    );
    let spending_tx3 = MonitoredTypes::SpendingUTXOTransaction(
        tx3.compute_txid(),
        2,
        tx1.compute_txid(),
        String::new(),
    );

    store.update_news(spending_tx1.clone())?;
    store.update_news(spending_tx2.clone())?;
    store.update_news(spending_tx3.clone())?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&spending_tx1));
    assert!(news.contains(&spending_tx2));
    assert!(news.contains(&spending_tx3));

    store.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        tx1.compute_txid(),
        0,
    ))?;
    store.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        tx2.compute_txid(),
        1,
    ))?;
    store.ack_news(AckMonitorNews::SpendingUTXOTransaction(
        tx3.compute_txid(),
        2,
    ))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test multiple new block notifications
    let block_news1 = MonitoredTypes::NewBlock;
    store.update_news(block_news1.clone())?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    assert!(news.contains(&block_news1));

    store.ack_news(AckMonitorNews::NewBlock)?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    clear_output();

    Ok(())
}

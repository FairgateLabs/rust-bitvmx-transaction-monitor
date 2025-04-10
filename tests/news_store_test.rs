use bitcoin::{absolute::LockTime, key::rand, Transaction};
use bitvmx_transaction_monitor::{
    store::{MonitorStore, MonitorStoreApi, TransactionMonitoredType},
    types::AckTransactionNews,
};
use std::{path::PathBuf, rc::Rc};
use storage_backend::storage::Storage;
use uuid::Uuid;

pub fn generate_random_string() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..10).map(|_| rng.gen_range('a'..='z')).collect()
}

/// Test the news functionality of the MonitorStore
/// This test verifies:
/// 1. Initial state - store starts with no news
/// 2. Single Transaction News
///    - Can add a single transaction to news
///    - Can acknowledge and remove it
/// 3. Group Transaction News  
///    - Can add a transaction to a group
///    - Can acknowledge and remove it
/// 4. RSK Pegin Transaction News
///    - Can add an RSK pegin transaction
///    - Can acknowledge and remove it
/// 5. Spending UTXO Transaction News
///    - Can add a spending UTXO transaction
///    - Can acknowledge and remove it
///
#[test]
fn news_test() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/address_test/{}", generate_random_string());
    let storage = Rc::new(Storage::new_with_path(&PathBuf::from(path))?);
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

    // Test single transaction news
    let single_tx_news = TransactionMonitoredType::Transaction(tx.compute_txid(), String::new());
    store.update_news(single_tx_news.clone())?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    store.ack_news(AckTransactionNews::Transaction(tx.compute_txid()))?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test group transaction news
    let group_id = Uuid::new_v4();
    let group_tx_news =
        TransactionMonitoredType::Transaction(tx.compute_txid(), group_id.to_string());
    store.update_news(group_tx_news.clone())?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0], group_tx_news);

    store.ack_news(AckTransactionNews::Transaction(tx.compute_txid()))?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test RSK pegin transaction news
    let rsk_tx_news = TransactionMonitoredType::RskPeginTransaction(tx.compute_txid());
    store.update_news(rsk_tx_news.clone())?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0], rsk_tx_news);

    store.ack_news(AckTransactionNews::RskPeginTransaction(tx.compute_txid()))?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test spending UTXO transaction news
    let spending_tx_news =
        TransactionMonitoredType::SpendingUTXOTransaction(tx.compute_txid(), 0, String::new());
    store.update_news(spending_tx_news.clone())?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0], spending_tx_news);

    store.ack_news(AckTransactionNews::SpendingUTXOTransaction(
        tx.compute_txid(),
        0,
    ))?;
    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    Ok(())
}

#[test]
fn test_duplicate_news() -> Result<(), anyhow::Error> {
    let path = format!(
        "test_outputs/test_duplicate_news/{}",
        generate_random_string()
    );
    let storage = Rc::new(Storage::new_with_path(&PathBuf::from(path))?);
    let store = MonitorStore::new(storage)?;
    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    // Test duplicate single transaction news
    let single_tx_news = TransactionMonitoredType::Transaction(tx.compute_txid(), String::new());
    store.update_news(single_tx_news.clone())?;
    store.update_news(single_tx_news.clone())?; // Try adding same tx again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should still only have 1 entry
    assert_eq!(news[0], single_tx_news);
    store.ack_news(AckTransactionNews::Transaction(tx.compute_txid()))?;

    // Test duplicate group transaction news
    let group_id = Uuid::new_v4();
    let group_tx_news =
        TransactionMonitoredType::Transaction(tx.compute_txid(), group_id.to_string());
    store.update_news(group_tx_news.clone())?;
    store.update_news(group_tx_news.clone())?; // Try adding same group tx again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should have only group tx
    assert!(news.contains(&group_tx_news));
    store.ack_news(AckTransactionNews::Transaction(tx.compute_txid()))?;

    // Test duplicate RSK pegin transaction news
    let rsk_tx_news = TransactionMonitoredType::RskPeginTransaction(tx.compute_txid());
    store.update_news(rsk_tx_news.clone())?;
    store.update_news(rsk_tx_news.clone())?; // Try adding same RSK tx again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should have only RSK tx
    assert!(news.contains(&rsk_tx_news));
    store.ack_news(AckTransactionNews::RskPeginTransaction(tx.compute_txid()))?;

    // Test duplicate spending UTXO transaction news
    let spending_tx_news =
        TransactionMonitoredType::SpendingUTXOTransaction(tx.compute_txid(), 0, String::new());
    store.update_news(spending_tx_news.clone())?;
    store.update_news(spending_tx_news.clone())?; // Try adding same spending tx again
    let news = store.get_news()?;
    assert_eq!(news.len(), 1); // Should have only spending tx
    assert!(news.contains(&spending_tx_news));
    store.ack_news(AckTransactionNews::SpendingUTXOTransaction(
        tx.compute_txid(),
        0,
    ))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0); // Should have no news after all acknowledgements

    Ok(())
}

#[test]
fn test_multiple_transactions_per_type() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/mul/{}", generate_random_string());
    let storage = Rc::new(Storage::new_with_path(&PathBuf::from(path))?);
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

    // Test multiple single transactions
    let single_tx1 = TransactionMonitoredType::Transaction(tx1.compute_txid(), String::new());
    let single_tx2 = TransactionMonitoredType::Transaction(tx2.compute_txid(), String::new());
    let single_tx3 = TransactionMonitoredType::Transaction(tx3.compute_txid(), String::new());

    store.update_news(single_tx1.clone())?;
    store.update_news(single_tx2.clone())?;
    store.update_news(single_tx3.clone())?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&single_tx1));
    assert!(news.contains(&single_tx2));
    assert!(news.contains(&single_tx3));

    store.ack_news(AckTransactionNews::Transaction(tx1.compute_txid()))?;
    store.ack_news(AckTransactionNews::Transaction(tx2.compute_txid()))?;
    store.ack_news(AckTransactionNews::Transaction(tx3.compute_txid()))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test multiple group transactions
    let group_id1 = Uuid::new_v4();
    let group_id2 = Uuid::new_v4();
    let group_id3 = Uuid::new_v4();

    let group_tx1 =
        TransactionMonitoredType::Transaction(tx1.compute_txid(), group_id1.to_string());
    let group_tx2 =
        TransactionMonitoredType::Transaction(tx2.compute_txid(), group_id2.to_string());
    let group_tx3 =
        TransactionMonitoredType::Transaction(tx3.compute_txid(), group_id3.to_string());

    store.update_news(group_tx1.clone())?;
    store.update_news(group_tx2.clone())?;
    store.update_news(group_tx3.clone())?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&group_tx1));
    assert!(news.contains(&group_tx2));
    assert!(news.contains(&group_tx3));

    store.ack_news(AckTransactionNews::Transaction(tx1.compute_txid()))?;
    store.ack_news(AckTransactionNews::Transaction(tx2.compute_txid()))?;
    store.ack_news(AckTransactionNews::Transaction(tx3.compute_txid()))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test multiple RSK pegin transactions
    let rsk_tx1 = TransactionMonitoredType::RskPeginTransaction(tx1.compute_txid());
    let rsk_tx2 = TransactionMonitoredType::RskPeginTransaction(tx2.compute_txid());
    let rsk_tx3 = TransactionMonitoredType::RskPeginTransaction(tx3.compute_txid());

    store.update_news(rsk_tx1.clone())?;
    store.update_news(rsk_tx2.clone())?;
    store.update_news(rsk_tx3.clone())?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&rsk_tx1));
    assert!(news.contains(&rsk_tx2));
    assert!(news.contains(&rsk_tx3));

    store.ack_news(AckTransactionNews::RskPeginTransaction(tx1.compute_txid()))?;
    store.ack_news(AckTransactionNews::RskPeginTransaction(tx2.compute_txid()))?;
    store.ack_news(AckTransactionNews::RskPeginTransaction(tx3.compute_txid()))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    // Test multiple spending UTXO transactions
    let spending_tx1 =
        TransactionMonitoredType::SpendingUTXOTransaction(tx1.compute_txid(), 0, String::new());
    let spending_tx2 =
        TransactionMonitoredType::SpendingUTXOTransaction(tx2.compute_txid(), 1, String::new());
    let spending_tx3 =
        TransactionMonitoredType::SpendingUTXOTransaction(tx3.compute_txid(), 2, String::new());

    store.update_news(spending_tx1.clone())?;
    store.update_news(spending_tx2.clone())?;
    store.update_news(spending_tx3.clone())?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 3);
    assert!(news.contains(&spending_tx1));
    assert!(news.contains(&spending_tx2));
    assert!(news.contains(&spending_tx3));

    store.ack_news(AckTransactionNews::SpendingUTXOTransaction(
        tx1.compute_txid(),
        0,
    ))?;
    store.ack_news(AckTransactionNews::SpendingUTXOTransaction(
        tx2.compute_txid(),
        1,
    ))?;
    store.ack_news(AckTransactionNews::SpendingUTXOTransaction(
        tx3.compute_txid(),
        2,
    ))?;

    let news = store.get_news()?;
    assert_eq!(news.len(), 0);

    Ok(())
}

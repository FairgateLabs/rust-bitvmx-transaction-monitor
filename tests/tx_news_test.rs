use bitcoin::{absolute::LockTime, key::rand, Transaction};
use bitvmx_transaction_monitor::{
    store::{MonitorStore, MonitorStoreApi},
    types::{BlockInfo, TransactionStatus},
};
use std::{path::PathBuf, rc::Rc, str::FromStr};
use storage_backend::storage::Storage;

pub fn generate_random_string() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..10).map(|_| rng.gen_range('a'..='z')).collect()
}

#[test]
fn tx_news_test() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/address_test/{}", generate_random_string());
    let storage = Rc::new(Storage::new_with_path(&PathBuf::from(path))?);
    let store = MonitorStore::new(storage)?;
    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    let block_hash = bitcoin::BlockHash::from_str(
        "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
    )?;

    let block_info = BlockInfo {
        block_height: 100,
        block_hash,
        is_orphan: false,
    };

    // No news for now
    let news = store.get_single_tx_news()?;
    assert_eq!(news, vec![]);

    // Save transaction and check news
    store.save_tx(&tx, block_info.clone())?;
    let news = store.get_single_tx_news()?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0].tx, Some(tx.clone()));
    assert_eq!(news[0].block_info, Some(block_info));

    // Acknowledge transaction news
    store.acknowledge_tx_news(tx.compute_txid())?;
    let news = store.get_single_tx_news()?;
    assert_eq!(news, Vec::<TransactionStatus>::new());

    // Check that the transaction is not in news after acknowledgment
    let news = store.get_single_tx_news()?;
    assert_eq!(news.len(), 0);

    Ok(())
}

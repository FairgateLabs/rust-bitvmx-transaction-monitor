use std::{path::PathBuf, rc::Rc, str::FromStr};

use bitcoin::{absolute::LockTime, key::rand, Address, Transaction};
use bitvmx_transaction_monitor::{
    store::{MonitorStore, MonitorStoreApi},
    types::{AddressStatus, BlockInfo},
};
use storage_backend::storage::Storage;

pub fn generate_random_string() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..10).map(|_| rng.gen_range('a'..='z')).collect()
}

#[test]
fn address_news_test() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/address_test/{}", generate_random_string());
    let storage = Rc::new(Storage::new_with_path(&PathBuf::from(path))?);
    let bitvmx_store = MonitorStore::new(storage)?;

    // Create two instances with one transaction each
    let address_1 = Address::from_str("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa")?.assume_checked();
    let address_2 =
        Address::from_str("bc1qtr59z7z74xett5avz7ml4hun6g92je0jlgclh2")?.assume_checked();

    //Validate: save address & get address
    bitvmx_store.save_address(address_1.clone())?;
    let addresses = bitvmx_store.get_addresses()?;
    assert_eq!(addresses, vec![address_1.clone()]);

    //Validate: Save address 2 and get addresses
    bitvmx_store.save_address(address_2.clone())?;
    let addresses = bitvmx_store.get_addresses()?;
    assert_eq!(addresses, vec![address_1.clone(), address_2.clone()]);

    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    // No news for now.
    let news = bitvmx_store.get_address_news()?;
    assert_eq!(news, Vec::<(Address, Vec<AddressStatus>)>::new());

    // It should have a news for address_1
    let block_hash = bitcoin::BlockHash::from_str(
        "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
    )?;
    bitvmx_store.update_address_news(address_1.clone(), &tx, 100, block_hash, false, 101)?;
    let news = bitvmx_store.get_address_news()?;
    println!("News: {:?}", news);
    assert_eq!(news[0].0, address_1.clone());

    let address_status = AddressStatus {
        tx: Some(tx.clone()),
        block_info: Some(BlockInfo {
            block_height: 100,
            block_hash,
            is_orphan: false,
        }),
        confirmations: 101,
    };

    assert_eq!(news[0].1, vec![address_status]);

    bitvmx_store.update_address_news(address_2.clone(), &tx, 100, block_hash, false, 101)?;
    let news = bitvmx_store.get_address_news()?;
    assert_eq!(news[0].0, address_1.clone());
    assert_eq!(news[1].0, address_2.clone());

    // acknowledge address_1 and address_2
    bitvmx_store.acknowledge_address_news(address_1.clone())?;
    let news = bitvmx_store.get_address_news()?;
    assert_eq!(news[0].0, address_2.clone());

    // acknowledge address_2
    bitvmx_store.acknowledge_address_news(address_2.clone())?;
    let news = bitvmx_store.get_address_news()?;
    assert_eq!(news, Vec::<(Address, Vec<AddressStatus>)>::new());

    Ok(())
}

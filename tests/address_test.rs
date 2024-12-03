use std::str::FromStr;

use bitcoin::{
    absolute::{self, LockTime},
    key::{rand, Secp256k1},
    secp256k1::{All, SecretKey},
    transaction, Address, Amount, CompressedPublicKey, Network, Transaction, TxOut,
};
use bitcoin_indexer::indexer::MockIndexerApi;
use bitvmx_transaction_monitor::{
    bitvmx_store::{BitvmxApi, BitvmxStore, MockBitvmxStore},
    monitor::Monitor,
};

pub fn generate_random_string() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..10).map(|_| rng.gen_range('a'..='z')).collect()
}

#[test]
fn detect_address_in_tx() -> Result<(), anyhow::Error> {
    let to = get_address();
    let to_clone = to.clone();

    let spend = TxOut {
        value: Amount::default(),
        script_pubkey: to.script_pubkey(),
    };

    let unsigned_tx = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![],
        output: vec![spend],
    };

    let mock_indexer = MockIndexerApi::new();
    let mock_bitvmx_store = MockBitvmxStore::new();
    let monitor = Monitor::new(mock_indexer, mock_bitvmx_store, None, 6);
    let matched = monitor.address_exist_in_output(to_clone, &unsigned_tx);

    assert!(matched);

    Ok(())
}

fn get_address() -> Address {
    let secp: Secp256k1<All> = Secp256k1::new();
    let sk = SecretKey::new(&mut rand::thread_rng());
    let pk = bitcoin::PublicKey::new(sk.public_key(&secp));
    let compressed = CompressedPublicKey::try_from(pk).unwrap();
    let to = Address::p2wpkh(&compressed, Network::Bitcoin);
    to
}

#[test]
fn address_test() -> Result<(), anyhow::Error> {
    let path = format!("test_outputs/address_test/{}", generate_random_string());
    let bitvmx_store = BitvmxStore::new_with_path(&path)?;
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
    assert_eq!(news, Vec::<(Address, Vec<Transaction>)>::new());

    // It should have a news for address_1
    bitvmx_store.update_address_news(address_1.clone(), &tx)?;
    let news = bitvmx_store.get_address_news()?;
    assert_eq!(news[0].0, address_1.clone());
    assert_eq!(news[0].1, vec![tx.clone()]);

    bitvmx_store.update_address_news(address_2.clone(), &tx)?;
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
    assert_eq!(news, Vec::<(Address, Vec<Transaction>)>::new());

    Ok(())
}

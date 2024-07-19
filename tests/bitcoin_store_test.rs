use rust_bitcoin_tx_monitor::stores::bitcoin_store::{BitcoinApi, BitcoinStore};

#[test]
#[ignore]

fn get_block_count() -> Result<(), anyhow::Error> {
    let database_url = String::from("postgres://postgres:admin@localhost:5433/bitcoin-indexer");
    let mut bitcoin_store = BitcoinStore::new(&database_url)?;
    let count = bitcoin_store.get_block_count()?;

    assert_eq!(count, 10400);

    Ok(())
}

#[test]
#[ignore]

fn exist_tx() -> Result<(), anyhow::Error> {
    let database_url = String::from("postgres://postgres:admin@localhost:5433/bitcoin-indexer");
    let mut bitcoin_store = BitcoinStore::new(&database_url)?;

    let tx_id = String::from("6fe2aef3426a6b9d4b9a774b58dafe7b736e7a67998ab54b53cf6e82df1a28b8");
    let exists_tx = bitcoin_store.tx_exists(&tx_id)?;

    println!("??? {:#?}", exists_tx);
    Ok(())
}

#[test]
#[ignore]

fn get_tx() -> Result<(), anyhow::Error> {
    let database_url = String::from("postgres://postgres:admin@localhost:5433/bitcoin-indexer");
    let mut bitcoin_store = BitcoinStore::new(&database_url)?;

    let tx_id = String::from("6fe2aef3426a6b9d4b9a774b58dafe7b736e7a67998ab54b53cf6e82df1a28b8");
    let _ = bitcoin_store.get_tx(&tx_id)?;

    Ok(())
}

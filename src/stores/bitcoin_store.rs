use crate::types::TxData;
use anyhow::{Ok, Result};
use mockall::{automock, predicate::*};
use postgres::{Client, NoTls};

pub struct BitcoinStore {
    client: Client,
}

pub trait BitcoinApi {
    /// Returns the height of the most-work fully-validated chain indexed
    fn get_block_count(&mut self) -> Result<u32>;

    //Return the tx data if exists otherwise None
    fn tx_exists(&mut self, tx_id: &str) -> Result<bool>;

    fn get_tx(&mut self, tx_id: &str) -> Result<Option<TxData>>;
}

impl BitcoinStore {
    pub fn new(bitcoin_indexer_db_url: &str) -> Result<Self> {
        let client = Client::connect(bitcoin_indexer_db_url, NoTls)?;

        Ok(BitcoinStore { client })
    }
}

#[automock]
impl BitcoinApi for BitcoinStore {
    /// Returns the height of the most-work fully-validated chain indexed
    fn get_block_count(&mut self) -> Result<u32> {
        let row = self
            .client
            .query_one("SELECT MAX(height) as height from block", &[])?;

        // In database is an integer, that is way we have to manage i32 here.
        let result: i32 = row.get("height");
        Ok(result as u32)
    }

    //Return the tx data if exists otherwise None
    fn tx_exists(&mut self, tx_id: &str) -> Result<bool> {
        let row = self.client.query_one(
            "SELECT EXISTS(SELECT 1 as exists FROM tx WHERE hash_id = $1::BYTEA)",
            &[&tx_id.as_bytes()],
        )?;

        Ok(row.get("exists"))
    }

    fn get_tx(&mut self, tx_id: &str) -> Result<Option<TxData>> {
        let row = self.client.query_one(
            "SELECT * FROM tx WHERE hash_id = $1::BYTEA;",
            &[&tx_id.as_bytes()],
        )?;

        // // let mempool_ts: NaiveDateTime = row.get("mempool_ts"); // Timestamp
        // let fee: i64 = row.get("fee"); // Int8 (i64)
        // let locktime: i64 = row.get("locktime"); // Int8 (i64)
        // let current_height: i32 = row.get("current_height"); // Int4
        // let weight: i32 = row.get("weight"); // Int4
        // let coinbase: bool = row.get("coinbase"); // Bool
        let hash_id: Vec<u8> = row.get("hash_id"); // Bytea
                                                   // let hash_rest: Vec<u8> = row.get("hash_rest"); // Bytea

        // // println!("mempool_ts: {:?}", mempool_ts);
        // println!("fee: {}", fee);
        // println!("locktime: {}", locktime);
        // println!("current_height: {}", current_height);
        // println!("weight: {}", weight);
        // println!("coinbase: {}", coinbase);
        println!("hash_id: {:?}", hash_id);
        // println!("hash_rest: {:?}", hash_rest);

        let tx_data = TxData {};
        Ok(Some(tx_data))
    }
}

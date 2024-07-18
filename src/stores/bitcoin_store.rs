use crate::types::TxData;
use anyhow::{Ok, Result};
use bitcoin::Txid;
use mockall::{automock, predicate::*};
use postgres::{Client, NoTls};

pub struct BitcoinStore {
    client: Client,
}

pub trait BitcoinApi {
    /// Returns the height of the most-work fully-validated chain indexed
    fn get_block_count(&mut self) -> Result<u32>;

    //Return the tx data if exists otherwise None
    fn tx_exists(&mut self, tx_id: &String) -> Result<bool>;

    fn get_tx(&mut self, tx_id: &String) -> Result<Option<TxData>>;
}

impl BitcoinStore {
    pub fn new(bitcoin_indexer_db_url: String) -> Result<Self> {
        let client = Client::connect(&bitcoin_indexer_db_url, NoTls)?;

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

        println!("{:?}", row);
        // In database is an integer, that is way we have to manage i32 here.
        let result: i32 = row.get("height");
        Ok(result as u32)
    }

    //Return the tx data if exists otherwise None
    fn tx_exists(&mut self, tx_id: &String) -> Result<bool> {
        let row = self.client.query_one(
            "SELECT EXISTS(SELECT 1 as exists FROM tx WHERE hash_id = $1::BYTEA)",
            &[],
        )?;

        Ok(row.get("exists"))
    }

    fn get_tx(&mut self, tx_id: &String) -> Result<Option<TxData>> {
        let row = self.client.query(
            "SELECT EXISTS(SELECT 1 FROM tx WHERE hash_id = $1::BYTEA);",
            &[],
        )?;

        let txData = TxData {};
        Ok(Some(txData))
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn get_block_count_test() -> Result<(), anyhow::Error> {
        let database_url = String::from("postgres://postgres:admin@localhost:5433/bitcoin-indexer");
        let mut bitcoin_store = BitcoinStore::new(database_url)?;
        let count = bitcoin_store.get_block_count()?;

        assert_eq!(count, 10400);

        Ok(())
    }
}

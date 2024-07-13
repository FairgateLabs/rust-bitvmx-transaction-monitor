use crate::types::{BlockHeight, TxData};
use anyhow::Result;
use bitcoin::Txid;

/// This is an abstraction of the data we can get to the real Bicoin Indexer we will implement
pub struct BitcoinStore {}

impl BitcoinStore {
    pub fn new(bitcoin_indexer_db_url: String) -> Result<Self> {
        Ok(Self {})
    }

    /// Returns the height of the most-work fully-validated chain indexed
    pub fn get_block_count() -> Result<Option<BlockHeight>> {
        Ok(Some(0))
    }

    // Return the tx data if exists otherwise None
    pub fn get_tx_id(tx_id: Txid) -> Result<Option<TxData>> {
        Ok(None)
    }
}

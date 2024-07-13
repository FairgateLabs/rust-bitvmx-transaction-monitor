use anyhow::Result;

use crate::stores::{bitcoin_store::BitcoinStore, operation_store::OperationStore};

pub struct Monitor {
    pub bitcoin_store: BitcoinStore,
    pub operation_store: OperationStore,
}

impl Monitor {
    pub fn new(bitcoin_indexer_db_url: String, operation_db_url: String) -> Result<Self> {
        let bitcoin_store = BitcoinStore::new(bitcoin_indexer_db_url)?;
        let operation_store = OperationStore::new(operation_db_url)?;

        Ok(Self {
            bitcoin_store,
            operation_store,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        //Intantiate Monitor, passing it the db_url and call the monitor check txns

        // should check pending operations in database and update each of them.

        // count existing operations get all thansaction that meet next rules:
        // operation transaction should no ve confirmn and > 5.
        // for the other trans we need to check:
        // transaction were not seen then we need to get the current block from the bitcoin api.

        // Get operations

        //for each operation get all thansactions

        //for each transaction do

        // check if txn was seen, then update the operation transaction row.

        Ok(())
    }
}

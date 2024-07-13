use std::str::FromStr;

use crate::types::{Operation, TxData};
use anyhow::Result;
use bitcoin::hash_types::Txid;
/// This is an abstraction of the data we can get to the real Bicoin Indexer we will implement
pub struct OperationStore {}

impl OperationStore {
    pub fn new(operation_db_url: String) -> Result<Self> {
        Ok(Self {})
    }

    /// Returns existing operations
    pub fn get_operations() -> Result<Option<Vec<Operation>>> {
        let mut operations: Vec<Operation> = Vec::new();

        let tx1 =
            Txid::from_str("d5d27987d2a3dfc724e359870c6644b40e497bdc0589a033220fe15429d885a0")
                .unwrap();
        let tx2 =
            Txid::from_str("d5d27987d2a3dfc724e359870c6644b40e497bdc0589a033220fe15429d885a0")
                .unwrap();

        let operation = Operation {
            id: 1,
            transaction_ids: vec![tx1, tx2],
            start_height: 100,
            last_verified_height: Some(100),
            tx_was_seen: false,
            block_tx_seen: Some(1),
            block_confirmations: Some(0),
        };

        operations.push(operation);

        Ok(Some(operations))
    }

    pub fn update_tx(tx_id: Txid) -> Result<Option<TxData>> {
        Ok(None)
    }
}

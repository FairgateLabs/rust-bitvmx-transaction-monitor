use bitcoin::{BlockHash, Transaction, Txid};
use bitcoin_indexer::{bitcoin_client::BitcoinClient, indexer::Indexer, store::IndexerStore};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{monitor::Monitor, store::MonitorStore};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransactionStore {
    pub tx_id: Txid,
    pub tx: Option<Transaction>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub struct AddressStatus {
    pub tx: Option<Transaction>,
    pub block_info: Option<BlockInfo>,
    pub confirmations: u32,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransactionStatus {
    pub tx_id: Txid,
    pub tx: Option<Transaction>,
    pub block_info: Option<BlockInfo>,
    pub confirmations: u32,
}

impl TransactionStatus {
    pub fn is_confirmed(&self) -> bool {
        self.block_info.is_some() && self.confirmations > 0
    }

    pub fn is_orphan(&self) -> bool {
        //Orphan should have:
        //  block_info because it was mined time before.
        //  confirmation == 0 , this is just a validation, orphan should be moved as confirmation 0.
        //  is_orphan = true
        self.block_info.is_some()
            && self.confirmations == 0
            && self.block_info.as_ref().unwrap().is_orphan
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct BlockInfo {
    pub block_height: BlockHeight,
    pub block_hash: BlockHash,
    pub is_orphan: bool,
}

pub struct BlockAgragatedInfo {
    pub block_height: BlockHeight,
    pub block_hash: BlockHash,
    pub confirmations: u32,
    pub is_orphan: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BitvmxInstance {
    //bitvmx instance id
    pub id: InstanceId,

    //bitvmx linked transactions data + speed up transactions data
    pub txs: Vec<TransactionStore>,

    //First height to start searching the bitvmx instance in the blockchain
    pub start_height: BlockHeight,
}
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct InstanceData {
    //bitvmx instance id
    pub instance_id: InstanceId,

    //bitvmx linked transactions data
    pub txs: Vec<Txid>,
}

pub type BlockHeight = u32;
pub type InstanceId = Uuid;
pub type MonitorType = Monitor<Indexer<BitcoinClient, IndexerStore>, MonitorStore>;
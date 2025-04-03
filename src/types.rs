use bitcoin::{BlockHash, Transaction, Txid};
use bitcoin_indexer::{indexer::Indexer, store::IndexerStore};
use bitvmx_bitcoin_rpc::{bitcoin_client::BitcoinClient, types::BlockHeight};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use uuid::Uuid;

use crate::{monitor::Monitor, store::MonitorStore};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransactionStore {
    pub tx_id: Txid,
    pub tx: Option<Transaction>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct TransactionStatus {
    pub tx_id: Txid,
    pub tx: Option<Transaction>,
    pub block_info: Option<BlockInfo>,
    pub confirmations: u32,
}

impl TransactionStatus {
    pub fn new(tx: Transaction, block_info: Option<BlockInfo>) -> Self {
        Self {
            tx_id: tx.compute_txid(),
            tx: Some(tx),
            block_info,
            confirmations: 0,
        }
    }
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

impl BlockInfo {
    pub fn new(block_height: BlockHeight, block_hash: BlockHash, is_orphan: bool) -> Self {
        Self {
            block_height,
            block_hash,
            is_orphan,
        }
    }
}

pub struct BlockAgragatedInfo {
    pub block_height: BlockHeight,
    pub block_hash: BlockHash,
    pub confirmations: u32,
    pub is_orphan: bool,
}

pub enum TransactionMonitorType {
    GroupTransaction(Id, Vec<Txid>),
    SingleTransaction(Txid),
    RskPeginTransaction,
    SpendingUTXOTransaction(Txid, Number),
}
pub enum MonitorNewType {
    GroupTransaction(Id, TransactionStatus),
    SingleTransaction(TransactionStatus),
    RskPeginTransaction(TransactionStatus),
    SpendingUTXOTransaction(TransactionStatus),
}

pub enum AcknowledgeNewType {
    GroupTransaction(Id, Txid),
    SingleTransaction(Txid),
    RskPeginTransaction(Txid),
    SpendingUTXOTransaction(Txid),
}

pub type Id = Uuid;

pub type MonitorType = Monitor<Indexer<BitcoinClient, IndexerStore>, MonitorStore>;

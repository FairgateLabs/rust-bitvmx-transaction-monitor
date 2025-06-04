use bitcoin::{BlockHash, Transaction, Txid};
use bitcoin_indexer::{indexer::Indexer, store::IndexerStore};
use bitvmx_bitcoin_rpc::{bitcoin_client::BitcoinClient, types::BlockHeight};
use serde::{Deserialize, Serialize};
use tracing::info;
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
    pub tx: Transaction,
    pub block_info: Option<BlockInfo>,
    pub confirmations: u32,
    pub status: TransactionBlockchainStatus,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub enum TransactionBlockchainStatus {
    // Represents a transaction that has been successfully confirmed by the network but a reorganizacion move it out of the chain.
    Orphan,
    // Represents a transaction that has been successfully confirmed by the network
    Confirmed,
    // Represents when the transaction was confirmed an amount of blocks
    Finalized,
}

impl TransactionStatus {
    pub fn new(
        tx: Transaction,
        block_info: Option<BlockInfo>,
        status: TransactionBlockchainStatus,
        confirmations: u32,
    ) -> Self {
        Self {
            tx_id: tx.compute_txid(),
            tx,
            block_info,
            confirmations,
            status,
        }
    }

    pub fn is_finalized(&self, confirmation_threshold: u32) -> bool {
        // A transaction is considered finalized if:
        // - It has block_info (was mined)
        // - The status is Finalized
        // - The number of confirmations meets or exceeds the confirmation threshold
        self.block_info.is_some()
            && self.confirmations >= confirmation_threshold
            && self.status == TransactionBlockchainStatus::Finalized
    }

    pub fn is_confirmed(&self) -> bool {
        //Confirmed should have:
        // A transaction is considered confirmed if it has been included in a block
        // and has at least one confirmation (confirmations > 0), regardless of the exact number of confirmations.
        self.block_info.is_some() && self.confirmations > 0
    }

    pub fn is_orphan(&self) -> bool {
        //Orphan should have:
        //  block_info because it was mined time before.
        //  confirmation == 0 , this is just a validation, orphan should be moved as confirmation 0.
        //  is_orphan = true
        //  status = Orphan
        self.block_info.is_some()
            && self.confirmations == 0
            && self.block_info.as_ref().unwrap().is_orphan
            && self.status == TransactionBlockchainStatus::Orphan
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

#[derive(Debug, Clone, PartialEq)]
pub enum TypesToMonitor {
    Transactions(Vec<Txid>, String),
    SpendingUTXOTransaction(Txid, u32, String),
    RskPeginTransaction,
    NewBlock,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MonitorNews {
    Transaction(Txid, TransactionStatus, String),
    SpendingUTXOTransaction(Txid, u32, TransactionStatus, String),
    RskPeginTransaction(Txid, TransactionStatus),
    NewBlock(BlockHeight, BlockHash),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum AckMonitorNews {
    Transaction(Txid),
    RskPeginTransaction(Txid),
    SpendingUTXOTransaction(Txid, u32),
    NewBlock,
}

pub type Id = Uuid;

pub type MonitorType = Monitor<Indexer<BitcoinClient, IndexerStore>, MonitorStore>;

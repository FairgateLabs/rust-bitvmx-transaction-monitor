use bitcoin::{BlockHash, Transaction, Txid};
use bitcoin_indexer::IndexerType;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use serde::{Deserialize, Serialize};
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
    pub block_info: FullBlock,
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
        block_info: FullBlock,
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
        // - The status is Finalized
        // - The number of confirmations meets or exceeds the confirmation threshold
        self.confirmations >= confirmation_threshold
            && self.status == TransactionBlockchainStatus::Finalized
    }

    pub fn is_confirmed(&self) -> bool {
        // A transaction is considered confirmed if it has been included in a block
        // and has at least one confirmation (confirmations > 0), regardless of the exact number of confirmations.
        // This means the transaction is in the main chain and not orphaned.
        self.confirmations > 0
    }

    pub fn is_orphan(&self) -> bool {
        //Orphan should have:
        //  block_info because it was mined time before.
        //  confirmation == 0 , this is just a validation, orphan should be moved as confirmation 0.
        //  is_orphan = true
        //  status = Orphan
        self.confirmations == 0
            && self.block_info.orphan
            && self.status == TransactionBlockchainStatus::Orphan
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct BlockInfo {
    pub block_height: BlockHeight,
    pub block_hash: BlockHash,
    pub is_orphan: bool,
    pub transactions: Vec<Txid>,
}

impl BlockInfo {
    pub fn new(
        block_height: BlockHeight,
        block_hash: BlockHash,
        is_orphan: bool,
        transactions: Vec<Txid>,
    ) -> Self {
        Self {
            block_height,
            block_hash,
            is_orphan,
            transactions,
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

pub type MonitorType = Monitor<IndexerType, MonitorStore>;

pub type FullBlock = bitcoin_indexer::types::FullBlock;

use bitcoin::{BlockHash, Transaction, Txid};
use bitcoin_indexer::{types::TransactionInfo, IndexerType};
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

#[derive(Debug, Clone, PartialEq)]
pub enum TypesToMonitor {
    // Transactions to monitor
    // - Vec<Txid>: The transaction IDs to monitor
    // - String: The context of the transaction
    // - Option<u32>: The number of confirmations to wait for receive news about the transaction
    Transactions(Vec<Txid>, String, Option<u32>),

    // Spending UTXO transaction to monitor
    // - Txid: The transaction ID to monitor
    // - u32: The vout index of the UTXO to monitor
    // - String: The context of the transaction
    // - Option<u32>: The number of confirmations to wait for receive news about the transaction
    SpendingUTXOTransaction(Txid, u32, String, Option<u32>),

    // Rsk pegin transaction to monitor
    // - Option<u32>: The number of confirmations to wait for receive news about the transaction
    RskPegin(Option<u32>),

    // New block to monitor
    // - BlockHeight: The block height to monitor
    NewBlock,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MonitorNews {
    // Transaction news
    // - Txid: The transaction ID
    // - TransactionInfo: The information of the transaction indexed
    // - String: The context of the transaction previously sent to the monitor
    Transaction(Txid, TransactionInfo, String),

    // Spending UTXO transaction news
    // - Txid: The transaction ID
    // - u32: The vout index of the UTXO
    // - TransactionInfo: The information of the transaction indexed
    // - String: The context of the transaction previously sent to the monitor
    SpendingUTXOTransaction(Txid, u32, TransactionInfo, String),

    // Rsk pegin transaction news
    // - Txid: The transaction ID
    // - TransactionInfo: The information of the transaction indexed
    RskPeginTransaction(Txid, TransactionInfo),

    // New block news
    // - BlockHeight: The block height
    // - BlockHash: The block hash
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

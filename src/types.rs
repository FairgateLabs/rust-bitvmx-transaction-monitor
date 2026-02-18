use bitcoin::{BlockHash, Transaction, Txid};
use bitcoin_indexer::types::TransactionStatus;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
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
    // - TransactionStatus: The information of the transaction indexed
    // - String: The context of the transaction previously sent to the monitor
    Transaction(Txid, TransactionStatus, String),

    // Spending UTXO transaction news
    // - Txid: The transaction ID
    // - u32: The vout index of the UTXO
    // - TransactionStatus: The information of the transaction indexed
    // - String: The context of the transaction previously sent to the monitor
    SpendingUTXOTransaction(Txid, u32, TransactionStatus, String),

    // Rsk pegin transaction news
    // - Txid: The transaction ID
    // - TransactionStatus: The information of the transaction indexed
    RskPeginTransaction(Txid, TransactionStatus),

    // New block news
    // - BlockHeight: The block height
    // - BlockHash: The block hash
    NewBlock(BlockHeight, BlockHash),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum AckMonitorNews {
    // Transaction news
    // - Txid: The transaction ID
    // - String: The context of the transaction
    Transaction(Txid, String),

    // Rsk pegin transaction news
    // - Txid: The transaction ID
    RskPeginTransaction(Txid),

    // Spending UTXO transaction news
    // - Txid: The transaction ID
    // - u32: The vout index of the UTXO
    // - String: The context of the transaction
    SpendingUTXOTransaction(Txid, u32, String),

    // New block news
    NewBlock,
}

pub type Id = Uuid;

pub type FullBlock = bitcoin_indexer::types::FullBlock;

// Storage types for monitor store

/// News acknowledgment info (block_hash, acknowledged)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct NewsAck {
    pub block_hash: BlockHash,
    pub acknowledged: bool,
}

impl NewsAck {
    pub fn new(block_hash: BlockHash, acknowledged: bool) -> Self {
        Self {
            block_hash,
            acknowledged,
        }
    }
}

/// Transaction news entry stored in storage
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TransactionNewsEntry {
    pub tx_id: Txid,
    pub extra_data: String,
    pub ack: NewsAck,
}

/// RskPegin transaction news entry stored in storage
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RskPeginNewsEntry {
    pub tx_id: Txid,
    pub ack: NewsAck,
}

/// SpendingUTXO transaction news entry stored in storage
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SpendingUTXONewsEntry {
    pub tx_id: Txid,
    pub utxo_index: u32,
    pub extra_data: String,
    pub spender_tx_id: Txid,
    pub ack: NewsAck,
}

/// Transaction monitor entry (extra_data, confirmation_trigger, trigger_sent)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TransactionMonitorEntry {
    pub extra_data: String,
    pub confirmation_trigger: Option<u32>,
    pub trigger_sent: bool,
}

/// Transaction monitor stored in active/inactive lists
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TransactionMonitor {
    pub tx_id: Txid,
    pub entries: Vec<TransactionMonitorEntry>,
}

/// SpendingUTXO monitor entry (extra_data, spender_tx_id, confirmation_trigger)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SpendingUTXOMonitorEntry {
    pub extra_data: String,
    pub spender_tx_id: Option<Txid>,
    pub confirmation_trigger: Option<u32>,
}

/// SpendingUTXO monitor stored in active/inactive lists
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SpendingUTXOMonitor {
    pub tx_id: Txid,
    pub vout: u32,
    pub entries: Vec<SpendingUTXOMonitorEntry>,
}

/// RskPegin monitor state (active, confirmation_trigger)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RskPeginMonitorState {
    pub active: bool,
    pub confirmation_trigger: Option<u32>,
}

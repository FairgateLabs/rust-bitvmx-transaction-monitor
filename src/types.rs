use bitcoin::{BlockHash, OutPoint, Txid};
use bitcoin_indexer::types::TransactionStatus;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use serde::{Deserialize, Serialize};

/// Generic filter for detecting transactions that match a specific output pattern.
///
/// Matches transactions where a specific output is an OP_RETURN whose pushed data starts
/// with the given `tag` bytes. Optionally enforces an upper bound on the total number of
/// outputs in the transaction.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutputPatternFilter {
    /// Index of the output to inspect (0-based).
    pub output_index: usize,
    /// Byte prefix that the OP_RETURN pushed data must start with.
    pub tag: Vec<u8>,
    /// If `Some(n)`, the transaction must have at most `n` outputs.
    pub max_outputs: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypesToMonitor {
    // Transactions to monitor
    // - Vec<Txid>: The transaction IDs to monitor
    // - String: The context of the transaction
    // - Option<u32>: The number of confirmations to wait for receive news about the transaction
    Transactions(Vec<Txid>, String, Option<u32>),

    // Spending UTXO transaction to monitor
    // - OutPoint: The UTXO to monitor
    // - String: The context of the transaction
    // - Option<u32>: The number of confirmations to wait for receive news about the transaction
    SpendingUTXOTransaction(OutPoint, String, Option<u32>),

    // Generic output pattern transaction to monitor
    // - OutputPatternFilter: The filter describing which output/tag to match
    // - Option<u32>: The number of confirmations to wait for before emitting news
    OutputPattern(OutputPatternFilter, Option<u32>),

    // New block to monitor
    // - BlockHeight: The block height to monitor
    NewBlock,
}

/// Payload of a transaction confirmation notification.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TransactionNews {
    pub tx_id: Txid,
    pub status: TransactionStatus,
    /// The context of the transaction previously sent to the monitor.
    pub context: String,
    /// True when this is a repeat notification caused by a chain reorganization: the tx already reached its
    /// confirmation trigger once, then a reorg re-mined it into a different block and it reached the trigger
    /// again. False for the first notification and for confirmations that keep growing inside the same block.
    pub resent_due_to_reorg: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MonitorNews {
    // Transaction confirmation news. Carries the reorg-resend flag in `resent_due_to_reorg`.
    Transaction(TransactionNews),

    // Spending UTXO transaction news
    // - OutPoint: The UTXO that was spent
    // - TransactionStatus: The information of the transaction indexed
    // - String: The context of the transaction previously sent to the monitor
    SpendingUTXOTransaction(OutPoint, TransactionStatus, String),

    // Generic output pattern transaction news
    // - Txid: The transaction ID
    // - TransactionStatus: The information of the transaction indexed
    // - Vec<u8>: The tag that was matched (identifies which pattern triggered)
    OutputPatternTransaction(Txid, TransactionStatus, Vec<u8>),

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

    // Generic output pattern transaction news
    // - Txid: The transaction ID
    // - Vec<u8>: The tag that was matched
    OutputPatternTransaction(Txid, Vec<u8>),

    // Spending UTXO transaction news
    // - OutPoint: The UTXO that was spent
    // - String: The context of the transaction
    SpendingUTXOTransaction(OutPoint, String),

    // New block news
    NewBlock,
}

pub type FullBlock = bitcoin_indexer::types::FullBlock;

// Storage types for monitor store

/// News acknowledgment info (block_hash, acknowledged)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct NewsAck {
    pub block_hash: BlockHash,
    pub acknowledged: bool,
}

/// New block news entry stored in storage.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct NewBlockNewsEntry {
    pub height: BlockHeight,
    pub ack: NewsAck,
}

/// Transaction news entry stored in storage
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TransactionNewsEntry {
    pub tx_id: Txid,
    pub context: String,
    pub ack: NewsAck,
    pub resent_due_to_reorg: bool, // True when this pending news is a reorg-caused resend.
}

/// SpendingUTXO transaction news entry stored in storage
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SpendingUTXONewsEntry {
    pub outpoint: OutPoint,
    pub context: String,
    pub spender_tx_id: Txid,
    pub ack: NewsAck,
}

/// The parameters of one subscription to a target. The same for every kind of monitor.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MonitorEntry {
    pub context: String,
    pub confirmation_trigger: Option<u32>,
    pub search_in_mempool: bool,
}

/// A subscription to a transaction. Only transactions are followed confirmation by confirmation, so only they
/// carry the state that follow needs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TransactionMonitorEntry {
    pub entry: MonitorEntry,
    /// Block hash of the block that included the tx the last time its confirmation trigger fired.
    pub notified_block_hash: Option<BlockHash>,
}

/// Every subscription to one transaction, under the contexts it was monitored with.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TransactionMonitor {
    pub tx_id: Txid,
    pub entries: Vec<TransactionMonitorEntry>,
}

/// Every subscription to the spending of one UTXO.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SpendingUtxoMonitor {
    pub outpoint: OutPoint,
    pub entries: Vec<MonitorEntry>,
}

/// Every subscription to one output pattern.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutputPatternMonitor {
    pub filter: OutputPatternFilter,
    pub entries: Vec<MonitorEntry>,
}

/// Output-pattern transaction news entry stored in storage
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutputPatternNewsEntry {
    pub tx_id: Txid,
    /// The tag bytes of the filter that matched this transaction
    pub tag: Vec<u8>,
    pub ack: NewsAck,
}

use bitcoin::{BlockHash, OutPoint, Txid};
use bitcoin_indexer::types::TransactionStatus;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use serde::{Deserialize, Serialize};

pub type FullBlock = bitcoin_indexer::types::FullBlock;

/// What the consumer asked to watch. It is the argument of monitor and cancel, and the key of a record.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum MonitorTarget {
    Transaction(Txid),
    SpendingUtxo(OutPoint),
    OutputPattern(OutputPatternFilter),
    NewBlock,
}

/// One subscription: what the consumer asked for, and the transactions it tracks because of it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MonitorEntry {
    pub context: String,                   // The consumer's context string.
    pub confirmation_trigger: Option<u32>, // If `Some(n)`, the consumer only wants news when the transaction reaches `n` confirmations.
    pub search_in_mempool: bool, // Registers the txid in the indexer's mempool watch list, so InMempool state is possible (only for a transaction target).
    pub tracked: Vec<TrackedTx>, // Transactions that are being followed because of this subscription. Empty when the subscription has not found anything yet.
}

/// A target and every subscription to it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MonitorRecord {
    pub target: MonitorTarget,
    pub entries: Vec<MonitorEntry>,
}

/// A transaction an entry tracks. Only transactions that are in a block are ever tracked.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TrackedTx {
    pub txid: Txid,
    pub confirmed_at: BlockRef, // The block it is in. Set when it is discovered and never changed afterwards (except for a reorg).
}

/// Where a block is in the chain.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct BlockRef {
    pub height: BlockHeight,
    pub hash: BlockHash,
}
/// Generic filter for detecting transactions that match a specific output pattern: the output at `output_index`
/// must be an OP_RETURN whose pushed data starts with `tag`, and the transaction must have at most `max_outputs`
/// outputs when that is set.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutputPatternFilter {
    pub output_index: usize,        // Index of the output to inspect.
    pub tag: Vec<u8>,               // Byte prefix that the OP_RETURN pushed data must start with.
    pub max_outputs: Option<usize>, // If `Some(n)`, the transaction must have at most `n` outputs.
}

/// One thing to report, to the subscription that asked for it. This is what `get_news` returns and what
/// `ack_news` takes back, so a consumer acknowledges the exact value it was given.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MonitorNews {
    pub target: MonitorTarget,
    pub context: String,
    pub kind: NewsKind,
}

/// What happened. A transaction event carries a status, a block event carries the block.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum NewsKind {
    // A transaction was found, or its status changed.
    Transaction {
        txid: Txid,
        status: TransactionStatus,
        due_to_reorg: bool, // True only when this item exists because the chain was reorganised
    },
    // A new block was added to the chain, which is what a new block subscription is for.
    Block(BlockRef),
    // A UTXO was already spent when the subscription was registered, its spender is in no block the indexer
    // holds and its output is older than all of them. The subscription stays, so a spender found later is
    // still reported. Consumer is responsible for deciding if waiting for a spender or cancel the subscription.
    ProbablyUnreachableUTXO,
    // A transaction was mined before the first block the indexer ever read, so before the monitor started. The
    // subscription is dropped with it, because no block the monitor reads will ever hold it.
    UnreachableTx,
}

//! Fixtures shared by the benches. Everything is built on storage alone: the node client points at a port nobody
//! listens on, so a pass that called the node by mistake would fail the bench instead of quietly timing the network.

#![allow(dead_code)] // Each bench file uses a different part of this module.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use bitcoin::hashes::Hash;
use bitcoin::script::PushBytesBuf;
use bitcoin::{Amount, BlockHash, Network, OutPoint, ScriptBuf, Transaction, TxOut, Txid};
use bitcoin_indexer::store::IndexerStore;
use bitcoin_indexer::types::FullBlock;
use bitcoin_indexer::Indexer;
use bitvmx_bitcoin_rpc::bitcoin_client::BitcoinClient;
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use bitvmx_transaction_monitor::config::{MonitorSettings, MonitorSettingsConfig};
use bitvmx_transaction_monitor::core::news::PendingNews;
use bitvmx_transaction_monitor::core::store::MonitorStore;
use bitvmx_transaction_monitor::core::subscriptions::Subscriptions;
use bitvmx_transaction_monitor::monitor::Monitor;
use bitvmx_transaction_monitor::test_utils::tx;
use bitvmx_transaction_monitor::types::{MonitorTarget, OutputPatternFilter};
use storage_backend::storage::Storage;
use storage_backend::storage_config::StorageConfig;

pub const SIZES: [u64; 3] = [10, 100, 1000]; // The sizes every group is run at. Ten times more input should cost about ten times more time.
pub const BLOCK_TXS: u32 = 2000; // Transactions in every block, in the range of a mainnet block's count.
pub const TAG: &[u8] = b"bitvmx"; // The tag every pattern looks for.
const NO_NODE: &str = "http://127.0.0.1:1"; // A port nothing listens on, so any node call fails at once.

/// One monitor on its own database, plus the parts a bench drives directly.
pub struct Fixture {
    pub monitor: Monitor,
    pub subscriptions: Subscriptions,
    pub news: PendingNews,
    pub indexer_store: IndexerStore,
    _dir: TempDir, // Last, so the database is closed before its directory is removed.
}

impl Fixture {
    pub fn new() -> Self {
        let dir = TempDir::new();
        let storage = Rc::new(
            Storage::new(&StorageConfig::new(
                dir.0.to_string_lossy().to_string(),
                None,
            ))
            .expect("bench storage"),
        );

        let rpc = RpcConfig::new(
            Network::Regtest,
            NO_NODE.to_string(),
            String::new(),
            String::new(),
            String::new(),
        );
        let settings = MonitorSettings::from(MonitorSettingsConfig::default());

        let monitor = Monitor::new(&rpc, storage.clone(), None).expect("monitor");
        let indexer = Rc::new(
            Indexer::new(
                BitcoinClient::new_from_config(&rpc).expect("client"),
                storage.clone(),
                settings.indexer_settings.clone(),
            )
            .expect("indexer"),
        );

        Self {
            monitor,
            subscriptions: Subscriptions::new(
                MonitorStore::new(storage.clone()),
                indexer,
                settings,
            ),
            news: PendingNews::new(MonitorStore::new(storage.clone())),
            indexer_store: IndexerStore::new(storage).expect("indexer store"),
            _dir: dir,
        }
    }

    /// Stores a block the way the indexer does on a tick, and moves the cursor to it.
    pub fn index_block(&self, block: &FullBlock) {
        if self.indexer_store.get_first_height().unwrap().is_none() {
            self.indexer_store.save_first_height(block.height).unwrap();
        }
        self.indexer_store.save_block(block).unwrap();
        self.indexer_store.save_cursor(block.height).unwrap();
    }

    /// Indexes a block and runs the block pass on it, keeping the news it produced, which is what a tick does.
    pub fn tick_with(&self, block: &FullBlock) {
        self.index_block(block);
        let news = self.subscriptions.process_block(block).unwrap();
        self.news.add(news).unwrap();
    }
}

/// A block at this height holding these transactions, filled up to `BLOCK_TXS` with transactions nobody watches.
pub fn block(height: BlockHeight, mut txs: Vec<Transaction>) -> FullBlock {
    let first_filler = height * BLOCK_TXS;
    let fillers = BLOCK_TXS.saturating_sub(txs.len() as u32);
    txs.extend((0..fillers).map(|i| plain_tx(1_000_000 + first_filler + i)));

    FullBlock {
        height,
        hash: block_hash(height),
        prev_hash: block_hash(height.saturating_sub(1)),
        txs,
        estimated_fee_rate: 1,
    }
}

/// A transaction with one input and one ordinary output, unique per seed and matching no pattern.
pub fn plain_tx(seed: u32) -> Transaction {
    tx(
        seed,
        vec![OutPoint::new(fake_txid(0, seed), 0)],
        vec![TxOut {
            value: Amount::from_sat(1000),
            script_pubkey: ScriptBuf::new(),
        }],
    )
}

/// A transaction whose output 0 is an OP_RETURN carrying the tag, so every pattern in the benches matches it.
pub fn tagged_tx(seed: u32) -> Transaction {
    let mut pushed = PushBytesBuf::new();
    pushed.extend_from_slice(TAG).unwrap();
    pushed.extend_from_slice(&seed.to_le_bytes()).unwrap();

    tx(
        seed,
        vec![OutPoint::new(fake_txid(0, seed), 0)],
        vec![TxOut {
            value: Amount::ZERO,
            script_pubkey: ScriptBuf::new_op_return(pushed),
        }],
    )
}

/// A pattern for the tag, with a bound on the outputs so that `seed` makes each one a different target.
pub fn pattern(seed: usize) -> OutputPatternFilter {
    OutputPatternFilter {
        output_index: 0,
        tag: TAG.to_vec(),
        max_outputs: Some(seed + 1),
    }
}

/// `n` targets of each kind that no block in the benches ever answers: transactions never mined, outputs never spent,
/// and patterns for a tag no transaction carries.
pub fn unanswered_targets(n: u64) -> Vec<MonitorTarget> {
    let n = n as u32;
    let mut targets = Vec::with_capacity(3 * n as usize);
    targets.extend((0..n).map(|i| MonitorTarget::Transaction(fake_txid(1, i))));
    targets.extend((0..n).map(|i| MonitorTarget::SpendingUtxo(OutPoint::new(fake_txid(2, i), 0))));
    targets.extend((0..n).map(|i| {
        MonitorTarget::OutputPattern(OutputPatternFilter {
            output_index: 0,
            tag: b"nobody".to_vec(),
            max_outputs: Some(i as usize + 1),
        })
    }));
    targets
}

/// A txid that no transaction has, unique per namespace and index.
pub fn fake_txid(namespace: u8, index: u32) -> Txid {
    let mut bytes = [0u8; 32];
    bytes[0] = namespace;
    bytes[1..5].copy_from_slice(&index.to_le_bytes());
    Txid::from_byte_array(bytes)
}

fn block_hash(height: BlockHeight) -> BlockHash {
    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&height.to_le_bytes());
    BlockHash::from_byte_array(bytes)
}

/// A directory under the system temp folder, removed when dropped so the benches leave nothing behind.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let path = std::env::temp_dir().join(format!(
            "bitvmx_monitor_bench_{}_{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);

        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

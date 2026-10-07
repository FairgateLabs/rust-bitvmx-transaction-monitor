#![allow(dead_code)]
//! Fixtures for the integration tests: a regtest node, a storage of its own, a monitor on top of both, and the
//! chain manipulation the tests need. One node and one database per test, and the tests take the node one at a
//! time, so nothing has to be run with `--test-threads=1`.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use bitcoin::consensus::{Decodable, Encodable};
use bitcoin::script::PushBytesBuf;
use bitcoin::{
    absolute, transaction, Address, Amount, Block, BlockHash, Network, OutPoint, ScriptBuf,
    Sequence, Transaction, TxIn, TxOut, Txid, Witness,
};
use bitcoin_indexer::config::IndexerSettings;
use bitcoin_indexer::indexer::Indexer;
use bitcoincore_rpc::json::CreateRawTransactionInput;
use bitcoincore_rpc::RpcApi;
use bitcoind::{bitcoind::Bitcoind, config::BitcoindConfig};
use bitvmx_bitcoin_rpc::{
    bitcoin_client::{BitcoinClient, BitcoinClientApi},
    rpc_config::RpcConfig,
    types::BlockHeight,
};
use bitvmx_transaction_monitor::config::MonitorSettingsConfig;
use bitvmx_transaction_monitor::errors::MonitorError;
use bitvmx_transaction_monitor::monitor::Monitor;
use bitvmx_transaction_monitor::types::{BlockRef, MonitorNews, MonitorTarget, NewsKind};
use bitvmx_transaction_monitor::TransactionStatus;
use storage_backend::{
    storage::{KeyValueStore, Storage},
    storage_config::StorageConfig,
};
use tracing::info;

/// Upper bound for the ticks a test waits for the monitor to reach the node's tip.
const MAX_SYNC_TICKS: u32 = 1_000;

pub fn init_trace() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
}

/// A name that is unique per process and call, used for storage directories.
fn unique_name(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "{prefix}_{}_{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

// =============================================================================
// Storage
// =============================================================================

/// Storage under `temp-runs/`, removed when this value is dropped.
pub struct TestStorage {
    path: String,
    storage: Option<Rc<Storage>>,
}

impl TestStorage {
    pub fn new() -> Self {
        let path = format!("temp-runs/{}", unique_name("monitor_storage"));
        let _ = std::fs::remove_dir_all(&path);

        let storage =
            Rc::new(Storage::new(&StorageConfig::new(path.clone(), None)).expect("test storage"));

        Self {
            path,
            storage: Some(storage),
        }
    }

    /// What the monitor is built from.
    pub fn storage(&self) -> Rc<Storage> {
        Rc::clone(self.storage.as_ref().expect("test storage already removed"))
    }
}

impl Default for TestStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestStorage {
    fn drop(&mut self) {
        self.storage.take();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// =============================================================================
// Regtest node
// =============================================================================

/// Only one regtest container can run at a time, because the container name and the RPC port are fixed. Every
/// test holds this for as long as it holds its node, which is what lets the file run without `--test-threads=1`.
static RPC_LOCK: Mutex<()> = Mutex::new(());

/// A fresh regtest bitcoind in Docker, with a funded wallet. The container is stopped when this value is dropped.
pub struct TestNode {
    pub rpc_config: RpcConfig,
    pub client: BitcoinClient,
    pub miner: Address,
    bitcoind: Bitcoind,
    _guard: MutexGuard<'static, ()>,
}

impl TestNode {
    /// Starts the node and mines `blocks` blocks to its wallet. Coinbase outputs need 100 blocks to be spendable,
    /// so a test that spends anything starts from at least 101.
    pub fn start(blocks: u64) -> anyhow::Result<Self> {
        let _guard = RPC_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let rpc_config = RpcConfig::new(
            Network::Regtest,
            "http://127.0.0.1:18443".to_string(),
            "foo".to_string(),
            "rpcpassword".to_string(),
            "test_wallet".to_string(),
        );

        let bitcoind = Bitcoind::new(BitcoindConfig::default(), rpc_config.clone(), None);
        info!("Starting bitcoind");
        bitcoind.start().map_err(|e| {
            anyhow::anyhow!("Failed to start bitcoind: {e:?}. Make sure Docker is running.")
        })?;

        let client = BitcoinClient::new_from_config(&rpc_config)?;

        // The container is started before bitcoind is answering, so the first call waits for it instead of failing
        // the test. Giving up here would only mean the next call reports the same thing.
        for _ in 0..20 {
            if client.get_tip_height().is_ok() {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        let miner = client.init_wallet("test_wallet")?;

        let node = Self {
            rpc_config,
            client,
            miner,
            bitcoind,
            _guard,
        };
        node.mine(blocks)?;

        Ok(node)
    }

    /// A monitor on this node, with its own RPC client and the indexer it owns. Finality is one, the minimum a
    /// configuration allows, because a test chain reorgs at will and these tests invalidate blocks a real finality
    /// would have protected. That is also what lets a test use the small triggers a few mined blocks allow. The
    /// test that cares about the bound sets its own finality.
    pub fn monitor(
        &self,
        storage: Rc<Storage>,
        max_monitoring_confirmations: u32,
        retention_depth: BlockHeight,
    ) -> Result<Monitor, MonitorError> {
        self.monitor_with_finality(storage, 1, max_monitoring_confirmations, retention_depth)
    }

    pub fn monitor_with_finality(
        &self,
        storage: Rc<Storage>,
        finality: u32,
        max_monitoring_confirmations: u32,
        retention_depth: BlockHeight,
    ) -> Result<Monitor, MonitorError> {
        Monitor::new(
            &self.rpc_config,
            storage,
            Some(MonitorSettingsConfig {
                max_monitoring_confirmations: Some(max_monitoring_confirmations),
                finality: Some(finality),
                indexer_settings: Some(IndexerSettings::new(retention_depth)),
            }),
        )
    }

    /// What the indexer alone knows about a transaction, with no node fallback. It reads the same storage the monitor
    /// gave its indexer, so a test can tell whether an answer the monitor gave could only have come from the node.
    pub fn stored_tx_status(
        &self,
        storage: Rc<Storage>,
        txid: &Txid,
    ) -> anyhow::Result<TransactionStatus> {
        let indexer = Indexer::new(BitcoinClient::new_from_config(&self.rpc_config)?, storage, None)?;

        Ok(indexer.get_stored_transaction(txid, false)?)
    }

    pub fn tip(&self) -> anyhow::Result<BlockHeight> {
        Ok(self.client.get_tip_height()?)
    }

    pub fn hash_at(&self, height: BlockHeight) -> anyhow::Result<BlockHash> {
        Ok(self.client.get_block_id_by_height(&height)?)
    }

    pub fn block_at(&self, height: BlockHeight) -> anyhow::Result<Block> {
        Ok(self.client.get_block_by_hash(&self.hash_at(height)?)?)
    }

    /// Where the node has this transaction, which is what a reported height is checked against.
    pub fn height_of(&self, tx_id: &Txid) -> anyhow::Result<BlockHeight> {
        let info = self
            .client
            .get_raw_transaction_info(tx_id)?
            .ok_or_else(|| anyhow::anyhow!("the node does not know {tx_id}"))?;
        let hash = info
            .blockhash
            .ok_or_else(|| anyhow::anyhow!("{tx_id} is not in a block"))?;

        Ok(self.client.get_block_header_info(&hash)?.height as BlockHeight)
    }

    /// Mines `blocks` blocks to the wallet. They include whatever is in the mempool.
    pub fn mine(&self, blocks: u64) -> anyhow::Result<()> {
        Ok(self
            .client
            .mine_blocks_to_address(blocks, &self.fresh_address()?)?)
    }

    /// Broadcasts these transactions and mines one block, so all of them land in the same block. That is what
    /// a test needs to say "this block holds a match and a transaction nobody watches".
    pub fn mine_including(&self, txs: &[Transaction]) -> anyhow::Result<()> {
        for tx in txs {
            self.broadcast(tx)?;
        }
        self.mine(1)
    }

    /// Invalidates the block at `height` and every block above it, which takes their transactions back to the
    /// mempool. Returns the invalidated hash, which `reconsider` can put back.
    pub fn invalidate(&self, height: BlockHeight) -> anyhow::Result<BlockHash> {
        let hash = self.hash_at(height)?;
        self.client.invalidate_block(&hash)?;
        Ok(hash)
    }

    /// Makes an invalidated block valid again, so the node can switch back to its chain.
    pub fn reconsider(&self, hash: &BlockHash) -> anyhow::Result<()> {
        Ok(self.client.client.reconsider_block(hash)?)
    }

    pub fn fresh_address(&self) -> anyhow::Result<Address> {
        Ok(self
            .client
            .client
            .get_new_address(None, None)?
            .assume_checked())
    }

    /// Funds a fresh wallet address with `sats`, mining one block, and locks the output so the wallet does not
    /// spend it on its own.
    pub fn fund_utxo(&self, sats: u64) -> anyhow::Result<OutPoint> {
        let address = self.fresh_address()?;
        let (tx, vout) = self.client.fund_address(&address, Amount::from_sat(sats))?;
        let outpoint = OutPoint {
            txid: tx.compute_txid(),
            vout,
        };
        self.client.client.lock_unspent(&[outpoint])?;

        Ok(outpoint)
    }

    /// A signed, not broadcast transaction spending `outpoint` to a fresh wallet address.
    pub fn sign_spend(&self, outpoint: OutPoint, value_out: u64) -> anyhow::Result<Transaction> {
        let inputs = [CreateRawTransactionInput {
            txid: outpoint.txid,
            vout: outpoint.vout,
            sequence: None,
        }];
        let mut outputs = HashMap::new();
        outputs.insert(
            self.fresh_address()?.to_string(),
            Amount::from_sat(value_out),
        );

        let raw = self
            .client
            .client
            .create_raw_transaction(&inputs, &outputs, None, None)?;

        let mut encoded = Vec::new();
        raw.consensus_encode(&mut encoded)?;

        self.sign(&encoded)
    }

    /// A signed, not broadcast transaction spending `outpoint` and paying `outputs`, which is how a transaction
    /// with an OP_RETURN at a chosen index is built: the node's wallet will not create one of those.
    pub fn sign_spend_to(
        &self,
        outpoint: OutPoint,
        outputs: Vec<TxOut>,
    ) -> anyhow::Result<Transaction> {
        let unsigned = Transaction {
            version: transaction::Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: outpoint,
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: outputs,
        };

        let mut encoded = Vec::new();
        unsigned.consensus_encode(&mut encoded)?;

        self.sign(&encoded)
    }

    fn sign(&self, raw: &[u8]) -> anyhow::Result<Transaction> {
        let signed = self
            .client
            .client
            .sign_raw_transaction_with_wallet(raw, None, None)?;
        anyhow::ensure!(signed.complete, "signing incomplete: {:?}", signed.errors);

        Ok(Transaction::consensus_decode(&mut &signed.hex[..])?)
    }

    pub fn broadcast(&self, tx: &Transaction) -> anyhow::Result<Txid> {
        Ok(self.client.send_transaction(tx)?)
    }

    /// Ticks until the monitor holds the node's tip. The first tick is what places the indexer's cursor, and
    /// nothing is reported here: a subscription made afterwards sees the chain as it is.
    pub fn sync(&self, monitor: &Monitor) -> anyhow::Result<()> {
        for _ in 0..MAX_SYNC_TICKS {
            monitor.tick()?;

            if monitor.is_ready()? {
                return Ok(());
            }
        }
        anyhow::bail!("the monitor did not reach the node's tip")
    }
}

impl Drop for TestNode {
    fn drop(&mut self) {
        info!("Stopping bitcoind");
        let _ = self.bitcoind.stop();
    }
}

// =============================================================================
// Driving the monitor
// =============================================================================

/// Mines `blocks` blocks and ticks once for each, so the monitor ends on the node's tip having seen every one
/// of them. A tick reads at most one block, which is what makes the confirmation counts in a test predictable.
pub fn mine_and_tick(node: &TestNode, monitor: &Monitor, blocks: u64) -> anyhow::Result<()> {
    node.mine(blocks)?;

    for _ in 0..blocks {
        monitor.tick()?;
    }

    assert_eq!(
        monitor.get_indexed_height()?,
        node.tip()?,
        "the monitor did not read every block that was mined"
    );

    Ok(())
}

/// Everything the monitor has to say right now, acknowledged on the way out, so the next call answers only for
/// what happened after this one. Acknowledging every item is also what the monitor's contract expects.
pub fn drain_news(monitor: &Monitor) -> anyhow::Result<Vec<MonitorNews>> {
    let news = monitor.get_news(None)?;

    for item in &news {
        monitor.ack_news(item)?;
    }

    assert!(
        monitor.get_news(None)?.is_empty(),
        "acknowledging everything left something pending"
    );

    Ok(news)
}

/// Whether the indexer still has this txid in its mempool watch list. It reads the indexer's own key, because nothing
/// in the public API exposes the list, and a watch nobody wants costs a node call on every tick.
pub fn is_mempool_watched(storage: &Storage, txid: &Txid) -> anyhow::Result<bool> {
    let list: Vec<(Txid, Option<BlockHeight>)> = storage
        .get("indexer/mempool_watch_list", None)?
        .unwrap_or_default();

    Ok(list.iter().any(|(watched, _)| watched == txid))
}

/// The items of one context, in the order they came out. Order holds inside one transaction or one block height,
/// which is what a test about one subscription relies on.
pub fn of_context<'a>(news: &'a [MonitorNews], context: &str) -> Vec<&'a MonitorNews> {
    news.iter().filter(|item| item.context == context).collect()
}

// =============================================================================
// Reading an item of news
// =============================================================================

/// The status of an item of transaction news, which is what carries the transaction and its count.
pub fn status_of(item: &MonitorNews) -> &TransactionStatus {
    match &item.kind {
        NewsKind::Transaction { status, .. } => status,
        other => panic!("expected transaction news, got {other:?}"),
    }
}

pub fn confirmations_of(item: &MonitorNews) -> u32 {
    status_of(item).confirmations()
}

pub fn txid_of(item: &MonitorNews) -> Txid {
    match &item.kind {
        NewsKind::Transaction { txid, .. } => *txid,
        other => panic!("expected transaction news, got {other:?}"),
    }
}

pub fn block_of(item: &MonitorNews) -> &BlockRef {
    match &item.kind {
        NewsKind::Block(block) => block,
        other => panic!("expected block news, got {other:?}"),
    }
}

/// Asserts a whole item of transaction news: who asked, what it is about, and how deep it was when it was
/// decided. Everything a consumer reads is checked in one place, so a test says what it expects in one line.
pub fn assert_tx_news(
    item: &MonitorNews,
    target: &MonitorTarget,
    context: &str,
    txid: Txid,
    confirmations: u32,
    due_to_reorg: bool,
) {
    assert_eq!(&item.target, target, "target");
    assert_eq!(item.context, context, "context");

    match &item.kind {
        NewsKind::Transaction {
            txid: reported,
            status,
            due_to_reorg: reorg,
        } => {
            assert_eq!(*reported, txid, "txid");
            assert_eq!(status.confirmations(), confirmations, "confirmations");
            assert_eq!(*reorg, due_to_reorg, "due_to_reorg");
        }
        other => panic!("expected transaction news, got {other:?}"),
    }
}

// =============================================================================
// Building transactions the tests watch
// =============================================================================

/// An OP_RETURN output carrying `tag`, which is what an output pattern subscription matches on.
pub fn op_return(tag: &[u8]) -> TxOut {
    let mut pushed = PushBytesBuf::new();
    pushed
        .extend_from_slice(tag)
        .expect("tag too large for a script push");

    TxOut {
        value: Amount::ZERO,
        script_pubkey: ScriptBuf::new_op_return(pushed),
    }
}

/// Outputs for a transaction whose output at `index` is an OP_RETURN carrying `tag`, padded below it and with
/// the rest of the funding paid to the wallet so the transaction is above the minimum relay size.
pub fn outputs_with_op_return(
    node: &TestNode,
    index: usize,
    tag: &[u8],
    change: u64,
) -> anyhow::Result<Vec<TxOut>> {
    let mut outputs = Vec::new();

    for _ in 0..index {
        outputs.push(TxOut {
            value: Amount::from_sat(1_000),
            script_pubkey: node.fresh_address()?.script_pubkey(),
        });
    }

    outputs.push(op_return(tag));
    outputs.push(TxOut {
        value: Amount::from_sat(change),
        script_pubkey: node.fresh_address()?.script_pubkey(),
    });

    Ok(outputs)
}

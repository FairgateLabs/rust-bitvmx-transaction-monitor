use std::rc::Rc;

use bitcoin::{BlockHash, Txid};
use bitcoin_indexer::indexer::Indexer;
use bitcoin_indexer::types::{TickResult, TransactionStatus};
use bitcoin_indexer::IndexerType;
use bitvmx_bitcoin_rpc::bitcoin_client::BitcoinClient;
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use storage_backend::storage::Storage;
use tracing::{debug, info};

use crate::config::{MonitorSettings, MonitorSettingsConfig};
use crate::core::news::PendingNews;
use crate::core::store::MonitorStore;
use crate::core::subscriptions::Subscriptions;
use crate::errors::MonitorError;
use crate::types::{FullBlock, MonitorNews, MonitorTarget};

/// Watches the chain on behalf of its consumers and tells them what happened to what they asked about. It owns
/// the indexer that feeds it, and delegates every rule, so this file holds the API and the shape of a tick.
pub struct Monitor {
    indexer: Rc<IndexerType>,
    settings: MonitorSettings,
    subscriptions: Subscriptions,
    news: PendingNews,
}

impl Monitor {
    /// Builds a monitor and the indexer under it, both writing to the storage the consumer owns. Nothing is read from the node here.
    ///
    /// * `rpc_config` - Bitcoin RPC endpoint and network.
    /// * `storage` - Shared persistent backend, the same database the indexer writes to.
    /// * `settings` - Optional overrides, defaults when `None`. Validated here.
    pub fn new(
        rpc_config: &RpcConfig,
        storage: Rc<Storage>,
        settings: Option<MonitorSettingsConfig>,
    ) -> Result<Self, MonitorError> {
        let settings = MonitorSettings::from(settings.unwrap_or_default());
        settings.validate()?; // Validate the settings before starting the monitor.

        let bitcoin_client = BitcoinClient::new_from_config(rpc_config)?;
        let indexer = Rc::new(Indexer::new(
            bitcoin_client,
            storage.clone(),
            settings.indexer_settings.clone(),
        )?);
        let subscriptions = Subscriptions::new(
            MonitorStore::new(storage.clone()),
            indexer.clone(),
            settings.clone(),
        );
        let news = PendingNews::new(MonitorStore::new(storage));

        Ok(Self {
            indexer,
            settings,
            subscriptions,
            news,
        })
    }

    /// Number of confirmations a transaction is watched for. Once it reaches that the monitor stops tracking
    /// it, so it is also the deepest reorg the monitor can still report on.
    /// TODO: remove this method from the public API
    pub fn max_monitoring_confirmations(&self) -> u32 {
        self.settings.max_monitoring_confirmations
    }

    /// True once the indexer has read every block up to the node's tip.
    pub fn is_ready(&self) -> Result<bool, MonitorError> {
        Ok(self.indexer.is_ready()?)
    }

    /// Moves the monitor one step and reports what that step meant for every subscription. A tick can do:
    ///
    /// 1. `Reorged(n)`: the reorg pass restates what the removed blocks had been reported for.
    /// 2. `Advanced`: the block pass walks the new block once and reports whatever is due at this depth.
    /// 3. `Idle`: the chain said nothing.
    /// 4. On every tick, whatever the chain did: the first check answers the targets registered since the last one.
    /// 5. What the passes decided is appended to the pending news, for `get_news` to hand over.
    pub fn tick(&self) -> Result<(), MonitorError> {
        let mut news = match self.indexer.tick()? {
            TickResult::Reorged(removed) => {
                // The chain got shorter by `removed` blocks.
                info!("Chain reorganised, {removed} blocks removed");
                self.subscriptions.update_after_reorg()?
            }
            TickResult::Advanced => {
                let block = self.indexer.get_last_indexed_block()?;
                self.subscriptions.process_block(&block)?
            }
            TickResult::Idle => {
                debug!("No new block and no reorg");
                Vec::new()
            }
        };

        // Registrations are answered on every tick, whether or not the chain moved.
        news.extend(self.subscriptions.run_first_checks()?);
        self.news.add(news)?;

        Ok(())
    }

    /// Subscribes `context` to every target given, and queues the first check for the ones that can look into the
    /// past. Registering nothing but storage: no node call is made here. A target this context is already subscribed
    /// to keeps what it has found and only takes the new parameters, so nothing is reported twice.
    ///
    /// * `targets` - What to watch: a transaction, the spend of an output, an output pattern, or every new block.
    /// * `context` - The consumer's own label, carried back in every item of news about these targets.
    /// * `confirmation_trigger` - Report once, at the first block where the count reaches this, instead of on every
    ///   block up to the maximum. It must satisfy `finality <= trigger <= max_monitoring_confirmations`, and a
    ///   `NewBlock` target refuses it outright, because a block has no confirmations of its own.
    /// * `search_in_mempool` - Add the txid to the indexer's mempool watch list, so a reorg that takes the
    ///   transaction out of its block restates it as `InMempool` rather than `NotFound` when it is back in the mempool.
    pub fn monitor(
        &self,
        targets: &[MonitorTarget],
        context: String,
        confirmation_trigger: Option<u32>,
        search_in_mempool: bool,
    ) -> Result<(), MonitorError> {
        self.subscriptions
            .add(targets, context, confirmation_trigger, search_in_mempool)
    }

    /// Cancels `context` on every target given, dropping what it was tracking and the news it had waiting, including
    /// news left by a subscription that already ended on its own. This is the one path that deletes news the consumer
    /// never acknowledged. A target left with no context at all loses its record, and cancelling what was never
    /// subscribed does nothing.
    ///
    /// * `targets` - The targets to stop watching under this context.
    /// * `context` - The label those subscriptions were registered under.
    pub fn cancel(&self, targets: &[MonitorTarget], context: &str) -> Result<(), MonitorError> {
        self.subscriptions.remove(targets, context)?;
        self.news.drop_for(targets, context)
    }

    /// Everything the consumer has not acknowledged yet, or what is pending about the first `max_keys` news.
    ///
    /// The limit counts transactions, not items, because everything pending about one transaction is stored and
    /// returned together. So the most items it can hand back is
    /// `max_keys` x (subscriptions reporting on that transaction) x (blocks each has pending), the last factor
    /// being normally 1, if the consumer acknowledges them correctly.
    ///
    /// Items about one transaction come back in the order they were decided, transactions in the lexicographic
    /// order of their txid. A limit therefore serves the same first transactions until they are acknowledged.
    ///
    /// * `max_keys` - How many transactions and block heights to read, `None` for every one of them.
    pub fn get_news(&self, max_keys: Option<usize>) -> Result<Vec<MonitorNews>, MonitorError> {
        self.news.get_all(max_keys)
    }

    /// Acknowledges one item and deletes it. Deleting is by value, and it is the only thing that removes an item
    /// a consumer has seen, so act on the item first and acknowledge afterwards.
    ///
    /// * `news` - The item exactly as `get_news` handed it over.
    pub fn ack_news(&self, news: &MonitorNews) -> Result<(), MonitorError> {
        self.news.ack(news)
    }

    /// Height of the highest block the indexer has read. Every confirmation count the monitor reports is measured
    /// from it, never from the node's tip. Fails with `NotSynced` until the first tick.
    pub fn get_indexed_height(&self) -> Result<BlockHeight, MonitorError> {
        Ok(self.indexer.get_indexed_height()?)
    }

    /// The block at this height and hash: from the indexer's storage while it holds it, from the node when it is below
    /// everything the indexer holds and still the node's block at that height, and `None` otherwise, which includes
    /// any block above the indexed height and one a reorg replaced.
    ///
    /// * `height` - Height of the block.
    /// * `hash` - Which block at that height, so a reorged one is never handed back in its place.
    pub fn get_block(
        &self,
        height: BlockHeight,
        hash: &BlockHash,
    ) -> Result<Option<FullBlock>, MonitorError> {
        Ok(self.indexer.get_block(height, hash)?)
    }

    /// What the indexer knows about a transaction right now, which is not what the news says: news is a snapshot
    /// of when it was decided, this is the current answer. Storage first, the node only when the indexer holds
    /// nothing about it.
    ///
    /// * `tx_id` - Transaction to look up.
    /// * `search_in_mempool` - Let the answer be `InMempool` for one that is in the mempool and in no block.
    pub fn get_tx_status(
        &self,
        tx_id: &Txid,
        search_in_mempool: bool,
    ) -> Result<TransactionStatus, MonitorError> {
        Ok(self.indexer.get_transaction(tx_id, search_in_mempool)?)
    }

    /// Live check against the node, bypassing everything the indexer holds: true when the UTXO is spent.
    ///
    /// * `txid` - Transaction that created the output.
    /// * `vout` - Index of that output in it.
    /// * `include_mempool` - Count a spend that is still only in the mempool as having spent it.
    pub fn rpc_is_utxo_spent(
        &self,
        txid: &Txid,
        vout: u32,
        include_mempool: bool,
    ) -> Result<bool, MonitorError> {
        Ok(self
            .indexer
            .rpc_is_utxo_spent(txid, vout, include_mempool)?)
    }

    /// Live confirmation count from the node. None when it does not know the transaction, zero in its mempool.
    ///
    /// * `txid` - Transaction to look up.
    pub fn rpc_get_tx_confirmations(&self, txid: &Txid) -> Result<Option<u32>, MonitorError> {
        Ok(self.indexer.rpc_get_tx_confirmations(txid)?)
    }

    /// Fee rate estimated from the most recently indexed block, in sat/vB. One node call each time it is asked. Fails
    /// with `NotSynced` unless the indexer is at the node's tip, and with `FeeRateNotEstimated` when that block holds
    /// five transactions or fewer.
    pub fn get_estimated_fee_rate(&self) -> Result<u64, MonitorError> {
        Ok(self.indexer.get_estimated_fee_rate()?)
    }
}

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

    /// Moves the monitor one step. The indexer says what the chain did, and a tick never unwinds and indexes at
    /// once, so restating a reorg and reading a new block can never land together.
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

    /// Subscribes `context` to every target given, reporting once the trigger is reached or on every block when
    /// there is none. Registering a target already subscribed under the same context keeps what it has found.
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

    /// Cancels `context` on every target given, with the transactions it was tracking and the news it had
    /// waiting. Related news are deleted without being acknowledged.
    pub fn cancel(&self, targets: &[MonitorTarget], context: &str) -> Result<(), MonitorError> {
        self.subscriptions.remove(targets, context)?;
        for target in targets {
            self.news.drop_for(target, context)?;
        }

        Ok(())
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
    pub fn get_news(&self, max_keys: Option<usize>) -> Result<Vec<MonitorNews>, MonitorError> {
        self.news.get_all(max_keys)
    }

    /// Acknowledges one item, by the value `get_news` handed over, and removes it.
    pub fn ack_news(&self, news: &MonitorNews) -> Result<(), MonitorError> {
        self.news.ack(news)
    }

    /// Height of the highest block the indexer has read.
    pub fn get_indexed_height(&self) -> Result<BlockHeight, MonitorError> {
        Ok(self.indexer.get_indexed_height()?)
    }

    /// The block at this height and hash, from the indexer's storage or from the node.
    pub fn get_block(
        &self,
        height: BlockHeight,
        hash: &BlockHash,
    ) -> Result<Option<FullBlock>, MonitorError> {
        Ok(self.indexer.get_block(height, hash)?)
    }

    /// What the indexer knows about a transaction right now, which is not what the news says: news is a snapshot
    /// of when it was decided, this is the current answer.
    pub fn get_tx_status(
        &self,
        tx_id: &Txid,
        search_in_mempool: bool,
    ) -> Result<TransactionStatus, MonitorError> {
        Ok(self.indexer.get_transaction(tx_id, search_in_mempool)?)
    }

    /// Live check against the node, bypassing everything the indexer holds: true when the UTXO is unspent.
    pub fn rpc_is_utxo_unspent(
        &self,
        txid: &Txid,
        vout: u32,
        include_mempool: bool,
    ) -> Result<bool, MonitorError> {
        Ok(self
            .indexer
            .rpc_is_utxo_unspent(txid, vout, include_mempool)?)
    }

    /// Live confirmation count from the node. None when it does not know the transaction, zero in its mempool.
    pub fn rpc_get_tx_confirmations(&self, txid: &Txid) -> Result<Option<u32>, MonitorError> {
        Ok(self.indexer.rpc_get_tx_confirmations(txid)?)
    }

    /// Fee rate estimated from the most recently indexed block, in sat/vB.
    pub fn get_estimated_fee_rate(&self) -> Result<u64, MonitorError> {
        Ok(self.indexer.get_estimated_fee_rate()?)
    }
}

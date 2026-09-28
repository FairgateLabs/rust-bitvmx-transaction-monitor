use crate::config::{MonitorSettings, MonitorSettingsConfig};
use crate::errors::MonitorError;
use crate::helper::{is_spending_output, matches_output_pattern};
use crate::store::{MonitorStore, MonitoredTypes};
use crate::types::{
    AckMonitorNews, MonitorEntry, MonitorNews, OutputPatternFilter, SpendingUtxoMonitor,
    TransactionMonitor, TransactionNews, TypesToMonitor,
};
use bitcoin::{OutPoint, Transaction, Txid};
use bitcoin_indexer::indexer::Indexer;
use bitcoin_indexer::types::{FullBlock, TransactionStatus};
use bitcoin_indexer::IndexerType;
use bitvmx_bitcoin_rpc::bitcoin_client::BitcoinClient;
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use std::rc::Rc;
use storage_backend::storage::Storage;
use tracing::{debug, info, warn};

/// Internal context prefix used to identify spending UTXO transactions in the monitor store.
/// The full context format is: "INTERNAL_SPENDING_UTXO:{target_tx_id}:{target_utxo_index}:{parent_context}"
/// This allows the monitor to track when a specific UTXO is spent and generate appropriate news.
const INTERNAL_SPENDING_UTXO: &str = "INTERNAL_SPENDING_UTXO";

const INTERNAL_OUTPUT_PATTERN: &str = "INTERNAL_OUTPUT_PATTERN_";

pub struct Monitor {
    indexer: IndexerType,
    store: MonitorStore,
    settings: MonitorSettings,
}

impl Monitor {
    /// Creates a new Monitor instance with the given RPC configuration, storage, and settings.
    ///
    /// # Arguments
    /// * `rpc_config` - The RPC configuration to use for the indexer
    /// * `storage` - The storage the monitor and the indexer write to
    /// * `settings` - The settings to use for the monitor
    ///
    /// # Returns
    /// - `Ok(Monitor)`: The new Monitor instance
    /// - `Err(MonitorError)`: If there was an error creating the Monitor instance
    pub fn new(
        rpc_config: &RpcConfig,
        storage: Rc<Storage>,
        settings: Option<MonitorSettingsConfig>,
    ) -> Result<Self, MonitorError> {
        let settings = MonitorSettings::from(settings.unwrap_or_default());
        settings.validate()?;

        let bitcoin_client = BitcoinClient::new_from_config(rpc_config)?;
        let indexer = Indexer::new(
            bitcoin_client,
            storage.clone(),
            settings.indexer_settings.clone(),
        )?;

        let store = MonitorStore::new(storage);

        Ok(Self {
            indexer,
            store,
            settings,
        })
    }

    /// Number of confirmations a transaction is monitored for. Once a transaction reaches it the monitor stops
    /// watching it, so it is also the reorg depth the monitor can still report on.
    pub fn max_monitoring_confirmations(&self) -> u32 {
        self.settings.max_monitoring_confirmations
    }

    /// Checks if the monitor is ready and fully synced with the blockchain.
    ///
    /// # Returns
    /// - `Ok(true)`: If the monitor is fully synced with the blockchain
    /// - `Ok(false)`: If the monitor is still syncing blocks
    /// - `Err`: If there was an error checking the sync status
    pub fn is_ready(&self) -> Result<bool, MonitorError> {
        let is_ready = self.indexer.is_ready()?;
        Ok(is_ready)
    }

    /// Processes one tick of the monitor's operation.
    ///
    /// # Returns
    /// - `Ok(())`: If the tick completed successfully
    /// - `Err`: If there was an error during processing
    pub fn tick(&self) -> Result<(), MonitorError> {
        // Whether a new block was indexed by the indexer since the last tick.
        let indexed = self.indexer.tick()?;
        // Whether some subscription was registered since the last tick, whose first check has not run yet.
        let pending_first_checks = self.store.has_pending_first_checks()?;

        // Without a new block nor a pending first check, there is nothing to do.
        if !indexed && !pending_first_checks {
            debug!("No new block and no pending first checks, skipping monitor tick");
            return Ok(());
        }

        let block = self.indexer.get_last_indexed_block()?;

        // Read from storage once, and given to both phases below. Each phase reads first_check_done to take only
        // the subscriptions it handles, so going over the lists twice adds no storage read, and the second time
        // only happens when a block was indexed. That is clearer than mixing both phases in a single loop.
        let transactions = self.store.get_transaction_monitors()?;
        let spending_utxos = self.store.get_spending_utxo_monitors()?;

        // Phase 1: Subscriptions registered since the last tick, whether or not a block was indexed.
        self.first_check_transactions(&transactions, &block)?;
        self.first_check_spending_utxos(&spending_utxos, &block)?;
        self.store.set_pending_first_checks(false)?; // All pending first checks have been processed.

        // Phase 2: Work a new block brings.
        if indexed {
            self.check_transactions(&transactions, &block)?;
            self.check_spending_utxos(&spending_utxos, &block)?;
            self.check_output_patterns(&block)?;
            self.notify_new_block(&block)?;
        }

        Ok(())
    }

    /// Looks up the transactions monitored since the last tick, asking the node when needed.
    fn first_check_transactions(
        &self,
        monitors: &[TransactionMonitor],
        block: &FullBlock,
    ) -> Result<(), MonitorError> {
        for monitor in monitors {
            for entry in monitor.entries.iter().filter(|e| !e.first_check_done) {
                // The only lookup allowed to ask the node, which also finds a transaction mined before the indexer window.
                let status = self
                    .indexer
                    .get_transaction(&monitor.tx_id, entry.entry.search_in_mempool)?;
                self.store
                    .mark_transaction_first_check_done(monitor.tx_id, &entry.entry.context)?;
                self.process_transaction(monitor.tx_id, &entry.entry, status, block, true)?;
            }
        }

        Ok(())
    }

    /// Updates the transactions already looked up, reading the block just indexed.
    fn check_transactions(
        &self,
        monitors: &[TransactionMonitor],
        block: &FullBlock,
    ) -> Result<(), MonitorError> {
        for monitor in monitors {
            for entry in monitor.entries.iter().filter(|e| e.first_check_done) {
                // Check the status in the local indexer storage.
                let status = self.indexer.get_stored_transaction(&monitor.tx_id, false)?;
                self.process_transaction(monitor.tx_id, &entry.entry, status, block, false)?;
            }
        }

        Ok(())
    }

    /// Looks for spenders of the UTXOs monitored since the last tick, which may have been spent before.
    fn first_check_spending_utxos(
        &self,
        monitors: &[SpendingUtxoMonitor],
        block: &FullBlock,
    ) -> Result<(), MonitorError> {
        for monitor in monitors {
            for entry in monitor.entries.iter().filter(|e| !e.first_check_done) {
                self.first_check_spending_utxo(monitor.outpoint, &entry.entry, block)?;
                self.store
                    .mark_spending_utxo_first_check_done(monitor.outpoint, &entry.entry.context)?;
            }
        }

        Ok(())
    }

    /// Looks for spenders of the monitored UTXOs in the block just indexed.
    fn check_spending_utxos(
        &self,
        monitors: &[SpendingUtxoMonitor],
        block: &FullBlock,
    ) -> Result<(), MonitorError> {
        for monitor in monitors {
            for entry in &monitor.entries {
                for tx in block.txs.iter() {
                    if is_spending_output(tx, monitor.outpoint) {
                        let context = Self::build_spending_utxo_context(
                            monitor.outpoint,
                            &entry.entry.context,
                        );
                        self.monitor_detected_transaction(tx, block, block, context, &entry.entry)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Monitors the transactions of the block just indexed that match a monitored output pattern.
    fn check_output_patterns(&self, block: &FullBlock) -> Result<(), MonitorError> {
        // A pattern is matched only against blocks indexed after it was registered.
        for monitor in self.store.get_output_pattern_monitors()? {
            for entry in &monitor.entries {
                let context = Self::build_output_pattern_context(&monitor.filter);
                for tx in block.txs.iter() {
                    if matches_output_pattern(tx, &monitor.filter) {
                        self.monitor_detected_transaction(tx, block, block, context.clone(), entry)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Reports the block just indexed, when new blocks are monitored.
    fn notify_new_block(&self, block: &FullBlock) -> Result<(), MonitorError> {
        if self.store.is_monitoring_new_block()? {
            self.store.update_news(
                MonitoredTypes::NewBlock(block.height, block.hash),
                block.hash,
            )?;
        }

        Ok(())
    }

    /// Looks for a spend of this UTXO that happened before the subscription existed.
    fn first_check_spending_utxo(
        &self,
        outpoint: OutPoint,
        entry: &MonitorEntry,
        tip: &FullBlock,
    ) -> Result<(), MonitorError> {
        // Ask the node whether the UTXO is still unspent.
        if self
            .indexer
            .rpc_is_utxo_unspent(&outpoint.txid, outpoint.vout, true)?
        {
            return Ok(()); // Unspent, so there is no spender to look for.
        }

        // Already spent: look for the spender in the stored blocks, newest first, following each prev_hash. Only
        // stored blocks are read, so a spend from before the indexer started is not reported: The retention depth
        // bounds the walk, which normally ends earlier.
        let mut block = tip.clone();

        for _ in 0..self.retention_depth() {
            if let Some(tx) = block.txs.iter().find(|tx| is_spending_output(tx, outpoint)) {
                let context = Self::build_spending_utxo_context(outpoint, &entry.context);
                self.monitor_detected_transaction(tx, &block, tip, context, entry)?;

                return Ok(()); // A UTXO is spent once, so this is the spender.
            }

            if block.height == 0 {
                break; // Genesis, there is nothing older to read.
            }

            match self
                .indexer
                .get_stored_block(block.height - 1, &block.prev_hash)?
            {
                Some(previous) => block = previous,
                None => break, // Nothing older stored, and the node is not asked for it.
            }
        }

        Ok(())
    }

    /// Starts monitoring a transaction that a spending UTXO or an output pattern detected in a block, and
    /// processes it right away so its news goes out in this same tick.
    fn monitor_detected_transaction(
        &self,
        tx: &Transaction,
        found_in: &FullBlock,
        tip: &FullBlock,
        context: String,
        parent: &MonitorEntry,
    ) -> Result<(), MonitorError> {
        let tx_id = tx.compute_txid();

        // It was found in a block the indexer holds, so its subscription needs no first check.
        let entry = MonitorEntry::new(
            context,
            parent.confirmation_trigger,
            parent.search_in_mempool,
        );
        self.store
            .add_found_transaction_monitor(tx_id, entry.clone())?;

        // The block it is in is known, so its status is built here instead of looking it up again.
        let confirmations = tip.height - found_in.height + 1;
        let status =
            TransactionStatus::new(tx.clone(), found_in.height, found_in.hash, confirmations);

        self.process_transaction(tx_id, &entry, status, tip, true)
    }

    /// Number of blocks the indexer keeps, which bounds how far back a first check can look.
    fn retention_depth(&self) -> BlockHeight {
        self.settings
            .indexer_settings
            .clone()
            .unwrap_or_default()
            .retention_depth
    }

    /// Gets the height of the last block the indexer has read.
    ///
    /// # Returns
    /// - `Ok(BlockHeight)`: The height of the last indexed block
    /// - `Err`: If the indexer has not read any block yet, or the height could not be retrieved
    pub fn get_indexed_height(&self) -> Result<BlockHeight, MonitorError> {
        Ok(self.indexer.get_indexed_height()?)
    }

    /// Gets the block at this height and hash, when it belongs to the chain the indexer has processed.
    ///
    /// # Returns
    /// - `Ok(Some(FullBlock))`: The block, read from the indexer or downloaded from the node
    /// - `Ok(None)`: The indexer has not processed that block
    /// - `Err`: If there was an error retrieving the block
    pub fn get_block(
        &self,
        height: BlockHeight,
        hash: &bitcoin::BlockHash,
    ) -> Result<Option<FullBlock>, MonitorError> {
        Ok(self.indexer.get_block(height, hash)?)
    }

    /// Starts monitoring transactions based on the provided monitor type.
    ///
    /// # Arguments
    /// * `data` - The type of monitoring to perform, which can be:
    ///   - Transactions: Monitor multiple transactions
    ///   - OutputPattern: Monitor transactions matching a specific output pattern
    ///   - SpendingUTXOTransaction: Monitor transactions spending a specific UTXO
    ///   - NewBlock: Monitor new blocks
    ///
    /// # Returns
    /// - `Ok(())`: If monitoring was set up successfully
    /// - `Err`: If there was an error setting up monitoring
    pub fn monitor(
        &self,
        data: TypesToMonitor,
        search_in_mempool: bool,
    ) -> Result<(), MonitorError> {
        // Check if the TypesToMonitor instance has a confirmation trigger (if it's a transaction), and if so,
        // ensure it does not exceed the configured max_monitoring_confirmations.
        // Max monitoring confirmations is the number of confirmations that the monitor will wait for before deactivating the monitor.
        // If it does, return an error.
        match &data {
            TypesToMonitor::Transactions(_, _, confirmation_trigger)
            | TypesToMonitor::OutputPattern(_, confirmation_trigger)
            | TypesToMonitor::SpendingUTXOTransaction(_, _, confirmation_trigger) => {
                if let Some(confirmation_trigger) = confirmation_trigger {
                    if *confirmation_trigger >= self.settings.max_monitoring_confirmations {
                        return Err(MonitorError::InvalidConfirmationTrigger(
                            *confirmation_trigger,
                            self.settings.max_monitoring_confirmations,
                        ));
                    }
                }
            }
            _ => {}
        }

        // When search_in_mempool=true, register txids in the indexer's watch
        // list so that tick() polls their mempool status each cycle.
        if search_in_mempool {
            if let TypesToMonitor::Transactions(txids, _, _) = &data {
                for txid in txids {
                    self.indexer.add_mempool_watch(*txid)?;
                }
            }
        }

        self.store.add_monitor(data, search_in_mempool)?;

        Ok(())
    }

    /// Cancels monitoring for a specific type of monitoring.
    ///
    /// # Arguments
    /// * `data` - The type of monitoring to cancel, which can be:
    ///   - Transactions: Monitor multiple transactions
    ///   - OutputPattern: Monitor transactions matching a specific output pattern
    ///   - SpendingUTXOTransaction: Monitor transactions spending a specific UTXO
    ///   - NewBlock: Monitor new blocks
    ///
    /// # Returns
    /// - `Ok(())`: If monitoring was canceled successfully
    /// - `Err`: If there was an error canceling monitoring
    pub fn cancel(&self, data: TypesToMonitor) -> Result<(), MonitorError> {
        // Remove txids from the indexer watch list on cancellation.
        if let TypesToMonitor::Transactions(txids, _, _) = &data {
            for txid in txids {
                let _ = self.indexer.remove_mempool_watch(txid);
            }
        }
        self.store.remove_monitor(data)?;

        Ok(())
    }

    /// Gets status updates for monitored transactions.
    ///
    /// Returns updates for transactions that have had status changes, such as:
    /// - New confirmations
    /// - Becoming orphaned
    /// - Being included in a block
    ///
    /// # Returns
    /// - `Ok(Vec<MonitorNews>)`: List of status updates grouped by monitor type
    /// - `Err`: If there was an error retrieving updates
    pub fn get_news(&self) -> Result<Vec<MonitorNews>, MonitorError> {
        let list_news = self.store.get_news()?;

        let mut return_news = Vec::new();

        for news in list_news {
            match news {
                MonitoredTypes::Transaction(tx_id, context, resent_due_to_reorg) => {
                    let status = self.get_tx_status(&tx_id, true)?;
                    return_news.push(MonitorNews::Transaction(TransactionNews {
                        tx_id,
                        status,
                        context: context,
                        resent_due_to_reorg,
                    }));
                }
                MonitoredTypes::OutputPatternTransaction(tx_id, tag) => {
                    let status = self.get_tx_status(&tx_id, true)?;
                    return_news.push(MonitorNews::OutputPatternTransaction(tx_id, status, tag));
                }
                MonitoredTypes::SpendingUTXOTransaction(outpoint, context, spender_tx_id) => {
                    let status = self.get_tx_status(&spender_tx_id, true)?;
                    return_news.push(MonitorNews::SpendingUTXOTransaction(
                        outpoint, status, context,
                    ));
                }
                MonitoredTypes::NewBlock(height, hash) => {
                    return_news.push(MonitorNews::NewBlock(height, hash));
                }
            }
        }

        Ok(return_news)
    }

    /// Acknowledges that a transaction status update has been processed.
    ///
    /// After processing a status update from get_news(), this method should be called
    /// to remove it from the pending updates queue.
    ///
    /// # Arguments
    /// * `data` - The type of monitoring to perform, which can be:
    ///   - Transactions: Monitor multiple transactions
    ///   - OutputPattern: Monitor transactions matching a specific output pattern
    ///   - SpendingUTXOTransaction: Monitor transactions spending a specific UTXO
    ///   - NewBlock: Monitor new blocks
    ///
    /// # Returns
    /// - `Ok(())`: If the update was successfully acknowledged
    /// - `Err`: If there was an error processing the acknowledgment
    pub fn ack_news(&self, data: AckMonitorNews) -> Result<(), MonitorError> {
        self.store.ack_news(data)?;
        Ok(())
    }

    /// Gets the current status of a specific transaction.
    ///
    /// # Arguments
    /// * `tx_id` - Hash of the transaction to check
    /// * `search_in_mempool` - If true, also consult the node mempool when the
    ///   transaction is not found or is orphan. If false, only indexed chain
    ///   data is used.
    ///
    /// # Returns
    /// - `Ok(TransactionStatus)`: Current information of the transaction
    /// - `Err`: If there was an error retrieving the status
    pub fn get_tx_status(
        &self,
        tx_id: &Txid,
        search_in_mempool: bool,
    ) -> Result<TransactionStatus, MonitorError> {
        Ok(self.indexer.get_transaction(tx_id, search_in_mempool)?)
    }

    /// Real-time RPC check for UTXO spendability via `gettxout`. Bypasses indexer cache.
    /// Returns true iff the `(txid, vout)` UTXO is currently unspent: in chain OR mempool
    ///  when `include_mempool` is true, chain-only when false.
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

    /// Live `getrawtransaction` confirmation probe (requires `-txindex`). Returns `None` if the node
    /// does not know the tx, `Some(0)` if in the mempool, `Some(n>=1)` if confirmed with `n` confs.
    pub fn rpc_get_tx_confirmations(&self, txid: &Txid) -> Result<Option<u32>, MonitorError> {
        Ok(self.indexer.rpc_get_tx_confirmations(txid)?)
    }

    /// Gets the estimated fee rate from the indexer.
    ///
    /// # Returns
    /// - `Ok(u64)`: The estimated fee rate in satoshis per byte
    /// - `Err`: If there was an error retrieving the fee rate
    pub fn get_estimated_fee_rate(&self) -> Result<u64, MonitorError> {
        Ok(self.indexer.get_estimated_fee_rate()?)
    }

    /// Builds the context string for spending UTXO transactions
    fn build_spending_utxo_context(outpoint: OutPoint, context: &str) -> String {
        format!(
            "{}:{}:{}:{}",
            INTERNAL_SPENDING_UTXO, outpoint.txid, outpoint.vout, context
        )
    }

    /// Builds the context string for the transactions an output pattern matches
    fn build_output_pattern_context(filter: &OutputPatternFilter) -> String {
        format!("{}{}", INTERNAL_OUTPUT_PATTERN, hex::encode(&filter.tag))
    }

    /// Parses the spending UTXO context and extracts the watched outpoint and the original extra data
    /// Returns None if the context is not valid or cannot be parsed
    fn parse_spending_utxo_context(context: &str) -> Option<(OutPoint, String)> {
        if !context.starts_with(INTERNAL_SPENDING_UTXO) {
            return None;
        }

        // Parse the context: INTERNAL_SPENDING_UTXO:{target_tx_id_hex}:{target_utxo_index}:{parent_context}
        let parts: Vec<&str> = context.split(':').collect();
        if parts.len() >= 4 {
            if let (Ok(target_tx_id), Ok(target_utxo_index)) =
                (parts[1].parse::<Txid>(), parts[2].parse::<u32>())
            {
                let parent_context = parts[3..].join(":");
                return Some((
                    OutPoint::new(target_tx_id, target_utxo_index),
                    parent_context,
                ));
            }
        }

        None
    }

    /// Decides whether to emit confirmation news, and whether that emission is a reorg-caused resend.
    ///
    /// Returns `(should_send, resent_due_to_reorg)`.
    fn should_send_news(
        &self,
        tx_id: Txid,
        context: &str,
        number_confirmation_trigger: Option<u32>,
        current_confirmations: u32,
        tx_block_hash: bitcoin::BlockHash,
        is_first_check: bool,
    ) -> Result<(bool, bool), MonitorError> {
        if let Some(trigger) = number_confirmation_trigger {
            // With a trigger, nothing is sent until the transaction reaches that many confirmations.
            if current_confirmations < trigger {
                return Ok((false, false));
            }
            let notified_block_hash = self
                .store
                .get_transaction_notified_block_hash(tx_id, context)?;
            match notified_block_hash {
                // First time the trigger is reached: notify, not a reorg.
                None => Ok((true, false)),
                // Already notified while included in the same block: confirmations only grew, do not resend.
                Some(prev) if prev == tx_block_hash => Ok((false, false)),
                // Already notified, now the tx is in a different block: a reorg re-included it, flag the resend.
                Some(_) => Ok((true, true)),
            }
        } else {
            // A first check reaches this point only when the transaction was found on chain, so it is reported
            // whatever its age. After that, news is sent on every block until the maximum confirmations.
            Ok((
                is_first_check
                    || current_confirmations < self.settings.max_monitoring_confirmations,
                false,
            ))
        }
    }

    /// Reports what `tx_info` says about a monitored transaction.
    fn process_transaction(
        &self,
        tx_id: Txid,
        entry: &MonitorEntry,
        tx_info: TransactionStatus,
        block: &FullBlock,
        is_first_check: bool,
    ) -> Result<(), MonitorError> {
        let context = &entry.context;
        let confirmation_trigger = entry.confirmation_trigger;

        let (tx_block_hash, confirmations) = match tx_info {
            TransactionStatus::Confirmed {
                block_hash,    // The block that includes the tx
                confirmations, // How many confirmations the tx has in that block
                ..
            } => (block_hash, confirmations),
            _ => return Ok(()), // Not in a block of the indexed chain, so there is nothing to report yet.
        };

        let (should_send_news, resent_due_to_reorg) = self.should_send_news(
            tx_id,
            context,
            confirmation_trigger,
            confirmations,
            tx_block_hash,
            is_first_check,
        )?;

        if should_send_news {
            // Dispatch news update based on context pattern to determine the monitor type
            match context.as_str() {
                ed if ed.starts_with(INTERNAL_OUTPUT_PATTERN) => {
                    let tag_hex = &ed[INTERNAL_OUTPUT_PATTERN.len()..];
                    let tag = hex::decode(tag_hex)
                        .map_err(|e| MonitorError::UnexpectedError(e.to_string()))?;
                    self.store.update_news(
                        MonitoredTypes::OutputPatternTransaction(tx_id, tag),
                        block.hash,
                    )?;
                }
                ed if ed.starts_with(INTERNAL_SPENDING_UTXO) => {
                    if let Some((outpoint, parent_context)) = Self::parse_spending_utxo_context(ed)
                    {
                        self.store.update_news(
                            MonitoredTypes::SpendingUTXOTransaction(
                                outpoint,
                                parent_context,
                                tx_id,
                            ),
                            block.hash,
                        )?;
                    }
                }
                _ => {
                    self.store.update_news(
                        MonitoredTypes::Transaction(tx_id, context.clone(), resent_due_to_reorg),
                        block.hash,
                    )?;
                }
            }

            if resent_due_to_reorg {
                warn!(
                    "Reorg resend: Transaction({}) re-notified after a reorg re-included it in a different block | Height({}) | Confirmations({})",
                    tx_id, block.height, confirmations,
                );
            } else {
                info!(
                    "News for Transaction({}) | Height({}) | Confirmations({})",
                    tx_id, block.height, confirmations,
                );
            }

            // Remember the block that included the tx at this notification, so a later reorg into a
            // different block is recognized as a resend.
            if confirmation_trigger.is_some() {
                self.store.update_transaction_notified_block_hash(
                    tx_id,
                    context,
                    Some(tx_block_hash),
                )?;
            }
        }

        // Check if we should deactivate monitor based on max_monitoring_confirmations
        // Once a transaction reaches the maximum monitoring confirmations, we stop tracking it
        // to avoid unnecessary processing and storage overhead
        if confirmations >= self.settings.max_monitoring_confirmations {
            self.store.remove_monitor(TypesToMonitor::Transactions(
                vec![tx_id],
                context.clone(),
                confirmation_trigger,
            ))?;
            // Also remove from the indexer mempool watch list since we are done
            // monitoring this transaction.
            let _ = self.indexer.remove_mempool_watch(&tx_id);

            info!(
                "Stop monitoring Transaction({}) | Height({}) | Confirmations({})",
                tx_id, block.height, self.settings.max_monitoring_confirmations,
            );

            // If this is a spending UTXO transaction, also remove the SpendingUTXOTransaction monitor
            // This ensures both the transaction monitor and the UTXO spending monitor are properly cleaned up
            if let Some((outpoint, parent_context)) = Self::parse_spending_utxo_context(context) {
                self.store
                    .remove_monitor(TypesToMonitor::SpendingUTXOTransaction(
                        outpoint,
                        parent_context,
                        confirmation_trigger,
                    ))?;

                info!(
                    "Stop monitoring SpendingUTXOTransaction({}) | Height({}) | Confirmations({})",
                    outpoint, block.height, self.settings.max_monitoring_confirmations,
                );
            }
        }

        Ok(())
    }
}

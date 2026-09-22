use crate::config::{MonitorSettings, MonitorSettingsConfig};
use crate::errors::MonitorError;
use crate::helper::{is_spending_output, matches_output_pattern};
use crate::store::{MonitorStore, MonitoredTypes, TypesToMonitorStore};
use crate::types::{
    AckMonitorNews, MonitorNews, OutputPatternFilter, TransactionNews, TypesToMonitor,
};
use bitcoin::Txid;
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
/// The full context format is: "INTERNAL_SPENDING_UTXO:{target_tx_id}:{target_utxo_index}:{original_extra_data}"
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

        let store = MonitorStore::new(storage)?;

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
    /// This method:
    /// - Checks for new blocks and updates the monitor's state
    /// - Updates confirmation counts for tracked transactions
    /// - Detects new transactions that need to be monitored
    /// - Triggers the indexer to continue syncing if needed
    ///
    /// # Returns
    /// - `Ok(())`: If the tick completed successfully
    /// - `Err`: If there was an error during processing
    pub fn tick(&self) -> Result<(), MonitorError> {
        let indexed = self.indexer.tick()?;

        if !indexed && !self.store.has_pending_work()? {
            debug!("No new block and no pending work, skipping tick");
            return Ok(());
        }

        let last_indexed_block = self.indexer.get_last_indexed_block()?;
        let indexed_height = last_indexed_block.height;
        let current_block_hash = last_indexed_block.hash;

        let txs_monitors = self.store.get_monitors()?;

        for tx_type in txs_monitors {
            match tx_type {
                TypesToMonitorStore::Transaction(
                    tx_id,
                    extra_data,
                    number_confirmation_trigger,
                    search_in_mempool,
                ) => {
                    self.process_transaction(
                        tx_id,
                        extra_data,
                        number_confirmation_trigger,
                        indexed_height,
                        current_block_hash,
                        false,
                        search_in_mempool,
                    )?;
                }
                TypesToMonitorStore::OutputPattern(
                    filter,
                    number_confirmation_trigger,
                    search_in_mempool,
                ) => {
                    self.process_output_pattern_transaction(
                        filter,
                        number_confirmation_trigger,
                        &last_indexed_block,
                        indexed_height,
                        current_block_hash,
                        search_in_mempool,
                    )?;
                }
                TypesToMonitorStore::SpendingUTXOTransaction(
                    target_tx_id,
                    target_utxo_index,
                    extra_data,
                    number_confirmation_trigger,
                    search_in_mempool,
                ) => {
                    self.process_spending_utxo_transaction(
                        target_tx_id,
                        target_utxo_index,
                        extra_data,
                        number_confirmation_trigger,
                        &last_indexed_block,
                        indexed_height,
                        current_block_hash,
                        search_in_mempool,
                    )?;
                }
                TypesToMonitorStore::NewBlock => {
                    self.store.update_news(
                        MonitoredTypes::NewBlock(indexed_height, current_block_hash),
                        current_block_hash,
                    )?;
                }
            }
        }

        self.store.set_pending_work(false)?;

        Ok(())
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
        if data != TypesToMonitor::NewBlock {
            self.store.set_pending_work(true)?;
        }

        // Check if the TypesToMonitor instance has a confirmation trigger (if it's a transaction), and if so,
        // ensure it does not exceed the configured max_monitoring_confirmations.
        // Max monitoring confirmations is the number of confirmations that the monitor will wait for before deactivating the monitor.
        // If it does, return an error.
        match &data {
            TypesToMonitor::Transactions(_, _, confirmation_trigger)
            | TypesToMonitor::OutputPattern(_, confirmation_trigger)
            | TypesToMonitor::SpendingUTXOTransaction(_, _, _, confirmation_trigger) => {
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
        self.store.cancel_monitor(data)?;

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
                MonitoredTypes::Transaction(tx_id, extra_data, resent_due_to_reorg) => {
                    let status = self.get_tx_status(&tx_id, true)?;
                    return_news.push(MonitorNews::Transaction(TransactionNews {
                        tx_id,
                        status,
                        context: extra_data,
                        resent_due_to_reorg,
                    }));
                }
                MonitoredTypes::OutputPatternTransaction(tx_id, tag) => {
                    let status = self.get_tx_status(&tx_id, true)?;
                    return_news.push(MonitorNews::OutputPatternTransaction(tx_id, status, tag));
                }
                MonitoredTypes::SpendingUTXOTransaction(
                    tx_id,
                    utxo_index,
                    extra_data,
                    spender_tx_id,
                ) => {
                    let status = self.get_tx_status(&spender_tx_id, true)?;
                    return_news.push(MonitorNews::SpendingUTXOTransaction(
                        tx_id, utxo_index, status, extra_data,
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
    fn build_spending_utxo_context(
        target_tx_id: Txid,
        target_utxo_index: u32,
        extra_data: &str,
    ) -> String {
        format!(
            "{}:{}:{}:{}",
            INTERNAL_SPENDING_UTXO, target_tx_id, target_utxo_index, extra_data
        )
    }

    /// Parses the spending UTXO context and extracts target_tx_id, target_utxo_index, and original_extra_data
    /// Returns None if the context is not valid or cannot be parsed
    fn parse_spending_utxo_context(extra_data: &str) -> Option<(Txid, u32, String)> {
        if !extra_data.starts_with(INTERNAL_SPENDING_UTXO) {
            return None;
        }

        // Parse the context: INTERNAL_SPENDING_UTXO:{target_tx_id_hex}:{target_utxo_index}:{original_extra_data}
        let parts: Vec<&str> = extra_data.split(':').collect();
        if parts.len() >= 4 {
            if let (Ok(target_tx_id), Ok(target_utxo_index)) =
                (parts[1].parse::<Txid>(), parts[2].parse::<u32>())
            {
                let original_extra_data = parts[3..].join(":");
                return Some((target_tx_id, target_utxo_index, original_extra_data));
            }
        }

        None
    }

    /// Determines if news should be sent based on the confirmation trigger.
    ///
    /// # Arguments
    /// * `tx_id` - The transaction ID being checked
    /// * `extra_data` - The context/extra data associated with the transaction
    /// * `number_confirmation_trigger` - Optional confirmation threshold. If Some(n), news is sent once when confirmations >= n
    /// * `current_confirmations` - Current number of confirmations for the transaction
    ///
    /// # Returns
    /// - `Ok(true)`: If news should be sent (trigger reached and not yet sent, or no trigger and within max confirmations)
    /// - `Ok(false)`: If news should not be sent
    /// - `Err`: If there was an error checking the trigger status
    ///
    /// # Behavior
    /// - With trigger: News is sent once when confirmations reach or exceed the trigger value
    /// - Without trigger: News is sent for every block until max_monitoring_confirmations is reached
    ///
    /// Decide whether to emit confirmation news, and whether that emission is a reorg-caused resend.
    ///
    /// Returns `(should_send, resent_due_to_reorg)`.
    fn should_send_news(
        &self,
        tx_id: Txid,
        extra_data: &str,
        number_confirmation_trigger: Option<u32>,
        current_confirmations: u32,
        tx_block_hash: bitcoin::BlockHash,
    ) -> Result<(bool, bool), MonitorError> {
        if let Some(trigger) = number_confirmation_trigger {
            if current_confirmations < trigger {
                return Ok((false, false));
            }
            let notified_block_hash = self
                .store
                .get_transaction_notified_block_hash(tx_id, extra_data)?;
            match notified_block_hash {
                // First time the trigger is reached: notify, not a reorg.
                None => Ok((true, false)),
                // Already notified while included in the same block: confirmations only grew, do not resend.
                Some(prev) if prev == tx_block_hash => Ok((false, false)),
                // Already notified, now the tx is in a different block: a reorg re-included it, flag the resend.
                Some(_) => Ok((true, true)),
            }
        } else {
            // If None, always send news when current confirmations are less than the max monitoring confirmations
            Ok((
                current_confirmations < self.settings.max_monitoring_confirmations,
                false,
            ))
        }
    }

    fn detect_output_pattern_txs(
        &self,
        full_block: FullBlock,
        filter: &OutputPatternFilter,
    ) -> Result<Vec<Txid>, MonitorError> {
        let mut txs_ids = Vec::new();

        for tx in full_block.txs.iter() {
            if matches_output_pattern(tx, filter) {
                txs_ids.push(tx.compute_txid());
            }
        }

        Ok(txs_ids)
    }

    fn process_output_pattern_transaction(
        &self,
        filter: OutputPatternFilter,
        number_confirmation_trigger: Option<u32>,
        last_indexed_block: &FullBlock,
        indexed_height: u32,
        current_block_hash: bitcoin::BlockHash,
        search_in_mempool: bool,
    ) -> Result<(), MonitorError> {
        let new_txs_ids = self.detect_output_pattern_txs(last_indexed_block.clone(), &filter)?;

        let tag_hex = hex::encode(&filter.tag);
        let context = format!("{}{}", INTERNAL_OUTPUT_PATTERN, tag_hex);

        // Add new transactions to monitoring using add_monitor with INTERNAL_OUTPUT_PATTERN context
        for tx_id in &new_txs_ids {
            self.store.add_monitor(
                TypesToMonitor::Transactions(
                    vec![*tx_id],
                    context.clone(),
                    number_confirmation_trigger,
                ),
                search_in_mempool,
            )?;

            self.process_transaction(
                *tx_id,
                context.clone(),
                number_confirmation_trigger,
                indexed_height,
                current_block_hash,
                true,
                search_in_mempool,
            )?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn process_transaction(
        &self,
        tx_id: Txid,
        extra_data: String,
        number_confirmation_trigger: Option<u32>,
        indexed_height: BlockHeight,
        current_block_hash: bitcoin::BlockHash,
        should_exist: bool,
        search_in_mempool: bool,
    ) -> Result<(), MonitorError> {
        let tx_info = self.indexer.get_transaction(&tx_id, search_in_mempool)?;

        // The block that includes the tx, and how many confirmations it has there. A transaction that is not in a
        // block of the indexed chain has nothing to report yet.
        let (tx_block_hash, confirmations) = match tx_info {
            TransactionStatus::Confirmed {
                block_hash,
                confirmations,
                ..
            } => (block_hash, confirmations),
            _ => {
                if should_exist {
                    return Err(MonitorError::UnexpectedError(format!(
                        "Transaction({}) not found or in mempool",
                        tx_id
                    )));
                }

                // If the transaction does not exist, nothing to do.
                return Ok(());
            }
        };

        // Check if we should send news based on number_confirmation_trigger, and whether this send is a
        // repeat caused by a reorg re-mining the tx into a different block.
        let (should_send_news, resent_due_to_reorg) = self.should_send_news(
            tx_id,
            &extra_data,
            number_confirmation_trigger,
            confirmations,
            tx_block_hash,
        )?;

        if should_send_news {
            // Dispatch news update based on extra_data pattern to determine the monitor type
            match extra_data.as_str() {
                ed if ed.starts_with(INTERNAL_OUTPUT_PATTERN) => {
                    let tag_hex = &ed[INTERNAL_OUTPUT_PATTERN.len()..];
                    let tag = hex::decode(tag_hex)
                        .map_err(|e| MonitorError::UnexpectedError(e.to_string()))?;
                    self.store.update_news(
                        MonitoredTypes::OutputPatternTransaction(tx_id, tag),
                        current_block_hash,
                    )?;
                }
                ed if ed.starts_with(INTERNAL_SPENDING_UTXO) => {
                    if let Some((target_tx_id, target_utxo_index, original_extra_data)) =
                        Self::parse_spending_utxo_context(ed)
                    {
                        self.store.update_news(
                            MonitoredTypes::SpendingUTXOTransaction(
                                target_tx_id,
                                target_utxo_index,
                                original_extra_data,
                                tx_id,
                            ),
                            current_block_hash,
                        )?;
                    }
                }
                _ => {
                    self.store.update_news(
                        MonitoredTypes::Transaction(tx_id, extra_data.clone(), resent_due_to_reorg),
                        current_block_hash,
                    )?;
                }
            }

            if resent_due_to_reorg {
                warn!(
                    "Reorg resend: Transaction({}) re-notified after a reorg re-included it in a different block | Height({}) | Confirmations({})",
                    tx_id, indexed_height, confirmations,
                );
            } else {
                info!(
                    "News for Transaction({}) | Height({}) | Confirmations({})",
                    tx_id, indexed_height, confirmations,
                );
            }

            // Remember the block that included the tx at this notification, so a later reorg into a
            // different block is recognized as a resend.
            if number_confirmation_trigger.is_some() {
                self.store.update_transaction_notified_block_hash(
                    tx_id,
                    &extra_data,
                    Some(tx_block_hash),
                )?;
            }
        }

        // Check if we should deactivate monitor based on max_monitoring_confirmations
        // Once a transaction reaches the maximum monitoring confirmations, we stop tracking it
        // to avoid unnecessary processing and storage overhead
        if confirmations >= self.settings.max_monitoring_confirmations {
            self.store.deactivate_monitor(TypesToMonitor::Transactions(
                vec![tx_id],
                extra_data.clone(),
                number_confirmation_trigger,
            ))?;
            // Also remove from the indexer mempool watch list since we are done
            // monitoring this transaction.
            let _ = self.indexer.remove_mempool_watch(&tx_id);

            info!(
                "Stop monitoring Transaction({}) | Height({}) | Confirmations({})",
                tx_id, indexed_height, self.settings.max_monitoring_confirmations,
            );

            // If this is a spending UTXO transaction, also deactivate the SpendingUTXOTransaction monitor
            // This ensures both the transaction monitor and the UTXO spending monitor are properly cleaned up
            if let Some((target_tx_id, target_utxo_index, original_extra_data)) =
                Self::parse_spending_utxo_context(&extra_data)
            {
                self.store
                    .deactivate_monitor(TypesToMonitor::SpendingUTXOTransaction(
                        target_tx_id,
                        target_utxo_index,
                        original_extra_data,
                        number_confirmation_trigger,
                    ))?;

                info!(
                        "Stop monitoring SpendingUTXOTransaction({}:{}) | Height({}) | Confirmations({})",
                        target_tx_id,
                        target_utxo_index,
                        indexed_height,
                        self.settings.max_monitoring_confirmations,
                    );
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn process_spending_utxo_transaction(
        &self,
        target_tx_id: Txid,
        target_utxo_index: u32,
        extra_data: String,
        number_confirmation_trigger: Option<u32>,
        last_indexed_block: &FullBlock,
        indexed_height: BlockHeight,
        current_block_hash: bitcoin::BlockHash,
        search_in_mempool: bool,
    ) -> Result<(), MonitorError> {
        // Check each transaction in the new block for a spending transaction of the target UTXO
        for tx in last_indexed_block.txs.iter() {
            let is_spending_output = is_spending_output(tx, target_tx_id, target_utxo_index);

            if is_spending_output {
                let spending_tx_id = tx.compute_txid();

                // Create a monitor for the spending transaction with the special context
                let spending_context =
                    Self::build_spending_utxo_context(target_tx_id, target_utxo_index, &extra_data);

                self.store.add_monitor(
                    TypesToMonitor::Transactions(
                        vec![spending_tx_id],
                        spending_context.clone(),
                        number_confirmation_trigger,
                    ),
                    search_in_mempool,
                )?;

                // Process the spending transaction monitor
                self.process_transaction(
                    spending_tx_id,
                    spending_context,
                    number_confirmation_trigger,
                    indexed_height,
                    current_block_hash,
                    true,
                    search_in_mempool,
                )?;
            }
        }

        Ok(())
    }
}

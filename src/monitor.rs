use crate::config::{MonitorSettings, MonitorSettingsConfig};
use crate::errors::MonitorError;
use crate::helper::{is_a_pegin_tx, is_spending_output};
use crate::store::{MonitorStore, MonitorStoreApi, MonitoredTypes, TypesToMonitorStore};
use crate::types::{AckMonitorNews, MonitorNews, TypesToMonitor};
use bitcoin::Txid;
use bitcoin_indexer::config::IndexerSettings;
use bitcoin_indexer::indexer::Indexer;
use bitcoin_indexer::indexer::IndexerApi;
use bitcoin_indexer::store::IndexerStore;
use bitcoin_indexer::types::{FullBlock, TransactionStatus};
use bitcoin_indexer::IndexerType;
use bitvmx_bitcoin_rpc::bitcoin_client::BitcoinClient;
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use std::rc::Rc;
use storage_backend::storage::Storage;
use tracing::{debug, error, info};

/// Internal context prefix used to identify RSK pegin transactions in the monitor store.
/// This allows the monitor to distinguish between regular transactions and RSK pegin transactions
/// when processing news updates.
const INTERNAL_RSK_PEGIN: &str = "INTERNAL_RSK_PEGIN";

/// Internal context prefix used to identify spending UTXO transactions in the monitor store.
/// The full context format is: "INTERNAL_SPENDING_UTXO:{target_tx_id}:{target_utxo_index}:{original_extra_data}"
/// This allows the monitor to track when a specific UTXO is spent and generate appropriate news.
const INTERNAL_SPENDING_UTXO: &str = "INTERNAL_SPENDING_UTXO";

pub struct Monitor {
    pub indexer: IndexerType,
    pub store: MonitorStore,
    pub settings: MonitorSettings,
}

impl Monitor {
    /// Creates a new Monitor instance with the given RPC configuration, storage, and settings.
    ///
    /// # Arguments
    /// * `rpc_config` - The RPC configuration to use for the indexer
    /// * `storage` - The storage to use for the monitor
    /// * `settings` - The settings to use for the monitor
    ///
    /// # Returns
    /// - `Ok(Monitor)`: The new Monitor instance
    /// - `Err(MonitorError)`: If there was an error creating the Monitor instance
    pub fn new_with_paths(
        rpc_config: &RpcConfig,
        storage: Rc<Storage>,
        settings: Option<MonitorSettingsConfig>,
    ) -> Result<Self, MonitorError> {
        let settings = MonitorSettings::from(settings.unwrap_or_default());
        let bitcoin_client = BitcoinClient::new_from_config(rpc_config)?;
        let indexer_store = IndexerStore::new(
            storage.clone(),
            settings
                .indexer_settings
                .as_ref()
                .unwrap_or(&IndexerSettings::default())
                .confirmation_threshold,
        )
        .map_err(|e| MonitorError::UnexpectedError(e.to_string()))?;
        let indexer = Indexer::new(
            bitcoin_client,
            Rc::new(indexer_store),
            settings.indexer_settings.clone(),
        )?;

        let store = MonitorStore::new(storage)?;

        Ok(Self {
            indexer,
            store,
            settings,
        })
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
        self.indexer.tick()?;

        if !self.is_pending_work()? {
            debug!("No pending work, skipping tick");
            return Ok(());
        }

        // Get the best block from the indexer
        // If there's no best block, we can't process anything, so return early
        let indexer_best_block = match self.indexer.get_best_block()? {
            Some(block) => block,
            None => {
                error!("No best block available from indexer, skipping tick");
                return Ok(());
            }
        };

        let indexer_best_block_height = indexer_best_block.height;
        let current_block_hash = indexer_best_block.hash;

        let txs_monitors = self.store.get_monitors()?;

        for tx_type in txs_monitors {
            match tx_type {
                TypesToMonitorStore::Transaction(
                    tx_id,
                    extra_data,
                    number_confirmation_trigger,
                ) => {
                    self.process_transaction(
                        tx_id,
                        extra_data,
                        number_confirmation_trigger,
                        indexer_best_block_height,
                        current_block_hash,
                        false,
                    )?;
                }
                TypesToMonitorStore::RskPegin(number_confirmation_trigger) => {
                    self.process_rsk_pegin_transaction(
                        number_confirmation_trigger,
                        &indexer_best_block,
                        indexer_best_block_height,
                        current_block_hash,
                    )?;
                }
                TypesToMonitorStore::SpendingUTXOTransaction(
                    target_tx_id,
                    target_utxo_index,
                    extra_data,
                    number_confirmation_trigger,
                ) => {
                    self.process_spending_utxo_transaction(
                        target_tx_id,
                        target_utxo_index,
                        extra_data,
                        number_confirmation_trigger,
                        &indexer_best_block,
                        indexer_best_block_height,
                        current_block_hash,
                    )?;
                }
                TypesToMonitorStore::NewBlock => {
                    self.store.update_news(
                        MonitoredTypes::NewBlock(current_block_hash),
                        current_block_hash,
                    )?;
                }
            }
        }

        self.store
            .update_monitor_height(indexer_best_block_height)?;

        self.store.set_pending_work(false)?;

        Ok(())
    }

    /// Gets the current block height that the monitor has processed.
    ///
    /// # Returns
    /// - `Ok(BlockHeight)`: The height of the last processed block
    /// - `Err`: If there was an error retrieving the height
    pub fn get_monitor_height(&self) -> Result<BlockHeight, MonitorError> {
        self.store
            .get_monitor_height()
            .map_err(|e| MonitorError::UnexpectedError(e.to_string()))
    }

    /// Gets the current block of the monitor.
    ///
    /// # Returns
    /// - `Ok(FullBlock)`: The current block of the monitor
    /// - `Err`: If there was an error retrieving the block
    pub fn get_current_block(&self) -> Result<Option<FullBlock>, MonitorError> {
        let block_height = self.get_monitor_height()?;
        let block = self.indexer.get_block_by_height(block_height)?;

        Ok(block)
    }

    /// Starts monitoring transactions based on the provided monitor type.
    ///
    /// # Arguments
    /// * `data` - The type of monitoring to perform, which can be:
    ///   - Transactions: Monitor multiple transactions
    ///   - RskPeginTransaction: Monitor RSK pegin transactions
    ///   - SpendingUTXOTransaction: Monitor transactions spending a specific UTXO
    ///   - NewBlock: Monitor new blocks
    ///
    /// # Returns
    /// - `Ok(())`: If monitoring was set up successfully
    /// - `Err`: If there was an error setting up monitoring
    pub fn monitor(&self, data: TypesToMonitor) -> Result<(), MonitorError> {
        if data != TypesToMonitor::NewBlock {
            self.store.set_pending_work(true)?;
        }

        // Check if the TypesToMonitor instance has a confirmation trigger (if it's a transaction), and if so,
        // ensure it does not exceed the configured max_monitoring_confirmations.
        // Max monitoring confirmations is the number of confirmations that the monitor will wait for before deactivating the monitor.
        // If it does, return an error.
        match &data {
            TypesToMonitor::Transactions(_, _, confirmation_trigger)
            | TypesToMonitor::RskPegin(confirmation_trigger)
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

        self.store.add_monitor(data)?;

        Ok(())
    }

    /// Cancels monitoring for a specific type of monitoring.
    ///
    /// # Arguments
    /// * `data` - The type of monitoring to cancel, which can be:
    ///   - Transactions: Monitor multiple transactions
    ///   - RskPeginTransaction: Monitor RSK pegin transactions
    ///   - SpendingUTXOTransaction: Monitor transactions spending a specific UTXO
    ///   - NewBlock: Monitor new blocks
    ///
    /// # Returns
    /// - `Ok(())`: If monitoring was canceled successfully
    /// - `Err`: If there was an error canceling monitoring
    pub fn cancel(&self, data: TypesToMonitor) -> Result<(), MonitorError> {
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
                MonitoredTypes::Transaction(tx_id, extra_data) => {
                    let status = self.get_tx_status(&tx_id)?;
                    return_news.push(MonitorNews::Transaction(tx_id, status, extra_data));
                }
                MonitoredTypes::RskPeginTransaction(tx_id) => {
                    let status = self.get_tx_status(&tx_id)?;
                    return_news.push(MonitorNews::RskPeginTransaction(tx_id, status));
                }
                MonitoredTypes::SpendingUTXOTransaction(
                    tx_id,
                    utxo_index,
                    extra_data,
                    spender_tx_id,
                ) => {
                    let status = self.get_tx_status(&spender_tx_id)?;
                    return_news.push(MonitorNews::SpendingUTXOTransaction(
                        tx_id, utxo_index, status, extra_data,
                    ));
                }
                MonitoredTypes::NewBlock(hash) => {
                    let block_info = self.indexer.get_block_by_hash(&hash)?;
                    if let Some(block_info) = block_info {
                        return_news.push(MonitorNews::NewBlock(block_info.height, block_info.hash));
                    }
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
    ///   - RskPeginTransaction: Monitor RSK pegin transactions
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
    ///
    /// # Returns
    /// - `Ok(TransactionStatus)`: Current information of the transaction
    /// - `Err`: If there was an error retrieving the status
    pub fn get_tx_status(&self, tx_id: &Txid) -> Result<TransactionStatus, MonitorError> {
        Ok(self.indexer.get_transaction(tx_id)?)
    }

    /// Gets the estimated fee rate from the indexer.
    ///
    /// # Returns
    /// - `Ok(u64)`: The estimated fee rate in satoshis per byte
    /// - `Err`: If there was an error retrieving the fee rate
    pub fn get_estimated_fee_rate(&self) -> Result<u64, MonitorError> {
        self.indexer
            .get_estimated_fee_rate()
            .map_err(MonitorError::IndexerError)
    }

    /// Checks if the monitor has pending work to be done.
    ///
    /// This method determines if the monitor needs to process new blocks by checking:
    /// 1. If there's a pending work flag set in the store
    /// 2. If the monitor's current block matches the indexer's best block
    ///
    /// # Returns
    /// - `Ok(true)`: If there's pending work (store flag is set, no block found, or block hash mismatch)
    /// - `Ok(false)`: If the monitor is fully synced with the indexer
    /// - `Err`: If there was an error checking the sync status
    fn is_pending_work(&self) -> Result<bool, MonitorError> {
        let is_pending_work = self.store.has_pending_work()?;

        if is_pending_work {
            return Ok(true);
        }

        let monitor_block = match self.get_current_block()? {
            Some(block) => block,
            None => {
                debug!("No block found in Monitor, pending work to be done");
                return Ok(true);
            }
        };

        let indexer_block = match self.indexer.get_best_block()? {
            Some(block) => block,
            None => return Ok(false),
        };

        if indexer_block.hash != monitor_block.hash {
            debug!("Best block hash mismatch, pending work to be done");
            return Ok(true);
        }

        Ok(false)
    }

    /// Builds the context string for spending UTXO transactions
    fn build_spending_utxo_context(
        target_tx_id: Txid,
        target_utxo_index: u32,
        extra_data: &str,
    ) -> String {
        format!(
            "{}:{}:{}:{}",
            INTERNAL_SPENDING_UTXO,
            target_tx_id.to_string(),
            target_utxo_index,
            extra_data
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
    fn should_send_news(
        &self,
        tx_id: Txid,
        extra_data: &str,
        number_confirmation_trigger: Option<u32>,
        current_confirmations: u32,
    ) -> Result<bool, MonitorError> {
        let trigger_sent = self.store.get_transaction_trigger_sent(tx_id, extra_data)?;

        if let Some(trigger) = number_confirmation_trigger {
            // Send news when confirmations are greater than or equal to the trigger value
            // but only once (when trigger_sent is false)
            Ok(current_confirmations >= trigger && !trigger_sent)
        } else {
            // If None, always send news when current confirmations are less than the max monitoring confirmations
            Ok(current_confirmations < self.settings.max_monitoring_confirmations)
        }
    }

    fn detect_rsk_pegin_txs(&self, full_block: FullBlock) -> Result<Vec<Txid>, MonitorError> {
        let mut txs_ids = Vec::new();

        for tx in full_block.txs.iter() {
            if is_a_pegin_tx(tx) {
                txs_ids.push(tx.compute_txid());
            }
        }

        Ok(txs_ids)
    }

    fn process_rsk_pegin_transaction(
        &self,
        number_confirmation_trigger: Option<u32>,
        indexer_best_block: &FullBlock,
        indexer_best_block_height: u32,
        current_block_hash: bitcoin::BlockHash,
    ) -> Result<(), MonitorError> {
        let new_txs_ids = self.detect_rsk_pegin_txs(indexer_best_block.clone())?;

        // Add new transactions to monitoring using add_monitor with INTERNAL_RSK_PEGIN context
        for tx_id in &new_txs_ids {
            self.store.add_monitor(TypesToMonitor::Transactions(
                vec![*tx_id],
                INTERNAL_RSK_PEGIN.to_string(),
                number_confirmation_trigger,
            ))?;

            self.process_transaction(
                *tx_id,
                INTERNAL_RSK_PEGIN.to_string(),
                number_confirmation_trigger,
                indexer_best_block_height,
                current_block_hash,
                true,
            )?;
        }

        Ok(())
    }

    fn process_transaction(
        &self,
        tx_id: Txid,
        extra_data: String,
        number_confirmation_trigger: Option<u32>,
        indexer_best_block_height: BlockHeight,
        current_block_hash: bitcoin::BlockHash,
        should_exist: bool,
    ) -> Result<(), MonitorError> {
        let tx_info = self.indexer.get_transaction(&tx_id)?;

        if tx_info.is_not_found() || tx_info.is_in_mempool() {
            if should_exist {
                return Err(MonitorError::UnexpectedError(format!(
                    "Transaction({}) not found or in mempool",
                    tx_id
                )));
            }

            // If the transaction does not exist, nothing to do.
            return Ok(());
        }

        if tx_info.is_orphan() {
            if let Some(block_info) = &tx_info.block_info {
                info!(
                    "Orphan Transaction({}) | Height({})",
                    tx_id, block_info.height
                );
            }
        }

        // Check if we should send news based on number_confirmation_trigger
        let should_send_news = self.should_send_news(
            tx_id,
            &extra_data,
            number_confirmation_trigger,
            tx_info.confirmations,
        )?;

        if should_send_news {
            // Dispatch news update based on extra_data pattern to determine the monitor type
            match extra_data.as_str() {
                ed if ed == INTERNAL_RSK_PEGIN => {
                    self.store.update_news(
                        MonitoredTypes::RskPeginTransaction(tx_id),
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
                        MonitoredTypes::Transaction(tx_id, extra_data.clone()),
                        current_block_hash,
                    )?;
                }
            }

            info!(
                "News for Transaction({}) | Height({}) | Confirmations({})",
                tx_id, indexer_best_block_height, tx_info.confirmations,
            );

            // Update trigger_sent flag if there's a trigger
            if number_confirmation_trigger.is_some() {
                self.store
                    .update_transaction_trigger_sent(tx_id, &extra_data, true)
                    .map_err(|e| MonitorError::UnexpectedError(e.to_string()))?;
            }
        }

        // Check if we should deactivate monitor based on max_monitoring_confirmations
        // Once a transaction reaches the maximum monitoring confirmations, we stop tracking it
        // to avoid unnecessary processing and storage overhead
        if tx_info.confirmations >= self.settings.max_monitoring_confirmations {
            self.store.deactivate_monitor(TypesToMonitor::Transactions(
                vec![tx_id],
                extra_data.clone(),
                number_confirmation_trigger,
            ))?;

            info!(
                "Stop monitoring Transaction({}) | Height({}) | Confirmations({})",
                tx_id, indexer_best_block_height, self.settings.max_monitoring_confirmations,
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
                        indexer_best_block_height,
                        self.settings.max_monitoring_confirmations,
                    );
            }
        }

        Ok(())
    }

    fn process_spending_utxo_transaction(
        &self,
        target_tx_id: Txid,
        target_utxo_index: u32,
        extra_data: String,
        number_confirmation_trigger: Option<u32>,
        indexer_best_block: &FullBlock,
        indexer_best_block_height: BlockHeight,
        current_block_hash: bitcoin::BlockHash,
    ) -> Result<(), MonitorError> {
        // Check each transaction in the new block for a spending transaction of the target UTXO
        for tx in indexer_best_block.txs.iter() {
            let is_spending_output = is_spending_output(tx, target_tx_id, target_utxo_index);

            if is_spending_output {
                let spending_tx_id = tx.compute_txid();

                // Create a monitor for the spending transaction with the special context
                let spending_context =
                    Self::build_spending_utxo_context(target_tx_id, target_utxo_index, &extra_data);

                self.store.add_monitor(TypesToMonitor::Transactions(
                    vec![spending_tx_id],
                    spending_context.clone(),
                    number_confirmation_trigger,
                ))?;

                // Process the spending transaction monitor
                self.process_transaction(
                    spending_tx_id,
                    spending_context,
                    number_confirmation_trigger,
                    indexer_best_block_height,
                    current_block_hash,
                    true,
                )?;
            }
        }

        Ok(())
    }
}

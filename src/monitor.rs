use crate::config::{MonitorSettings, MonitorSettingsConfig};
use crate::errors::MonitorError;
use crate::helper::{is_a_pegin_tx, is_spending_output};
use crate::store::{MonitorStore, MonitorStoreApi, MonitoredTypes, TypesToMonitorStore};
use crate::types::{
    AckMonitorNews, MonitorNews, TransactionBlockchainStatus, TransactionStatus, TypesToMonitor,
};
use bitcoin::Txid;
use bitcoin_indexer::indexer::Indexer;
use bitcoin_indexer::indexer::IndexerApi;
use bitcoin_indexer::store::IndexerStore;
use bitcoin_indexer::types::FullBlock;
use bitcoin_indexer::IndexerType;
use bitvmx_bitcoin_rpc::bitcoin_client::BitcoinClient;
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use mockall::automock;
use std::rc::Rc;
use storage_backend::storage::Storage;
use tracing::{debug, info};

pub struct Monitor<I, B>
where
    I: IndexerApi,
    B: MonitorStoreApi,
{
    pub indexer: I,
    pub store: B,
    pub settings: MonitorSettings,
}

impl Monitor<IndexerType, MonitorStore> {
    pub fn new_with_paths(
        rpc_config: &RpcConfig,
        storage: Rc<Storage>,
        settings: Option<MonitorSettingsConfig>,
    ) -> Result<Self, MonitorError> {
        let settings = MonitorSettings::from(settings.unwrap_or_default());
        let bitcoin_client = BitcoinClient::new_from_config(rpc_config)?;
        let indexer_store = IndexerStore::new(storage.clone())
            .map_err(|e| MonitorError::UnexpectedError(e.to_string()))?;
        let indexer = Indexer::new(
            bitcoin_client,
            Rc::new(indexer_store),
            settings.indexer_settings.clone(),
        )?;
        let bitvmx_store = MonitorStore::new(storage)?;
        let monitor = Monitor::new(indexer, bitvmx_store, settings)?;

        Ok(monitor)
    }
}

#[automock]
pub trait MonitorApi {
    /// Checks if the monitor is ready and fully synced with the blockchain.
    ///
    /// # Returns
    /// - `Ok(true)`: If the monitor is fully synced with the blockchain
    /// - `Ok(false)`: If the monitor is still syncing blocks
    /// - `Err`: If there was an error checking the sync status
    fn is_ready(&self) -> Result<bool, MonitorError>;

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
    fn tick(&self) -> Result<(), MonitorError>;

    /// Gets the current block height that the monitor has processed.
    ///
    /// # Returns
    /// - `Ok(BlockHeight)`: The height of the last processed block
    /// - `Err`: If there was an error retrieving the height
    fn get_monitor_height(&self) -> Result<BlockHeight, MonitorError>;

    /// Gets the current block of the monitor.
    ///
    /// # Returns
    /// - `Ok(FullBlock)`: The current block of the monitor
    /// - `Err`: If there was an error retrieving the block
    fn get_current_block(&self) -> Result<Option<FullBlock>, MonitorError>;

    /// Gets the configured confirmation threshold for transactions.
    ///
    /// The confirmation threshold determines when a transaction is considered final.
    /// A transaction needs this many confirmations (blocks mined on top of its block)
    /// before the monitor considers it irreversible.
    ///
    /// # Returns
    /// The number of confirmations required for finality
    fn get_confirmation_threshold(&self) -> u32;

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
    fn monitor(&self, data: TypesToMonitor) -> Result<(), MonitorError>;

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
    fn cancel(&self, data: TypesToMonitor) -> Result<(), MonitorError>;

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
    fn get_news(&self) -> Result<Vec<MonitorNews>, MonitorError>;

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
    fn ack_news(&self, data: AckMonitorNews) -> Result<(), MonitorError>;

    /// Gets the current status of a specific transaction.
    ///
    /// # Arguments
    /// * `tx_id` - Hash of the transaction to check
    ///
    /// # Returns
    /// - `Ok(TransactionStatus)`: Current status of the transaction
    /// - `Err(MonitorError::TransactionNotFound)`: If the transaction is not found
    /// - `Err`: If there was an error retrieving the status
    fn get_tx_status(&self, tx_id: &Txid) -> Result<TransactionStatus, MonitorError>;

    fn get_estimated_fee_rate(&self) -> Result<u64, MonitorError>;
}

impl MonitorApi for Monitor<IndexerType, MonitorStore> {
    fn tick(&self) -> Result<(), MonitorError> {
        self.tick()
    }

    fn get_monitor_height(&self) -> Result<BlockHeight, MonitorError> {
        self.get_monitor_height()
    }

    fn monitor(&self, data: TypesToMonitor) -> Result<(), MonitorError> {
        if data != TypesToMonitor::NewBlock {
            self.store.set_pending_work(true)?;
        }

        self.store.add_monitor(data)?;

        Ok(())
    }

    fn cancel(&self, data: TypesToMonitor) -> Result<(), MonitorError> {
        self.store.cancel_monitor(data)?;

        Ok(())
    }

    fn get_news(&self) -> Result<Vec<MonitorNews>, MonitorError> {
        self.get_news()
    }

    fn ack_news(&self, data: AckMonitorNews) -> Result<(), MonitorError> {
        self.ack_news(data)
    }

    fn get_tx_status(&self, tx_id: &Txid) -> Result<TransactionStatus, MonitorError> {
        self.get_tx_status(tx_id)
    }

    fn is_ready(&self) -> Result<bool, MonitorError> {
        let is_ready = self.indexer.is_ready()?;
        Ok(is_ready)
    }

    fn get_confirmation_threshold(&self) -> u32 {
        self.settings.confirmation_threshold
    }

    fn get_current_block(&self) -> Result<Option<FullBlock>, MonitorError> {
        self.get_current_block()
    }

    fn get_estimated_fee_rate(&self) -> Result<u64, MonitorError> {
        self.get_estimated_fee_rate()
    }
}

impl<I, B> Monitor<I, B>
where
    I: IndexerApi,
    B: MonitorStoreApi,
{
    pub fn new(
        indexer: I,
        bitvmx_store: B,
        settings: MonitorSettings,
    ) -> Result<Self, MonitorError> {
        Ok(Self {
            indexer,
            store: bitvmx_store,
            settings,
        })
    }

    pub fn save_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorError> {
        if data != TypesToMonitor::NewBlock {
            self.store.set_pending_work(true)?;
        }

        self.store.add_monitor(data)?;

        Ok(())
    }

    pub fn get_monitor_height(&self) -> Result<BlockHeight, MonitorError> {
        self.store
            .get_monitor_height()
            .map_err(|e| MonitorError::UnexpectedError(e.to_string()))
    }

    // This method checks if the monitor has pending work to be done.
    // It checks if the block in the monitor is the same as the best block in the indexer.
    // If the block is not the same, it means that the monitor is not synced with the indexer, so it has pending work to be done to sync it.
    // If the block is the same, it means that the monitor is synced with the indexer, so it has no pending work to be done.
    pub fn is_pending_work(&self) -> Result<bool, MonitorError> {
        let is_pending_work = self.store.has_pending_work()?;

        if is_pending_work {
            return Ok(true);
        }

        let block = self.get_current_block()?;

        if block.is_none() {
            debug!("No block found in Monitor, pending work to be done");
            return Ok(true);
        }

        let monitor_block = block.unwrap();
        let block = self.indexer.get_best_block()?;

        if block.is_none() {
            return Ok(false);
        }

        let block = block.unwrap();

        if block.hash != monitor_block.hash {
            debug!("Best block hash mismatch, pending work to be done");
            return Ok(true);
        }

        Ok(false)
    }

    /// Determines if news should be sent based on the confirmation trigger.
    fn should_send_news(
        &self,
        number_confirmation_trigger: Option<u32>,
        current_confirmations: u32,
    ) -> bool {
        if let Some(trigger) = number_confirmation_trigger {
            // Only send news when confirmations exactly match the trigger value
            current_confirmations == trigger
        } else {
            // If None, always send news when current confirmations are less than the max monitoring confirmations
            current_confirmations < self.settings.max_monitoring_confirmations
        }
    }

    pub fn tick(&self) -> Result<(), MonitorError> {
        self.indexer.tick()?;

        if !self.is_pending_work()? {
            debug!("No pending work, skipping tick");
            return Ok(());
        }

        let indexer_best_block = self.indexer.get_best_block()?;
        let indexer_best_block = indexer_best_block.unwrap();
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
                    self.process_transaction_monitor(
                        tx_id,
                        extra_data,
                        number_confirmation_trigger,
                        indexer_best_block_height,
                        current_block_hash,
                    )?;
                }
                TypesToMonitorStore::RskPegin(from) => {
                    self.process_rsk_pegin_transaction(
                        from,
                        &indexer_best_block,
                        indexer_best_block_height,
                        current_block_hash,
                    )?;
                }
                TypesToMonitorStore::SpendingUTXOTransaction(
                    target_tx_id,
                    target_utxo_index,
                    extra_data,
                    tx_id_spending,
                    number_confirmation_trigger,
                ) => {
                    self.process_spending_utxo_transaction(
                        target_tx_id,
                        target_utxo_index,
                        extra_data,
                        tx_id_spending,
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
        indexer_best_block_height: BlockHeight,
        current_block_hash: bitcoin::BlockHash,
    ) -> Result<(), MonitorError> {
        const INTERNAL_RSK_PEGUIN: &str = "INTERNAL_RSK_PEGIN";

        let new_txs_ids = self.detect_rsk_pegin_txs(indexer_best_block.clone())?;

        // Add new transactions to monitoring using add_monitor with INTERNAL_RSK_PEGUIN context
        for tx_id in &new_txs_ids {
            self.store.add_monitor(TypesToMonitor::Transactions(
                vec![*tx_id],
                INTERNAL_RSK_PEGUIN.to_string(),
                number_confirmation_trigger,
            ))?;

            self.process_transaction_monitor(
                *tx_id,
                INTERNAL_RSK_PEGUIN.to_string(),
                number_confirmation_trigger,
                indexer_best_block_height,
                current_block_hash,
            )?;
        }

        Ok(())
    }

    fn process_transaction_monitor(
        &self,
        tx_id: Txid,
        extra_data: String,
        number_confirmation_trigger: Option<u32>,
        indexer_best_block_height: BlockHeight,
        current_block_hash: bitcoin::BlockHash,
    ) -> Result<(), MonitorError> {
        let tx_info = self.indexer.get_tx(&tx_id)?;

        if let Some(tx) = tx_info {
            if tx.block_info.orphan {
                info!(
                    "Orphan Transaction({}) | Height({})",
                    tx_id, tx.block_info.height
                );
            }

            // Check if we should send news based on number_confirmation_trigger
            let should_send_news =
                self.should_send_news(number_confirmation_trigger, tx.confirmations);

            if should_send_news {
                if extra_data == "INTERNAL_RSK_PEGIN" {
                    self.store.update_news(
                        MonitoredTypes::RskPeginTransaction(tx_id),
                        current_block_hash,
                    )?;
                } else {
                    self.store.update_news(
                        MonitoredTypes::Transaction(tx_id, extra_data.clone()),
                        current_block_hash,
                    )?;
                }

                info!(
                    "News for Transaction({}) | Height({}) | Confirmations({})",
                    tx_id, indexer_best_block_height, tx.confirmations,
                );
            }

            // Check if we should deactivate monitor based on max_monitoring_confirmations
            if tx.confirmations >= self.settings.max_monitoring_confirmations {
                self.store.deactivate_monitor(TypesToMonitor::Transactions(
                    vec![tx_id],
                    extra_data.clone(),
                    number_confirmation_trigger,
                ))?;

                info!(
                    "Stop monitoring Transaction({}) | Height({}) | Confirmations({})",
                    tx_id, indexer_best_block_height, self.settings.max_monitoring_confirmations,
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
        tx_id_spending: Option<Txid>,
        number_confirmation_trigger: Option<u32>,
        indexer_best_block: &FullBlock,
        indexer_best_block_height: BlockHeight,
        current_block_hash: bitcoin::BlockHash,
    ) -> Result<(), MonitorError> {
        if let Some(tx_id_spending) = tx_id_spending {
            let tx_info = match self.indexer.get_tx(&tx_id_spending)? {
                Some(tx_info) => tx_info,
                None => {
                    return Err(MonitorError::TransactionNotFound(
                        tx_id_spending.to_string(),
                    ));
                }
            };

            if tx_info.block_info.orphan {
                info!(
                    "Orphan SpendingUTXOTransaction({}:{}) | Height({}) | Confirmations({})",
                    target_tx_id,
                    target_utxo_index,
                    indexer_best_block_height,
                    tx_info.confirmations,
                );

                self.store
                    .update_spending_utxo_monitor((target_tx_id, target_utxo_index, None))?;

                // Don't skip searching in the new block for the spending transaction.
                // Because it is needed to check if there is a new spending transaction.
            } else {
                // Check if we should send news based on number_confirmation_trigger
                let should_send_news =
                    self.should_send_news(number_confirmation_trigger, tx_info.confirmations);

                if should_send_news {
                    self.store.update_news(
                        MonitoredTypes::SpendingUTXOTransaction(
                            target_tx_id,
                            target_utxo_index,
                            extra_data.clone(),
                            tx_id_spending,
                        ),
                        current_block_hash,
                    )?;

                    info!(
                        "News for SpendingUTXOTransaction({}:{}) | Height({}) | Confirmations({})",
                        target_tx_id,
                        target_utxo_index,
                        indexer_best_block_height,
                        tx_info.confirmations,
                    );
                }

                // Check if we should deactivate monitor based on max_monitoring_confirmations
                if tx_info.confirmations >= self.settings.max_monitoring_confirmations {
                    info!("Stop monitoring SpendingUTXOTransaction({}:{}) | Height({}) | Confirmations({})",
                        target_tx_id,
                        target_utxo_index,
                        indexer_best_block_height,
                        self.settings.max_monitoring_confirmations,
                    );

                    self.store
                        .deactivate_monitor(TypesToMonitor::SpendingUTXOTransaction(
                            target_tx_id,
                            target_utxo_index,
                            extra_data.clone(),
                            number_confirmation_trigger,
                        ))?;

                    // Skip searching in the new block for the spending transaction.
                    // Because it is already confirmed.
                    return Ok(());
                }

                // Skip searching in the new block for the spending transaction.
                // Because it is already confirmed.
                return Ok(());
            }
        }

        // Check each transaction in the new block for a spending transaction of the target UTXO
        for tx in indexer_best_block.txs.iter() {
            let is_spending_output = is_spending_output(tx, target_tx_id, target_utxo_index);

            if is_spending_output {
                let tx_info = match self.indexer.get_tx(&tx.compute_txid())? {
                    Some(tx_info) => tx_info,
                    None => {
                        return Err(MonitorError::TransactionNotFound(
                            tx.compute_txid().to_string(),
                        ))
                    }
                };

                self.store.update_spending_utxo_monitor((
                    target_tx_id,
                    target_utxo_index,
                    Some(tx.compute_txid()),
                ))?;

                let should_send_news =
                    self.should_send_news(number_confirmation_trigger, tx_info.confirmations);

                if should_send_news {
                    self.store.update_news(
                        MonitoredTypes::SpendingUTXOTransaction(
                            target_tx_id,
                            target_utxo_index,
                            extra_data.clone(),
                            tx.compute_txid(),
                        ),
                        current_block_hash,
                    )?;

                    info!(
                        "News for SpendingUTXOTransaction({}:{}) | Height({}) | Confirmations({})",
                        target_tx_id,
                        target_utxo_index,
                        indexer_best_block_height,
                        tx_info.confirmations,
                    );
                }
            }
        }

        Ok(())
    }

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

    pub fn ack_news(&self, data: AckMonitorNews) -> Result<(), MonitorError> {
        self.store.ack_news(data)?;
        Ok(())
    }

    pub fn get_tx_status(&self, tx_id: &Txid) -> Result<TransactionStatus, MonitorError> {
        let tx_status = self
            .indexer
            .get_tx(tx_id)?
            .ok_or_else(|| MonitorError::TransactionNotFound(tx_id.to_string()))?;

        let status = if tx_status.block_info.orphan {
            TransactionBlockchainStatus::Orphan
        } else if tx_status.confirmations >= self.settings.confirmation_threshold {
            TransactionBlockchainStatus::Finalized
        } else {
            TransactionBlockchainStatus::Confirmed
        };

        let return_tx_status = TransactionStatus::new(
            tx_status.tx,
            tx_status.block_info,
            status,
            tx_status.confirmations,
        );

        Ok(return_tx_status)
    }

    pub fn get_current_block(&self) -> Result<Option<FullBlock>, MonitorError> {
        let block_height = self.get_monitor_height()?;
        let block = self.indexer.get_block_by_height(block_height)?;

        Ok(block)
    }

    pub fn get_estimated_fee_rate(&self) -> Result<u64, MonitorError> {
        self.indexer
            .get_estimated_fee_rate()
            .map_err(MonitorError::IndexerError)
    }
}

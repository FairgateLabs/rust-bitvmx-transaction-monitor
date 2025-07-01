use crate::config::MonitorConstants;
use crate::errors::MonitorError;
use crate::helper::{is_a_pegin_tx, is_spending_output};
use crate::store::{MonitorStore, MonitorStoreApi, MonitoredTypes, TypesToMonitorStore};
use crate::types::{
    AckMonitorNews, MonitorNews, TransactionBlockchainStatus, TransactionStatus, TypesToMonitor,
};
use bitcoin::Txid;
use bitcoin_indexer::config::IndexerConstants;
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
use tracing::info;

pub struct Monitor<I, B>
where
    I: IndexerApi,
    B: MonitorStoreApi,
{
    pub indexer: I,
    pub store: B,
    pub constants: MonitorConstants,
}

impl Monitor<IndexerType, MonitorStore> {
    pub fn new_with_paths(
        rpc_config: &RpcConfig,
        storage: Rc<Storage>,
        indexer_constants: Option<IndexerConstants>,
        constants: Option<MonitorConstants>,
    ) -> Result<Self, MonitorError> {
        let constants = constants.unwrap_or_default();
        let bitcoin_client = BitcoinClient::new_from_config(rpc_config)?;
        let indexer_store = IndexerStore::new(storage.clone())
            .map_err(|e| MonitorError::UnexpectedError(e.to_string()))?;
        let indexer = Indexer::new(bitcoin_client, Rc::new(indexer_store), indexer_constants)?;
        let bitvmx_store = MonitorStore::new(storage)?;
        let monitor = Monitor::new(indexer, bitvmx_store, constants)?;

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
    /// - `Ok(Vec<MonitorNewType>)`: List of status updates grouped by monitor type
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
}

impl MonitorApi for Monitor<IndexerType, MonitorStore> {
    fn tick(&self) -> Result<(), MonitorError> {
        self.tick()
    }

    fn get_monitor_height(&self) -> Result<BlockHeight, MonitorError> {
        self.get_monitor_height()
    }

    fn monitor(&self, data: TypesToMonitor) -> Result<(), MonitorError> {
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
        self.constants.confirmation_threshold
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
        constants: MonitorConstants,
    ) -> Result<Self, MonitorError> {
        Ok(Self {
            indexer,
            store: bitvmx_store,
            constants,
        })
    }

    pub fn save_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorError> {
        self.store.add_monitor(data)?;

        Ok(())
    }

    pub fn get_monitor_height(&self) -> Result<BlockHeight, MonitorError> {
        self.store
            .get_monitor_height()
            .map_err(|e| MonitorError::UnexpectedError(e.to_string()))
    }

    pub fn tick(&self) -> Result<(), MonitorError> {
        self.indexer.tick()?;

        let indexer_best_block = self.indexer.get_best_block()?;

        if indexer_best_block.is_none() {
            return Ok(());
        }

        let indexer_best_block = indexer_best_block.unwrap();
        let indexer_best_block_height = indexer_best_block.height;
        let current_height = self.get_monitor_height()?;

        let txs_types = self.store.get_monitors()?;

        for tx_type in txs_types {
            match tx_type {
                TypesToMonitorStore::Transaction(tx_id, extra_data) => {
                    let tx_info = self.indexer.get_tx(&tx_id)?;

                    if let Some(tx) = tx_info {
                        if tx.block_info.orphan {
                            info!(
                                "Orphan Transaction({}) | Height({})",
                                tx_id, tx.block_info.height
                            );

                            self.store.update_news(MonitoredTypes::Transaction(
                                tx_id,
                                extra_data.clone(),
                            ))?;
                        }

                        // Transaction exists in the blockchain.
                        if tx.confirmations <= self.constants.confirmation_threshold {
                            self.store.update_news(MonitoredTypes::Transaction(
                                tx_id,
                                extra_data.clone(),
                            ))?;

                            info!(
                                "News for Transaction({}) | Height({}) | Confirmations({})",
                                tx_id, indexer_best_block_height, tx.confirmations,
                            );
                        } else if tx.confirmations >= self.constants.max_monitoring_confirmations {
                            // Deactivate monitor after 100 confirmations
                            self.store.deactivate_monitor(TypesToMonitor::Transactions(
                                vec![tx_id],
                                extra_data.clone(),
                            ))?;

                            info!(
                                "Stop monitoring Transaction({}) | Height({}) | Confirmations({})",
                                tx_id,
                                indexer_best_block_height,
                                self.constants.max_monitoring_confirmations,
                            );
                        }
                    }
                }
                TypesToMonitorStore::RskPeginTransaction => {
                    let txs_ids = self.detect_rsk_pegin_txs(indexer_best_block.clone())?;

                    for tx_id in txs_ids {
                        self.store
                            .update_news(MonitoredTypes::RskPeginTransaction(tx_id))?;

                        if let Some(tx) = self.indexer.get_tx(&tx_id)? {
                            info!(
                                "News for RSK pegin Transaction({}) | Height({}) | Confirmations({})",
                                tx_id,
                                indexer_best_block_height,
                                indexer_best_block_height - tx.block_info.height + 1,
                            );
                        }
                    }
                }
                TypesToMonitorStore::SpendingUTXOTransaction(
                    target_tx_id,
                    target_utxo_index,
                    extra_data,
                ) => {
                    // Check each transaction in the block for spending the target UTXO
                    for tx in indexer_best_block.txs.iter() {
                        let is_spending_output =
                            is_spending_output(tx, target_tx_id, target_utxo_index);

                        if is_spending_output {
                            let tx_info = self.indexer.get_tx(&tx.compute_txid())?;
                            if let Some(tx_info) = tx_info {
                                let confirmations =
                                    indexer_best_block_height - tx_info.block_info.height + 1;

                                if confirmations <= self.constants.confirmation_threshold {
                                    self.store.update_news(
                                        MonitoredTypes::SpendingUTXOTransaction(
                                            target_tx_id,
                                            target_utxo_index,
                                            extra_data.clone(),
                                        ),
                                    )?;

                                    info!(
                                        "News for SpendingUTXOTransaction({}:{}) | Height({}) | Confirmations({})",
                                        target_tx_id,
                                        target_utxo_index,
                                        indexer_best_block_height,
                                        confirmations,
                                    );
                                } else if confirmations
                                    >= self.constants.max_monitoring_confirmations
                                {
                                    // Deactivate monitor after 100 confirmations
                                    self.store.deactivate_monitor(
                                        TypesToMonitor::SpendingUTXOTransaction(
                                            target_tx_id,
                                            target_utxo_index,
                                            extra_data.clone(),
                                        ),
                                    )?;

                                    info!(
                                        "Stop monitoring SpendingUTXOTransaction({}:{}) | Height({}) | Confirmations({})",
                                        target_tx_id,
                                        target_utxo_index,
                                        indexer_best_block_height,
                                            self.constants.max_monitoring_confirmations,
                                    );
                                }
                            }
                        }
                    }
                }
                TypesToMonitorStore::NewBlock => {
                    if current_height != indexer_best_block_height {
                        self.store.update_news(MonitoredTypes::NewBlock)?;
                    }
                }
            }
        }

        self.store
            .update_monitor_height(indexer_best_block_height)?;

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
                MonitoredTypes::SpendingUTXOTransaction(tx_id, utxo_index, extra_data) => {
                    let status = self.get_tx_status(&tx_id)?;
                    return_news.push(MonitorNews::SpendingUTXOTransaction(
                        tx_id, utxo_index, status, extra_data,
                    ));
                }
                MonitoredTypes::NewBlock => {
                    let block_info = self.indexer.get_best_block()?;
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
        } else if tx_status.confirmations >= self.constants.confirmation_threshold {
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
}

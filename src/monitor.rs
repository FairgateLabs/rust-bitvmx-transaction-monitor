use crate::errors::MonitorError;
use crate::rsk_helper::is_a_pegin_tx;
use crate::store::{
    MonitorStore, MonitorStoreApi, TransactionMonitoredType, TransactionToMonitorType,
};
use crate::types::{BlockInfo, Id, MonitorNewType, TransactionMonitorType, TransactionStatus};
use bitcoin::Txid;
use bitcoin_indexer::indexer::IndexerApi;
use bitcoin_indexer::store::IndexerStore;
use bitcoin_indexer::{helper::define_height_to_sync, indexer::Indexer};
use bitvmx_bitcoin_rpc::bitcoin_client::{BitcoinClient, BitcoinClientApi};
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use bitvmx_bitcoin_rpc::types::{BlockHeight, FullBlock};
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
    confirmation_threshold: u32,
}

impl Monitor<Indexer<BitcoinClient, IndexerStore>, MonitorStore> {
    pub fn new_with_paths(
        rpc_config: &RpcConfig,
        storage: Rc<Storage>,
        checkpoint: Option<BlockHeight>,
        confirmation_threshold: u32,
    ) -> Result<Self, MonitorError> {
        let bitcoin_client = BitcoinClient::new_from_config(rpc_config)?;
        let blockchain_height = bitcoin_client.get_best_block()? as BlockHeight;
        let indexer_store = IndexerStore::new(storage.clone())
            .map_err(|e| MonitorError::UnexpectedError(e.to_string()))?;
        let indexer = Indexer::new(bitcoin_client, indexer_store);
        let best_block = indexer.get_best_block()?;
        let bitvmx_store = MonitorStore::new(storage)?;
        let current_height =
            define_height_to_sync(checkpoint, blockchain_height, best_block.map(|b| b.height))?;
        let monitor = Monitor::new(
            indexer,
            bitvmx_store,
            Some(current_height),
            confirmation_threshold,
        )?;

        Ok(monitor)
    }

    pub fn new_with_paths_and_rpc_details(
        rpc_config: &RpcConfig,
        storage: Rc<Storage>,
        checkpoint: Option<BlockHeight>,
        confirmation_threshold: u32,
    ) -> Result<Self, MonitorError> {
        let bitcoin_client = BitcoinClient::new_from_config(rpc_config)?;
        let blockchain_height = bitcoin_client.get_best_block()? as BlockHeight;
        let indexer_store = IndexerStore::new(storage.clone())
            .map_err(|e| MonitorError::UnexpectedError(e.to_string()))?;
        let indexer = Indexer::new(bitcoin_client, indexer_store);
        let best_block = indexer.get_best_block()?;
        let bitvmx_store = MonitorStore::new(storage)?;
        let current_height =
            define_height_to_sync(checkpoint, blockchain_height, best_block.map(|b| b.height))?;
        let monitor = Monitor::new(
            indexer,
            bitvmx_store,
            Some(current_height),
            confirmation_threshold,
        )?;

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
    ///   - GroupTransaction: Monitor multiple transactions for a given group
    ///   - SingleTransaction: Monitor a single transaction
    ///   - RskPeginTransaction: Monitor RSK pegin transactions
    ///   - SpendingUTXOTransaction: Monitor transactions spending a specific UTXO
    ///
    /// # Returns
    /// - `Ok(())`: If monitoring was set up successfully
    /// - `Err`: If there was an error setting up monitoring
    fn monitor(&self, data: TransactionMonitorType) -> Result<(), MonitorError>;

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
    fn get_news(&self) -> Result<Vec<MonitorNewType>, MonitorError>;

    /// Acknowledges that a transaction status update has been processed.
    ///
    /// After processing a status update from get_news(), this method should be called
    /// to remove it from the pending updates queue.
    ///
    /// # Arguments
    /// * `instance_id` - ID of the instance the transaction belongs to
    /// * `tx_id` - Hash of the transaction that was processed
    ///
    /// # Returns
    /// - `Ok(())`: If the update was successfully acknowledged
    /// - `Err`: If there was an error processing the acknowledgment
    fn acknowledge_news(&self, instance_id: Id, tx_id: &Txid) -> Result<(), MonitorError>;

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

impl MonitorApi for Monitor<Indexer<BitcoinClient, IndexerStore>, MonitorStore> {
    fn tick(&self) -> Result<(), MonitorError> {
        self.tick()
    }

    fn get_monitor_height(&self) -> Result<BlockHeight, MonitorError> {
        self.get_current_height()
    }

    fn monitor(&self, data: TransactionMonitorType) -> Result<(), MonitorError> {
        let bitcoind_height = self.indexer.bitcoin_client.get_best_block()?;
        self.save_monitor(data, bitcoind_height)
    }

    fn get_news(&self) -> Result<Vec<MonitorNewType>, MonitorError> {
        self.get_news()
    }

    fn acknowledge_news(&self, instance_id: Id, tx_id: &Txid) -> Result<(), MonitorError> {
        self.acknowledge_news(instance_id, tx_id)
    }

    fn get_tx_status(&self, tx_id: &Txid) -> Result<TransactionStatus, MonitorError> {
        self.get_tx_status(tx_id)
    }

    fn is_ready(&self) -> Result<bool, MonitorError> {
        let current_height = self.get_current_height()?;
        let blockchain_height = self.indexer.bitcoin_client.get_best_block()?;
        Ok(current_height == blockchain_height)
    }

    fn get_confirmation_threshold(&self) -> u32 {
        self.confirmation_threshold
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
        current_height: Option<BlockHeight>,
        confirmation_threshold: u32,
    ) -> Result<Self, MonitorError> {
        let current_height = current_height.unwrap_or(0);
        bitvmx_store
            .set_current_block_height(current_height)
            .map_err(|e| MonitorError::UnexpectedError(e.to_string()))?;

        Ok(Self {
            indexer,
            store: bitvmx_store,
            confirmation_threshold,
        })
    }

    pub fn save_monitor(
        &self,
        data: TransactionMonitorType,
        start_monitoring: BlockHeight,
    ) -> Result<(), MonitorError> {
        self.store.save(data, start_monitoring)?;

        Ok(())
    }

    pub fn get_current_height(&self) -> Result<BlockHeight, MonitorError> {
        self.store
            .get_current_block_height()
            .map_err(|e| MonitorError::UnexpectedError(e.to_string()))
    }

    pub fn tick(&self) -> Result<(), MonitorError> {
        let current_height = self.get_current_height()?;
        let new_height = self.indexer.tick(&current_height)?;
        let best_block = self.indexer.get_best_block()?;

        if best_block.is_none() {
            return Ok(());
        }

        let best_full_block = best_block.unwrap();
        let best_block_height = best_full_block.height;

        // Get operations that have already started
        let txs_types = self.store.get_txs_ready_to_monitor(best_block_height)?;

        // Count existing operations get all thansaction that meet next rules:
        for tx_type in txs_types {
            match tx_type {
                TransactionToMonitorType::GroupTransaction(id, tx_id) => {
                    let tx_info = self.indexer.get_tx(&tx_id)?;

                    if let Some(tx) = tx_info {
                        if best_block_height > tx.block_height
                            && (best_block_height - tx.block_height) <= self.confirmation_threshold
                        {
                            self.store
                                .update_news(TransactionMonitoredType::GroupTransaction(
                                    id, tx_id,
                                ))?;

                            info!(
                                "Update confirmation for group: {} | tx_id: {} | at height: {} | confirmations: {}", 
                                id,
                                tx_id,
                                best_block_height,
                                best_block_height - tx.block_height + 1,
                            );
                        }
                    }
                }
                TransactionToMonitorType::SingleTransaction(tx_id) => {
                    let tx_info = self.indexer.get_tx(&tx_id)?;

                    if let Some(tx) = tx_info {
                        if best_block_height > tx.block_height
                            && (best_block_height - tx.block_height) <= self.confirmation_threshold
                        {
                            self.store
                                .update_news(TransactionMonitoredType::SingleTransaction(tx_id))?;

                            info!(
                                "Update confirmation for single tx: {} | at height: {} | confirmations: {}", 
                                tx_id,
                                best_block_height,
                                best_block_height - tx.block_height + 1,
                            );
                        }
                    }
                }
                TransactionToMonitorType::RskPeginTransaction => {
                    let txs_ids = self.detect_rsk_pegin_txs(best_full_block.clone())?;

                    for tx_id in txs_ids {
                        self.store
                            .update_news(TransactionMonitoredType::RskPeginTransaction(tx_id))?;

                        if let Some(tx) = self.indexer.get_tx(&tx_id)? {
                            info!(
                                "Update confirmation for RSK pegin tx: {} | at height: {} | confirmations: {}", 
                                tx_id,
                                best_block_height,
                                best_block_height - tx.block_height + 1,
                            );
                        }
                    }
                }

                TransactionToMonitorType::SpendingUTXOTransaction(_tx_id, _utxo_index) => {
                    // TODO: detect spending utxo txs here
                }
            }
        }

        self.store.set_current_block_height(new_height)?;

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

    pub fn get_news(&self) -> Result<Vec<MonitorNewType>, MonitorError> {
        let news = self.store.get_news()?;

        let mut return_news = Vec::new();

        for news in news {
            let tx_id = match &news {
                TransactionMonitoredType::GroupTransaction(_, tx_id) => tx_id,
                TransactionMonitoredType::SingleTransaction(tx_id) => tx_id,
                TransactionMonitoredType::RskPeginTransaction(tx_id) => tx_id,
                TransactionMonitoredType::SpendingUTXOTransaction(tx_id, _) => tx_id,
            };

            let status = self.get_tx_status(tx_id)?;

            match news {
                TransactionMonitoredType::GroupTransaction(id, _) => {
                    return_news.push(MonitorNewType::GroupTransaction(id, status));
                }
                TransactionMonitoredType::SingleTransaction(_) => {
                    return_news.push(MonitorNewType::SingleTransaction(status));
                }
                TransactionMonitoredType::RskPeginTransaction(_) => {
                    return_news.push(MonitorNewType::RskPeginTransaction(status));
                }
                TransactionMonitoredType::SpendingUTXOTransaction(_, utxo_index) => {
                    return_news.push(MonitorNewType::SpendingUTXOTransaction(utxo_index, status));
                }
            }
        }

        Ok(return_news)
    }

    pub fn acknowledge_news(&self, instance_id: Id, tx_id: &Txid) -> Result<(), MonitorError> {
        self.store.acknowledge_news(instance_id, tx_id)?;
        Ok(())
    }

    pub fn get_tx_status(&self, tx_id: &Txid) -> Result<TransactionStatus, MonitorError> {
        let tx_status = self
            .indexer
            .get_tx(tx_id)?
            .ok_or_else(|| MonitorError::TransactionNotFound(tx_id.to_string()))?;

        let block_info = Some(BlockInfo::new(
            tx_status.block_height,
            tx_status.block_hash,
            tx_status.orphan,
        ));

        let return_tx_status = TransactionStatus::new(tx_status.tx, block_info);

        Ok(return_tx_status)
    }
}

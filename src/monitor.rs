use crate::errors::MonitorError;
use crate::rsk_helper::is_a_pegin_tx;
use crate::store::{MonitorStore, MonitorStoreApi};
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
    pub bitvmx_store: B,
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
    /// - `Ok(Some(TransactionStatus))`: Current status if the transaction is found
    /// - `Ok(None)`: If the transaction is not found
    /// - `Err`: If there was an error retrieving the status
    fn get_tx_status(&self, tx_id: &Txid) -> Result<Option<TransactionStatus>, MonitorError>;
}

impl MonitorApi for Monitor<Indexer<BitcoinClient, IndexerStore>, MonitorStore> {
    fn tick(&self) -> Result<(), MonitorError> {
        self.tick()
    }

    fn get_monitor_height(&self) -> Result<BlockHeight, MonitorError> {
        self.get_current_height()
    }

    fn monitor(&self, data: TransactionMonitorType) -> Result<(), MonitorError> {
        let current_height = self.get_current_height()?;
        self.save(data, current_height)
    }

    fn get_news(&self) -> Result<Vec<MonitorNewType>, MonitorError> {
        self.get_news()
    }

    fn acknowledge_news(&self, instance_id: Id, tx_id: &Txid) -> Result<(), MonitorError> {
        self.acknowledge_news(instance_id, tx_id)
    }

    fn get_tx_status(&self, tx_id: &Txid) -> Result<Option<TransactionStatus>, MonitorError> {
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
            bitvmx_store,
            confirmation_threshold,
        })
    }

    pub fn save(
        &self,
        data: TransactionMonitorType,
        current_height: BlockHeight,
    ) -> Result<(), MonitorError> {
        self.bitvmx_store.save_instances(&data, current_height)?;

        Ok(())
    }

    pub fn save_transaction_for_tracking(
        &self,
        instance_id: Id,
        tx_id: Txid,
    ) -> Result<(), MonitorError> {
        self.bitvmx_store
            .save_instance_transaction(instance_id, &tx_id)?;
        Ok(())
    }

    fn remove_transaction_for_tracking(
        &self,
        instance_id: Id,
        tx_id: Txid,
    ) -> Result<(), MonitorError> {
        self.bitvmx_store.remove_transaction(instance_id, &tx_id)?;
        Ok(())
    }

    pub fn get_current_height(&self) -> Result<BlockHeight, MonitorError> {
        self.bitvmx_store
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

        // Get operations that have already started
        let instances = self
            .bitvmx_store
            .get_instances_ready_to_track(best_full_block.height)?;

        // Count existing operations get all thansaction that meet next rules:
        for instance in instances {
            for tx_instance in instance.txs {
                // if Trasanction is None, means it was not mined.
                let tx_info = self.indexer.get_tx(&tx_instance.tx_id)?;

                if let Some(_tx_info) = tx_info {
                    if best_full_block.height > _tx_info.block_height
                        && (best_full_block.height - _tx_info.block_height)
                            <= self.confirmation_threshold
                    {
                        self.bitvmx_store
                            .update_instance_news(instance.id, tx_instance.tx_id)?;

                        info!(
                                    "Update confirmation for bitvmx intance: {} | tx_id: {} | at height: {} | confirmations: {}", 
                                    instance.id,
                                    tx_instance.tx_id,
                                    best_full_block.height,
                                    best_full_block.height - _tx_info.block_height + 1,
                                );
                    }
                }
            }
        }

        self.bitvmx_store.set_current_block_height(new_height)?;

        self.detect_txs_in_block(best_full_block)?;

        Ok(())
    }

    fn detect_txs_in_block(&self, full_block: FullBlock) -> Result<(), MonitorError> {
        for tx in full_block.txs.iter() {
            let is_pegin = is_a_pegin_tx(tx);

            if is_pegin {
                let block_info =
                    BlockInfo::new(full_block.height, full_block.hash, full_block.orphan);
                self.bitvmx_store.save_tx(tx, block_info)?;
            }

            //TODO: detect other txs that we need to track here...
        }

        Ok(())
    }

    pub fn get_news(&self) -> Result<Vec<MonitorNewType>, MonitorError> {
        let news = self.bitvmx_store.get_news()?;

        let mut return_news = Vec::new();

        for news in news_list {
            let mut return_tx = Vec::new();

            for tx_id in txs {
                if let Ok(Some(status)) = self.get_tx_status(&tx_id) {
                    return_tx.push(status);
                } else {
                    return Err(MonitorError::UnexpectedError(format!(
                        "Transaction not found: {}",
                        tx_id
                    )));
                }
            }

            if !return_tx.is_empty() {
                return_news.push((instance_id, return_tx));
            }
        }
        Ok(return_news)
    }

    pub fn acknowledge_news(&self, instance_id: Id, tx_id: &Txid) -> Result<(), MonitorError> {
        self.bitvmx_store.acknowledge_news(instance_id, tx_id)?;
        Ok(())
    }

    pub fn get_tx_status(&self, tx_id: &Txid) -> Result<Option<TransactionStatus>, MonitorError> {
        let tx_status = self.indexer.get_tx(tx_id)?;

        let tx_status_response = tx_status.map(|tx_status| {
            let block_info = Some(BlockInfo::new(
                tx_status.block_height,
                tx_status.block_hash,
                tx_status.orphan,
            ));

            TransactionStatus::new(tx_status.tx, block_info)
        });

        Ok(tx_status_response)
    }
}

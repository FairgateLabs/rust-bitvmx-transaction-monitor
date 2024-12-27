use crate::bitvmx_store::{BitvmxApi, BitvmxStore};
use crate::types::{
    AddressStatus, BitvmxInstance, BlockInfo, InstanceData, InstanceId, TransactionStatus,
    TransactionStore,
};
use anyhow::{Context, Result};
use bitcoin::{Address, Network, Transaction, Txid};
use bitcoin_indexer::types::FullBlock;
use bitcoin_indexer::{
    bitcoin_client::{BitcoinClient, BitcoinClientApi},
    helper::define_height_to_sync,
    indexer::Indexer,
    store::Store,
};
use bitcoin_indexer::{indexer::IndexerApi, types::BlockHeight};
use log::info;
use mockall::automock;
pub struct Monitor<I, B>
where
    I: IndexerApi,
    B: BitvmxApi,
{
    pub indexer: I,
    pub bitvmx_store: B,
    current_height: BlockHeight,
    confirmation_threshold: u32,
}

impl Monitor<Indexer<BitcoinClient, Store>, BitvmxStore> {
    pub fn new_with_paths(
        node_rpc_url: &str,
        db_file_path: &str,
        checkpoint: Option<BlockHeight>,
        confirmation_threshold: u32,
    ) -> Result<Self> {
        let bitcoin_client = BitcoinClient::new(node_rpc_url)?;
        let blockchain_height = bitcoin_client.get_best_block()? as BlockHeight;
        let indexer = Indexer::new_with_path(bitcoin_client, db_file_path)?;
        let best_block = indexer.get_best_block()?;
        let bitvmx_store = BitvmxStore::new_with_path(db_file_path)?;
        let current_height =
            define_height_to_sync(checkpoint, blockchain_height, best_block.map(|b| b.height))?;
        let monitor = Monitor::new(
            indexer,
            bitvmx_store,
            Some(current_height),
            confirmation_threshold,
        );

        Ok(monitor)
    }
}

#[automock]
pub trait MonitorApi {
    /// Checks if the monitor is ready and fully synced with the blockchain.
    ///
    /// # Returns
    /// - `Ok(bool)`: Returns true if the monitor is ready and synced, false otherwise.
    /// - `Err`: If there was an error checking the sync status.
    fn is_ready(&mut self) -> Result<bool>;

    /// Processes one tick of the monitor's operation.
    ///
    /// This method monitors transaction statuses for stored instances and checks for confirmations.
    /// It also triggers the indexer to continue syncing if not yet synchronized with the blockchain.
    ///
    /// # Returns
    /// - `Ok(())`: If the tick operation completed successfully.
    /// - `Err`: If there was an error during processing.
    fn tick(&mut self) -> Result<()>;

    /// Gets the current block height that the monitor has processed.
    ///
    /// # Returns
    /// The current block height as a `BlockHeight`.
    fn get_current_height(&self) -> BlockHeight;

    /// Saves multiple instances for monitoring.
    ///
    /// # Arguments
    /// * `instances` - Vector of instance data to be tracked
    ///
    /// # Returns
    /// - `Ok(())`: If instances were saved successfully.
    /// - `Err`: If there was an error saving the instances.
    fn save_instances_for_tracking(&self, instances: Vec<InstanceData>) -> Result<()>;

    /// Saves a single transaction to be monitored for a specific instance.
    ///
    /// # Arguments
    /// * `instance_id` - ID of the instance the transaction belongs to
    /// * `tx_id` - Transaction ID to monitor
    ///
    /// # Returns
    /// - `Ok(())`: If the transaction was saved successfully.
    /// - `Err`: If there was an error saving the transaction.
    fn save_transaction_for_tracking(&self, instance_id: InstanceId, tx_id: Txid) -> Result<()>;

    /// Removes a transaction from being monitored for a specific instance.
    ///
    /// # Arguments
    /// * `instance_id` - ID of the instance the transaction belongs to
    /// * `tx_id` - Transaction ID to stop monitoring
    ///
    /// # Returns
    /// - `Ok(())`: If the transaction was removed successfully.
    /// - `Err`: If there was an error removing the transaction.
    fn remove_transaction_for_tracking(&self, instance_id: InstanceId, tx_id: Txid) -> Result<()>;

    /// Gets all instances currently being tracked.
    ///
    /// # Returns
    /// - `Ok(Vec<BitvmxInstance>)`: Vector of all tracked instances.
    /// - `Err`: If there was an error retrieving the instances.
    fn get_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>>;

    /// Saves an address to be monitored for transactions.
    ///
    /// # Arguments
    /// * `address` - Bitcoin address to monitor
    ///
    /// # Returns
    /// - `Ok(())`: If the address was saved successfully.
    /// - `Err`: If there was an error saving the address.
    fn save_address_for_tracking(&self, address: Address) -> Result<()>;

    /// Gets status updates for transactions belonging to monitored instances.
    ///
    /// # Returns
    /// - `Ok(Vec<(InstanceId, Vec<TransactionStatus>)>)`: Vector of tuples containing:
    ///   - `InstanceId`: The BitVMX instance ID
    ///   - `Vec<TransactionStatus>`: Vector of status updates for the instance's transactions
    /// - `Err`: If there was an error retrieving the updates.
    fn get_instance_news(&self) -> Result<Vec<(InstanceId, Vec<TransactionStatus>)>>;

    /// Acknowledges that a transaction status update has been processed.
    ///
    /// This removes the status update from the pending news queue.
    ///
    /// # Arguments
    /// * `instance_id` - ID of the instance the transaction belongs to
    /// * `tx_id` - Transaction ID that was processed
    ///
    /// # Returns
    /// - `Ok(())`: If the acknowledgment was successful.
    /// - `Err`: If there was an error processing the acknowledgment.
    fn acknowledge_instance_tx_news(&self, instance_id: InstanceId, tx_id: &Txid) -> Result<()>;

    /// Gets the current status of a specific transaction for an instance.
    ///
    /// # Arguments
    /// * `instance_id` - ID of the instance the transaction belongs to
    /// * `tx_id` - Transaction ID to check
    ///
    /// # Returns
    /// - `Ok(Option<TransactionStatus>)`: The transaction's status if found.
    /// - `Err`: If there was an error retrieving the status.
    fn get_instance_tx_status(
        &self,
        instance_id: InstanceId,
        tx_id: &Txid,
    ) -> Result<Option<TransactionStatus>>;

    /// Gets status updates for monitored addresses.
    ///
    /// # Returns
    /// - `Ok(Vec<(Address, Vec<AddressStatus>)>)`: Vector of address/status pairs.
    /// - `Err`: If there was an error retrieving the updates.
    fn get_address_news(&self) -> Result<Vec<(Address, Vec<AddressStatus>)>>;

    /// Acknowledges that an address status update has been processed.
    ///
    /// # Arguments
    /// * `address` - The address whose updates were processed
    ///
    /// # Returns
    /// - `Ok(())`: If the acknowledgment was successful.
    /// - `Err`: If there was an error processing the acknowledgment.
    fn acknowledge_address_news(&self, address: Address) -> Result<()>;

    /// Gets the configured confirmation threshold that determines when a transaction is considered final.
    /// This threshold represents the minimum number of blocks that must be mined on top of the block
    /// containing the transaction before it is treated as irreversible.
    ///
    /// # Returns
    /// The confirmation threshold as a u32.
    fn get_confirmation_threshold(&self) -> u32;
}

impl MonitorApi for Monitor<Indexer<BitcoinClient, Store>, BitvmxStore> {
    fn tick(&mut self) -> Result<()> {
        self.tick()
    }

    fn get_current_height(&self) -> BlockHeight {
        self.get_current_height()
    }

    fn save_instances_for_tracking(&self, instances: Vec<InstanceData>) -> Result<()> {
        let bitvmx_instances: Vec<BitvmxInstance> = instances
            .into_iter()
            .map(|instance_data| {
                let txs = instance_data
                    .txs
                    .into_iter()
                    .map(|tx_id| TransactionStore { tx_id, tx: None })
                    .collect();
                BitvmxInstance {
                    id: instance_data.instance_id,
                    txs,
                    start_height: self.current_height,
                }
            })
            .collect();

        self.save_instances_for_tracking(bitvmx_instances)
    }
    fn save_transaction_for_tracking(&self, instance_id: InstanceId, tx_id: Txid) -> Result<()> {
        self.save_transaction_for_tracking(instance_id, tx_id)
    }

    fn remove_transaction_for_tracking(&self, instance_id: InstanceId, tx_id: Txid) -> Result<()> {
        self.remove_transaction_for_tracking(instance_id, tx_id)
    }

    fn get_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>> {
        self.get_instances_for_tracking()
    }

    fn get_instance_news(&self) -> Result<Vec<(InstanceId, Vec<TransactionStatus>)>> {
        self.get_instance_news()
    }

    fn acknowledge_instance_tx_news(&self, instance_id: InstanceId, tx_id: &Txid) -> Result<()> {
        self.acknowledge_instance_tx_news(instance_id, tx_id)
    }

    fn get_instance_tx_status(
        &self,
        instance_id: InstanceId,
        tx_id: &Txid,
    ) -> Result<Option<TransactionStatus>> {
        self.get_instance_tx_status(instance_id, tx_id)
    }

    fn is_ready(&mut self) -> Result<bool> {
        let current_height = self.get_current_height();
        let blockchain_height = self.indexer.bitcoin_client.get_best_block()?;
        info!("Monitor is ready? {}", current_height == blockchain_height);
        Ok(current_height == blockchain_height)
    }

    fn get_confirmation_threshold(&self) -> u32 {
        self.confirmation_threshold
    }

    fn save_address_for_tracking(&self, address: Address) -> Result<()> {
        self.bitvmx_store.save_address(address)
    }

    fn get_address_news(&self) -> Result<Vec<(Address, Vec<AddressStatus>)>> {
        self.bitvmx_store.get_address_news()
    }

    fn acknowledge_address_news(&self, address: Address) -> Result<()> {
        self.bitvmx_store.acknowledge_address_news(address)
    }
}

impl<I, B> Monitor<I, B>
where
    I: IndexerApi,
    B: BitvmxApi,
{
    pub fn new(
        indexer: I,
        bitvmx_store: B,
        current_height: Option<BlockHeight>,
        confirmation_threshold: u32,
    ) -> Self {
        let current_height = current_height.unwrap_or(0);

        Self {
            indexer,
            bitvmx_store,
            current_height,
            confirmation_threshold,
        }
    }

    pub fn save_instances_for_tracking(&self, instances: Vec<BitvmxInstance>) -> Result<()> {
        self.bitvmx_store.save_instances(&instances)?;

        Ok(())
    }

    pub fn save_transaction_for_tracking(
        &self,
        instance_id: InstanceId,
        tx_id: Txid,
    ) -> Result<()> {
        self.bitvmx_store.save_transaction(instance_id, &tx_id)?;
        Ok(())
    }

    fn remove_transaction_for_tracking(&self, instance_id: InstanceId, tx_id: Txid) -> Result<()> {
        self.bitvmx_store.remove_transaction(instance_id, &tx_id)
    }

    pub fn get_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>> {
        self.bitvmx_store.get_all_instances_for_tracking()
    }

    pub fn get_current_height(&self) -> BlockHeight {
        self.current_height
    }

    pub fn tick(&mut self) -> Result<()> {
        let new_height = self.indexer.tick(&self.current_height)?;

        let best_block = self
            .indexer
            .get_best_block()
            .context("Failed to retrieve current block")?;

        if best_block.is_none() {
            return Ok(());
        }

        let best_full_block = best_block.unwrap();

        // Get operations that have already started
        let instances = self
            .bitvmx_store
            .get_instances_ready_to_track(best_full_block.height)
            .context("Failed to retrieve operations")?;

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

        self.current_height = new_height;

        self.detect_addresses_in_transactions(best_full_block)
            .context("Failed to detect addresses in transactions")?;

        Ok(())
    }

    fn detect_addresses_in_transactions(&self, full_block: FullBlock) -> Result<()> {
        let addresses = self
            .bitvmx_store
            .get_addresses()
            .context("Failed to get addresses")?;

        for address in addresses {
            for tx in full_block.txs.iter() {
                let matched_with_the_address = self.address_exist_in_output(address.clone(), tx);

                if matched_with_the_address {
                    let confirmations = self.current_height - full_block.height + 1;

                    self.bitvmx_store
                        .update_address_news(
                            address.clone(),
                            tx,
                            full_block.height,
                            full_block.hash,
                            full_block.orphan,
                            confirmations,
                        )
                        .context(format!(
                            "Failed to save transaction for address {}",
                            address
                        ))?;
                }
            }
        }

        Ok(())
    }

    pub fn address_exist_in_output(&self, address: Address, tx: &Transaction) -> bool {
        //TODO: Bitcoin Network is hardcoded here, we need to use the network from configuration

        // Iterate through outputs to find the address
        for output in tx.output.iter() {
            if let Ok(output_address) =
                bitcoin::Address::from_script(&output.script_pubkey, Network::Bitcoin)
            {
                if output_address == address {
                    return true;
                }
            }
        }

        false
    }

    pub fn get_instance_news(&self) -> Result<Vec<(InstanceId, Vec<TransactionStatus>)>> {
        let instances = self.bitvmx_store.get_instance_news()?;

        let mut news = Vec::new();

        for (instance_id, txs) in instances {
            let mut tx_responses = Vec::new();

            for tx_id in txs {
                if let Ok(Some(status)) = self.get_instance_tx_status(instance_id, &tx_id) {
                    tx_responses.push(status);
                } else {
                    anyhow::bail!("Failed to get transaction status");
                }
            }

            if !tx_responses.is_empty() {
                news.push((instance_id, tx_responses));
            }
        }
        Ok(news)
    }

    pub fn acknowledge_instance_tx_news(
        &self,
        instance_id: InstanceId,
        tx_id: &Txid,
    ) -> Result<()> {
        self.bitvmx_store
            .acknowledge_instance_tx_news(instance_id, tx_id)?;
        Ok(())
    }

    pub fn get_instance_tx_status(
        &self,
        instance_id: InstanceId,
        tx_id: &Txid,
    ) -> Result<Option<TransactionStatus>> {
        let tx_status = self.indexer.get_tx(tx_id).context(format!(
            "Failed to get transaction status for tx_id {} in instance {}",
            tx_id, instance_id
        ))?;

        let tx_status_response = tx_status.map(|tx_status| TransactionStatus {
            tx_id: tx_status.tx.compute_txid(),
            tx: Some(tx_status.tx),
            block_info: Some(BlockInfo {
                block_height: tx_status.block_height,
                block_hash: tx_status.block_hash,
                is_orphan: tx_status.orphan,
            }),
            confirmations: tx_status.confirmations,
        });

        Ok(tx_status_response)
    }
}

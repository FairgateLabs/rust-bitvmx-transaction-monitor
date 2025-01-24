use std::rc::Rc;
use std::str::FromStr;

use crate::errors::MonitorError;
use crate::store::{MonitorStore, MonitorStoreApi};
use crate::types::{
    AddressStatus, BitvmxInstance, BlockInfo, InstanceData, InstanceId, TransactionStatus,
    TransactionStore,
};
use bitcoin::hex::DisplayHex;
use bitcoin::script::Instruction;
use bitcoin::{Address, Network, Script, Transaction, Txid};
use bitcoin_indexer::indexer::IndexerApi;
use bitcoin_indexer::store::IndexerStore;
use bitcoin_indexer::{helper::define_height_to_sync, indexer::Indexer};
use bitvmx_bitcoin_rpc::bitcoin_client::{BitcoinClient, BitcoinClientApi};
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use bitvmx_bitcoin_rpc::types::{BlockHeight, FullBlock};
use mockall::automock;
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
    /// - `Ok(bool)`: Returns true if the monitor is ready and synced, false otherwise.
    /// - `Err`: If there was an error checking the sync status.
    fn is_ready(&self) -> Result<bool, MonitorError>;

    /// Processes one tick of the monitor's operation.
    ///
    /// This method monitors transaction statuses for stored instances and checks for confirmations.
    /// It also triggers the indexer to continue syncing if not yet synchronized with the blockchain.
    ///
    /// # Returns
    /// - `Ok(())`: If the tick operation completed successfully.
    /// - `Err`: If there was an error during processing.
    fn tick(&self) -> Result<(), MonitorError>;

    /// Gets the current block height that the monitor has processed.
    ///
    /// # Returns
    /// The current block height as a `BlockHeight`.
    fn get_current_height(&self) -> Result<BlockHeight, MonitorError>;

    /// Saves multiple instances for monitoring.
    ///
    /// # Arguments
    /// * `instances` - Vector of instance data to be tracked
    ///
    /// # Returns
    /// - `Ok(())`: If instances were saved successfully.
    /// - `Err`: If there was an error saving the instances.
    fn save_instances_for_tracking(&self, instances: Vec<InstanceData>)
        -> Result<(), MonitorError>;

    /// Saves a single transaction to be monitored for a specific instance.
    ///
    /// # Arguments
    /// * `instance_id` - ID of the instance the transaction belongs to
    /// * `tx_id` - Transaction ID to monitor
    ///
    /// # Returns
    /// - `Ok(())`: If the transaction was saved successfully.
    /// - `Err`: If there was an error saving the transaction.
    fn save_transaction_for_tracking(
        &self,
        instance_id: InstanceId,
        tx_id: Txid,
    ) -> Result<(), MonitorError>;

    /// Removes a transaction from being monitored for a specific instance.
    ///
    /// # Arguments
    /// * `instance_id` - ID of the instance the transaction belongs to
    /// * `tx_id` - Transaction ID to stop monitoring
    ///
    /// # Returns
    /// - `Ok(())`: If the transaction was removed successfully.
    /// - `Err`: If there was an error removing the transaction.
    fn remove_transaction_for_tracking(
        &self,
        instance_id: InstanceId,
        tx_id: Txid,
    ) -> Result<(), MonitorError>;

    /// Gets all instances currently being tracked.
    ///
    /// # Returns
    /// - `Ok(Vec<BitvmxInstance>)`: Vector of all tracked instances.
    /// - `Err`: If there was an error retrieving the instances.
    fn get_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>, MonitorError>;

    /// Saves an address to be monitored for transactions.
    ///
    /// # Arguments
    /// * `address` - Bitcoin address to monitor
    ///
    /// # Returns
    /// - `Ok(())`: If the address was saved successfully.
    /// - `Err`: If there was an error saving the address.
    fn save_address_for_tracking(&self, address: Address) -> Result<(), MonitorError>;

    /// Gets status updates for transactions belonging to monitored instances.
    ///
    /// # Returns
    /// - `Ok(Vec<(InstanceId, Vec<TransactionStatus>)>)`: Vector of tuples containing:
    ///   - `InstanceId`: The BitVMX instance ID
    ///   - `Vec<TransactionStatus>`: Vector of status updates for the instance's transactions
    /// - `Err`: If there was an error retrieving the updates.
    fn get_instance_news(&self) -> Result<Vec<(InstanceId, Vec<TransactionStatus>)>, MonitorError>;

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
    fn acknowledge_instance_tx_news(
        &self,
        instance_id: InstanceId,
        tx_id: &Txid,
    ) -> Result<(), MonitorError>;

    /// Gets the current status of a specific transaction.
    ///
    /// # Arguments
    /// * `tx_id` - Transaction ID to check
    ///
    /// # Returns
    /// - `Ok(Option<TransactionStatus>)`: The transaction's status if found.
    /// - `Err`: If there was an error retrieving the status.
    fn get_instance_tx_status(
        &self,
        tx_id: &Txid,
    ) -> Result<Option<TransactionStatus>, MonitorError>;

    /// Gets status updates for monitored addresses.
    ///
    /// # Returns
    /// - `Ok(Vec<(Address, Vec<AddressStatus>)>)`: Vector of address/status pairs.
    /// - `Err`: If there was an error retrieving the updates.
    fn get_address_news(&self) -> Result<Vec<(Address, Vec<AddressStatus>)>, MonitorError>;

    /// Acknowledges that an address status update has been processed.
    ///
    /// # Arguments
    /// * `address` - The address whose updates were processed
    ///
    /// # Returns
    /// - `Ok(())`: If the acknowledgment was successful.
    /// - `Err`: If there was an error processing the acknowledgment.
    fn acknowledge_address_news(&self, address: Address) -> Result<(), MonitorError>;

    /// Gets the configured confirmation threshold that determines when a transaction is considered final.
    /// This threshold represents the minimum number of blocks that must be mined on top of the block
    /// containing the transaction before it is treated as irreversible.
    ///
    /// # Returns
    /// The confirmation threshold as a u32.
    fn get_confirmation_threshold(&self) -> u32;
}

impl MonitorApi for Monitor<Indexer<BitcoinClient, IndexerStore>, MonitorStore> {
    fn tick(&self) -> Result<(), MonitorError> {
        self.tick()
    }

    fn get_current_height(&self) -> Result<BlockHeight, MonitorError> {
        self.get_current_height()
    }

    fn save_instances_for_tracking(
        &self,
        instances: Vec<InstanceData>,
    ) -> Result<(), MonitorError> {
        let current_height = self.get_current_height()?;

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
                    start_height: current_height,
                }
            })
            .collect();

        self.save_instances_for_tracking(bitvmx_instances)
    }
    fn save_transaction_for_tracking(
        &self,
        instance_id: InstanceId,
        tx_id: Txid,
    ) -> Result<(), MonitorError> {
        self.save_transaction_for_tracking(instance_id, tx_id)
    }

    fn remove_transaction_for_tracking(
        &self,
        instance_id: InstanceId,
        tx_id: Txid,
    ) -> Result<(), MonitorError> {
        self.remove_transaction_for_tracking(instance_id, tx_id)
    }

    fn get_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>, MonitorError> {
        self.get_instances_for_tracking()
    }

    fn get_instance_news(&self) -> Result<Vec<(InstanceId, Vec<TransactionStatus>)>, MonitorError> {
        self.get_instance_news()
    }

    fn acknowledge_instance_tx_news(
        &self,
        instance_id: InstanceId,
        tx_id: &Txid,
    ) -> Result<(), MonitorError> {
        self.acknowledge_instance_tx_news(instance_id, tx_id)
    }

    fn get_instance_tx_status(
        &self,
        tx_id: &Txid,
    ) -> Result<Option<TransactionStatus>, MonitorError> {
        self.get_instance_tx_status(tx_id)
    }

    fn is_ready(&self) -> Result<bool, MonitorError> {
        let current_height = self.get_current_height()?;
        let blockchain_height = self.indexer.bitcoin_client.get_best_block()?;
        info!("Monitor is ready? {}", current_height == blockchain_height);
        Ok(current_height == blockchain_height)
    }

    fn get_confirmation_threshold(&self) -> u32 {
        self.confirmation_threshold
    }

    fn save_address_for_tracking(&self, address: Address) -> Result<(), MonitorError> {
        self.bitvmx_store.save_address(address)?;
        Ok(())
    }

    fn get_address_news(&self) -> Result<Vec<(Address, Vec<AddressStatus>)>, MonitorError> {
        let address_news = self.bitvmx_store.get_address_news()?;
        Ok(address_news)
    }

    fn acknowledge_address_news(&self, address: Address) -> Result<(), MonitorError> {
        self.bitvmx_store.acknowledge_address_news(address)?;
        Ok(())
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

    pub fn save_instances_for_tracking(
        &self,
        instances: Vec<BitvmxInstance>,
    ) -> Result<(), MonitorError> {
        self.bitvmx_store.save_instances(&instances)?;

        Ok(())
    }

    pub fn save_transaction_for_tracking(
        &self,
        instance_id: InstanceId,
        tx_id: Txid,
    ) -> Result<(), MonitorError> {
        self.bitvmx_store.save_transaction(instance_id, &tx_id)?;
        Ok(())
    }

    fn remove_transaction_for_tracking(
        &self,
        instance_id: InstanceId,
        tx_id: Txid,
    ) -> Result<(), MonitorError> {
        self.bitvmx_store.remove_transaction(instance_id, &tx_id)?;
        Ok(())
    }

    pub fn get_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>, MonitorError> {
        let instances = self.bitvmx_store.get_all_instances_for_tracking()?;
        Ok(instances)
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

        self.detect_addresses_in_transactions(best_full_block)?;

        Ok(())
    }

    fn detect_addresses_in_transactions(&self, full_block: FullBlock) -> Result<(), MonitorError> {
        let addresses = self.bitvmx_store.get_addresses()?;

        for address in addresses {
            for tx in full_block.txs.iter() {
                let matched_with_the_address = self.is_a_pegin_tx(address.clone(), tx);

                if matched_with_the_address {
                    let confirmations = self.get_current_height()? - full_block.height + 1;

                    self.bitvmx_store.update_address_news(
                        address.clone(),
                        tx,
                        full_block.height,
                        full_block.hash,
                        full_block.orphan,
                        confirmations,
                    )?;
                }
            }
        }

        Ok(())
    }

    pub fn extract_output_data(script: &Script) -> Vec<Vec<u8>> {
        // Iterate over script instructions to find pushed data
        let instructions = script.instructions_minimal();
        let mut result = Vec::new();

        for inst in instructions.flatten() {
            if let Instruction::PushBytes(data) = inst {
                result.push(data.as_bytes().to_vec());
            }
        }

        result
    }

    /// Validates the OP_RETURN data to ensure it contains 4 fields and starts with "RSK_PEGIN".
    pub fn is_valid_op_return_data(data: Vec<Vec<u8>>) -> bool {
        // Expected OP_RETURN format: "RSK_PEGIN N A R"
        if data.len() != 4 {
            return false;
        }

        // First part should be "RSK_PEGIN"
        let first_part = String::from_utf8_lossy(&data[0]);
        if first_part != "RSK_PEGIN" {
            return false;
        }
       
        // Second part should be a number for the packet number
        if data[1].len() != 8 {
            return false;
        }

        //TODO: validate packet number

        // Third part should be RSK address
        let third_part = data[2].as_hex().to_string();
        if !Self::is_valid_rsk_address(&third_part) {
            return false;
        }

        // Fourth part should be Bitcoin address
        let fourth_part = String::from_utf8_lossy(&data[3]);
        if Address::from_str(&fourth_part).is_err() {
            return false;
        }

        return true;
    }

    pub fn is_valid_rsk_address(address: &str) -> bool {
            address.len() == 40 &&
            address.chars().all(|c| c.is_ascii_hexdigit())
    }

    /// Validates if a transaction is a valid peg-in transaction by checking:
    /// 1. The first output matches the given committee address (N)
    /// 2. The second output is a valid OP_RETURN containing:
    ///    - "RSK_PEGIN" identifier
    ///    - Packet number
    ///    - RSK destination address
    ///    - Bitcoin reimbursement address (R)
    pub fn is_a_pegin_tx(&self, address: Address, tx: &Transaction) -> bool {
        // Ensure at least 2 outputs exist
        if tx.output.len() < 2 {
            return false;
        }

        // Check the first output for the matching address
        let mut first_output_match = false;

        if let Some(first_output) = tx.output.first() {
            //TODO: get Network::Bitcoin from configuration.
            if let Ok(output_address) =
                Address::from_script(&first_output.script_pubkey, Network::Bitcoin)
            {
                if output_address == address {
                    first_output_match = true;
                }
            }
        }

        if !first_output_match {
            return false;
        }

        // Check the second output for the OP_RETURN structure
        if let Some(op_return_output) = tx.output.get(1) {
            if op_return_output.script_pubkey.is_op_return() {
                let data = Self::extract_output_data(&op_return_output.script_pubkey);

                if Self::is_valid_op_return_data(data) {
                    return true; // OP_RETURN has valid format
                }
            }
        }

        false
    }

    pub fn get_instance_news(
        &self,
    ) -> Result<Vec<(InstanceId, Vec<TransactionStatus>)>, MonitorError> {
        let instances = self.bitvmx_store.get_instance_news()?;

        let mut news = Vec::new();

        for (instance_id, txs) in instances {
            let mut tx_responses = Vec::new();

            for tx_id in txs {
                if let Ok(Some(status)) = self.get_instance_tx_status(&tx_id) {
                    tx_responses.push(status);
                } else {
                    return Err(MonitorError::UnexpectedError(format!(
                        "Transaction not found: {}",
                        tx_id
                    )));
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
    ) -> Result<(), MonitorError> {
        self.bitvmx_store
            .acknowledge_instance_tx_news(instance_id, tx_id)?;
        Ok(())
    }

    pub fn get_instance_tx_status(
        &self,
        tx_id: &Txid,
    ) -> Result<Option<TransactionStatus>, MonitorError> {
        let tx_status = self.indexer.get_tx(tx_id)?;

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

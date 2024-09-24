use crate::bitvmx_store::{BitvmxApi, BitvmxStore};
use crate::types::{BitvmxInstance, InstanceId, TxStatus};
use anyhow::{Context, Ok, Result};
use bitcoin::Txid;
use bitcoin_indexer::{
    bitcoin_client::{BitcoinClient, BitcoinClientApi},
    helper::define_height_to_sync,
    indexer::Indexer,
    store::Store,
};
use bitcoin_indexer::{indexer::IndexerApi, types::BlockHeight};
use log::info;
pub struct Monitor<I, B>
where
    I: IndexerApi,
    B: BitvmxApi,
{
    pub indexer: I,
    pub bitvmx_store: B,
    current_height: BlockHeight,
}

impl Monitor<Indexer<BitcoinClient, Store>, BitvmxStore> {
    pub fn new_with_paths(
        node_rpc_url: &str,
        db_file_path: &str,
        checkpoint: Option<BlockHeight>,
    ) -> Result<Self> {
        let bitcoin_client = BitcoinClient::new(node_rpc_url)?;
        let blockchain_height = bitcoin_client.get_best_block()? as BlockHeight;
        let indexer = Indexer::new_with_path(bitcoin_client, db_file_path)?;
        let indexed_height = indexer.get_best_block()?;
        let bitvmx_store = BitvmxStore::new_with_path(db_file_path)?;
        let current_height = define_height_to_sync(checkpoint, blockchain_height, indexed_height)?;
        let monitor = Monitor::new(indexer, bitvmx_store, Some(current_height));

        Ok(monitor)
    }
}

pub trait MonitorApi {
    fn detect_instances(&mut self) -> Result<()>;
    fn get_current_height(&self) -> BlockHeight;
    fn save_instances_for_tracking(&self, instances: Vec<BitvmxInstance>) -> Result<()>;
    fn save_transaction_for_tracking(&self, instance_id: InstanceId, tx_id: Txid) -> Result<()>;
    fn get_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>>;

    /// Notifies about changes in the status of every transaction (tx) that belongs
    /// to a BitVMX instance.
    ///
    /// # Returns
    /// - `Ok(Vec<(InstanceId, TxStatus>)>`: A vector of tuples where each tuple contains:
    ///   - `InstanceId`: The Bitvmx instance id
    ///   - `TxStatus`: The current status of the transaction.
    fn get_instance_news(&self) -> Result<Vec<(InstanceId, Vec<Txid>)>>;

    /// Acknowledges or marks a intance id as processed, effectively
    /// removing it from the list of pending changes.
    fn acknowledge_instance_news(&self, instance_id: InstanceId) -> Result<()>;

    fn get_tx_status(&self, instance_id: InstanceId, tx_id: Txid) -> Result<Option<TxStatus>>;
}

impl MonitorApi for Monitor<Indexer<BitcoinClient, Store>, BitvmxStore> {
    fn detect_instances(&mut self) -> Result<()> {
        self.detect_instances()
    }
    fn get_current_height(&self) -> BlockHeight {
        self.get_current_height()
    }
    fn save_instances_for_tracking(&self, instances: Vec<BitvmxInstance>) -> Result<()> {
        self.save_instances_for_tracking(instances)
    }
    fn save_transaction_for_tracking(&self, instance_id: InstanceId, tx_id: Txid) -> Result<()> {
        self.save_transaction_for_tracking(instance_id, tx_id)
    }
    fn get_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>> {
        self.get_instances_for_tracking()
    }

    fn get_instance_news(&self) -> Result<Vec<(InstanceId, Vec<Txid>)>> {
        self.get_instance_news()
    }

    fn acknowledge_instance_news(&self, instance_id: InstanceId) -> Result<()> {
        self.acknowledge_instance_news(instance_id)
    }

    fn get_tx_status(&self, instance_id: InstanceId, tx_id: Txid) -> Result<Option<TxStatus>> {
        self.get_tx_status(instance_id, tx_id)
    }
}

impl<I, B> Monitor<I, B>
where
    I: IndexerApi,
    B: BitvmxApi,
{
    pub fn new(indexer: I, bitvmx_store: B, current_height: Option<BlockHeight>) -> Self {
        let current_height = current_height.unwrap_or(0);

        Self {
            indexer,
            bitvmx_store,
            current_height,
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
        self.bitvmx_store.save_transaction(instance_id, tx_id)?;
        Ok(())
    }

    pub fn get_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>> {
        self.bitvmx_store.get_all_instances_for_tracking()
    }

    pub fn get_current_height(&self) -> BlockHeight {
        self.current_height
    }

    pub fn detect_instances(&mut self) -> Result<()> {
        let new_height = self.indexer.index_height(&self.current_height)?;

        //Get current block from Bitcoin Indexer
        let current_height = self
            .indexer
            .get_best_block()
            .context("Failed to retrieve current block")?;

        if current_height.is_none() {
            return Ok(());
        }

        let current_height = current_height.unwrap();

        // Get operations that have already started
        let instances = self
            .bitvmx_store
            .get_instances_ready_to_track(current_height)
            .context("Failed to retrieve operations")?;

        // Count existing operations get all thansaction that meet next rules:

        for instance in instances {
            for tx in instance.txs {
                if tx.tx_was_seen && tx.confirmations >= 6 {
                    continue;
                }
                // Tx exist means was found
                let tx_exists_height = self.indexer.tx_exists(&tx.tx_id)?;

                if tx_exists_height.0 {
                    if tx.tx_was_seen && current_height > tx.height_tx_seen.unwrap() {
                        self.bitvmx_store.update_instance_tx_confirmations(
                            instance.id,
                            &tx.tx_id,
                            current_height,
                        )?;

                        info!(
                            "Update confirmation for bitvmx intance: {} | tx_id: {} | at height: {}",
                            instance.id,
                            tx.tx_id,
                            current_height
                        );

                        continue;
                    }

                    if !tx.tx_was_seen {
                        let tx_hex = self.indexer.get_tx(&tx.tx_id)?;

                        self.bitvmx_store.update_instance_tx_seen(
                            instance.id,
                            &tx.tx_id,
                            tx_exists_height.1.unwrap(),
                            &tx_hex,
                        )?;

                        info!(
                            "Found bitvmx intance: {} | tx_id: {} | at height: {}",
                            instance.id, tx.tx_id, current_height
                        );
                    }
                }
            }
        }

        self.current_height = new_height;

        Ok(())
    }

    pub fn get_instance_news(&self) -> Result<Vec<(InstanceId, Vec<Txid>)>> {
        let instances = self.bitvmx_store.get_instance_news()?;

        Ok(instances)
    }

    pub fn acknowledge_instance_news(&self, instance_id: InstanceId) -> Result<()> {
        self.bitvmx_store.acknowledge_instance_news(instance_id)?;
        Ok(())
    }

    pub fn get_tx_status(&self, instance_id: InstanceId, tx_id: Txid) -> Result<Option<TxStatus>> {
        let tx_status = self.bitvmx_store.get_tx_status(instance_id, &tx_id)?;

        Ok(tx_status)
    }
}

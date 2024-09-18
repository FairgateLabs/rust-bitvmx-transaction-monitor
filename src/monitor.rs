use crate::bitvmx_store::{BitvmxApi, BitvmxStore};
use crate::types::BitvmxInstance;
use anyhow::{Context, Ok, Result};
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

    pub fn get_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>> {
        self.bitvmx_store.get_instances_for_tracking()
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
            .get_pending_instances(current_height)
            .context("Failed to retrieve operations")?;

        // Count existing operations get all thansaction that meet next rules:

        for instance in instances {
            for tx in instance.txs {
                if tx.tx_was_seen && tx.confirmations >= 6 {
                    continue;
                }
                // Tx exist means was found
                let tx_exists_height = self.indexer.tx_exists(&tx.txid)?;

                if tx_exists_height.0 {
                    if tx.tx_was_seen && current_height > tx.height_tx_seen.unwrap() {
                        self.bitvmx_store.update_instance_tx_confirmations(
                            instance.id,
                            &tx.txid,
                            current_height,
                        )?;

                        info!(
                            "Update confirmation for bitvmx intance: {} | tx_id: {} | at height: {}",
                            instance.id,
                            tx.txid,
                            current_height
                        );

                        continue;
                    }

                    if !tx.tx_was_seen {
                        let tx_hex = self.indexer.get_tx(&tx.txid)?;

                        self.bitvmx_store.update_instance_tx_seen(
                            instance.id,
                            &tx.txid,
                            tx_exists_height.1.unwrap(),
                            &tx_hex,
                        )?;

                        info!(
                            "Found bitvmx intance: {} | tx_id: {} | at height: {}",
                            instance.id, tx.txid, current_height
                        );
                    }
                }
            }
        }

        self.current_height = new_height;

        Ok(())
    }
}

use crate::bitvmx_store::BitvmxApi;
use anyhow::{Context, Ok, Result};
use bitcoin_indexer::{indexer::IndexerApi, types::BlockHeight};
use log::info;

pub struct Monitor<I, B>
where
    I: IndexerApi,
    B: BitvmxApi,
{
    pub indexer: I,
    pub bitvmx_store: B,
}

impl<I, B> Monitor<I, B>
where
    I: IndexerApi,
    B: BitvmxApi,
{
    pub fn new(indexer: I, bitvmx_store: B) -> Self {
        Self {
            indexer,
            bitvmx_store,
        }
    }

    pub fn detect_instances_at_height(&self, height_to_sync: BlockHeight) -> Result<BlockHeight> {
        let new_height = self.indexer.index_height(&height_to_sync)?;

        //Get current block from Bitcoin Indexer
        let current_height = self
            .indexer
            .get_best_block()
            .context("Failed to retrieve current block")?;

        if current_height.is_none() {
            return Ok(new_height);
        }

        let current_height = current_height.unwrap();

        // Get operations that have already started
        let operations = self
            .bitvmx_store
            .get_pending_bitvmx_instances(current_height)
            .context("Failed to retrieve operations")?;

        // Count existing operations get all thansaction that meet next rules:

        for instance in operations {
            assert!(
                !instance.finished,
                "Error double checking finished instance"
            );

            for tx in instance.txs {
                if tx.tx_was_seen && tx.confirmations >= 6 {
                    continue;
                }
                // Tx exist means was found
                let tx_exists_height = self.indexer.tx_exists(&tx.txid)?;

                if tx_exists_height.0 {
                    if tx.tx_was_seen && current_height > tx.height_tx_seen.unwrap() {
                        self.bitvmx_store.update_bitvmx_tx_confirmations(
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

                        self.bitvmx_store.update_bitvmx_tx_seen(
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

        Ok(new_height)
    }
}

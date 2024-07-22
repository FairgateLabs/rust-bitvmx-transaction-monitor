use crate::stores::{bitcoin_store::BitcoinApi, bitvmx_store::BitvmxApi};
use anyhow::{Context, Ok, Result};
use log::info;

#[derive(Default)]
pub struct Monitor<B: BitcoinApi, V: BitvmxApi> {
    pub bitcoin_store: B,
    pub bitvmx_store: V,
    pub is_running: bool,
}

impl<B: BitcoinApi, V: BitvmxApi> Monitor<B, V> {
    pub fn run(&mut self) -> Result<()> {
        //Get current block from Bitcoin Indexer
        let current_height = self
            .bitcoin_store
            .get_block_count()
            .context("Failed to retrieve current block")?;

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
                if tx.tx_was_seen && tx.confirmations > 6 {
                    continue;
                }
                // Tx exist means was found
                let tx_exists = self.bitcoin_store.tx_exists(&tx.txid)?;

                if tx_exists {
                    if tx.tx_was_seen && current_height > tx.fist_height_tx_seen.unwrap() {
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
                        self.bitvmx_store.update_bitvmx_tx_seen(
                            instance.id,
                            &tx.txid,
                            current_height,
                        )?;

                        info!(
                            "Found bitvmx intance: {} | tx_id: {} | at height: {}",
                            instance.id, tx.txid, current_height
                        );
                    }
                }
            }
        }

        Ok(())
    }
}

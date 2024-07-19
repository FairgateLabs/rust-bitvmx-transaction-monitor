use crate::stores::{bitcoin_store::BitcoinApi, bitvmx_store::BitvmxApi};
use anyhow::{Context, Result};
use log::{info, trace};

pub struct Monitor<B: BitcoinApi, V: BitvmxApi> {
    pub bitcoin_store: B,
    pub bitvmx_store: V,
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        stores::{bitcoin_store::MockBitcoinStore, bitvmx_store::MockBitvmxStore},
        types::{BitvmxInstance, BitvmxTxData},
    };
    use mockall::predicate::*;

    #[test]
    fn no_instances() -> Result<(), anyhow::Error> {
        let mut mock_bitcoin_store = MockBitcoinStore::new();
        let mut mock_bitvmx_store = MockBitvmxStore::new();

        let block_100 = 100;

        mock_bitcoin_store
            .expect_get_block_count()
            .returning(move || Ok(block_100));

        // Return an empty bitvmx array
        mock_bitvmx_store
            .expect_get_pending_bitvmx_instances()
            .with(eq(block_100))
            .times(1)
            .returning(|_| Ok(vec![]));

        // Then we never call update_bitvmx_tx_confirmations
        mock_bitvmx_store
            .expect_update_bitvmx_tx_confirmations()
            .times(0);

        // Then we never call update_bitvmx_tx_seen
        mock_bitvmx_store.expect_update_bitvmx_tx_seen().times(0);

        let mut monitor = Monitor {
            bitcoin_store: mock_bitcoin_store,
            bitvmx_store: mock_bitvmx_store,
        };

        monitor.run()?;

        Ok(())
    }

    #[test]
    fn instance_tx_detected() -> Result<(), anyhow::Error> {
        let mut mock_bitcoin_store = MockBitcoinStore::new();
        let mut mock_bitvmx_store = MockBitvmxStore::new();

        let block_200 = 200;
        let intance_id = 2;
        let tx_to_seen = "3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f";

        let instances = vec![BitvmxInstance {
            id: intance_id,
            txs: vec![
                BitvmxTxData {
                    txid: "e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b"
                        .to_string(),
                    tx_was_seen: true,
                    fist_height_tx_seen: Some(190),
                    confirmations: 10,
                },
                BitvmxTxData {
                    txid: tx_to_seen.to_string().clone(),
                    tx_was_seen: false,
                    fist_height_tx_seen: None,
                    confirmations: 0,
                },
            ],
            start_height: 180,
            finished: false,
        }];

        mock_bitcoin_store
            .expect_get_block_count()
            .returning(move || Ok(block_200));

        mock_bitvmx_store
            .expect_get_pending_bitvmx_instances()
            .with(eq(block_200))
            .times(1)
            .returning(move |_| Ok(instances.clone()));

        // Tx was found by the indexer and is already in the blockchain.
        mock_bitcoin_store
            .expect_tx_exists()
            .with(eq(tx_to_seen.to_string().clone()))
            .times(1)
            .returning(|_| Ok(true));

        // The first time was seen the tx should not call update_bitvmx_tx_confirmations
        mock_bitvmx_store
            .expect_update_bitvmx_tx_confirmations()
            .times(0);

        // Then call update_bitvmx_tx_seen for the first time
        mock_bitvmx_store
            .expect_update_bitvmx_tx_seen()
            .with(eq(intance_id), eq(tx_to_seen.to_string()), eq(block_200))
            .times(1)
            .returning(|_, _, _| Ok(()));

        let mut monitor = Monitor {
            bitcoin_store: mock_bitcoin_store,
            bitvmx_store: mock_bitvmx_store,
        };

        monitor.run()?;

        Ok(())
    }

    #[test]
    fn instance_tx_already_detected_increase_confirmation() -> Result<(), anyhow::Error> {
        let mut mock_bitcoin_store = MockBitcoinStore::new();
        let mut mock_bitvmx_store = MockBitvmxStore::new();

        let block_201 = 200;
        let intance_id = 2;
        let tx_to_seen = "3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f";
        let confirmations = 1;
        let instances = vec![BitvmxInstance {
            id: intance_id,
            txs: vec![BitvmxTxData {
                txid: tx_to_seen.to_string().clone(),
                tx_was_seen: true,
                fist_height_tx_seen: Some(200),
                confirmations,
            }],
            start_height: 180,
            finished: false,
        }];

        mock_bitcoin_store
            .expect_get_block_count()
            .returning(move || Ok(block_201));

        mock_bitvmx_store
            .expect_get_pending_bitvmx_instances()
            .with(eq(block_201))
            .times(1)
            .returning(move |_| Ok(instances.clone()));

        // Tx was found by the indexer and is already in the blockchain.
        mock_bitcoin_store
            .expect_tx_exists()
            .with(eq(tx_to_seen.to_string().clone()))
            .times(1)
            .returning(|_| Ok(true));

        // Do no Increase confirmations given the block is the same were was found
        mock_bitvmx_store
            .expect_update_bitvmx_tx_confirmations()
            .times(0);

        // Also the update_bitvmx_tx_seen is not call
        mock_bitvmx_store.expect_update_bitvmx_tx_seen().times(0);

        let mut monitor = Monitor {
            bitcoin_store: mock_bitcoin_store,
            bitvmx_store: mock_bitvmx_store,
        };

        monitor.run()?;

        Ok(())
    }
}

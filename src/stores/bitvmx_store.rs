use crate::types::BitvmxInstance;
use anyhow::Context;
use anyhow::Result;
use log::warn;
use mockall::automock;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};

pub struct BitvmxStore {
    file_path: String,
}

pub trait BitvmxApi {
    /// Return pending bitvmx instances
    fn get_pending_bitvmx_instances(&mut self, current_height: u32) -> Result<Vec<BitvmxInstance>>;

    fn update_bitvmx_tx_seen(&mut self, id: u32, txid: &str, current_height: u32) -> Result<()>;

    fn update_bitvmx_tx_confirmations(
        &mut self,
        id: u32,
        txid: &str,
        current_height: u32,
    ) -> Result<()>;
}

impl BitvmxStore {
    pub fn new(file_path: &str) -> Result<Self> {
        Ok(Self {
            file_path: file_path.to_string(),
        })
    }

    pub fn get_data(&mut self) -> Result<Vec<BitvmxInstance>> {
        let mut file = File::open(&self.file_path).context("Error opening file")?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let bitvmx_instances: Vec<BitvmxInstance> =
            serde_json::from_str(&contents).context("Error deserializing data")?;

        Ok(bitvmx_instances)
    }

    fn write_data(&self, instances: Vec<BitvmxInstance>) -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true) // Truncate the file (clear existing content)
            .open(&self.file_path)
            .context("Error opening file")?;

        let json_data = serde_json::to_string_pretty(&instances)?;

        file.write_all(json_data.as_bytes())?;

        Ok(())
    }
}

#[automock]
impl BitvmxApi for BitvmxStore {
    fn get_pending_bitvmx_instances(&mut self, current_height: u32) -> Result<Vec<BitvmxInstance>> {
        // This method will return bitvmx instances excluding the onces are already seen and is finished

        let mut bitvmx_instances = self.get_data()?;

        bitvmx_instances.retain(|i| i.start_height <= current_height && !i.finished);

        Ok(bitvmx_instances)
    }

    fn update_bitvmx_tx_seen(&mut self, id: u32, txid: &str, current_height: u32) -> Result<()> {
        let mut bitvmx_instances = self.get_data()?;

        let mut found = false;

        for instance in bitvmx_instances.iter_mut() {
            if instance.id == id {
                for tx in instance.txs.iter_mut() {
                    if tx.txid == *txid {
                        if tx.tx_was_seen {
                            warn!("Txn already seen, looks this methods is being calling more than what should be")
                        }
                        tx.tx_was_seen = true;
                        tx.confirmations = 1;
                        tx.fist_height_tx_seen = Some(current_height);
                        found = true;
                        break;
                    }
                }
            }
        }

        if !found {
            warn!(
                "Txn for the bitvmx instance {} txid {} was not found",
                id, txid
            );
        }

        self.write_data(bitvmx_instances)?;

        Ok(())
    }

    fn update_bitvmx_tx_confirmations(
        &mut self,
        id: u32,
        txid: &str,
        current_height: u32,
    ) -> Result<()> {
        let mut bitvmx_instances = self.get_data()?;

        let mut found = false;

        for instance in bitvmx_instances.iter_mut() {
            if instance.id == id {
                let mut all_txs_confirm = true;
                for tx in instance.txs.iter_mut() {
                    all_txs_confirm = all_txs_confirm && tx.confirmations >= 6;

                    if tx.txid == *txid {
                        assert!(
                            tx.tx_was_seen,
                            "Txn already seen, looks this methods is being calling more than what should be"
                        );

                        assert!(
                            current_height >= tx.fist_height_tx_seen.unwrap(),
                            "Looks txn is been updated in a incorrect block"
                        );

                        tx.confirmations = current_height - tx.fist_height_tx_seen.unwrap();
                        found = true
                    }

                    if !all_txs_confirm {
                        break;
                    }
                }

                //Bitvmx instance is complete, means all txns were find and confirm.
                instance.finished = true;
            }
        }

        if !found {
            warn!(
                "Txn for the bitvmx instance {} txid {} was not found",
                id, txid
            );
        }

        self.write_data(bitvmx_instances)?;

        Ok(())
    }
}

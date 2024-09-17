use crate::types::{BitvmxInstance, BitvmxTxData};
use anyhow::{bail, Context, Ok, Result};
use bitcoin::Txid;
use log::warn;
use mockall::automock;
use std::path::PathBuf;
use storage_backend::storage::{KeyValueStore, Storage};

pub struct BitvmxStore {
    store: Storage,
}

pub trait BitvmxApi {
    /// Return pending bitvmx instances
    fn get_pending_instances(&self, current_height: u32) -> Result<Vec<BitvmxInstance>>;

    fn update_instance_tx_seen(
        &self,
        id: u32,
        txid: &Txid,
        current_height: u32,
        tx_hex: &str,
    ) -> Result<()>;

    fn update_instance_tx_confirmations(
        &self,
        id: u32,
        txid: &Txid,
        current_height: u32,
    ) -> Result<()>;

    /// Save a single bitvmx instance
    fn save_instance(&self, instance: &BitvmxInstance) -> Result<()>;

    /// Save a vector of bitvmx instances
    fn save_instances(&self, instances: &[BitvmxInstance]) -> Result<()>;
}

impl BitvmxStore {
    pub fn new_with_path(store_path: &str) -> Result<Self> {
        let store = Storage::new_with_path(&PathBuf::from(store_path))?;
        Ok(Self { store })
    }

    pub fn new(store: Storage) -> Result<Self> {
        Ok(Self { store })
    }

    pub fn get_instance(&self, id: u32) -> Result<Option<BitvmxInstance>> {
        let instance_key = format!("instance/{}", id);
        let instance = self
            .store
            .get::<&str, BitvmxInstance>(&instance_key)
            .context(format!(
                "There was an error getting an instance {}",
                instance_key
            ))
            .unwrap();

        Ok(instance)
    }

    pub fn get_instance_tx(&self, instance_id: u32, tx_id: &Txid) -> Result<Option<BitvmxTxData>> {
        let instance_tx_key = format!("instance/{}/tx/{}", instance_id, tx_id);
        let tx = self
            .store
            .get::<&str, BitvmxTxData>(&instance_tx_key)
            .context(format!("There was an error getting {}", instance_tx_key))
            .unwrap();

        Ok(tx)
    }

    pub fn save_instance_tx(&self, instance_id: u32, tx: &BitvmxTxData) -> Result<()> {
        let instance_tx_key = format!("instance/{}/tx/{}", instance_id, tx.txid);
        self.store
            .set::<&str, BitvmxTxData>(&instance_tx_key, tx.clone())
            .context(format!("There was an error getting {}", instance_tx_key))
            .unwrap();

        let instance_key = format!("instance/{}", instance_id);
        let instance = self
            .store
            .get::<&str, BitvmxInstance>(&instance_key)
            .context(format!("There was an error getting {}", instance_key))
            .unwrap();

        match instance {
            Some(mut _instance) =>
            // Find the index of the transaction you want to replace
            {
                if let Some(pos) = _instance
                    .txs
                    .iter()
                    .position(|tx_old| tx_old.txid == tx.txid)
                {
                    // Replace the old transaction with the new one
                    _instance.txs[pos] = tx.clone();

                    self.store.set(instance_key, _instance)?;
                }
            }
            None => bail!(
                "There was an error trying to save instance {}",
                instance_key
            ),
        }

        Ok(())
    }

    pub fn get_instances(&self) -> Result<Vec<BitvmxInstance>> {
        let mut instances = Vec::<BitvmxInstance>::new();

        let instances_key = "instance/list";
        let all_instance_ids = self
            .store
            .get::<_, Vec<u32>>(instances_key)?
            .unwrap_or_default();

        for id in all_instance_ids {
            let instance = self.get_instance(id)?;

            match instance {
                Some(inst) => instances.push(inst),
                None => bail!("There is an error trying to get instance"),
            }
        }

        Ok(instances)
    }
}

#[automock]
impl BitvmxApi for BitvmxStore {
    fn get_pending_instances(&self, current_height: u32) -> Result<Vec<BitvmxInstance>> {
        // This method will return bitvmx instances excluding the onces are already seen
        let mut bitvmx_instances = self.get_instances()?;
        bitvmx_instances.retain(|i| (i.start_height <= current_height));

        Ok(bitvmx_instances)
    }

    fn update_instance_tx_seen(
        &self,
        instance_id: u32,
        txid: &Txid,
        height_tx_was_seen: u32,
        tx_hex: &str,
    ) -> Result<()> {
        let tx_instance = self.get_instance_tx(instance_id, txid)?;

        match tx_instance {
            Some(mut tx) => {
                if tx.tx_was_seen {
                    warn!("Txn already seen, looks this methods is being calling more than what should be")
                }
                tx.tx_was_seen = true;
                tx.confirmations = 1;
                tx.height_tx_seen = Some(height_tx_was_seen);
                tx.tx_hex = Some(tx_hex.to_string());
                self.save_instance_tx(instance_id, &tx.clone())?;
            }
            None => warn!(
                "Txn for the bitvmx instance {} txid {} was not found",
                instance_id, txid
            ),
        }

        Ok(())
    }

    fn update_instance_tx_confirmations(
        &self,
        instance_id: u32,
        txid: &Txid,
        current_height: u32,
    ) -> Result<()> {
        let tx_instance = self.get_instance_tx(instance_id, txid)?;

        match tx_instance {
            Some(mut tx) => {
                tx.confirmations = current_height - tx.height_tx_seen.unwrap();
                self.save_instance_tx(instance_id, &tx)?;
            }
            None => warn!(
                "Txn for the bitvmx instance {} txid {} was not found",
                instance_id, txid
            ),
        }

        Ok(())
    }

    fn save_instances(&self, instances: &[BitvmxInstance]) -> Result<()> {
        for instance in instances {
            self.save_instance(instance)?;
        }
        Ok(())
    }

    fn save_instance(&self, instance: &BitvmxInstance) -> Result<()> {
        let instance_key = format!("instance/{}", instance.id);

        // Store the instance under its ID
        self.store.set(&instance_key, instance).context(format!(
            "Failed to store instance under key {}",
            instance_key
        ))?;

        // Index each transaction instance by its txid
        for tx in &instance.txs {
            let tx_key = format!("instance/{}/tx/{}", instance.id, tx.txid);
            self.store.set(&tx_key, tx).context(format!(
                "Failed to store txid {} under key {}",
                tx.txid, tx_key
            ))?;
        }

        // Maintain a list of all instances
        let instances_key = "instance/list";
        let mut all_instances = self
            .store
            .get::<_, Vec<u32>>(instances_key)
            .unwrap_or_default()
            .unwrap_or_default();

        if !all_instances.contains(&instance.id) {
            all_instances.push(instance.id);
            self.store
                .set(instances_key, &all_instances)
                .context("Failed to update instances list")?;
        }

        Ok(())
    }
}

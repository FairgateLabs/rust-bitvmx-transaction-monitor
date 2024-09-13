use crate::types::{BitvmxInstance, BitvmxTxData};
use anyhow::{bail, Context, Ok, Result};
use bitcoin::Txid;
use log::warn;
use mockall::automock;
use rust_bitvmx_storage_backend::storage::{KeyValueStore, Storage};
use std::path::PathBuf;

pub struct BitvmxStore {
    db: Storage,
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
    fn save_instances(&self, instances: Vec<BitvmxInstance>) -> Result<()>;
}

impl BitvmxStore {
    pub fn new(file_path: &str) -> Result<Self> {
        let store = Storage::new_with_path(&PathBuf::from(file_path))?;
        Ok(Self { db: store })
    }

    pub fn get_instance(&self, id: u32) -> Result<Option<BitvmxInstance>> {
        let instance_key = format!("instance/{}", id);
        let instance = self
            .db
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
            .db
            .get::<&str, BitvmxTxData>(&instance_tx_key)
            .context(format!("There was an error getting {}", instance_tx_key))
            .unwrap();

        Ok(tx)
    }

    pub fn save_instance_tx(&self, instance_id: u32, tx: &BitvmxTxData) -> Result<()> {
        let instance_tx_key = format!("instance/{}/tx/{}", instance_id, tx.txid);
        let tx = self
            .db
            .set::<&str, BitvmxTxData>(&instance_tx_key, tx.clone())
            .context(format!("There was an error getting {}", instance_tx_key))
            .unwrap();

        Ok(tx)
    }

    pub fn get_instances(&self) -> Result<Vec<BitvmxInstance>> {
        let mut instances = Vec::<BitvmxInstance>::new();

        let instances_key = "instances_list";
        let all_instance_ids = self
            .db
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
        current_height: u32,
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
                tx.height_tx_seen = Some(current_height);
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
        intance_id: u32,
        txid: &Txid,
        current_height: u32,
    ) -> Result<()> {
        let mut bitvmx_instances = self
            .db
            .get::<&str, Vec<BitvmxInstance>>("instances")?
            .unwrap();

        let mut found = false;

        for instance in bitvmx_instances.iter_mut() {
            if instance.id == intance_id {
                for tx in instance.txs.iter_mut() {
                    if tx.txid == *txid {
                        assert!(
                            tx.tx_was_seen,
                            "Txn already seen, looks this methods is being calling more than what should be"
                        );

                        assert!(
                            current_height >= tx.height_tx_seen.unwrap(),
                            "Looks txn is been updated in a incorrect block"
                        );

                        tx.confirmations = current_height - tx.height_tx_seen.unwrap();
                        found = true;
                    }
                }
            }
        }

        if !found {
            warn!(
                "Txn for the bitvmx instance {} txid {} was not found",
                intance_id, txid
            );

            return Ok(());
        }

        self.db.set("instances", bitvmx_instances)?;

        Ok(())
    }

    fn save_instances(&self, instances: Vec<BitvmxInstance>) -> Result<()> {
        for instance in instances {
            self.save_instance(&instance)?;
        }
        Ok(())
    }

    fn save_instance(&self, instance: &BitvmxInstance) -> Result<()> {
        let instance_key = format!("instance/{}", instance.id);

        // Store the instance under its ID
        self.db.set(&instance_key, &instance).context(format!(
            "Failed to store instance under key {}",
            instance_key
        ))?;

        // Index each transaction instance by its txid
        for tx in &instance.txs {
            let tx_key = format!("instance/{}/txid/{}", instance.id, tx.txid);
            self.db.set(&tx_key, &tx).context(format!(
                "Failed to store txid {} under key {}",
                tx.txid, tx_key
            ))?;
        }

        // Maintain a list of all instances
        let instances_key = "instance/list";
        let mut all_instances = self
            .db
            .get::<_, Vec<u32>>(instances_key)
            .unwrap_or_default()
            .unwrap_or_default();

        if !all_instances.contains(&instance.id) {
            all_instances.push(instance.id);
            self.db
                .set(instances_key, &all_instances)
                .context("Failed to update instances list")?;
        }

        Ok(())
    }
}

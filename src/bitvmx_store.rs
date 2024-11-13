use crate::types::{BitvmxInstance, BlockInfo, InstanceId, TxStatus};
use anyhow::{bail, Context, Ok, Result};
use bitcoin::{BlockHash, Txid};
use bitcoin_indexer::types::BlockHeight;
use log::warn;
use mockall::automock;
use std::path::PathBuf;
use storage_backend::storage::{KeyValueStore, Storage};

pub struct BitvmxStore {
    store: Storage,
}
enum InstanceKey<'a> {
    Instance(InstanceId),
    InstanceTx(InstanceId, &'a Txid),
    InstanceList,
    InstanceNews,
}

pub trait BitvmxApi {
    fn get_all_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>>;

    fn get_instances_ready_to_track(
        &self,
        current_height: BlockHeight,
    ) -> Result<Vec<BitvmxInstance>>;

    fn update_instance_tx_seen(
        &self,
        instance_id: InstanceId,
        txid: &Txid,
        tx_height: BlockHeight,
        tx_block_hash: BlockHash,
        tx_is_orphan: bool,
        tx_hex: &str,
    ) -> Result<()>;

    fn save_instance(&self, instance: &BitvmxInstance) -> Result<()>;
    fn save_instances(&self, instances: &[BitvmxInstance]) -> Result<()>;
    fn save_transaction(&self, instance_id: InstanceId, tx: &Txid) -> Result<()>;
    fn remove_transaction(&self, instance_id: InstanceId, tx: &Txid) -> Result<()>;

    fn get_instance_news(&self) -> Result<Vec<(InstanceId, Vec<Txid>)>>;
    fn acknowledge_instance_tx_news(&self, instance_id: InstanceId, tx: &Txid) -> Result<()>;
    fn get_instance_tx_status(
        &self,
        instance_id: InstanceId,
        tx_id: &Txid,
    ) -> Result<Option<TxStatus>>;

    fn update_news(&self, instance_id: InstanceId, txid: Txid) -> Result<()>;
}

impl BitvmxStore {
    fn get_instance_key(&self, key: InstanceKey) -> String {
        match key {
            InstanceKey::Instance(instance_id) => format!("instance/{}", instance_id),
            InstanceKey::InstanceTx(instance_id, tx_id) => {
                format!("instance/{}/tx/{}", instance_id, tx_id)
            }
            InstanceKey::InstanceList => "instance/list".to_string(),
            InstanceKey::InstanceNews => "instance/news".to_string(),
        }
    }

    pub fn new_with_path(store_path: &str) -> Result<Self> {
        let store = Storage::new_with_path(&PathBuf::from(format!("{}/monitor", store_path)))?;
        Ok(Self { store })
    }

    fn get_instance(&self, instance_id: InstanceId) -> Result<Option<BitvmxInstance>> {
        let instance_key = self.get_instance_key(InstanceKey::Instance(instance_id));
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

    fn get_instance_tx(&self, instance_id: InstanceId, tx_id: &Txid) -> Result<Option<TxStatus>> {
        let instance_tx_key = self.get_instance_key(InstanceKey::InstanceTx(instance_id, tx_id));
        let tx = self
            .store
            .get::<&str, TxStatus>(&instance_tx_key)
            .context(format!("There was an error getting {}", instance_tx_key))
            .unwrap();

        Ok(tx)
    }

    fn save_instance_tx(&self, instance_id: InstanceId, tx: &TxStatus) -> Result<()> {
        let instance_tx_key =
            self.get_instance_key(InstanceKey::InstanceTx(instance_id, &tx.tx_id));
        self.store
            .set::<&str, TxStatus>(&instance_tx_key, tx.clone())
            .context(format!("There was an error getting {}", instance_tx_key))
            .unwrap();

        let instance_key = self.get_instance_key(InstanceKey::Instance(instance_id));
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
                    .position(|tx_old| tx_old.tx_id == tx.tx_id)
                {
                    // Replace the old transaction with the new one
                    _instance.txs[pos] = tx.clone();
                } else {
                    _instance.txs.push(tx.clone());
                }
                self.store.set(instance_key, _instance)?;
            }
            None => bail!(
                "There was an error trying to save instance {}",
                instance_key
            ),
        }

        Ok(())
    }

    fn remove_instance_tx(&self, instance_id: InstanceId, tx_id: &Txid) -> Result<()> {
        let instance_tx_key = self.get_instance_key(InstanceKey::InstanceTx(instance_id, tx_id));

        // Remove the transaction from the store
        self.store
            .delete(&instance_tx_key)
            .context(format!("There was an error removing {}", instance_tx_key))?;

        // Retrieve the instance using the instance_id
        let instance = self.get_instance(instance_id)?;

        match instance {
            Some(mut _instance) => {
                // Find the index of the transaction to remove from the instance's txs list
                if let Some(pos) = _instance
                    .txs
                    .iter()
                    .position(|tx_old| tx_old.tx_id == *tx_id)
                {
                    // Remove the transaction from the list of transactions
                    _instance.txs.remove(pos);

                    // Update the instance in the store after removal
                    self.save_instance(&_instance)?;
                } else {
                    bail!(
                        "Transaction with id {} not found in instance {}",
                        tx_id,
                        instance_id
                    );
                }
            }
            None => bail!("Instance not found: {}", instance_id),
        }

        Ok(())
    }

    fn get_instances(&self) -> Result<Vec<BitvmxInstance>> {
        let mut instances = Vec::<BitvmxInstance>::new();

        let instances_key = self.get_instance_key(InstanceKey::InstanceList);
        let all_instance_ids = self
            .store
            .get::<_, Vec<InstanceId>>(instances_key)?
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
    fn get_all_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>> {
        self.get_instances()
    }

    fn get_instance_tx_status(
        &self,
        instance_id: InstanceId,
        tx_id: &Txid,
    ) -> Result<Option<TxStatus>> {
        let instance = self.get_instance_tx(instance_id, tx_id)?;
        Ok(instance)
    }

    fn get_instance_news(&self) -> Result<Vec<(InstanceId, Vec<Txid>)>> {
        let instance_news_key = self.get_instance_key(InstanceKey::InstanceNews);
        let instance_news = self
            .store
            .get::<_, Vec<(InstanceId, Vec<Txid>)>>(&instance_news_key)
            .unwrap_or_default();

        match instance_news {
            Some(news) => Ok(news),
            None => Ok(vec![]),
        }
    }

    fn acknowledge_instance_tx_news(&self, instance_id: InstanceId, tx_id: &Txid) -> Result<()> {
        let instance_news_key = self.get_instance_key(InstanceKey::InstanceNews);

        let mut instances_news = self
            .store
            .get::<_, Vec<(InstanceId, Vec<Txid>)>>(&instance_news_key)?
            .unwrap_or_default();

        if let Some(index) = instances_news.iter().position(|(id, _)| *id == instance_id) {
            let (_, txs) = &mut instances_news[index];
            txs.retain(|tx| tx != tx_id);

            // If all transactions for this instance have been acknowledged, remove the instance
            if txs.is_empty() {
                instances_news.remove(index);
            }

            self.store.set(&instance_news_key, &instances_news)?;
        } else {
            // If the instance is not found in the news, we can either ignore it or log a warning
            warn!("No news found for instance {}", instance_id);
        }

        Ok(())
    }

    fn get_instances_ready_to_track(
        &self,
        current_height: BlockHeight,
    ) -> Result<Vec<BitvmxInstance>> {
        // This method will return bitvmx instances excluding the onces are not ready to track
        let mut bitvmx_instances = self.get_instances()?;
        bitvmx_instances.retain(|i| (i.start_height <= current_height));

        Ok(bitvmx_instances)
    }

    fn update_instance_tx_seen(
        &self,
        instance_id: InstanceId,
        txid: &Txid,
        tx_height: BlockHeight,
        tx_block_hash: BlockHash,
        tx_is_orphan: bool,
        tx_hex: &str,
    ) -> Result<()> {
        let tx_instance = self.get_instance_tx(instance_id, txid)?;

        match tx_instance {
            Some(mut tx) => {
                if tx.block_info.is_some() {
                    warn!("Txn already seen, looks this methods is being calling more than what should be")
                }
                tx.block_info = Some(BlockInfo {
                    block_height: tx_height,
                    block_hash: tx_block_hash,
                    is_orphan: tx_is_orphan,
                });
                tx.tx_hex = Some(tx_hex.to_string());
                self.save_instance_tx(instance_id, &tx)?;
            }
            None => warn!(
                "Txn for the bitvmx instance {} txid {} was not found",
                instance_id, txid
            ),
        }

        self.update_news(instance_id, *txid)?;

        Ok(())
    }

    fn save_instances(&self, instances: &[BitvmxInstance]) -> Result<()> {
        for instance in instances {
            self.save_instance(instance)?;
        }
        Ok(())
    }

    fn save_instance(&self, instance: &BitvmxInstance) -> Result<()> {
        let instance_key = self.get_instance_key(InstanceKey::Instance(instance.id));

        // Store the instance under its ID
        self.store.set(&instance_key, instance).context(format!(
            "Failed to store instance under key {}",
            instance_key
        ))?;

        // Index each transaction instance by its txid
        for tx in &instance.txs {
            let tx_key = self.get_instance_key(InstanceKey::InstanceTx(instance.id, &tx.tx_id));
            self.store.set(&tx_key, tx).context(format!(
                "Failed to store txid {} under key {}",
                tx.tx_id, tx_key
            ))?;
        }

        // Maintain a list of all instances
        let instances_key = self.get_instance_key(InstanceKey::InstanceList);
        let mut all_instances = self
            .store
            .get::<_, Vec<InstanceId>>(&instances_key)
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

    fn save_transaction(&self, instance_id: InstanceId, tx_id: &Txid) -> Result<()> {
        let tx_data = TxStatus {
            tx_id: *tx_id,
            tx_hex: None,
            block_info: None,
        };

        self.save_instance_tx(instance_id, &tx_data)?;

        Ok(())
    }

    fn remove_transaction(&self, instance_id: InstanceId, tx_id: &Txid) -> Result<()> {
        self.remove_instance_tx(instance_id, tx_id)
    }

    fn update_news(&self, instance_id: InstanceId, txid: Txid) -> Result<()> {
        let instance_news_key = self.get_instance_key(InstanceKey::InstanceNews);
        let mut instance_news = self
            .store
            .get::<_, Vec<(InstanceId, Vec<Txid>)>>(&instance_news_key)?
            .unwrap_or_default();

        // Find the index of the instance in the news
        if let Some(index) = instance_news.iter().position(|(id, _)| *id == instance_id) {
            // If the instance exists, update its transactions
            if !instance_news[index].1.contains(&txid) {
                instance_news[index].1.push(txid);
            }
        } else {
            // If the instance doesn't exist, add it with the new transaction
            instance_news.push((instance_id, vec![txid]));
        }

        self.store.set(&instance_news_key, &instance_news)?;

        Ok(())
    }
}

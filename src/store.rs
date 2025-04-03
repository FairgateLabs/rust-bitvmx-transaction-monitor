use crate::{
    errors::MonitorStoreError,
    types::{
        BlockInfo, Id, MonitorNewType, TransactionMonitorType, TransactionStatus, TransactionStore,
    },
};
use bitcoin::{Transaction, Txid};
use bitvmx_bitcoin_rpc::types::BlockHeight;
use mockall::automock;
use std::rc::Rc;
use storage_backend::storage::{KeyValueStore, Storage};
use tracing::warn;

pub struct MonitorStore {
    store: Rc<Storage>,
}
enum InstanceKey {
    Instance(Id),
    InstanceList,
    InstanceNews,
}
enum TransactionKey {
    Transaction(Txid),
    TransactionList,
    TransactionNews,
}

enum BlockchainKey {
    CurrentBlockHeight,
}

pub trait MonitorStoreApi {
    fn get_all_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>, MonitorStoreError>;

    fn get_instances_ready_to_track(
        &self,
        current_height: BlockHeight,
    ) -> Result<Vec<BitvmxInstance>, MonitorStoreError>;

    fn save_instance(&self, instance: &BitvmxInstance) -> Result<(), MonitorStoreError>;
    fn save(
        &self,
        data: TransactionMonitorType,
        current_height: BlockHeight,
    ) -> Result<(), MonitorStoreError>;
    fn save_instance_transaction(
        &self,
        instance_id: Id,
        tx: &Txid,
    ) -> Result<(), MonitorStoreError>;
    fn remove_transaction(&self, instance_id: Id, tx: &Txid) -> Result<(), MonitorStoreError>;

    fn update_instance_news(&self, instance_id: Id, txid: Txid) -> Result<(), MonitorStoreError>;
    fn get_news(&self) -> Result<Vec<MonitorNewType>, MonitorStoreError>;
    fn acknowledge_news(&self, instance_id: Id, tx: &Txid) -> Result<(), MonitorStoreError>;

    //Transaction Methods
    fn save_tx(&self, tx: &Transaction, block_info: BlockInfo) -> Result<(), MonitorStoreError>;
    fn get_single_tx_news(&self) -> Result<Vec<TransactionStatus>, MonitorStoreError>;
    fn acknowledge_tx_news(&self, tx_id: Txid) -> Result<(), MonitorStoreError>;

    fn get_current_block_height(&self) -> Result<BlockHeight, MonitorStoreError>;
    fn set_current_block_height(&self, height: BlockHeight) -> Result<(), MonitorStoreError>;
}

impl MonitorStore {
    pub fn new(store: Rc<Storage>) -> Result<Self, MonitorStoreError> {
        Ok(Self { store })
    }

    fn get_instance_key(&self, key: InstanceKey) -> String {
        let prefix = "monitor";
        match key {
            InstanceKey::InstanceList => format!("{prefix}/instance/list"),
            InstanceKey::Instance(instance_id) => format!("{prefix}/instance/{}", instance_id),
            InstanceKey::InstanceNews => format!("{prefix}/instance/news"),
        }
    }

    fn get_tx_key(&self, key: TransactionKey) -> String {
        let prefix = "monitor";
        match key {
            TransactionKey::TransactionList => format!("{prefix}/tx/list"),
            TransactionKey::Transaction(tx_id) => format!("{prefix}/tx/{}", tx_id),
            TransactionKey::TransactionNews => format!("{prefix}/tx/news"),
        }
    }

    fn get_blockchain_key(&self, key: BlockchainKey) -> String {
        let prefix = "monitor";
        match key {
            BlockchainKey::CurrentBlockHeight => {
                format!("{prefix}/blockchain/current_block_height")
            }
        }
    }

    fn get_instance(&self, instance_id: Id) -> Result<Option<BitvmxInstance>, MonitorStoreError> {
        let instance_key = self.get_instance_key(InstanceKey::Instance(instance_id));
        let instance = self.store.get::<&str, BitvmxInstance>(&instance_key)?;

        Ok(instance)
    }

    fn save_instance_tx(
        &self,
        instance_id: Id,
        tx_status: &TransactionStore,
    ) -> Result<(), MonitorStoreError> {
        let instance_key = self.get_instance_key(InstanceKey::Instance(instance_id));
        let instance = self.store.get::<&str, BitvmxInstance>(&instance_key)?;

        match instance {
            Some(mut _instance) =>
            // Find the index of the transaction you want to replace
            {
                if let Some(pos) = _instance
                    .txs
                    .iter()
                    .position(|tx_old| tx_old.tx_id == tx_status.tx_id)
                {
                    // Replace the old transaction with the new one
                    _instance.txs[pos] = tx_status.clone();
                } else {
                    _instance.txs.push(tx_status.clone());
                }
                self.store.set(instance_key, _instance, None)?;
            }
            None => {
                return Err(MonitorStoreError::UnexpectedError(format!(
                    "There was an error trying to save instance {}",
                    instance_key
                )))
            }
        }

        Ok(())
    }

    fn remove_instance_tx(&self, instance_id: Id, tx_id: &Txid) -> Result<(), MonitorStoreError> {
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
                    return Err(MonitorStoreError::UnexpectedError(format!(
                        "Transaction with id {} not found in instance {}",
                        tx_id, instance_id
                    )));
                }
            }
            None => {
                return Err(MonitorStoreError::UnexpectedError(format!(
                    "Instance {} not found",
                    instance_id
                )));
            }
        }

        Ok(())
    }

    fn get_instances(&self) -> Result<Vec<BitvmxInstance>, MonitorStoreError> {
        let mut instances = Vec::<BitvmxInstance>::new();

        let instances_key = self.get_instance_key(InstanceKey::InstanceList);
        let all_instance_ids = self
            .store
            .get::<_, Vec<Id>>(instances_key)?
            .unwrap_or_default();

        for id in all_instance_ids {
            let instance = self.get_instance(id)?;

            match instance {
                Some(inst) => instances.push(inst),
                None => {
                    return Err(MonitorStoreError::UnexpectedError(
                        "There is an error trying to get instance".to_string(),
                    ))
                }
            }
        }

        Ok(instances)
    }
}

#[automock]
impl MonitorStoreApi for MonitorStore {
    fn get_current_block_height(&self) -> Result<BlockHeight, MonitorStoreError> {
        let last_block_height_key = self.get_blockchain_key(BlockchainKey::CurrentBlockHeight);
        let last_block_height = self
            .store
            .get::<_, BlockHeight>(&last_block_height_key)?
            .unwrap_or_default();

        Ok(last_block_height)
    }

    fn set_current_block_height(&self, height: BlockHeight) -> Result<(), MonitorStoreError> {
        let last_block_height_key = self.get_blockchain_key(BlockchainKey::CurrentBlockHeight);
        self.store.set(last_block_height_key, height, None)?;
        Ok(())
    }

    fn get_all_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>, MonitorStoreError> {
        self.get_instances()
    }

    fn get_news(&self) -> Result<Vec<(Id, Vec<Txid>)>, MonitorStoreError> {
        let instance_news_key = self.get_instance_key(InstanceKey::InstanceNews);
        let instance_news = self
            .store
            .get::<_, Vec<(Id, Vec<Txid>)>>(&instance_news_key)
            .unwrap_or_default();

        match instance_news {
            Some(news) => Ok(news),
            None => Ok(vec![]),
        }
    }

    fn get_single_tx_news(&self) -> Result<Vec<TransactionStatus>, MonitorStoreError> {
        let tx_news_key = self.get_tx_key(TransactionKey::TransactionNews);
        let tx_ids = self
            .store
            .get::<_, Vec<Txid>>(&tx_news_key)?
            .unwrap_or_else(Vec::new);

        let mut txs = Vec::new();

        for tx in tx_ids {
            let tx_news_key = self.get_tx_key(TransactionKey::Transaction(tx));

            let tx_status = self.store.get::<&str, TransactionStatus>(&tx_news_key)?;

            if let Some(mut tx_status) = tx_status {
                let current_height = self.get_current_block_height()?;

                tx_status.confirmations = current_height
                    .saturating_sub(tx_status.block_info.clone().unwrap().block_height)
                    + 1;

                txs.push(tx_status);
            } else {
                warn!("No tx status found for tx {}", tx);
            }
        }

        Ok(txs)
    }

    fn acknowledge_news(&self, instance_id: Id, tx_id: &Txid) -> Result<(), MonitorStoreError> {
        let instance_news_key = self.get_instance_key(InstanceKey::InstanceNews);

        let mut instances_news = self
            .store
            .get::<_, Vec<(Id, Vec<Txid>)>>(&instance_news_key)?
            .unwrap_or_default();

        if let Some(index) = instances_news.iter().position(|(id, _)| *id == instance_id) {
            let (_, txs) = &mut instances_news[index];
            txs.retain(|tx| tx != tx_id);

            // If all transactions for this instance have been acknowledged, remove the instance
            if txs.is_empty() {
                instances_news.remove(index);
            }

            self.store.set(&instance_news_key, &instances_news, None)?;
        } else {
            // If the instance is not found in the news, we can either ignore it or log a warning
            warn!("No news found for instance {}", instance_id);
        }

        Ok(())
    }

    fn get_instances_ready_to_track(
        &self,
        current_height: BlockHeight,
    ) -> Result<Vec<BitvmxInstance>, MonitorStoreError> {
        // This method will return bitvmx instances excluding the onces are not ready to track
        let mut bitvmx_instances = self.get_instances()?;
        bitvmx_instances.retain(|i| (i.start_height <= current_height));

        Ok(bitvmx_instances)
    }

    fn save(&self, instances: &[BitvmxInstance]) -> Result<(), MonitorStoreError> {
        for instance in instances {
            self.save_instance(instance)?;
        }
        Ok(())
    }

    fn save_instance(&self, instance: &BitvmxInstance) -> Result<(), MonitorStoreError> {
        let instance_key = self.get_instance_key(InstanceKey::Instance(instance.id));

        // Store the instance under its ID
        self.store.set(instance_key, instance, None)?;

        // Maintain a list of all instances
        let instances_key = self.get_instance_key(InstanceKey::InstanceList);
        let mut all_instances = self
            .store
            .get::<_, Vec<Id>>(&instances_key)
            .unwrap_or_default()
            .unwrap_or_default();

        if !all_instances.contains(&instance.id) {
            all_instances.push(instance.id);
            self.store.set(instances_key, &all_instances, None)?;
        }

        Ok(())
    }

    fn save_instance_transaction(
        &self,
        instance_id: Id,
        tx_id: &Txid,
    ) -> Result<(), MonitorStoreError> {
        let tx_data = TransactionStore {
            tx_id: *tx_id,
            tx: None,
        };

        self.save_instance_tx(instance_id, &tx_data)?;

        Ok(())
    }

    fn save_tx(&self, tx: &Transaction, block_info: BlockInfo) -> Result<(), MonitorStoreError> {
        // Save the transaction status
        let tx_key = self.get_tx_key(TransactionKey::Transaction(tx.compute_txid()));
        let tx_status = TransactionStatus::new(tx.clone(), Some(block_info));
        self.store.set(&tx_key, tx_status, None)?;

        // Update the transaction news
        let txs_news_key = self.get_tx_key(TransactionKey::TransactionNews);
        let mut txs = self
            .store
            .get::<_, Vec<Txid>>(&txs_news_key)?
            .unwrap_or_else(Vec::new);

        if !txs.iter().any(|tx_id| tx_id == &tx.compute_txid()) {
            txs.push(tx.compute_txid());
            self.store.set(&txs_news_key, &txs, None)?;
        }

        // Save tx in list
        let txs_list_key = self.get_tx_key(TransactionKey::TransactionList);
        let mut txs = self
            .store
            .get::<_, Vec<Txid>>(&txs_list_key)?
            .unwrap_or_else(Vec::new);
        txs.push(tx.compute_txid());
        self.store.set(&txs_list_key, &txs, None)?;

        Ok(())
    }

    fn remove_transaction(&self, instance_id: Id, tx_id: &Txid) -> Result<(), MonitorStoreError> {
        self.remove_instance_tx(instance_id, tx_id)
    }

    fn update_instance_news(&self, instance_id: Id, txid: Txid) -> Result<(), MonitorStoreError> {
        let instance_news_key = self.get_instance_key(InstanceKey::InstanceNews);
        let mut instance_news = self
            .store
            .get::<_, Vec<(Id, Vec<Txid>)>>(&instance_news_key)?
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

        self.store.set(&instance_news_key, &instance_news, None)?;

        Ok(())
    }

    fn acknowledge_tx_news(&self, tx_id: Txid) -> Result<(), MonitorStoreError> {
        let tx_news_key = self.get_tx_key(TransactionKey::TransactionNews);
        let mut tx_news = self
            .store
            .get::<_, Vec<Txid>>(&tx_news_key)?
            .unwrap_or_default();

        if let Some(pos) = tx_news.iter().position(|a| a == &tx_id) {
            tx_news.remove(pos);
            self.store.set(&tx_news_key, &tx_news, None)?;
        }

        Ok(())
    }
}

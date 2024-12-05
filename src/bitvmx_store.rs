use crate::types::{AddressStatus, BitvmxInstance, BlockInfo, InstanceId, TransactionStatus};
use anyhow::{bail, Context, Ok, Result};
use bitcoin::{address::NetworkUnchecked, Address, BlockHash, Transaction, Txid};
use bitcoin_indexer::types::BlockHeight;
use log::warn;
use mockall::automock;
use std::path::PathBuf;
use storage_backend::storage::{KeyValueStore, Storage};

pub struct BitvmxStore {
    store: Storage,
}
enum InstanceKey {
    Instance(InstanceId),
    InstanceList,
    InstanceNews,
}
enum AddressKey {
    Address(Address),
    AddressList,
    AddressNews,
}

pub trait BitvmxApi {
    fn get_all_instances_for_tracking(&self) -> Result<Vec<BitvmxInstance>>;

    fn get_instances_ready_to_track(
        &self,
        current_height: BlockHeight,
    ) -> Result<Vec<BitvmxInstance>>;

    fn save_instance(&self, instance: &BitvmxInstance) -> Result<()>;
    fn save_instances(&self, instances: &[BitvmxInstance]) -> Result<()>;
    fn save_transaction(&self, instance_id: InstanceId, tx: &Txid) -> Result<()>;
    fn remove_transaction(&self, instance_id: InstanceId, tx: &Txid) -> Result<()>;

    fn get_instance_news(&self) -> Result<Vec<(InstanceId, Vec<Txid>)>>;
    fn acknowledge_instance_tx_news(&self, instance_id: InstanceId, tx: &Txid) -> Result<()>;

    fn update_instance_news(&self, instance_id: InstanceId, txid: Txid) -> Result<()>;

    //Address Methods
    fn get_addresses(&self) -> Result<Vec<Address>>;
    fn save_address(&self, address: Address) -> Result<()>;
    fn update_address_news(
        &self,
        address: Address,
        tx: &Transaction,
        block_height: BlockHeight,
        block_hash: BlockHash,
        orphan: bool,
    ) -> Result<()>;
    fn get_address_news(&self) -> Result<Vec<(Address, Vec<AddressStatus>)>>;
    fn acknowledge_address_news(&self, address: Address) -> Result<()>;
}

impl BitvmxStore {
    fn get_instance_key(&self, key: InstanceKey) -> String {
        match key {
            InstanceKey::Instance(instance_id) => format!("instance/{}", instance_id),
            InstanceKey::InstanceList => "instance/list".to_string(),
            InstanceKey::InstanceNews => "instance/news".to_string(),
        }
    }

    fn get_address_key(&self, key: AddressKey) -> String {
        match key {
            AddressKey::AddressList => "address/list".to_string(),
            AddressKey::Address(address) => format!("address/{}", address).to_string(),
            AddressKey::AddressNews => "address/news".to_string(),
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

    fn save_instance_tx(
        &self,
        instance_id: InstanceId,
        tx_status: &TransactionStatus,
    ) -> Result<()> {
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
                    .position(|tx_old| tx_old.tx_id == tx_status.tx_id)
                {
                    // Replace the old transaction with the new one
                    _instance.txs[pos] = tx_status.clone();
                } else {
                    _instance.txs.push(tx_status.clone());
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

    fn get_address_news(&self) -> Result<Vec<(Address, Vec<AddressStatus>)>> {
        let address_news_key = self.get_address_key(AddressKey::AddressNews);
        let address_news = self
            .store
            .get::<_, Vec<Address<NetworkUnchecked>>>(&address_news_key)
            .unwrap_or(None)
            .unwrap_or_else(Vec::new);

        let mut address_txs = Vec::new();
        for address in address_news {
            let address_news_key =
                self.get_address_key(AddressKey::Address(address.clone().assume_checked()));

            let address_status = self
                .store
                .get::<&str, Vec<AddressStatus>>(&address_news_key)
                .context(format!("There was an error getting {}", address_news_key))
                .unwrap_or(None)
                .unwrap_or_else(Vec::new);

            address_txs.push((address.assume_checked(), address_status));
        }

        Ok(address_txs)
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
        let tx_data = TransactionStatus {
            tx_id: *tx_id,
            tx: None,
        };

        self.save_instance_tx(instance_id, &tx_data)?;

        Ok(())
    }

    fn save_address(&self, address: Address) -> Result<()> {
        let address_list_key = self.get_address_key(AddressKey::AddressList);
        let mut addresses = self
            .store
            .get::<&str, Vec<Address<NetworkUnchecked>>>(&address_list_key)
            .context(format!("There was an error getting {}", address_list_key))
            .unwrap_or(None)
            .unwrap_or_else(Vec::new);

        if !addresses.iter().any(|a| a == address.as_unchecked()) {
            addresses.push(address.as_unchecked().clone());

            self.store
                .set(&address_list_key, &addresses)
                .context("Failed to update address list")?;
        }

        Ok(())
    }

    fn update_address_news(
        &self,
        address: Address,
        tx: &Transaction,
        block_height: BlockHeight,
        block_hash: BlockHash,
        orphan: bool,
    ) -> Result<()> {
        let address_key = self.get_address_key(AddressKey::Address(address.clone()));

        let mut address_status = self
            .store
            .get::<&str, Vec<AddressStatus>>(&address_key)
            .context("There was an error getting address status")?
            .unwrap_or_else(Vec::new);

        let block_info = Some(BlockInfo {
            block_height,
            block_hash,
            is_orphan: orphan,
        });

        address_status.push(AddressStatus {
            tx: Some(tx.clone()),
            block_info,
        });

        self.store
            .set(&address_key, address_status)
            .context("Failed to update address list")?;

        let address_news_key = self.get_address_key(AddressKey::AddressNews);
        let mut addresses = self
            .store
            .get::<_, Vec<Address<NetworkUnchecked>>>(&address_news_key)
            .unwrap_or(None)
            .unwrap_or_else(Vec::new);

        if !addresses.iter().any(|a| a == address.as_unchecked()) {
            addresses.push(address.as_unchecked().clone());
            self.store
                .set(&address_news_key, &addresses)
                .context("Failed to update address news")?;
        }

        Ok(())
    }

    fn get_addresses(&self) -> Result<Vec<Address>> {
        let address_list_key = self.get_address_key(AddressKey::AddressList);
        let addresses = self
            .store
            .get::<&str, Vec<Address<NetworkUnchecked>>>(&address_list_key)
            .context(format!("There was an error getting {}", address_list_key))
            .unwrap_or(Some(vec![]))
            .unwrap();

        let addreses_checked: Vec<Address> = addresses
            .iter()
            .map(|a| a.clone().assume_checked())
            .collect();

        Ok(addreses_checked)
    }

    fn remove_transaction(&self, instance_id: InstanceId, tx_id: &Txid) -> Result<()> {
        self.remove_instance_tx(instance_id, tx_id)
    }

    fn update_instance_news(&self, instance_id: InstanceId, txid: Txid) -> Result<()> {
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

    fn acknowledge_address_news(&self, address: Address) -> Result<()> {
        let address_news_key = self.get_address_key(AddressKey::AddressNews);
        let mut address_news = self
            .store
            .get::<_, Vec<Address<NetworkUnchecked>>>(&address_news_key)?
            .unwrap_or_default();

        let address_checked: Vec<Address> = address_news
            .iter()
            .map(|a| a.clone().assume_checked())
            .collect();

        if let Some(pos) = address_checked.iter().position(|a| a == &address) {
            address_news.remove(pos);
            self.store.set(&address_news_key, &address_news)?;
        }

        Ok(())
    }
}

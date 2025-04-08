use crate::{
    errors::MonitorStoreError,
    types::{AcknowledgeTransactionNews, ExtraData, TransactionMonitor},
};
use bitcoin::Txid;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use mockall::automock;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use storage_backend::storage::{KeyValueStore, Storage};

pub struct MonitorStore {
    store: Rc<Storage>,
}
enum TransactionKey {
    TransactionList,
    RskPeginTransaction,
    SpendingUTXOTransactionList,
    TransactionNews,
    RskPeginTransactionNews,
    SpendingUTXOTransactionNews,
}

enum BlockchainKey {
    CurrentBlockHeight,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum TransactionMonitoredType {
    Transaction(Txid, ExtraData),
    RskPeginTransaction(Txid),
    SpendingUTXOTransaction(Txid, u32, ExtraData),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum TransactionMonitorType {
    Transaction(Txid, ExtraData),
    RskPeginTransaction,
    SpendingUTXOTransaction(Txid, u32, ExtraData),
}

pub trait MonitorStoreApi {
    fn get_monitors(
        &self,
        current_height: BlockHeight,
    ) -> Result<Vec<TransactionMonitorType>, MonitorStoreError>;

    fn save_monitor(
        &self,
        data: TransactionMonitor,
        start_height: BlockHeight,
    ) -> Result<(), MonitorStoreError>;

    fn remove_monitor(&self, data: TransactionMonitor) -> Result<(), MonitorStoreError>;

    fn get_news(&self) -> Result<Vec<TransactionMonitoredType>, MonitorStoreError>;
    fn update_news(&self, data: TransactionMonitoredType) -> Result<(), MonitorStoreError>;
    fn acknowledge_news(&self, data: AcknowledgeTransactionNews) -> Result<(), MonitorStoreError>;

    fn get_monitor_height(&self) -> Result<BlockHeight, MonitorStoreError>;
    fn set_monitor_height(&self, height: BlockHeight) -> Result<(), MonitorStoreError>;
}

impl MonitorStore {
    pub fn new(store: Rc<Storage>) -> Result<Self, MonitorStoreError> {
        Ok(Self { store })
    }

    fn get_key(&self, key: TransactionKey) -> String {
        let prefix = "monitor";
        match key {
            TransactionKey::TransactionList => format!("{prefix}/tx/list"),
            TransactionKey::RskPeginTransaction => format!("{prefix}/rsk/tx"),
            TransactionKey::SpendingUTXOTransactionList => {
                format!("{prefix}/spending/utxo/tx/list")
            }
            TransactionKey::TransactionNews => format!("{prefix}/tx/news"),
            TransactionKey::RskPeginTransactionNews => format!("{prefix}/rsk/tx/news"),
            TransactionKey::SpendingUTXOTransactionNews => {
                format!("{prefix}/spending/utxo/tx/news")
            }
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
}

#[automock]
impl MonitorStoreApi for MonitorStore {
    fn get_monitor_height(&self) -> Result<BlockHeight, MonitorStoreError> {
        let last_block_height_key = self.get_blockchain_key(BlockchainKey::CurrentBlockHeight);
        let last_block_height = self
            .store
            .get::<_, BlockHeight>(&last_block_height_key)?
            .unwrap_or_default();

        Ok(last_block_height)
    }

    fn set_monitor_height(&self, height: BlockHeight) -> Result<(), MonitorStoreError> {
        let last_block_height_key = self.get_blockchain_key(BlockchainKey::CurrentBlockHeight);
        self.store.set(last_block_height_key, height, None)?;
        Ok(())
    }

    fn get_news(&self) -> Result<Vec<TransactionMonitoredType>, MonitorStoreError> {
        let mut news = Vec::new();

        let key = self.get_key(TransactionKey::TransactionNews);
        let txs_news = self
            .store
            .get::<_, Vec<(Txid, ExtraData)>>(&key)?
            .unwrap_or_default();

        for (tx_id, extra_data) in txs_news {
            news.push(TransactionMonitoredType::Transaction(tx_id, extra_data));
        }

        let rsk_news_key = self.get_key(TransactionKey::RskPeginTransactionNews);
        let rsk_news = self
            .store
            .get::<_, Vec<Txid>>(&rsk_news_key)?
            .unwrap_or_default();

        for tx_id in rsk_news {
            news.push(TransactionMonitoredType::RskPeginTransaction(tx_id));
        }

        let spending_news_key = self.get_key(TransactionKey::SpendingUTXOTransactionNews);
        let spending_news = self
            .store
            .get::<_, Vec<(Txid, u32, ExtraData)>>(&spending_news_key)?
            .unwrap_or_default();

        for (tx_id, utxo_index, extra_data) in spending_news {
            news.push(TransactionMonitoredType::SpendingUTXOTransaction(
                tx_id, utxo_index, extra_data,
            ));
        }

        Ok(news)
    }

    fn update_news(&self, data: TransactionMonitoredType) -> Result<(), MonitorStoreError> {
        match data {
            TransactionMonitoredType::Transaction(tx_id, extra_data) => {
                let key = self.get_key(TransactionKey::TransactionNews);
                let mut txs_news = self
                    .store
                    .get::<_, Vec<(Txid, ExtraData)>>(&key)?
                    .unwrap_or_default();

                if !txs_news.contains(&(tx_id, extra_data.clone())) {
                    txs_news.push((tx_id, extra_data.clone()));
                }

                self.store.set(&key, &txs_news, None)?;
            }
            TransactionMonitoredType::RskPeginTransaction(tx_id) => {
                let rsk_news_key = self.get_key(TransactionKey::RskPeginTransactionNews);
                let mut rsk_news = self
                    .store
                    .get::<_, Vec<Txid>>(&rsk_news_key)?
                    .unwrap_or_default();

                if !rsk_news.contains(&tx_id) {
                    rsk_news.push(tx_id);
                }

                self.store.set(&rsk_news_key, &rsk_news, None)?;
            }
            TransactionMonitoredType::SpendingUTXOTransaction(tx_id, utxo_index, extra_data) => {
                let utxo_news_key = self.get_key(TransactionKey::SpendingUTXOTransactionNews);
                let mut utxo_news = self
                    .store
                    .get::<_, Vec<(Txid, u32, ExtraData)>>(&utxo_news_key)?
                    .unwrap_or_default();

                if !utxo_news.contains(&(tx_id, utxo_index, extra_data.clone())) {
                    utxo_news.push((tx_id, utxo_index, extra_data));
                }

                self.store.set(&utxo_news_key, &utxo_news, None)?;
            }
        }

        Ok(())
    }

    fn acknowledge_news(&self, data: AcknowledgeTransactionNews) -> Result<(), MonitorStoreError> {
        match data {
            AcknowledgeTransactionNews::Transaction(tx_id) => {
                let key = self.get_key(TransactionKey::TransactionNews);
                let mut txs_news = self
                    .store
                    .get::<_, Vec<(Txid, ExtraData)>>(&key)?
                    .unwrap_or_default();

                txs_news.retain(|(tx, _)| tx != &tx_id);
                self.store.set(&key, &txs_news, None)?;
            }
            AcknowledgeTransactionNews::RskPeginTransaction(tx_id) => {
                let rsk_news_key = self.get_key(TransactionKey::RskPeginTransactionNews);
                let mut rsk_news = self
                    .store
                    .get::<_, Vec<Txid>>(&rsk_news_key)?
                    .unwrap_or_default();

                rsk_news.retain(|tx| tx != &tx_id);
                self.store.set(&rsk_news_key, &rsk_news, None)?;
            }
            AcknowledgeTransactionNews::SpendingUTXOTransaction(tx_id, utxo_index) => {
                let utxo_news_key = self.get_key(TransactionKey::SpendingUTXOTransactionNews);
                let mut utxo_news = self
                    .store
                    .get::<_, Vec<(Txid, u32, ExtraData)>>(&utxo_news_key)?
                    .unwrap_or_default();

                utxo_news.retain(|(tx, utxo_i, _)| *tx != tx_id || *utxo_i != utxo_index);
                self.store.set(&utxo_news_key, &utxo_news, None)?;
            }
        }

        Ok(())
    }

    fn get_monitors(
        &self,
        current_height: BlockHeight,
    ) -> Result<Vec<TransactionMonitorType>, MonitorStoreError> {
        let mut monitors = Vec::<(TransactionMonitorType, BlockHeight)>::new();

        let txs_key = self.get_key(TransactionKey::TransactionList);
        let txs = self
            .store
            .get::<_, Vec<(Txid, ExtraData, BlockHeight)>>(txs_key)?
            .unwrap_or_default();

        for (tx_id, extra_data, height) in txs {
            monitors.push((
                TransactionMonitorType::Transaction(tx_id, extra_data),
                height,
            ));
        }

        let rsk_pegin_key = self.get_key(TransactionKey::RskPeginTransaction);
        let rsk_pegin_height = self
            .store
            .get::<_, (bool, BlockHeight)>(rsk_pegin_key)?
            .unwrap_or_default();

        if rsk_pegin_height.0 {
            monitors.push((
                TransactionMonitorType::RskPeginTransaction,
                rsk_pegin_height.1,
            ));
        }

        let spending_utxo_key = self.get_key(TransactionKey::SpendingUTXOTransactionList);
        let spending_utxos = self
            .store
            .get::<_, Vec<(Txid, u32, ExtraData, BlockHeight)>>(spending_utxo_key)?
            .unwrap_or_default();

        for (tx_id, utxo_index, extra_data, height) in spending_utxos {
            monitors.push((
                TransactionMonitorType::SpendingUTXOTransaction(tx_id, utxo_index, extra_data),
                height,
            ));
        }

        let filtered_monitors = monitors
            .into_iter()
            .filter(|(_, height)| *height <= current_height)
            .map(|(monitor_type, _)| monitor_type)
            .collect::<Vec<_>>();

        Ok(filtered_monitors)
    }

    fn save_monitor(
        &self,
        data: TransactionMonitor,
        start_height: BlockHeight,
    ) -> Result<(), MonitorStoreError> {
        match data {
            TransactionMonitor::Transactions(tx_ids, extra_data) => {
                let key = self.get_key(TransactionKey::TransactionList);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, ExtraData, BlockHeight)>>(&key)?
                    .unwrap_or_default();

                for txid in &tx_ids {
                    if let Some(pos) = txs.iter().position(|(i, _, _)| *i == *txid) {
                        // Update the existing entry with the new extra_data
                        txs[pos] = (*txid, extra_data.clone(), start_height);
                    } else {
                        // Add a new entry if the txid doesn't exist
                        txs.push((*txid, extra_data.clone(), start_height));
                    }
                }

                self.store.set(&key, &txs, None)?;
            }
            TransactionMonitor::RskPeginTransaction => {
                let key = self.get_key(TransactionKey::RskPeginTransaction);
                self.store.set(&key, (true, start_height), None)?;
            }
            TransactionMonitor::SpendingUTXOTransaction(txid, vout, extra_data) => {
                let key = self.get_key(TransactionKey::SpendingUTXOTransactionList);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, ExtraData, BlockHeight)>>(&key)?
                    .unwrap_or_default();
                if !txs.contains(&(txid, vout, extra_data.clone(), start_height)) {
                    txs.push((txid, vout, extra_data.clone(), start_height));
                    self.store.set(&key, &txs, None)?;
                }
            }
        }

        Ok(())
    }

    fn remove_monitor(&self, data: TransactionMonitor) -> Result<(), MonitorStoreError> {
        match data {
            TransactionMonitor::Transactions(tx_ids, _) => {
                let key = self.get_key(TransactionKey::TransactionList);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, ExtraData, BlockHeight)>>(&key)?
                    .unwrap_or_default();

                // Filter out transactions that are in tx_ids
                txs.retain(|(tx_id, _, _)| {
                    // Keep the entry if none of its txids are in tx_ids
                    !tx_ids.iter().any(|txid| *txid == *tx_id)
                });

                self.store.set(&key, &txs, None)?;
            }

            TransactionMonitor::RskPeginTransaction => {
                let key = self.get_key(TransactionKey::RskPeginTransaction);
                self.store.set(&key, (false, 0), None)?;
            }
            TransactionMonitor::SpendingUTXOTransaction(txid, vout, _) => {
                let key = self.get_key(TransactionKey::SpendingUTXOTransactionList);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, ExtraData, BlockHeight)>>(&key)?
                    .unwrap_or_default();
                txs.retain(|(tx_id, utxo_index, _, _)| *tx_id != txid || *utxo_index != vout);
                self.store.set(&key, &txs, None)?;
            }
        }

        Ok(())
    }
}

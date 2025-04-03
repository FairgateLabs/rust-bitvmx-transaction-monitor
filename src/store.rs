use crate::{
    errors::MonitorStoreError,
    types::{AcknowledgeNewsType, Id, TransactionMonitorType},
};
use bitcoin::Txid;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use mockall::automock;
use std::rc::Rc;
use storage_backend::storage::{KeyValueStore, Storage};

pub struct MonitorStore {
    store: Rc<Storage>,
}
enum TransactionKey {
    GroupTransactionList,
    SingleTransactionList,
    RskPeginTransaction,
    SpendingUTXOTransactionList,
    GroupTransactionNews,
    SingleTransactionNews,
    RskPeginTransactionNews,
    SpendingUTXOTransactionNews,
}

enum BlockchainKey {
    CurrentBlockHeight,
}

pub enum TransactionToMonitorType {
    GroupTransaction(Id, Txid),
    SingleTransaction(Txid),
    RskPeginTransaction,
    SpendingUTXOTransaction(Txid, u32),
}

pub enum TransactionMonitoredType {
    GroupTransaction(Id, Txid),
    SingleTransaction(Txid),
    RskPeginTransaction(Txid),
    SpendingUTXOTransaction(Txid, u32),
}

pub trait MonitorStoreApi {
    fn get_monitors(
        &self,
        current_height: BlockHeight,
    ) -> Result<Vec<TransactionMonitorType>, MonitorStoreError>;

    fn save_monitor(
        &self,
        data: TransactionMonitorType,
        start_height: BlockHeight,
    ) -> Result<(), MonitorStoreError>;

    fn get_news(&self) -> Result<Vec<TransactionMonitoredType>, MonitorStoreError>;
    fn update_news(&self, data: TransactionMonitoredType) -> Result<(), MonitorStoreError>;
    fn acknowledge_news(&self, data: AcknowledgeNewsType) -> Result<(), MonitorStoreError>;

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
            TransactionKey::GroupTransactionList => format!("{prefix}/group/tx/list"),
            TransactionKey::SingleTransactionList => format!("{prefix}/single/tx/list"),
            TransactionKey::RskPeginTransaction => format!("{prefix}/rsk/tx"),
            TransactionKey::SpendingUTXOTransactionList => {
                format!("{prefix}/spending/utxo/tx/list")
            }
            TransactionKey::GroupTransactionNews => format!("{prefix}/group/tx/news"),
            TransactionKey::SingleTransactionNews => format!("{prefix}/single/tx/news"),
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

    fn get_monitors(
        &self,
    ) -> Result<Vec<(TransactionMonitorType, BlockHeight)>, MonitorStoreError> {
        let mut monitors = Vec::<(TransactionMonitorType, BlockHeight)>::new();

        let single_tx_key = self.get_key(TransactionKey::SingleTransactionList);
        let single_txs = self
            .store
            .get::<_, Vec<(Txid, BlockHeight)>>(single_tx_key)?
            .unwrap_or_default();

        for (tx_id, height) in single_txs {
            monitors.push((TransactionMonitorType::SingleTransaction(tx_id), height));
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
            .get::<_, Vec<((Txid, u32), BlockHeight)>>(spending_utxo_key)?
            .unwrap_or_default();

        for ((tx_id, utxo_index), height) in spending_utxos {
            monitors.push((
                TransactionMonitorType::SpendingUTXOTransaction(tx_id, utxo_index),
                height,
            ));
        }

        let group_tx_key = self.get_key(TransactionKey::GroupTransactionList);
        let group_txs = self
            .store
            .get::<_, Vec<(Id, Vec<Txid>, BlockHeight)>>(group_tx_key)?
            .unwrap_or_default();

        for (id, txids, height) in group_txs {
            monitors.push((TransactionMonitorType::GroupTransaction(id, txids), height));
        }

        Ok(monitors)
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
        // Get news from each transaction type
        let group_news_key = self.get_key(TransactionKey::GroupTransactionNews);
        let group_news = self
            .store
            .get::<_, Vec<(Id, Vec<Txid>)>>(&group_news_key)?
            .unwrap_or_default();

        for (id, txids) in group_news {
            for tx_id in txids {
                news.push(TransactionMonitoredType::GroupTransaction(id, tx_id));
            }
        }

        let single_news_key = self.get_key(TransactionKey::SingleTransactionNews);
        let single_news = self
            .store
            .get::<_, Vec<Txid>>(&single_news_key)?
            .unwrap_or_default();

        for tx_id in single_news {
            news.push(TransactionMonitoredType::SingleTransaction(tx_id));
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
            .get::<_, Vec<(Txid, u32)>>(&spending_news_key)?
            .unwrap_or_default();

        for (tx_id, utxo_index) in spending_news {
            news.push(TransactionMonitoredType::SpendingUTXOTransaction(
                tx_id, utxo_index,
            ));
        }

        Ok(news)
    }

    fn update_news(&self, data: TransactionMonitoredType) -> Result<(), MonitorStoreError> {
        match data {
            TransactionMonitoredType::GroupTransaction(id, tx_id) => {
                let group_news_key = self.get_key(TransactionKey::GroupTransactionList);
                let mut group_news = self
                    .store
                    .get::<_, Vec<(Id, Vec<Txid>)>>(&group_news_key)?
                    .unwrap_or_default();

                if let Some(index) = group_news.iter().position(|(group_id, _)| *group_id == id) {
                    if !group_news[index].1.contains(&tx_id) {
                        group_news[index].1.push(tx_id);
                    }
                } else {
                    group_news.push((id, vec![tx_id]));
                }

                self.store.set(&group_news_key, &group_news, None)?;
            }
            TransactionMonitoredType::SingleTransaction(tx_id) => {
                let single_news_key = self.get_key(TransactionKey::SingleTransactionList);
                let mut single_news = self
                    .store
                    .get::<_, Vec<Txid>>(&single_news_key)?
                    .unwrap_or_default();

                if !single_news.contains(&tx_id) {
                    single_news.push(tx_id);
                }

                self.store.set(&single_news_key, &single_news, None)?;
            }
            TransactionMonitoredType::RskPeginTransaction(tx_id) => {
                let rsk_news_key = self.get_key(TransactionKey::RskPeginTransaction);
                let mut rsk_news = self
                    .store
                    .get::<_, Vec<Txid>>(&rsk_news_key)?
                    .unwrap_or_default();

                if !rsk_news.contains(&tx_id) {
                    rsk_news.push(tx_id);
                }

                self.store.set(&rsk_news_key, &rsk_news, None)?;
            }
            TransactionMonitoredType::SpendingUTXOTransaction(tx_id, utxo_index) => {
                let utxo_news_key = self.get_key(TransactionKey::SpendingUTXOTransactionList);
                let mut utxo_news = self
                    .store
                    .get::<_, Vec<(Txid, u32)>>(&utxo_news_key)?
                    .unwrap_or_default();

                if !utxo_news.contains(&(tx_id, utxo_index)) {
                    utxo_news.push((tx_id, utxo_index));
                }

                self.store.set(&utxo_news_key, &utxo_news, None)?;
            }
        }

        Ok(())
    }

    fn acknowledge_news(&self, data: AcknowledgeNewsType) -> Result<(), MonitorStoreError> {
        match data {
            AcknowledgeNewsType::GroupTransaction(id, tx_id) => {
                let group_news_key = self.get_key(TransactionKey::GroupTransactionNews);
                let mut group_news = self
                    .store
                    .get::<_, Vec<(Id, Vec<Txid>)>>(&group_news_key)?
                    .unwrap_or_default();

                if let Some(index) = group_news.iter().position(|(group_id, _)| *group_id == id) {
                    let (_, txs) = &mut group_news[index];
                    txs.retain(|tx| tx != &tx_id);

                    if txs.is_empty() {
                        group_news.remove(index);
                    }

                    self.store.set(&group_news_key, &group_news, None)?;
                }
            }
            AcknowledgeNewsType::SingleTransaction(tx_id) => {
                let single_news_key = self.get_key(TransactionKey::SingleTransactionNews);
                let mut single_news = self
                    .store
                    .get::<_, Vec<Txid>>(&single_news_key)?
                    .unwrap_or_default();

                single_news.retain(|tx| tx != &tx_id);
                self.store.set(&single_news_key, &single_news, None)?;
            }
            AcknowledgeNewsType::RskPeginTransaction(tx_id) => {
                let rsk_news_key = self.get_key(TransactionKey::RskPeginTransactionNews);
                let mut rsk_news = self
                    .store
                    .get::<_, Vec<Txid>>(&rsk_news_key)?
                    .unwrap_or_default();

                rsk_news.retain(|tx| tx != &tx_id);
                self.store.set(&rsk_news_key, &rsk_news, None)?;
            }
            AcknowledgeNewsType::SpendingUTXOTransaction(tx_id) => {
                let utxo_news_key = self.get_key(TransactionKey::SpendingUTXOTransactionNews);
                let mut utxo_news = self
                    .store
                    .get::<_, Vec<(Txid, u32)>>(&utxo_news_key)?
                    .unwrap_or_default();

                utxo_news.retain(|(tx, _)| tx != &tx_id);
                self.store.set(&utxo_news_key, &utxo_news, None)?;
            }
        }

        Ok(())
    }

    fn get_monitors(
        &self,
        current_height: BlockHeight,
    ) -> Result<Vec<TransactionMonitorType>, MonitorStoreError> {
        // This method will return bitvmx instances excluding the onces are not ready to track
        let monitors = self.get_monitors()?;
        let filtered_monitors = monitors
            .into_iter()
            .filter(|(_, height)| *height <= current_height)
            .map(|(monitor_type, _)| monitor_type)
            .collect::<Vec<_>>();

        Ok(filtered_monitors)
    }

    fn save_monitor(
        &self,
        data: TransactionMonitorType,
        start_height: BlockHeight,
    ) -> Result<(), MonitorStoreError> {
        match data {
            TransactionMonitorType::GroupTransaction(id, tx_ids) => {
                let key = self.get_key(TransactionKey::GroupTransactionList);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Id, Vec<Txid>, BlockHeight)>>(&key)?
                    .unwrap_or_default();

                if let Some(pos) = txs.iter().position(|(i, _, _)| *i == id) {
                    for txid in tx_ids {
                        if !txs[pos].1.contains(&txid) {
                            txs[pos].1.push(txid);
                        }
                    }
                } else {
                    txs.push((id, tx_ids, start_height));
                }

                self.store.set(&key, &txs, None)?;
            }
            TransactionMonitorType::SingleTransaction(txid) => {
                let key = self.get_key(TransactionKey::SingleTransactionList);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, BlockHeight)>>(&key)?
                    .unwrap_or_default();

                if !txs.contains(&(txid, start_height)) {
                    txs.push((txid, start_height));
                    self.store.set(&key, &txs, None)?;
                }
            }
            TransactionMonitorType::RskPeginTransaction => {
                let key = self.get_key(TransactionKey::RskPeginTransaction);
                self.store.set(&key, (true, start_height), None)?;
            }
            TransactionMonitorType::SpendingUTXOTransaction(txid, vout) => {
                let key = self.get_key(TransactionKey::SpendingUTXOTransactionList);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, BlockHeight)>>(&key)?
                    .unwrap_or_default();
                if !txs.contains(&(txid, vout, start_height)) {
                    txs.push((txid, vout, start_height));
                    self.store.set(&key, &txs, None)?;
                }
            }
        }

        Ok(())
    }
}

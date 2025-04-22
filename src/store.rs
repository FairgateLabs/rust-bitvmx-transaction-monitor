use crate::{
    errors::MonitorStoreError,
    types::{AckMonitorNews, TypesToMonitor},
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
enum MonitorKey {
    Transactions,
    RskPeginTransaction,
    NewBlock,
    SpendingUTXOTransactions,
    TransactionsNews,
    RskPeginTransactionsNews,
    SpendingUTXOTransactionsNews,
    NewBlockNews,
}

enum BlockchainKey {
    CurrentBlockHeight,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MonitoredTypes {
    Transaction(Txid, String),
    RskPeginTransaction(Txid),
    SpendingUTXOTransaction(Txid, u32, String),
    NewBlock,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum TypesToMonitorStore {
    Transaction(Txid, String),
    SpendingUTXOTransaction(Txid, u32, String),
    RskPeginTransaction,
    NewBlock,
}

pub trait MonitorStoreApi {
    fn get_monitors(&self) -> Result<Vec<TypesToMonitorStore>, MonitorStoreError>;
    fn add_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorStoreError>;
    fn deactivate_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorStoreError>;

    fn get_news(&self) -> Result<Vec<MonitoredTypes>, MonitorStoreError>;
    fn update_news(&self, data: MonitoredTypes) -> Result<(), MonitorStoreError>;
    fn ack_news(&self, data: AckMonitorNews) -> Result<(), MonitorStoreError>;

    fn get_monitor_height(&self) -> Result<BlockHeight, MonitorStoreError>;
    fn update_monitor_height(&self, height: BlockHeight) -> Result<(), MonitorStoreError>;
}

impl MonitorStore {
    pub fn new(store: Rc<Storage>) -> Result<Self, MonitorStoreError> {
        Ok(Self { store })
    }

    fn get_key(&self, key: MonitorKey) -> String {
        let prefix = "monitor";
        match key {
            // Monitors
            MonitorKey::Transactions => format!("{prefix}/tx/list"),
            MonitorKey::RskPeginTransaction => format!("{prefix}/rsk/tx"),
            MonitorKey::SpendingUTXOTransactions => {
                format!("{prefix}/spending/utxo/tx/list")
            }
            MonitorKey::NewBlock => format!("{prefix}/new/block"),

            // News
            MonitorKey::TransactionsNews => format!("{prefix}/tx/news"),
            MonitorKey::RskPeginTransactionsNews => format!("{prefix}/rsk/tx/news"),
            MonitorKey::SpendingUTXOTransactionsNews => {
                format!("{prefix}/spending/utxo/tx/news")
            }
            MonitorKey::NewBlockNews => format!("{prefix}/new/block/news"),
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

    fn update_monitor_height(&self, height: BlockHeight) -> Result<(), MonitorStoreError> {
        let last_block_height_key = self.get_blockchain_key(BlockchainKey::CurrentBlockHeight);
        self.store.set(last_block_height_key, height, None)?;
        Ok(())
    }

    fn get_news(&self) -> Result<Vec<MonitoredTypes>, MonitorStoreError> {
        let mut news = Vec::new();

        let key = self.get_key(MonitorKey::TransactionsNews);
        let txs_news = self
            .store
            .get::<_, Vec<(Txid, String)>>(&key)?
            .unwrap_or_default();

        for (tx_id, extra_data) in txs_news {
            news.push(MonitoredTypes::Transaction(tx_id, extra_data));
        }

        let rsk_news_key = self.get_key(MonitorKey::RskPeginTransactionsNews);
        let rsk_news = self
            .store
            .get::<_, Vec<Txid>>(&rsk_news_key)?
            .unwrap_or_default();

        for tx_id in rsk_news {
            news.push(MonitoredTypes::RskPeginTransaction(tx_id));
        }

        let spending_news_key = self.get_key(MonitorKey::SpendingUTXOTransactionsNews);
        let spending_news = self
            .store
            .get::<_, Vec<(Txid, u32, String)>>(&spending_news_key)?
            .unwrap_or_default();

        for (tx_id, utxo_index, extra_data) in spending_news {
            news.push(MonitoredTypes::SpendingUTXOTransaction(
                tx_id, utxo_index, extra_data,
            ));
        }

        let block_news_key = self.get_key(MonitorKey::NewBlockNews);
        let block_news = self
            .store
            .get::<_, bool>(&block_news_key)?
            .unwrap_or_default();

        if block_news {
            news.push(MonitoredTypes::NewBlock);
        }

        Ok(news)
    }

    fn update_news(&self, data: MonitoredTypes) -> Result<(), MonitorStoreError> {
        match data {
            MonitoredTypes::Transaction(tx_id, extra_data) => {
                let key = self.get_key(MonitorKey::TransactionsNews);
                let mut txs_news = self
                    .store
                    .get::<_, Vec<(Txid, String)>>(&key)?
                    .unwrap_or_default();

                if !txs_news.contains(&(tx_id, extra_data.clone())) {
                    txs_news.push((tx_id, extra_data.clone()));
                }

                self.store.set(&key, &txs_news, None)?;
            }
            MonitoredTypes::RskPeginTransaction(tx_id) => {
                let rsk_news_key = self.get_key(MonitorKey::RskPeginTransactionsNews);
                let mut rsk_news = self
                    .store
                    .get::<_, Vec<Txid>>(&rsk_news_key)?
                    .unwrap_or_default();

                if !rsk_news.contains(&tx_id) {
                    rsk_news.push(tx_id);
                }

                self.store.set(&rsk_news_key, &rsk_news, None)?;
            }
            MonitoredTypes::SpendingUTXOTransaction(tx_id, utxo_index, extra_data) => {
                let utxo_news_key = self.get_key(MonitorKey::SpendingUTXOTransactionsNews);
                let mut utxo_news = self
                    .store
                    .get::<_, Vec<(Txid, u32, String)>>(&utxo_news_key)?
                    .unwrap_or_default();

                if !utxo_news.contains(&(tx_id, utxo_index, extra_data.clone())) {
                    utxo_news.push((tx_id, utxo_index, extra_data));
                }

                self.store.set(&utxo_news_key, &utxo_news, None)?;
            }
            MonitoredTypes::NewBlock => {
                let key = self.get_key(MonitorKey::NewBlockNews);
                self.store.set(&key, true, None)?;
            }
        }

        Ok(())
    }

    fn ack_news(&self, data: AckMonitorNews) -> Result<(), MonitorStoreError> {
        match data {
            AckMonitorNews::Transaction(tx_id) => {
                let key = self.get_key(MonitorKey::TransactionsNews);
                let mut txs_news = self
                    .store
                    .get::<_, Vec<(Txid, String)>>(&key)?
                    .unwrap_or_default();

                txs_news.retain(|(tx, _)| tx != &tx_id);
                self.store.set(&key, &txs_news, None)?;
            }
            AckMonitorNews::RskPeginTransaction(tx_id) => {
                let rsk_news_key = self.get_key(MonitorKey::RskPeginTransactionsNews);
                let mut rsk_news = self
                    .store
                    .get::<_, Vec<Txid>>(&rsk_news_key)?
                    .unwrap_or_default();

                rsk_news.retain(|tx| tx != &tx_id);
                self.store.set(&rsk_news_key, &rsk_news, None)?;
            }
            AckMonitorNews::SpendingUTXOTransaction(tx_id, utxo_index) => {
                let utxo_news_key = self.get_key(MonitorKey::SpendingUTXOTransactionsNews);
                let mut utxo_news = self
                    .store
                    .get::<_, Vec<(Txid, u32, String)>>(&utxo_news_key)?
                    .unwrap_or_default();

                utxo_news.retain(|(tx, utxo_i, _)| *tx != tx_id || *utxo_i != utxo_index);
                self.store.set(&utxo_news_key, &utxo_news, None)?;
            }
            AckMonitorNews::NewBlock => {
                let key = self.get_key(MonitorKey::NewBlockNews);
                self.store.set(&key, false, None)?;
            }
        }

        Ok(())
    }

    fn get_monitors(&self) -> Result<Vec<TypesToMonitorStore>, MonitorStoreError> {
        let mut monitors = Vec::<TypesToMonitorStore>::new();

        let txs_key = self.get_key(MonitorKey::Transactions);
        let txs = self
            .store
            .get::<_, Vec<(Txid, String, bool)>>(txs_key)?
            .unwrap_or_default();

        for (tx_id, extra_data, active) in txs {
            if active {
                monitors.push(TypesToMonitorStore::Transaction(tx_id, extra_data));
            }
        }

        let rsk_pegin_key = self.get_key(MonitorKey::RskPeginTransaction);
        let monitor_rsk_pegin = self
            .store
            .get::<_, bool>(rsk_pegin_key)?
            .unwrap_or_default();

        if monitor_rsk_pegin {
            monitors.push(TypesToMonitorStore::RskPeginTransaction);
        }

        let spending_utxo_key = self.get_key(MonitorKey::SpendingUTXOTransactions);
        let spending_utxos = self
            .store
            .get::<_, Vec<(Txid, u32, String, bool)>>(spending_utxo_key)?
            .unwrap_or_default();

        for (tx_id, utxo_index, extra_data, active) in spending_utxos {
            if active {
                let monitor =
                    TypesToMonitorStore::SpendingUTXOTransaction(tx_id, utxo_index, extra_data);
                monitors.push(monitor);
            }
        }

        let new_block_key = self.get_key(MonitorKey::NewBlock);
        let monitor_new_block = self
            .store
            .get::<_, bool>(new_block_key)?
            .unwrap_or_default();

        if monitor_new_block {
            monitors.push(TypesToMonitorStore::NewBlock);
        }

        Ok(monitors)
    }

    fn add_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorStoreError> {
        match data {
            TypesToMonitor::Transactions(tx_ids, extra_data) => {
                let key = self.get_key(MonitorKey::Transactions);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, String, bool)>>(&key)?
                    .unwrap_or_default();

                for txid in &tx_ids {
                    if let Some(pos) = txs.iter().position(|(i, _, _)| *i == *txid) {
                        // Update the existing entry with the new extra_data if it is empty
                        if txs[pos].1.is_empty() {
                            txs[pos] = (*txid, extra_data.clone(), true);
                        } else {
                            // Keep the existing extra_data and height
                            txs[pos] = (txs[pos].0, txs[pos].1.clone(), true);
                        }
                    } else {
                        // Add a new entry if the txid doesn't exist
                        txs.push((*txid, extra_data.clone(), true));
                    }
                }

                self.store.set(&key, &txs, None)?;
            }
            TypesToMonitor::RskPeginTransaction => {
                let key = self.get_key(MonitorKey::RskPeginTransaction);
                self.store.set(&key, true, None)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(txid, vout, extra_data) => {
                let key = self.get_key(MonitorKey::SpendingUTXOTransactions);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, String, bool)>>(&key)?
                    .unwrap_or_default();

                // Check if the transaction with the same txid, vout, and extra_data already exists
                let exists = txs
                    .iter()
                    .any(|(t, v, e, _)| *t == txid && *v == vout && *e == extra_data);

                if !exists {
                    txs.push((txid, vout, extra_data.clone(), true));
                    self.store.set(&key, &txs, None)?;
                }
            }
            TypesToMonitor::NewBlock => {
                let key = self.get_key(MonitorKey::NewBlock);
                self.store.set(&key, true, None)?;
            }
        }

        Ok(())
    }

    fn deactivate_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorStoreError> {
        match data {
            TypesToMonitor::Transactions(tx_ids, _) => {
                let key = self.get_key(MonitorKey::Transactions);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, String, bool)>>(&key)?
                    .unwrap_or_default();

                // Update active status for matching transactions
                for (txid, _, active) in txs.iter_mut() {
                    if tx_ids.contains(txid) {
                        *active = false;
                    }
                }

                self.store.set(&key, &txs, None)?;
            }

            TypesToMonitor::RskPeginTransaction => {
                let key = self.get_key(MonitorKey::RskPeginTransaction);
                self.store.set(&key, false, None)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(txid, vout, _) => {
                let key = self.get_key(MonitorKey::SpendingUTXOTransactions);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, String, bool)>>(&key)?
                    .unwrap_or_default();

                // Update active status for matching transactions
                for (tx_txid, tx_vout, _, active) in txs.iter_mut() {
                    if *tx_txid == txid && *tx_vout == vout {
                        *active = false;
                    }
                }

                self.store.set(&key, &txs, None)?;
            }
            TypesToMonitor::NewBlock => {
                let key = self.get_key(MonitorKey::NewBlock);
                self.store.set(&key, false, None)?;
            }
        }

        Ok(())
    }
}

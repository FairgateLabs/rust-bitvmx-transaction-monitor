use crate::{
    errors::MonitorStoreError,
    types::{AckMonitorNews, TypesToMonitor},
};
use bitcoin::{BlockHash, Txid};
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
    PendingWork,
}

enum BlockchainKey {
    CurrentBlockHeight,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MonitoredTypes {
    Transaction(Txid, String),
    RskPeginTransaction(Txid),
    SpendingUTXOTransaction(Txid, u32, String, Txid),
    NewBlock(BlockHash),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum TypesToMonitorStore {
    Transaction(Txid, String),
    SpendingUTXOTransaction(Txid, u32, String, Option<Txid>),
    RskPeginTransaction,
    NewBlock,
}

pub trait MonitorStoreApi {
    fn get_monitors(&self) -> Result<Vec<TypesToMonitorStore>, MonitorStoreError>;
    fn add_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorStoreError>;
    fn update_spending_utxo_monitor(
        &self,
        data: (Txid, u32, Option<Txid>),
    ) -> Result<(), MonitorStoreError>;
    fn cancel_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorStoreError>;
    fn deactivate_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorStoreError>;

    fn get_news(&self) -> Result<Vec<MonitoredTypes>, MonitorStoreError>;
    fn update_news(
        &self,
        data: MonitoredTypes,
        current_block_hash: BlockHash,
    ) -> Result<(), MonitorStoreError>;

    fn ack_news(&self, data: AckMonitorNews) -> Result<(), MonitorStoreError>;

    fn get_monitor_height(&self) -> Result<BlockHeight, MonitorStoreError>;
    fn update_monitor_height(&self, height: BlockHeight) -> Result<(), MonitorStoreError>;
    fn has_pending_work(&self) -> Result<bool, MonitorStoreError>;
    fn set_pending_work(&self, is_pending_work: bool) -> Result<(), MonitorStoreError>;
}

impl MonitorStore {
    pub fn new(store: Rc<Storage>) -> Result<Self, MonitorStoreError> {
        Ok(Self { store })
    }

    fn get_key(&self, key: MonitorKey) -> String {
        let prefix = "monitor";
        match key {
            // Monitors
            MonitorKey::PendingWork => format!("{prefix}/all/pending_work"),
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
    fn set_pending_work(&self, is_pending_work: bool) -> Result<(), MonitorStoreError> {
        let key = self.get_key(MonitorKey::PendingWork);
        self.store.set(&key, is_pending_work, None)?;
        Ok(())
    }

    fn has_pending_work(&self) -> Result<bool, MonitorStoreError> {
        let key = self.get_key(MonitorKey::PendingWork);
        let pending_work = self.store.get::<_, bool>(&key)?.unwrap_or(false);
        Ok(pending_work)
    }

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
            .get::<_, Vec<(Txid, String, (BlockHash, bool))>>(&key)?
            .unwrap_or_default();

        for (tx_id, extra_data, (_, ack)) in txs_news {
            if !ack {
                news.push(MonitoredTypes::Transaction(tx_id, extra_data));
            }
        }

        let rsk_news_key = self.get_key(MonitorKey::RskPeginTransactionsNews);
        let rsk_news = self
            .store
            .get::<_, Vec<(Txid, (BlockHash, bool))>>(&rsk_news_key)?
            .unwrap_or_default();

        for (tx_id, (_, ack)) in rsk_news {
            if !ack {
                news.push(MonitoredTypes::RskPeginTransaction(tx_id));
            }
        }

        let spending_news_key = self.get_key(MonitorKey::SpendingUTXOTransactionsNews);
        let spending_news = self
            .store
            .get::<_, Vec<(Txid, u32, String, Txid, (BlockHash, bool))>>(&spending_news_key)?
            .unwrap_or_default();

        for (tx_id, utxo_index, extra_data, spender_tx_id, (_, ack)) in spending_news {
            if !ack {
                news.push(MonitoredTypes::SpendingUTXOTransaction(
                    tx_id,
                    utxo_index,
                    extra_data,
                    spender_tx_id,
                ));
            }
        }

        let block_news_key = self.get_key(MonitorKey::NewBlockNews);
        let block_news = self.store.get::<_, (BlockHash, bool)>(&block_news_key)?;

        if let Some((hash, ack)) = block_news {
            if !ack {
                news.push(MonitoredTypes::NewBlock(hash));
            }
        }

        Ok(news)
    }

    fn update_news(
        &self,
        data: MonitoredTypes,
        current_block_hash: BlockHash,
    ) -> Result<(), MonitorStoreError> {
        // Notifiaction will be updated if the block_hash is different
        // If the notification is already in the store, it will be updated with the new block_hash and ack in false.

        match data {
            MonitoredTypes::Transaction(tx_id, extra_data) => {
                let key = self.get_key(MonitorKey::TransactionsNews);
                let mut txs_news = self
                    .store
                    .get::<_, Vec<(Txid, String, (BlockHash, bool))>>(&key)?
                    .unwrap_or_default();

                let is_new_news = txs_news.iter().position(|(id, _, _)| id == &tx_id);

                match is_new_news {
                    None => {
                        // Insert news with current block hash and ack in false
                        txs_news.push((tx_id, extra_data.clone(), (current_block_hash, false)));
                    }
                    Some(pos) => {
                        let (_, _, (existing_block_hash, _)) = &txs_news[pos];
                        if existing_block_hash == &current_block_hash {
                            // We already have this news, do not update
                            return Ok(());
                        } else {
                            // Replace the notification if the block hash is different
                            txs_news[pos] =
                                (tx_id, extra_data.clone(), (current_block_hash, false));
                        }
                    }
                }

                self.store.set(&key, &txs_news, None)?;
            }
            MonitoredTypes::RskPeginTransaction(tx_id) => {
                let rsk_news_key = self.get_key(MonitorKey::RskPeginTransactionsNews);
                let mut rsk_news = self
                    .store
                    .get::<_, Vec<(Txid, (BlockHash, bool))>>(&rsk_news_key)?
                    .unwrap_or_default();

                let is_new_news = rsk_news.iter().position(|(id, _)| id == &tx_id);

                match is_new_news {
                    None => rsk_news.push((tx_id, (current_block_hash, false))),
                    Some(pos) => {
                        let (_, (existing_block_hash, _)) = &rsk_news[pos];
                        if existing_block_hash == &current_block_hash {
                            // We already have this news, do not update
                            return Ok(());
                        } else {
                            // Replace the notification if the block hash is different
                            rsk_news[pos] = (tx_id, (current_block_hash, false));
                        }
                    }
                }

                self.store.set(&rsk_news_key, &rsk_news, None)?;
            }
            MonitoredTypes::SpendingUTXOTransaction(
                tx_id,
                utxo_index,
                extra_data,
                spender_tx_id,
            ) => {
                let utxo_news_key = self.get_key(MonitorKey::SpendingUTXOTransactionsNews);
                let mut utxo_news = self
                    .store
                    .get::<_, Vec<(Txid, u32, String, Txid, (BlockHash, bool))>>(&utxo_news_key)?
                    .unwrap_or_default();

                let is_new_news = utxo_news
                    .iter()
                    .position(|(id, utxo_i, _, _, _)| id == &tx_id && *utxo_i == utxo_index);

                match is_new_news {
                    None => utxo_news.push((
                        tx_id,
                        utxo_index,
                        extra_data.clone(),
                        spender_tx_id,
                        (current_block_hash, false),
                    )),
                    Some(pos) => {
                        let (_, _, _, _, (_, _)) = &utxo_news[pos];
                        // Replace the notification if the block hash is different
                        utxo_news[pos] = (
                            tx_id,
                            utxo_index,
                            extra_data.clone(),
                            spender_tx_id,
                            (current_block_hash, false),
                        );
                    }
                }

                self.store.set(&utxo_news_key, &utxo_news, None)?;
            }
            MonitoredTypes::NewBlock(hash) => {
                let key = self.get_key(MonitorKey::NewBlockNews);

                let data = self.store.get::<_, (BlockHash, bool)>(&key)?;

                if let Some((last_block_hash, _)) = data {
                    if last_block_hash == hash {
                        // We already have this new block news, do not update
                        return Ok(());
                    } else {
                        // Replace the notification if the block hash is different
                        self.store.set(&key, (current_block_hash, false), None)?;
                    }
                }

                self.store.set(&key, (current_block_hash, false), None)?;
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
                    .get::<_, Vec<(Txid, String, (BlockHash, bool))>>(&key)?
                    .unwrap_or_default();

                if let Some((_, _, (_, ack))) =
                    txs_news.iter_mut().find(|(txid, _, _)| txid == &tx_id)
                {
                    *ack = true;
                    self.store.set(&key, &txs_news, None)?;
                }
            }
            AckMonitorNews::RskPeginTransaction(tx_id) => {
                let key = self.get_key(MonitorKey::RskPeginTransactionsNews);
                let mut txs_news = self
                    .store
                    .get::<_, Vec<(Txid, (BlockHash, bool))>>(&key)?
                    .unwrap_or_default();

                if let Some((_, (_, ack))) = txs_news.iter_mut().find(|(txid, _)| txid == &tx_id) {
                    *ack = true;
                    self.store.set(&key, &txs_news, None)?;
                }
            }
            AckMonitorNews::SpendingUTXOTransaction(tx_id, utxo_index) => {
                let key = self.get_key(MonitorKey::SpendingUTXOTransactionsNews);
                let mut txs_news = self
                    .store
                    .get::<_, Vec<(Txid, u32, String, Txid, (BlockHash, bool))>>(&key)?
                    .unwrap_or_default();

                if let Some((_, _, _, _, (_, ack))) = txs_news
                    .iter_mut()
                    .find(|(tx, utxo_i, _, _, _)| *tx == tx_id && *utxo_i == utxo_index)
                {
                    *ack = true;
                    self.store.set(&key, &txs_news, None)?;
                }
            }
            AckMonitorNews::NewBlock => {
                let key = self.get_key(MonitorKey::NewBlockNews);
                let mut new_block_news = self.store.get::<_, (BlockHash, bool)>(&key)?;

                if let Some((block_hash, _)) = new_block_news.as_mut() {
                    new_block_news = Some((*block_hash, true));
                    self.store.set(&key, new_block_news, None)?;
                }
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
            .get::<_, Vec<(Txid, u32, String, Option<Txid>, bool)>>(spending_utxo_key)?
            .unwrap_or_default();

        for (tx_id, utxo_index, extra_data, tx_id_spending, active) in spending_utxos {
            if active {
                let monitor = TypesToMonitorStore::SpendingUTXOTransaction(
                    tx_id,
                    utxo_index,
                    extra_data,
                    tx_id_spending,
                );
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
                    .get::<_, Vec<(Txid, u32, String, Option<Txid>, bool)>>(&key)?
                    .unwrap_or_default();

                // Check if the transaction with the same txid, vout, and extra_data already exists
                let exists = txs
                    .iter()
                    .any(|(t, v, e, _, _)| *t == txid && *v == vout && *e == extra_data);

                if !exists {
                    txs.push((txid, vout, extra_data.clone(), None, true));
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
                    .get::<_, Vec<(Txid, u32, String, Option<Txid>, bool)>>(&key)?
                    .unwrap_or_default();

                // Update active status for matching transactions
                for (tx_txid, tx_vout, _, _, active) in txs.iter_mut() {
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

    fn cancel_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorStoreError> {
        match data {
            TypesToMonitor::Transactions(tx_ids, _) => {
                let key = self.get_key(MonitorKey::Transactions);
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, String, bool)>>(&key)?
                    .unwrap_or_default();

                txs.retain(|(txid, _, _)| !tx_ids.contains(txid));
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
                    .get::<_, Vec<(Txid, u32, String, Option<Txid>, bool)>>(&key)?
                    .unwrap_or_default();

                txs.retain(|(tx_txid, tx_vout, _, _, _)| *tx_txid != txid || *tx_vout != vout);
                self.store.set(&key, &txs, None)?;
            }
            TypesToMonitor::NewBlock => {
                let key = self.get_key(MonitorKey::NewBlock);
                self.store.set(&key, false, None)?;
            }
        }

        Ok(())
    }

    fn update_spending_utxo_monitor(
        &self,
        data: (Txid, u32, Option<Txid>),
    ) -> Result<(), MonitorStoreError> {
        let key = self.get_key(MonitorKey::SpendingUTXOTransactions);
        let mut txs = self
            .store
            .get::<_, Vec<(Txid, u32, String, Option<Txid>, bool)>>(&key)?
            .unwrap_or_default();

        for (id, utxo_i, _, spender_id, _) in &mut txs {
            if id == &data.0 && *utxo_i == data.1 {
                *spender_id = data.2;
                self.store.set(&key, &txs, None)?;
                break;
            }
        }

        Ok(())
    }
}

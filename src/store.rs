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
    Transactions(bool),
    SpendingUTXOTransactions(bool),
    PendingWork,
    RskPeginTransaction,
    NewBlock,
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
    SpendingUTXOTransaction(Txid, u32, String, Txid),
    NewBlock(BlockHash),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum TypesToMonitorStore {
    Transaction(Txid, String, Option<u32>),
    SpendingUTXOTransaction(Txid, u32, String, Option<Txid>, Option<u32>),
    RskPeginTransaction(Option<u32>),
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
            MonitorKey::Transactions(is_active) => format!(
                "{prefix}/tx/list/{status}",
                status = if is_active { "active" } else { "inactive" }
            ),
            MonitorKey::SpendingUTXOTransactions(is_active) => format!(
                "{prefix}/spending/utxo/tx/list/{status}",
                status = if is_active { "active" } else { "inactive" }
            ),
            MonitorKey::PendingWork => format!("{prefix}/all/pending_work"),
            MonitorKey::RskPeginTransaction => format!("{prefix}/rsk/tx"),
            MonitorKey::NewBlock => format!("{prefix}/new/block"),
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

        // Get active transactions
        let txs_key = self.get_key(MonitorKey::Transactions(true));
        let txs = self
            .store
            .get::<_, Vec<(Txid, String, Option<u32>)>>(&txs_key)?
            .unwrap_or_default();

        for (tx_id, extra_data, number_confirmation_trigger) in txs {
            monitors.push(TypesToMonitorStore::Transaction(
                tx_id,
                extra_data,
                number_confirmation_trigger,
            ));
        }

        // Get RSK pegin transaction monitor
        let rsk_pegin_key = self.get_key(MonitorKey::RskPeginTransaction);
        let monitor_rsk_pegin = self
            .store
            .get::<_, (bool, Option<u32>)>(&rsk_pegin_key)?
            .unwrap_or((false, None));

        if monitor_rsk_pegin.0 {
            monitors.push(TypesToMonitorStore::RskPeginTransaction(
                monitor_rsk_pegin.1,
            ));
        }

        // Get active spending UTXO transactions
        let spending_utxo_key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
        let spending_utxos = self
            .store
            .get::<_, Vec<(Txid, u32, String, Option<Txid>, Option<u32>)>>(&spending_utxo_key)?
            .unwrap_or_default();

        for (tx_id, utxo_index, extra_data, tx_id_spending, number_confirmation_trigger) in
            spending_utxos
        {
            let monitor = TypesToMonitorStore::SpendingUTXOTransaction(
                tx_id,
                utxo_index,
                extra_data,
                tx_id_spending,
                number_confirmation_trigger,
            );
            monitors.push(monitor);
        }

        // Get new block monitor
        let new_block_key = self.get_key(MonitorKey::NewBlock);
        let monitor_new_block = self
            .store
            .get::<_, bool>(&new_block_key)?
            .unwrap_or_default();

        if monitor_new_block {
            monitors.push(TypesToMonitorStore::NewBlock);
        }

        Ok(monitors)
    }

    fn add_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorStoreError> {
        match data {
            TypesToMonitor::Transactions(tx_ids, extra_data, from) => {
                let key = self.get_key(MonitorKey::Transactions(true));

                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, String, Option<u32>)>>(&key)?
                    .unwrap_or_default();

                for txid in &tx_ids {
                    // Add or update in active
                    if let Some(pos) = txs.iter().position(|(i, _, _)| *i == *txid) {
                        // Update the existing entry with the new extra_data if it is empty
                        if txs[pos].1.is_empty() {
                            txs[pos] = (*txid, extra_data.clone(), from);
                        }
                        // Otherwise keep the existing extra_data
                    } else {
                        // Add a new entry if the txid doesn't exist
                        txs.push((*txid, extra_data.clone(), from));
                    }
                }

                self.store.set(&key, &txs, None)?;
            }
            TypesToMonitor::RskPeginTransaction(from) => {
                let key = self.get_key(MonitorKey::RskPeginTransaction);
                self.store.set(&key, (true, from), None)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(txid, vout, extra_data, from) => {
                let key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));

                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, String, Option<Txid>, Option<u32>)>>(&key)?
                    .unwrap_or_default();

                // Check if the transaction with the same txid, vout, and extra_data already exists in active
                let exists = txs
                    .iter()
                    .any(|(t, v, e, _, _)| *t == txid && *v == vout && *e == extra_data);

                if !exists {
                    txs.push((txid, vout, extra_data.clone(), None, from));
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
            TypesToMonitor::Transactions(tx_ids, _, _) => {
                let active_key = self.get_key(MonitorKey::Transactions(true));
                let inactive_key = self.get_key(MonitorKey::Transactions(false));

                let mut active_txs = self
                    .store
                    .get::<_, Vec<(Txid, String, Option<u32>)>>(&active_key)?
                    .unwrap_or_default();

                let mut inactive_txs = self
                    .store
                    .get::<_, Vec<(Txid, String, Option<u32>)>>(&inactive_key)?
                    .unwrap_or_default();

                // Move matching transactions from active to inactive
                let mut to_move = Vec::new();
                active_txs.retain(|(txid, extra_data, number_confirmation_trigger)| {
                    if tx_ids.contains(txid) {
                        to_move.push((*txid, extra_data.clone(), *number_confirmation_trigger));
                        false // Remove from active
                    } else {
                        true // Keep in active
                    }
                });

                // Add to inactive (avoid duplicates)
                for (txid, extra_data, number_confirmation_trigger) in to_move {
                    if !inactive_txs.iter().any(|(i, _, _)| *i == txid) {
                        inactive_txs.push((txid, extra_data, number_confirmation_trigger));
                    }
                }

                self.store.set(&active_key, &active_txs, None)?;
                self.store.set(&inactive_key, &inactive_txs, None)?;
            }

            TypesToMonitor::RskPeginTransaction(from) => {
                let key = self.get_key(MonitorKey::RskPeginTransaction);
                self.store.set(&key, (false, from), None)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(txid, vout, _, _) => {
                let active_key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
                let inactive_key = self.get_key(MonitorKey::SpendingUTXOTransactions(false));

                let mut active_txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, String, Option<Txid>, Option<u32>)>>(&active_key)?
                    .unwrap_or_default();

                let mut inactive_txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, String, Option<Txid>, Option<u32>)>>(&inactive_key)?
                    .unwrap_or_default();

                // Move matching transaction from active to inactive
                let mut to_move = None;
                active_txs.retain(
                    |(tx_txid, tx_vout, extra_data, spender_tx_id, number_confirmation_trigger)| {
                        if *tx_txid == txid && *tx_vout == vout {
                            to_move = Some((
                                *tx_txid,
                                *tx_vout,
                                extra_data.clone(),
                                spender_tx_id.clone(),
                                *number_confirmation_trigger,
                            ));
                            false // Remove from active
                        } else {
                            true // Keep in active
                        }
                    },
                );

                // Add to inactive if not already present
                if let Some((t, v, e, spender_tx_id, number_confirmation_trigger)) = to_move {
                    if !inactive_txs
                        .iter()
                        .any(|(ti, vi, _, _, _)| *ti == t && *vi == v)
                    {
                        inactive_txs.push((t, v, e, spender_tx_id, number_confirmation_trigger));
                    }
                }

                self.store.set(&active_key, &active_txs, None)?;
                self.store.set(&inactive_key, &inactive_txs, None)?;
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
            TypesToMonitor::Transactions(tx_ids, _, _) => {
                let active_key = self.get_key(MonitorKey::Transactions(true));
                let inactive_key = self.get_key(MonitorKey::Transactions(false));

                let mut active_txs = self
                    .store
                    .get::<_, Vec<(Txid, String, Option<u32>)>>(&active_key)?
                    .unwrap_or_default();

                let mut inactive_txs = self
                    .store
                    .get::<_, Vec<(Txid, String, Option<u32>)>>(&inactive_key)?
                    .unwrap_or_default();

                active_txs.retain(|(txid, _, _)| !tx_ids.contains(txid));
                inactive_txs.retain(|(txid, _, _)| !tx_ids.contains(txid));

                self.store.set(&active_key, &active_txs, None)?;
                self.store.set(&inactive_key, &inactive_txs, None)?;
            }
            TypesToMonitor::RskPeginTransaction(from) => {
                let key = self.get_key(MonitorKey::RskPeginTransaction);
                self.store.set(&key, (false, from), None)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(txid, vout, _, _) => {
                let active_key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
                let inactive_key = self.get_key(MonitorKey::SpendingUTXOTransactions(false));

                let mut active_txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, String, Option<Txid>, Option<u32>)>>(&active_key)?
                    .unwrap_or_default();

                let mut inactive_txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, String, Option<Txid>, Option<u32>)>>(&inactive_key)?
                    .unwrap_or_default();

                active_txs
                    .retain(|(tx_txid, tx_vout, _, _, _)| *tx_txid != txid || *tx_vout != vout);
                inactive_txs
                    .retain(|(tx_txid, tx_vout, _, _, _)| *tx_txid != txid || *tx_vout != vout);

                self.store.set(&active_key, &active_txs, None)?;
                self.store.set(&inactive_key, &inactive_txs, None)?;
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
        let key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
        let mut txs = self
            .store
            .get::<_, Vec<(Txid, u32, String, Option<Txid>, Option<u32>)>>(&key)?
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

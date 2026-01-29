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
    RskPegin,
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
    SpendingUTXOTransaction(Txid, u32, String, Option<u32>),
    NewBlock,
    RskPegin(Option<u32>),
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

    fn get_transaction_trigger_sent(
        &self,
        tx_id: Txid,
        extra_data: &str,
    ) -> Result<bool, MonitorStoreError>;
    fn update_transaction_trigger_sent(
        &self,
        tx_id: Txid,
        extra_data: &str,
        trigger_sent: bool,
    ) -> Result<(), MonitorStoreError>;
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
            MonitorKey::RskPegin => format!("{prefix}/rsk/pegin"),
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
            .get::<_, Vec<(Txid, Vec<(String, Option<u32>, bool)>)>>(&txs_key)?
            .unwrap_or_default();

        for (tx_id, tx_data) in txs {
            for (extra_data, confirmations, _) in tx_data {
                monitors.push(TypesToMonitorStore::Transaction(
                    tx_id,
                    extra_data,
                    confirmations,
                ));
            }
        }

        // Get RSK pegin monitor (if active)
        let rsk_pegin_key = self.get_key(MonitorKey::RskPegin);
        let rsk_pegin_active: Option<(bool, Option<u32>)> = self.store.get(&rsk_pegin_key)?;

        if let Some((active, from)) = rsk_pegin_active {
            if active {
                monitors.push(TypesToMonitorStore::RskPegin(from));
            }
        }

        // Get active spending UTXO transactions from list
        let spending_utxo_key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
        let spending_utxos = self
            .store
            .get::<_, Vec<(Txid, u32, Vec<(String, Option<Txid>, Option<u32>)>)>>(
                &spending_utxo_key,
            )?
            .unwrap_or_default();

        for (tx_id, utxo_index, tx_data) in spending_utxos {
            for (extra_data, _spender_tx_id, confirmations) in tx_data {
                monitors.push(TypesToMonitorStore::SpendingUTXOTransaction(
                    tx_id,
                    utxo_index,
                    extra_data,
                    confirmations,
                ));
            }
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
                    .get::<_, Vec<(Txid, Vec<(String, Option<u32>, bool)>)>>(&key)?
                    .unwrap_or_default();

                for txid in &tx_ids {
                    if let Some((_, tx_data)) = txs.iter_mut().find(|(id, _)| id == txid) {
                        // If tx exists and extra_data is the same, override Option<u32> and move trigger sent in false
                        if let Some(pos) = tx_data.iter().position(|(e, _, _)| e == &extra_data) {
                            let (_, _, _) = tx_data[pos].clone();
                            tx_data[pos] = (extra_data.clone(), from, false);
                        } else {
                            // If extra_data is different, add it as a new tx_id-to-monitor entry
                            tx_data.push((extra_data.clone(), from, false));
                        }
                    } else {
                        // New txid, store it with its first (extra_data, trigger) entry
                        txs.push((*txid, vec![(extra_data.clone(), from, false)]));
                    }
                }

                self.store.set(&key, &txs, None)?;
            }
            TypesToMonitor::RskPegin(from) => {
                let key = self.get_key(MonitorKey::RskPegin);
                self.store.set(&key, (true, from), None)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(txid, vout, extra_data, from) => {
                let key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
                let mut txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, Vec<(String, Option<Txid>, Option<u32>)>)>>(&key)?
                    .unwrap_or_default();

                if let Some((_, _, tx_data)) =
                    txs.iter_mut().find(|(t, v, _)| *t == txid && *v == vout)
                {
                    // If extra_data is the same, override confirmation trigger and keep spender_tx_id
                    if let Some(pos) = tx_data
                        .iter()
                        .position(|(e, _spender, _from)| e == &extra_data)
                    {
                        let (_, existing_spender_tx_id, _) = tx_data[pos].clone();
                        tx_data[pos] = (extra_data.clone(), existing_spender_tx_id, from);
                    } else {
                        // If extra_data is different, add it as a new entry
                        tx_data.push((extra_data.clone(), None, from));
                    }
                } else {
                    // New (txid,vout)
                    txs.push((txid, vout, vec![(extra_data.clone(), None, from)]));
                }

                self.store.set(&key, &txs, None)?;
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
            TypesToMonitor::Transactions(tx_ids, extra_data, _) => {
                let active_key = self.get_key(MonitorKey::Transactions(true));
                let inactive_key = self.get_key(MonitorKey::Transactions(false));

                let mut active_txs = self
                    .store
                    .get::<_, Vec<(Txid, Vec<(String, Option<u32>, bool)>)>>(&active_key)?
                    .unwrap_or_default();

                let mut inactive_txs = self
                    .store
                    .get::<_, Vec<(Txid, Vec<(String, Option<u32>, bool)>)>>(&inactive_key)?
                    .unwrap_or_default();

                // Move matching transactions from active to inactive
                // For each matching txid, move only the entry with matching extra_data
                let mut to_move = Vec::new();
                for txid in &tx_ids {
                    if let Some((_, tx_info)) = active_txs.iter_mut().find(|(id, _)| *id == *txid) {
                        // Find and remove the entry with matching extra_data
                        tx_info.retain(|(context_data, confirmations, trigger_sent)| {
                            if *context_data == extra_data {
                                to_move.push((
                                    *txid,
                                    context_data.clone(),
                                    *confirmations,
                                    *trigger_sent,
                                ));
                                false // Remove from active
                            } else {
                                true // Keep in active
                            }
                        });

                        // If no tx_data left for this txid, remove the txid entirely
                        if tx_info.is_empty() {
                            active_txs.retain(|(id, _)| *id != *txid);
                        }
                    }
                }

                // Add moved entries to inactive
                for (txid, extra_data, confirmations, trigger_sent) in to_move {
                    if let Some((_, inactive_tx_data)) =
                        inactive_txs.iter_mut().find(|(id, _)| *id == txid)
                    {
                        // Add to existing inactive txid (avoid duplicates)
                        if !inactive_tx_data.iter().any(|(ie, _, _)| *ie == extra_data) {
                            inactive_tx_data.push((extra_data, confirmations, trigger_sent));
                        }
                    } else {
                        // Create new inactive txid entry
                        inactive_txs.push((txid, vec![(extra_data, confirmations, trigger_sent)]));
                    }
                }

                self.store.set(&active_key, &active_txs, None)?;
                self.store.set(&inactive_key, &inactive_txs, None)?;
            }

            TypesToMonitor::RskPegin(from) => {
                let key = self.get_key(MonitorKey::RskPegin);
                self.store.set(&key, (false, from), None)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(txid, vout, extra_data, _) => {
                let active_key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
                let inactive_key = self.get_key(MonitorKey::SpendingUTXOTransactions(false));

                let mut active_txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, Vec<(String, Option<Txid>, Option<u32>)>)>>(
                        &active_key,
                    )?
                    .unwrap_or_default();

                let mut inactive_txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, Vec<(String, Option<Txid>, Option<u32>)>)>>(
                        &inactive_key,
                    )?
                    .unwrap_or_default();

                // Move matching transaction from active to inactive
                // Find the matching (txid, vout) and move only the entry with matching extra_data
                let mut to_move = None;
                if let Some((_, _, tx_data)) = active_txs
                    .iter_mut()
                    .find(|(t, v, _)| *t == txid && *v == vout)
                {
                    // Find and remove the entry with matching extra_data
                    tx_data.retain(|(context_data, spender, confirmation_trigger)| {
                        if *context_data == extra_data {
                            to_move = Some((
                                txid,
                                vout,
                                context_data.clone(),
                                *spender,
                                *confirmation_trigger,
                            ));
                            false // Remove from active
                        } else {
                            true // Keep in active
                        }
                    });

                    // If no tx_data left for this (txid, vout), remove it entirely
                    if tx_data.is_empty() {
                        active_txs.retain(|(t, v, _)| *t != txid || *v != vout);
                    }
                }

                // Add moved entry to inactive
                if let Some((t, v, e, spender, confirmations)) = to_move {
                    if let Some((_, _, inactive_tx_data)) = inactive_txs
                        .iter_mut()
                        .find(|(ti, vi, _)| *ti == t && *vi == v)
                    {
                        // Add to existing inactive (txid, vout) (avoid duplicates)
                        if !inactive_tx_data.iter().any(|(ie, _, _)| *ie == e) {
                            inactive_tx_data.push((e, spender, confirmations));
                        }
                    } else {
                        // Create new inactive (txid, vout) entry
                        inactive_txs.push((t, v, vec![(e, spender, confirmations)]));
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
            TypesToMonitor::Transactions(tx_ids, extra_data, _) => {
                let active_key = self.get_key(MonitorKey::Transactions(true));
                let inactive_key = self.get_key(MonitorKey::Transactions(false));

                let mut active_txs = self
                    .store
                    .get::<_, Vec<(Txid, Vec<(String, Option<u32>, bool)>)>>(&active_key)?
                    .unwrap_or_default();

                let mut inactive_txs = self
                    .store
                    .get::<_, Vec<(Txid, Vec<(String, Option<u32>, bool)>)>>(&inactive_key)?
                    .unwrap_or_default();

                // Remove only the entry with matching extra_data for each txid
                for txid in &tx_ids {
                    // Remove from active
                    if let Some((_, tx_data)) = active_txs.iter_mut().find(|(id, _)| *id == *txid) {
                        tx_data.retain(|(e, _, _)| *e != extra_data);
                        // If no tx_data left for this txid, remove the txid entirely
                        if tx_data.is_empty() {
                            active_txs.retain(|(id, _)| *id != *txid);
                        }
                    }

                    // Remove from inactive
                    if let Some((_, tx_data)) = inactive_txs.iter_mut().find(|(id, _)| *id == *txid)
                    {
                        tx_data.retain(|(e, _, _)| *e != extra_data);
                        // If no tx_data left for this txid, remove the txid entirely
                        if tx_data.is_empty() {
                            inactive_txs.retain(|(id, _)| *id != *txid);
                        }
                    }
                }

                self.store.set(&active_key, &active_txs, None)?;
                self.store.set(&inactive_key, &inactive_txs, None)?;
            }
            TypesToMonitor::RskPegin(from) => {
                let key = self.get_key(MonitorKey::RskPegin);
                self.store.set(&key, (false, from), None)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(txid, vout, extra_data, _) => {
                let active_key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
                let inactive_key = self.get_key(MonitorKey::SpendingUTXOTransactions(false));

                let mut active_txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, Vec<(String, Option<Txid>, Option<u32>)>)>>(
                        &active_key,
                    )?
                    .unwrap_or_default();

                let mut inactive_txs = self
                    .store
                    .get::<_, Vec<(Txid, u32, Vec<(String, Option<Txid>, Option<u32>)>)>>(
                        &inactive_key,
                    )?
                    .unwrap_or_default();

                // Remove only the entry with matching extra_data from active
                if let Some((_, _, tx_data)) = active_txs
                    .iter_mut()
                    .find(|(t, v, _)| *t == txid && *v == vout)
                {
                    tx_data.retain(|(e, _, _)| *e != extra_data);
                    // If no tx_data left for this (txid, vout), remove it entirely
                    if tx_data.is_empty() {
                        active_txs.retain(|(t, v, _)| *t != txid || *v != vout);
                    }
                }

                // Remove only the entry with matching extra_data from inactive
                if let Some((_, _, tx_data)) = inactive_txs
                    .iter_mut()
                    .find(|(t, v, _)| *t == txid && *v == vout)
                {
                    tx_data.retain(|(e, _, _)| *e != extra_data);
                    // If no tx_data left for this (txid, vout), remove it entirely
                    if tx_data.is_empty() {
                        inactive_txs.retain(|(t, v, _)| *t != txid || *v != vout);
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

    fn update_spending_utxo_monitor(
        &self,
        data: (Txid, u32, Option<Txid>),
    ) -> Result<(), MonitorStoreError> {
        // Update spender_tx_id for the given (txid,vout) across all entries.
        let key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
        let mut txs = self
            .store
            .get::<_, Vec<(Txid, u32, Vec<(String, Option<Txid>, Option<u32>)>)>>(&key)?
            .unwrap_or_default();

        if let Some((_, _, tx_data)) = txs
            .iter_mut()
            .find(|(txid, vout, _)| *txid == data.0 && *vout == data.1)
        {
            for (_extra_data, spender_tx_id, _from) in tx_data.iter_mut() {
                *spender_tx_id = data.2;
            }
            self.store.set(&key, &txs, None)?;
        }

        Ok(())
    }

    fn get_transaction_trigger_sent(
        &self,
        tx_id: Txid,
        extra_data: &str,
    ) -> Result<bool, MonitorStoreError> {
        let key = self.get_key(MonitorKey::Transactions(true));
        let txs = self
            .store
            .get::<_, Vec<(Txid, Vec<(String, Option<u32>, bool)>)>>(&key)?
            .unwrap_or_default();

        if let Some((_, tx_data)) = txs.iter().find(|(id, _)| *id == tx_id) {
            if let Some((_, _, trigger_sent)) = tx_data.iter().find(|(e, _, _)| e == extra_data) {
                Ok(*trigger_sent)
            } else {
                Err(MonitorStoreError::TransactionNotFound(format!(
                    "Transaction with tx_id {} and extra_data {} not found when trying to get trigger_sent flag",
                    tx_id, extra_data
                )))
            }
        } else {
            Err(MonitorStoreError::TransactionNotFound(format!(
                "Transaction with tx_id {} not found when trying to get trigger_sent flag",
                tx_id
            )))
        }
    }

    fn update_transaction_trigger_sent(
        &self,
        tx_id: Txid,
        extra_data: &str,
        trigger_sent: bool,
    ) -> Result<(), MonitorStoreError> {
        let key = self.get_key(MonitorKey::Transactions(true));
        let mut txs = self
            .store
            .get::<_, Vec<(Txid, Vec<(String, Option<u32>, bool)>)>>(&key)?
            .unwrap_or_default();

        if let Some((_, tx_data)) = txs.iter_mut().find(|(id, _)| *id == tx_id) {
            if let Some(pos) = tx_data.iter().position(|(e, _, _)| e == extra_data) {
                let (e, from, _) = tx_data[pos].clone();
                tx_data[pos] = (e, from, trigger_sent);
                self.store.set(&key, &txs, None)?;
            }
        }

        Ok(())
    }
}

use crate::{
    errors::MonitorStoreError,
    types::{
        AckMonitorNews, NewsAck, OutputPatternFilter, OutputPatternNewsEntry,
        OutputPatternSubscription, SpendingUTXOMonitor,
        SpendingUTXOMonitorEntry, SpendingUTXONewsEntry, TransactionMonitor,
        TransactionMonitorEntry, TransactionNewsEntry, TypesToMonitor,
    },
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
    NewBlock,
    TransactionsNews,
    SpendingUTXOTransactionsNews,
    NewBlockNews,
    OutputPatternSubscriptions,
    OutputPatternTransactionsNews,
}

enum BlockchainKey {
    CurrentBlockHeight,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MonitoredTypes {
    Transaction(Txid, String),
    SpendingUTXOTransaction(Txid, u32, String, Txid),
    NewBlock(BlockHash),
    OutputPatternTransaction(Txid, Vec<u8>),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum TypesToMonitorStore {
    Transaction(Txid, String, Option<u32>),
    SpendingUTXOTransaction(Txid, u32, String, Option<u32>),
    NewBlock,
    OutputPattern(OutputPatternFilter, Option<u32>),
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
            MonitorKey::NewBlock => format!("{prefix}/new/block"),
            MonitorKey::TransactionsNews => format!("{prefix}/tx/news"),
            MonitorKey::SpendingUTXOTransactionsNews => {
                format!("{prefix}/spending/utxo/tx/news")
            }
            MonitorKey::NewBlockNews => format!("{prefix}/new/block/news"),
            MonitorKey::OutputPatternSubscriptions => {
                format!("{prefix}/output_pattern/subscriptions")
            }
            MonitorKey::OutputPatternTransactionsNews => {
                format!("{prefix}/output_pattern/tx/news")
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
    fn set_pending_work(&self, is_pending_work: bool) -> Result<(), MonitorStoreError> {
        let key = self.get_key(MonitorKey::PendingWork);
        self.store.set(&key, is_pending_work, None)?;
        Ok(())
    }

    fn has_pending_work(&self) -> Result<bool, MonitorStoreError> {
        let key = self.get_key(MonitorKey::PendingWork);
        let pending_work = self.store.get::<_, bool>(&key, None)?.unwrap_or(false);
        Ok(pending_work)
    }

    fn get_monitor_height(&self) -> Result<BlockHeight, MonitorStoreError> {
        let last_block_height_key = self.get_blockchain_key(BlockchainKey::CurrentBlockHeight);
        let last_block_height = self
            .store
            .get::<_, BlockHeight>(&last_block_height_key, None)?
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
        let txs_news: Vec<TransactionNewsEntry> = self.store.get(&key, None)?.unwrap_or_default();

        for entry in txs_news {
            if !entry.ack.acknowledged {
                news.push(MonitoredTypes::Transaction(entry.tx_id, entry.extra_data));
            }
        }

        let spending_news_key = self.get_key(MonitorKey::SpendingUTXOTransactionsNews);
        let spending_news: Vec<SpendingUTXONewsEntry> =
            self.store.get(&spending_news_key, None)?.unwrap_or_default();

        for entry in spending_news {
            if !entry.ack.acknowledged {
                news.push(MonitoredTypes::SpendingUTXOTransaction(
                    entry.tx_id,
                    entry.utxo_index,
                    entry.extra_data,
                    entry.spender_tx_id,
                ));
            }
        }

        let op_news_key = self.get_key(MonitorKey::OutputPatternTransactionsNews);
        let op_news: Vec<OutputPatternNewsEntry> =
            self.store.get(&op_news_key, None)?.unwrap_or_default();

        for entry in op_news {
            if !entry.ack.acknowledged {
                news.push(MonitoredTypes::OutputPatternTransaction(
                    entry.tx_id,
                    entry.tag,
                ));
            }
        }

        let block_news_key = self.get_key(MonitorKey::NewBlockNews);
        let block_news: Option<NewsAck> = self.store.get(&block_news_key, None)?;

        if let Some(ack) = block_news {
            if !ack.acknowledged {
                news.push(MonitoredTypes::NewBlock(ack.block_hash));
            }
        }

        Ok(news)
    }

    fn update_news(
        &self,
        data: MonitoredTypes,
        current_block_hash: BlockHash,
    ) -> Result<(), MonitorStoreError> {
        // Notification will be updated if the block_hash is different
        // If the notification is already in the store, it will be updated with the new block_hash and ack set to false.

        match data {
            MonitoredTypes::Transaction(tx_id, extra_data) => {
                let key = self.get_key(MonitorKey::TransactionsNews);
                let mut txs_news: Vec<TransactionNewsEntry> =
                    self.store.get(&key, None)?.unwrap_or_default();

                // Check if news already exists for this (tx_id, extra_data) combination
                // Different extra_data should generate separate news entries
                let is_new_news = txs_news
                    .iter()
                    .position(|e| e.tx_id == tx_id && e.extra_data == extra_data);

                match is_new_news {
                    None => {
                        // Insert news with current block hash and ack in false
                        txs_news.push(TransactionNewsEntry {
                            tx_id,
                            extra_data: extra_data.clone(),
                            ack: NewsAck::new(current_block_hash, false),
                        });
                    }
                    Some(pos) => {
                        if txs_news[pos].ack.block_hash != current_block_hash {
                            // Replace the notification with the new block hash
                            txs_news[pos] = TransactionNewsEntry {
                                tx_id,
                                extra_data: extra_data.clone(),
                                ack: NewsAck::new(current_block_hash, false),
                            };
                        }
                    }
                }

                self.store.set(&key, &txs_news, None)?;
            }
            MonitoredTypes::SpendingUTXOTransaction(
                tx_id,
                utxo_index,
                extra_data,
                spender_tx_id,
            ) => {
                let utxo_news_key = self.get_key(MonitorKey::SpendingUTXOTransactionsNews);
                let mut utxo_news: Vec<SpendingUTXONewsEntry> =
                    self.store.get(&utxo_news_key, None)?.unwrap_or_default();

                // Check if news already exists for this (tx_id, utxo_index, extra_data)
                // Different extra_data should generate separate news entries
                let is_new_news = utxo_news.iter().position(|e| {
                    e.tx_id == tx_id && e.utxo_index == utxo_index && e.extra_data == extra_data
                });

                match is_new_news {
                    None => utxo_news.push(SpendingUTXONewsEntry {
                        tx_id,
                        utxo_index,
                        extra_data: extra_data.clone(),
                        spender_tx_id,
                        ack: NewsAck::new(current_block_hash, false),
                    }),
                    Some(pos) => {
                        // Replace the notification only if the block hash is different
                        if utxo_news[pos].ack.block_hash != current_block_hash {
                            utxo_news[pos] = SpendingUTXONewsEntry {
                                tx_id,
                                utxo_index,
                                extra_data: extra_data.clone(),
                                spender_tx_id,
                                ack: NewsAck::new(current_block_hash, false),
                            };
                        }
                    }
                }

                self.store.set(&utxo_news_key, &utxo_news, None)?;
            }
            MonitoredTypes::OutputPatternTransaction(tx_id, tag) => {
                let key = self.get_key(MonitorKey::OutputPatternTransactionsNews);
                let mut op_news: Vec<OutputPatternNewsEntry> =
                    self.store.get(&key, None)?.unwrap_or_default();

                let existing = op_news
                    .iter()
                    .position(|e| e.tx_id == tx_id && e.tag == tag);

                match existing {
                    None => {
                        op_news.push(OutputPatternNewsEntry {
                            tx_id,
                            tag,
                            ack: NewsAck::new(current_block_hash, false),
                        });
                    }
                    Some(pos) => {
                        if op_news[pos].ack.block_hash != current_block_hash {
                            op_news[pos] = OutputPatternNewsEntry {
                                tx_id,
                                tag,
                                ack: NewsAck::new(current_block_hash, false),
                            };
                        }
                    }
                }

                self.store.set(&key, &op_news, None)?;
            }
            MonitoredTypes::NewBlock(hash) => {
                let key = self.get_key(MonitorKey::NewBlockNews);

                let data: Option<NewsAck> = self.store.get(&key, None)?;

                if let Some(ack) = data {
                    if ack.block_hash != hash {
                        // Replace the notification with the new block hash
                        self.store
                            .set(&key, NewsAck::new(current_block_hash, false), None)?;
                    }
                } else {
                    self.store
                        .set(&key, NewsAck::new(current_block_hash, false), None)?;
                }
            }
        }

        Ok(())
    }

    fn ack_news(&self, data: AckMonitorNews) -> Result<(), MonitorStoreError> {
        match data {
            AckMonitorNews::Transaction(tx_id, extra_data) => {
                let key = self.get_key(MonitorKey::TransactionsNews);
                let mut txs_news: Vec<TransactionNewsEntry> =
                    self.store.get(&key, None)?.unwrap_or_default();

                // Acknowledge only the news entry matching both tx_id and extra_data
                if let Some(entry) = txs_news
                    .iter_mut()
                    .find(|e| e.tx_id == tx_id && e.extra_data == extra_data)
                {
                    entry.ack.acknowledged = true;
                    self.store.set(&key, &txs_news, None)?;
                }
            }
            AckMonitorNews::SpendingUTXOTransaction(tx_id, utxo_index, extra_data) => {
                let key = self.get_key(MonitorKey::SpendingUTXOTransactionsNews);
                let mut txs_news: Vec<SpendingUTXONewsEntry> =
                    self.store.get(&key, None)?.unwrap_or_default();

                // Acknowledge only the news entry matching (tx_id, utxo_index, extra_data)
                if let Some(entry) = txs_news.iter_mut().find(|e| {
                    e.tx_id == tx_id && e.utxo_index == utxo_index && e.extra_data == extra_data
                }) {
                    entry.ack.acknowledged = true;
                    self.store.set(&key, &txs_news, None)?;
                }
            }
            AckMonitorNews::OutputPatternTransaction(tx_id, tag) => {
                let key = self.get_key(MonitorKey::OutputPatternTransactionsNews);
                let mut op_news: Vec<OutputPatternNewsEntry> =
                    self.store.get(&key, None)?.unwrap_or_default();

                if let Some(entry) = op_news
                    .iter_mut()
                    .find(|e| e.tx_id == tx_id && e.tag == tag)
                {
                    entry.ack.acknowledged = true;
                    self.store.set(&key, &op_news, None)?;
                }
            }
            AckMonitorNews::NewBlock => {
                let key = self.get_key(MonitorKey::NewBlockNews);
                let mut new_block_news: Option<NewsAck> = self.store.get(&key, None)?;

                if let Some(ack) = new_block_news.as_mut() {
                    ack.acknowledged = true;
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
        let txs: Vec<TransactionMonitor> = self.store.get(&txs_key, None)?.unwrap_or_default();

        for monitor in txs {
            for entry in monitor.entries {
                monitors.push(TypesToMonitorStore::Transaction(
                    monitor.tx_id,
                    entry.extra_data,
                    entry.confirmation_trigger,
                ));
            }
        }

        // Get active spending UTXO transactions from list
        let spending_utxo_key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
        let spending_utxos: Vec<SpendingUTXOMonitor> =
            self.store.get(&spending_utxo_key, None)?.unwrap_or_default();

        for monitor in spending_utxos {
            for entry in monitor.entries {
                monitors.push(TypesToMonitorStore::SpendingUTXOTransaction(
                    monitor.tx_id,
                    monitor.vout,
                    entry.extra_data,
                    entry.confirmation_trigger,
                ));
            }
        }

        // Get output pattern subscriptions
        let op_key = self.get_key(MonitorKey::OutputPatternSubscriptions);
        let op_subscriptions: Vec<OutputPatternSubscription> =
            self.store.get(&op_key, None)?.unwrap_or_default();

        for sub in op_subscriptions {
            monitors.push(TypesToMonitorStore::OutputPattern(
                sub.filter,
                sub.confirmation_trigger,
            ));
        }

        // Get new block monitor
        let new_block_key = self.get_key(MonitorKey::NewBlock);
        let monitor_new_block = self
            .store
            .get::<_, bool>(&new_block_key, None)?
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
                let mut txs: Vec<TransactionMonitor> = self.store.get(&key, None)?.unwrap_or_default();

                for txid in &tx_ids {
                    if let Some(monitor) = txs.iter_mut().find(|m| m.tx_id == *txid) {
                        // If tx exists and extra_data is the same, override Option<u32> and move trigger sent in false
                        if let Some(pos) = monitor
                            .entries
                            .iter()
                            .position(|e| e.extra_data == extra_data)
                        {
                            monitor.entries[pos] = TransactionMonitorEntry {
                                extra_data: extra_data.clone(),
                                confirmation_trigger: from,
                                trigger_sent: false,
                            };
                        } else {
                            // If extra_data is different, add it as a new tx_id-to-monitor entry
                            monitor.entries.push(TransactionMonitorEntry {
                                extra_data: extra_data.clone(),
                                confirmation_trigger: from,
                                trigger_sent: false,
                            });
                        }
                    } else {
                        // New txid, store it with its first (extra_data, trigger) entry
                        txs.push(TransactionMonitor {
                            tx_id: *txid,
                            entries: vec![TransactionMonitorEntry {
                                extra_data: extra_data.clone(),
                                confirmation_trigger: from,
                                trigger_sent: false,
                            }],
                        });
                    }
                }

                self.store.set(&key, &txs, None)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(txid, vout, extra_data, from) => {
                let key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
                let mut txs: Vec<SpendingUTXOMonitor> = self.store.get(&key, None)?.unwrap_or_default();

                if let Some(monitor) = txs.iter_mut().find(|m| m.tx_id == txid && m.vout == vout) {
                    // If extra_data is the same, override confirmation trigger and keep spender_tx_id
                    if let Some(pos) = monitor
                        .entries
                        .iter()
                        .position(|e| e.extra_data == extra_data)
                    {
                        let existing_spender_tx_id = monitor.entries[pos].spender_tx_id;
                        monitor.entries[pos] = SpendingUTXOMonitorEntry {
                            extra_data: extra_data.clone(),
                            spender_tx_id: existing_spender_tx_id,
                            confirmation_trigger: from,
                        };
                    } else {
                        // If extra_data is different, add it as a new entry
                        monitor.entries.push(SpendingUTXOMonitorEntry {
                            extra_data: extra_data.clone(),
                            spender_tx_id: None,
                            confirmation_trigger: from,
                        });
                    }
                } else {
                    // New (txid,vout)
                    txs.push(SpendingUTXOMonitor {
                        tx_id: txid,
                        vout,
                        entries: vec![SpendingUTXOMonitorEntry {
                            extra_data: extra_data.clone(),
                            spender_tx_id: None,
                            confirmation_trigger: from,
                        }],
                    });
                }

                self.store.set(&key, &txs, None)?;
            }
            TypesToMonitor::OutputPattern(filter, confirmation_trigger) => {
                let key = self.get_key(MonitorKey::OutputPatternSubscriptions);
                let mut subs: Vec<OutputPatternSubscription> =
                    self.store.get(&key, None)?.unwrap_or_default();

                // Add or update subscription by filter identity
                if let Some(existing) = subs.iter_mut().find(|s| s.filter == filter) {
                    existing.confirmation_trigger = confirmation_trigger;
                } else {
                    subs.push(OutputPatternSubscription {
                        filter,
                        confirmation_trigger,
                    });
                }

                self.store.set(&key, &subs, None)?;
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

                let mut active_txs: Vec<TransactionMonitor> =
                    self.store.get(&active_key, None)?.unwrap_or_default();

                let mut inactive_txs: Vec<TransactionMonitor> =
                    self.store.get(&inactive_key, None)?.unwrap_or_default();

                // Move matching transactions from active to inactive
                // For each matching txid, move only the entry with matching extra_data
                let mut to_move = Vec::new();
                for txid in &tx_ids {
                    if let Some(monitor) = active_txs.iter_mut().find(|m| m.tx_id == *txid) {
                        // Find and remove the entry with matching extra_data
                        let mut entry_to_move = None;
                        monitor.entries.retain(|e| {
                            if e.extra_data == extra_data {
                                entry_to_move = Some(e.clone());
                                false // Remove from active
                            } else {
                                true // Keep in active
                            }
                        });

                        // If no entries left for this txid, remove the txid entirely
                        if monitor.entries.is_empty() {
                            active_txs.retain(|m| m.tx_id != *txid);
                        }

                        if let Some(entry) = entry_to_move {
                            to_move.push((*txid, entry));
                        }
                    }
                }

                // Add moved entries to inactive
                for (txid, entry) in to_move {
                    if let Some(monitor) = inactive_txs.iter_mut().find(|m| m.tx_id == txid) {
                        // Add to existing inactive txid (avoid duplicates)
                        if !monitor
                            .entries
                            .iter()
                            .any(|e| e.extra_data == entry.extra_data)
                        {
                            monitor.entries.push(entry);
                        }
                    } else {
                        // Create new inactive txid entry
                        inactive_txs.push(TransactionMonitor {
                            tx_id: txid,
                            entries: vec![entry],
                        });
                    }
                }

                self.store.set(&active_key, &active_txs, None)?;
                self.store.set(&inactive_key, &inactive_txs, None)?;
            }

            TypesToMonitor::SpendingUTXOTransaction(txid, vout, extra_data, _) => {
                let active_key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
                let inactive_key = self.get_key(MonitorKey::SpendingUTXOTransactions(false));

                let mut active_txs: Vec<SpendingUTXOMonitor> =
                    self.store.get(&active_key, None)?.unwrap_or_default();

                let mut inactive_txs: Vec<SpendingUTXOMonitor> =
                    self.store.get(&inactive_key, None)?.unwrap_or_default();

                // Move matching transaction from active to inactive
                // Find the matching (txid, vout) and move only the entry with matching extra_data
                let mut entry_to_move = None;
                if let Some(monitor) = active_txs
                    .iter_mut()
                    .find(|m| m.tx_id == txid && m.vout == vout)
                {
                    // Find and remove the entry with matching extra_data
                    monitor.entries.retain(|e| {
                        if e.extra_data == extra_data {
                            entry_to_move = Some(e.clone());
                            false // Remove from active
                        } else {
                            true // Keep in active
                        }
                    });

                    // If no entries left for this (txid, vout), remove it entirely
                    if monitor.entries.is_empty() {
                        active_txs.retain(|m| m.tx_id != txid || m.vout != vout);
                    }
                }

                // Add moved entry to inactive
                if let Some(entry) = entry_to_move {
                    if let Some(monitor) = inactive_txs
                        .iter_mut()
                        .find(|m| m.tx_id == txid && m.vout == vout)
                    {
                        // Add to existing inactive (txid, vout) (avoid duplicates)
                        if !monitor
                            .entries
                            .iter()
                            .any(|e| e.extra_data == entry.extra_data)
                        {
                            monitor.entries.push(entry);
                        }
                    } else {
                        // Create new inactive (txid, vout) entry
                        inactive_txs.push(SpendingUTXOMonitor {
                            tx_id: txid,
                            vout,
                            entries: vec![entry],
                        });
                    }
                }

                self.store.set(&active_key, &active_txs, None)?;
                self.store.set(&inactive_key, &inactive_txs, None)?;
            }
            TypesToMonitor::OutputPattern(filter, _) => {
                let key = self.get_key(MonitorKey::OutputPatternSubscriptions);
                let mut subs: Vec<OutputPatternSubscription> =
                    self.store.get(&key, None)?.unwrap_or_default();
                subs.retain(|s| s.filter != filter);
                self.store.set(&key, &subs, None)?;
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

                let mut active_txs: Vec<TransactionMonitor> =
                    self.store.get(&active_key, None)?.unwrap_or_default();

                let mut inactive_txs: Vec<TransactionMonitor> =
                    self.store.get(&inactive_key, None)?.unwrap_or_default();

                // Remove only the entry with matching extra_data for each txid
                for txid in &tx_ids {
                    // Remove from active
                    if let Some(monitor) = active_txs.iter_mut().find(|m| m.tx_id == *txid) {
                        monitor.entries.retain(|e| e.extra_data != extra_data);
                        // If no entries left for this txid, remove the txid entirely
                        if monitor.entries.is_empty() {
                            active_txs.retain(|m| m.tx_id != *txid);
                        }
                    }

                    // Remove from inactive
                    if let Some(monitor) = inactive_txs.iter_mut().find(|m| m.tx_id == *txid) {
                        monitor.entries.retain(|e| e.extra_data != extra_data);
                        // If no entries left for this txid, remove the txid entirely
                        if monitor.entries.is_empty() {
                            inactive_txs.retain(|m| m.tx_id != *txid);
                        }
                    }
                }

                self.store.set(&active_key, &active_txs, None)?;
                self.store.set(&inactive_key, &inactive_txs, None)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(txid, vout, extra_data, _) => {
                let active_key = self.get_key(MonitorKey::SpendingUTXOTransactions(true));
                let inactive_key = self.get_key(MonitorKey::SpendingUTXOTransactions(false));

                let mut active_txs: Vec<SpendingUTXOMonitor> =
                    self.store.get(&active_key, None)?.unwrap_or_default();

                let mut inactive_txs: Vec<SpendingUTXOMonitor> =
                    self.store.get(&inactive_key, None)?.unwrap_or_default();

                // Remove only the entry with matching extra_data from active
                if let Some(monitor) = active_txs
                    .iter_mut()
                    .find(|m| m.tx_id == txid && m.vout == vout)
                {
                    monitor.entries.retain(|e| e.extra_data != extra_data);
                    // If no entries left for this (txid, vout), remove it entirely
                    if monitor.entries.is_empty() {
                        active_txs.retain(|m| m.tx_id != txid || m.vout != vout);
                    }
                }

                // Remove only the entry with matching extra_data from inactive
                if let Some(monitor) = inactive_txs
                    .iter_mut()
                    .find(|m| m.tx_id == txid && m.vout == vout)
                {
                    monitor.entries.retain(|e| e.extra_data != extra_data);
                    // If no entries left for this (txid, vout), remove it entirely
                    if monitor.entries.is_empty() {
                        inactive_txs.retain(|m| m.tx_id != txid || m.vout != vout);
                    }
                }

                self.store.set(&active_key, &active_txs, None)?;
                self.store.set(&inactive_key, &inactive_txs, None)?;
            }
            TypesToMonitor::OutputPattern(filter, _) => {
                let key = self.get_key(MonitorKey::OutputPatternSubscriptions);
                let mut subs: Vec<OutputPatternSubscription> =
                    self.store.get(&key, None)?.unwrap_or_default();
                subs.retain(|s| s.filter != filter);
                self.store.set(&key, &subs, None)?;
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
        let mut txs: Vec<SpendingUTXOMonitor> = self.store.get(&key, None)?.unwrap_or_default();

        if let Some(monitor) = txs
            .iter_mut()
            .find(|m| m.tx_id == data.0 && m.vout == data.1)
        {
            for entry in monitor.entries.iter_mut() {
                entry.spender_tx_id = data.2;
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
        let txs: Vec<TransactionMonitor> = self.store.get(&key, None)?.unwrap_or_default();

        if let Some(monitor) = txs.iter().find(|m| m.tx_id == tx_id) {
            if let Some(entry) = monitor.entries.iter().find(|e| e.extra_data == extra_data) {
                Ok(entry.trigger_sent)
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
        let mut txs: Vec<TransactionMonitor> = self.store.get(&key, None)?.unwrap_or_default();

        if let Some(monitor) = txs.iter_mut().find(|m| m.tx_id == tx_id) {
            if let Some(entry) = monitor
                .entries
                .iter_mut()
                .find(|e| e.extra_data == extra_data)
            {
                entry.trigger_sent = trigger_sent;
                self.store.set(&key, &txs, None)?;
            }
        }

        Ok(())
    }
}

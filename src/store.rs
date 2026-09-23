//! # Store Module
//!
//! This module provides the storage layer for the transaction monitor.
//!
//! ## Storage Structure
//!
//! One list per kind of monitor, where a record holds a target and every subscription to it, plus the new block
//! subscription, the pending work flag, and one news list per kind of monitor.

use crate::{
    errors::MonitorError,
    types::{
        AckMonitorNews, MonitorEntry, NewBlockNewsEntry, NewsAck, OutputPatternMonitor,
        OutputPatternNewsEntry, SpendingUTXONewsEntry, SpendingUtxoMonitor, TransactionMonitor,
        TransactionMonitorEntry, TransactionNewsEntry, TypesToMonitor,
    },
};
use bitcoin::{BlockHash, OutPoint, Txid};
use bitvmx_bitcoin_rpc::types::BlockHeight;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use storage_backend::storage::{KeyValueStore, Storage};

pub struct MonitorStore {
    store: Rc<Storage>,
}

enum MonitorKey {
    Transactions,
    SpendingUtxos,
    OutputPatterns,
    NewBlock,
    PendingWork,
    TransactionsNews,
    SpendingUtxosNews,
    OutputPatternsNews,
    NewBlockNews,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MonitoredTypes {
    Transaction(Txid, String, bool), // Txid, context, resent_due_to_reorg.
    SpendingUTXOTransaction(OutPoint, String, Txid), // Spent outpoint, context, spender.
    NewBlock(BlockHeight, BlockHash), // Height and hash of the indexed block.
    OutputPatternTransaction(Txid, Vec<u8>),
}

impl MonitorStore {
    pub fn new(store: Rc<Storage>) -> Self {
        Self { store }
    }

    fn get_key(&self, key: MonitorKey) -> String {
        let prefix = "monitor";
        match key {
            MonitorKey::Transactions => format!("{prefix}/tx/list"),
            MonitorKey::SpendingUtxos => format!("{prefix}/spending/utxo/tx/list"),
            MonitorKey::OutputPatterns => format!("{prefix}/output_pattern/list"),
            MonitorKey::NewBlock => format!("{prefix}/new/block"),
            MonitorKey::PendingWork => format!("{prefix}/all/pending_work"),
            MonitorKey::TransactionsNews => format!("{prefix}/tx/news"),
            MonitorKey::SpendingUtxosNews => format!("{prefix}/spending/utxo/tx/news"),
            MonitorKey::OutputPatternsNews => format!("{prefix}/output_pattern/tx/news"),
            MonitorKey::NewBlockNews => format!("{prefix}/new/block/news"),
        }
    }

    /// Reads a stored list, which is empty until something is written to it.
    fn get_list<T: for<'de> Deserialize<'de>>(
        &self,
        key: MonitorKey,
    ) -> Result<Vec<T>, MonitorError> {
        let key = self.get_key(key);
        Ok(self.store.get(&key, None)?.unwrap_or_default())
    }

    fn set_list<T: Serialize>(&self, key: MonitorKey, list: &[T]) -> Result<(), MonitorError> {
        let key = self.get_key(key);
        self.store.set(&key, list, None)?;
        Ok(())
    }

    // =========================================================================
    // Pending work
    // =========================================================================

    pub fn set_pending_work(&self, is_pending_work: bool) -> Result<(), MonitorError> {
        let key = self.get_key(MonitorKey::PendingWork);
        self.store.set(&key, is_pending_work, None)?;
        Ok(())
    }

    /// Whether a monitor was registered since the last tick. False while the key is unwritten, which is nothing to do.
    pub fn has_pending_work(&self) -> Result<bool, MonitorError> {
        let key = self.get_key(MonitorKey::PendingWork);
        let pending_work = self.store.get::<_, bool>(&key, None)?.unwrap_or(false);
        Ok(pending_work)
    }

    // =========================================================================
    // News
    // =========================================================================

    pub fn get_news(&self) -> Result<Vec<MonitoredTypes>, MonitorError> {
        let mut news = Vec::new();

        let txs_news: Vec<TransactionNewsEntry> = self.get_list(MonitorKey::TransactionsNews)?;

        for entry in txs_news {
            if !entry.ack.acknowledged {
                news.push(MonitoredTypes::Transaction(
                    entry.tx_id,
                    entry.context,
                    entry.resent_due_to_reorg,
                ));
            }
        }

        let spending_news: Vec<SpendingUTXONewsEntry> =
            self.get_list(MonitorKey::SpendingUtxosNews)?;

        for entry in spending_news {
            if !entry.ack.acknowledged {
                news.push(MonitoredTypes::SpendingUTXOTransaction(
                    entry.outpoint,
                    entry.context,
                    entry.spender_tx_id,
                ));
            }
        }

        let op_news: Vec<OutputPatternNewsEntry> = self.get_list(MonitorKey::OutputPatternsNews)?;

        for entry in op_news {
            if !entry.ack.acknowledged {
                news.push(MonitoredTypes::OutputPatternTransaction(
                    entry.tx_id,
                    entry.tag,
                ));
            }
        }

        let block_news_key = self.get_key(MonitorKey::NewBlockNews);
        let block_news: Option<NewBlockNewsEntry> = self.store.get(&block_news_key, None)?;

        if let Some(entry) = block_news {
            if !entry.ack.acknowledged {
                news.push(MonitoredTypes::NewBlock(entry.height, entry.ack.block_hash));
            }
        }

        Ok(news)
    }

    pub fn update_news(
        &self,
        data: MonitoredTypes,
        current_block_hash: BlockHash,
    ) -> Result<(), MonitorError> {
        // A notification is replaced, and made unacknowledged again, when it is about a different block than the
        // one already stored for it.

        match data {
            MonitoredTypes::Transaction(tx_id, context, resent_due_to_reorg) => {
                let mut txs_news: Vec<TransactionNewsEntry> =
                    self.get_list(MonitorKey::TransactionsNews)?;

                let entry = TransactionNewsEntry {
                    tx_id,
                    context: context.clone(),
                    ack: NewsAck::new(current_block_hash, false),
                    resent_due_to_reorg,
                };

                // Different contexts on the same transaction are separate news entries.
                match txs_news
                    .iter()
                    .position(|e| e.tx_id == tx_id && e.context == context)
                {
                    None => txs_news.push(entry),
                    Some(pos) => {
                        if txs_news[pos].ack.block_hash != current_block_hash {
                            txs_news[pos] = entry;
                        }
                    }
                }

                self.set_list(MonitorKey::TransactionsNews, &txs_news)?;
            }
            MonitoredTypes::OutputPatternTransaction(tx_id, tag) => {
                let mut op_news: Vec<OutputPatternNewsEntry> =
                    self.get_list(MonitorKey::OutputPatternsNews)?;

                let entry = OutputPatternNewsEntry {
                    tx_id,
                    tag: tag.clone(),
                    ack: NewsAck::new(current_block_hash, false),
                };

                match op_news
                    .iter()
                    .position(|e| e.tx_id == tx_id && e.tag == tag)
                {
                    None => op_news.push(entry),
                    Some(pos) => {
                        if op_news[pos].ack.block_hash != current_block_hash {
                            op_news[pos] = entry;
                        }
                    }
                }

                self.set_list(MonitorKey::OutputPatternsNews, &op_news)?;
            }
            MonitoredTypes::SpendingUTXOTransaction(outpoint, context, spender_tx_id) => {
                let mut utxo_news: Vec<SpendingUTXONewsEntry> =
                    self.get_list(MonitorKey::SpendingUtxosNews)?;

                let entry = SpendingUTXONewsEntry {
                    outpoint,
                    context: context.clone(),
                    spender_tx_id,
                    ack: NewsAck::new(current_block_hash, false),
                };

                // Different contexts on the same UTXO are separate news entries.
                match utxo_news
                    .iter()
                    .position(|e| e.outpoint == outpoint && e.context == context)
                {
                    None => utxo_news.push(entry),
                    Some(pos) => {
                        if utxo_news[pos].ack.block_hash != current_block_hash {
                            utxo_news[pos] = entry;
                        }
                    }
                }

                self.set_list(MonitorKey::SpendingUtxosNews, &utxo_news)?;
            }
            MonitoredTypes::NewBlock(height, hash) => {
                let key = self.get_key(MonitorKey::NewBlockNews);
                let stored: Option<NewBlockNewsEntry> = self.store.get(&key, None)?;

                let entry = NewBlockNewsEntry {
                    height,
                    ack: NewsAck::new(hash, false),
                };

                match stored {
                    Some(stored) if stored.ack.block_hash == hash => {}
                    _ => self.store.set(&key, entry, None)?,
                }
            }
        }

        Ok(())
    }

    pub fn ack_news(&self, data: AckMonitorNews) -> Result<(), MonitorError> {
        match data {
            AckMonitorNews::Transaction(tx_id, context) => {
                let mut txs_news: Vec<TransactionNewsEntry> =
                    self.get_list(MonitorKey::TransactionsNews)?;

                // Acknowledge only the news entry matching both the txid and the context.
                if let Some(entry) = txs_news
                    .iter_mut()
                    .find(|e| e.tx_id == tx_id && e.context == context)
                {
                    entry.ack.acknowledged = true;
                    self.set_list(MonitorKey::TransactionsNews, &txs_news)?;
                }
            }
            AckMonitorNews::OutputPatternTransaction(tx_id, tag) => {
                let mut op_news: Vec<OutputPatternNewsEntry> =
                    self.get_list(MonitorKey::OutputPatternsNews)?;

                if let Some(entry) = op_news
                    .iter_mut()
                    .find(|e| e.tx_id == tx_id && e.tag == tag)
                {
                    entry.ack.acknowledged = true;
                    self.set_list(MonitorKey::OutputPatternsNews, &op_news)?;
                }
            }
            AckMonitorNews::SpendingUTXOTransaction(outpoint, context) => {
                let mut utxo_news: Vec<SpendingUTXONewsEntry> =
                    self.get_list(MonitorKey::SpendingUtxosNews)?;

                if let Some(entry) = utxo_news
                    .iter_mut()
                    .find(|e| e.outpoint == outpoint && e.context == context)
                {
                    entry.ack.acknowledged = true;
                    self.set_list(MonitorKey::SpendingUtxosNews, &utxo_news)?;
                }
            }
            AckMonitorNews::NewBlock => {
                let key = self.get_key(MonitorKey::NewBlockNews);
                let mut new_block_news: Option<NewBlockNewsEntry> = self.store.get(&key, None)?;

                if let Some(entry) = new_block_news.as_mut() {
                    entry.ack.acknowledged = true;
                    self.store.set(&key, new_block_news, None)?;
                }
            }
        }

        Ok(())
    } // =========================================================================
      // Monitors
      // =========================================================================

    pub fn get_transaction_monitors(&self) -> Result<Vec<TransactionMonitor>, MonitorError> {
        self.get_list(MonitorKey::Transactions)
    }

    pub fn get_spending_utxo_monitors(&self) -> Result<Vec<SpendingUtxoMonitor>, MonitorError> {
        self.get_list(MonitorKey::SpendingUtxos)
    }

    pub fn get_output_pattern_monitors(&self) -> Result<Vec<OutputPatternMonitor>, MonitorError> {
        self.get_list(MonitorKey::OutputPatterns)
    }

    /// Whether new block news was subscribed to. False while the key is unwritten, which is nobody subscribed.
    pub fn is_monitoring_new_block(&self) -> Result<bool, MonitorError> {
        let key = self.get_key(MonitorKey::NewBlock);
        Ok(self.store.get::<_, bool>(&key, None)?.unwrap_or(false))
    }

    /// Adds a subscription, or replaces the one this target already has under the same context.
    pub fn add_monitor(
        &self,
        data: TypesToMonitor,
        search_in_mempool: bool,
    ) -> Result<(), MonitorError> {
        match data {
            TypesToMonitor::Transactions(tx_ids, context, confirmation_trigger) => {
                let mut monitors = self.get_transaction_monitors()?;

                for tx_id in tx_ids {
                    let entry =
                        MonitorEntry::new(context.clone(), confirmation_trigger, search_in_mempool);

                    match monitors.iter_mut().find(|m| m.tx_id == tx_id) {
                        Some(monitor) => monitor.add_or_replace(entry),
                        None => monitors.push(TransactionMonitor {
                            tx_id,
                            entries: vec![TransactionMonitorEntry {
                                entry,
                                notified_block_hash: None,
                            }],
                        }),
                    }
                }

                self.set_list(MonitorKey::Transactions, &monitors)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(outpoint, context, confirmation_trigger) => {
                let mut monitors = self.get_spending_utxo_monitors()?;

                let entry = MonitorEntry::new(context, confirmation_trigger, search_in_mempool);

                match monitors.iter_mut().find(|m| m.outpoint == outpoint) {
                    Some(monitor) => monitor.add_or_replace(entry),
                    None => monitors.push(SpendingUtxoMonitor {
                        outpoint,
                        entries: vec![entry],
                    }),
                }

                self.set_list(MonitorKey::SpendingUtxos, &monitors)?;
            }
            TypesToMonitor::OutputPattern(filter, confirmation_trigger) => {
                let mut monitors = self.get_output_pattern_monitors()?;

                let entry =
                    MonitorEntry::new(String::new(), confirmation_trigger, search_in_mempool);

                match monitors.iter_mut().find(|m| m.filter == filter) {
                    Some(monitor) => monitor.add_or_replace(entry),
                    None => monitors.push(OutputPatternMonitor {
                        filter,
                        entries: vec![entry],
                    }),
                }

                self.set_list(MonitorKey::OutputPatterns, &monitors)?;
            }
            TypesToMonitor::NewBlock => {
                let key = self.get_key(MonitorKey::NewBlock);
                self.store.set(&key, true, None)?;
            }
        }

        Ok(())
    }

    /// Removes a subscription. A target with no subscription left is removed with it.
    pub fn remove_monitor(&self, data: TypesToMonitor) -> Result<(), MonitorError> {
        match data {
            TypesToMonitor::Transactions(tx_ids, context, _) => {
                let mut monitors = self.get_transaction_monitors()?;

                for tx_id in &tx_ids {
                    if let Some(monitor) = monitors.iter_mut().find(|m| m.tx_id == *tx_id) {
                        monitor.entries.retain(|e| e.entry.context != context);
                    }
                }
                monitors.retain(|m| !m.entries.is_empty());

                self.set_list(MonitorKey::Transactions, &monitors)?;
            }
            TypesToMonitor::SpendingUTXOTransaction(outpoint, context, _) => {
                let mut monitors = self.get_spending_utxo_monitors()?;

                if let Some(monitor) = monitors.iter_mut().find(|m| m.outpoint == outpoint) {
                    monitor.entries.retain(|e| e.context != context);
                }
                monitors.retain(|m| !m.entries.is_empty());

                self.set_list(MonitorKey::SpendingUtxos, &monitors)?;
            }
            TypesToMonitor::OutputPattern(filter, _) => {
                let mut monitors = self.get_output_pattern_monitors()?;
                monitors.retain(|m| m.filter != filter);

                self.set_list(MonitorKey::OutputPatterns, &monitors)?;
            }
            TypesToMonitor::NewBlock => {
                let key = self.get_key(MonitorKey::NewBlock);
                self.store.set(&key, false, None)?;
            }
        }

        Ok(())
    }

    /// Block hash of the block that included the transaction the last time its confirmation trigger fired.
    pub fn get_transaction_notified_block_hash(
        &self,
        tx_id: Txid,
        context: &str,
    ) -> Result<Option<BlockHash>, MonitorError> {
        let monitors = self.get_transaction_monitors()?;

        let notified_block_hash = monitors
            .iter()
            .find(|m| m.tx_id == tx_id)
            .and_then(|m| m.entries.iter().find(|e| e.entry.context == context))
            .and_then(|e| e.notified_block_hash);

        Ok(notified_block_hash)
    }

    pub fn update_transaction_notified_block_hash(
        &self,
        tx_id: Txid,
        context: &str,
        block_hash: Option<BlockHash>,
    ) -> Result<(), MonitorError> {
        let mut monitors = self.get_transaction_monitors()?;

        if let Some(entry) = monitors
            .iter_mut()
            .find(|m| m.tx_id == tx_id)
            .and_then(|m| m.entries.iter_mut().find(|e| e.entry.context == context))
        {
            entry.notified_block_hash = block_hash;
            self.set_list(MonitorKey::Transactions, &monitors)?;
        }

        Ok(())
    }
}

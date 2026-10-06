//! Persistence. It reads and writes records, the first check queue and the pending news.

use bitcoin::{OutPoint, Txid};
use bitvmx_bitcoin_rpc::types::BlockHeight;
use std::borrow::Borrow;
use std::rc::Rc;
use storage_backend::storage::{KeyValueStore, Storage};

use crate::errors::MonitorError;
use crate::types::{MonitorNews, MonitorRecord, MonitorTarget, NewsKind, OutputPatternFilter};

const PREFIX: &str = "monitor";

/// Every key the monitor owns.
enum StoreKey {
    TransactionMonitor(Txid), // One transaction and every subscription to it.
    SpendingUtxoMonitor(OutPoint), // One outpoint and every subscription to its spending.
    OutputPatternMonitor(OutputPatternFilter), // One pattern and every subscription to it.
    NewBlockMonitor, // The subscriptions to every new block. There is only ever one such record.
    FirstCheckQueue, // The targets waiting for their first lookup, so in the next tick they can be checked in previous blocks.
    TransactionNews(Txid), // Pending news about one transaction, in the order it was decided.
    BlockNews(BlockHeight), // Pending news about the blocks at one height.
}

/// A view of the one database the consumer created.
pub struct MonitorStore {
    store: Rc<Storage>,
}

impl MonitorStore {
    pub fn new(store: Rc<Storage>) -> Self {
        Self { store }
    }

    /// The record of one target, or None when nobody is subscribed to it.
    pub fn get_record(
        &self,
        target: &MonitorTarget,
    ) -> Result<Option<MonitorRecord>, MonitorError> {
        Ok(self.store.get(self.record_key(target), None)?)
    }

    /// Writes one record under its own target's key, so it never rewrites another.
    pub fn save_record(&self, record: &MonitorRecord) -> Result<(), MonitorError> {
        self.store
            .set(self.record_key(&record.target), record, None)?;
        Ok(())
    }

    /// Writes several records, which is what registering a whole list of targets in one call does.
    pub fn save_records(&self, records: &[MonitorRecord]) -> Result<(), MonitorError> {
        for record in records {
            self.save_record(record)?;
        }

        Ok(())
    }

    /// Removes a record, which is what the last entry of a target leaving does.
    pub fn delete_record(&self, target: &MonitorTarget) -> Result<(), MonitorError> {
        self.store.remove(self.record_key(target), None)?;
        Ok(())
    }

    pub fn get_transaction_records(&self) -> Result<Vec<MonitorRecord>, MonitorError> {
        self.records_in(&self.transaction_space())
    }

    pub fn get_spending_utxo_records(&self) -> Result<Vec<MonitorRecord>, MonitorError> {
        self.records_in(&self.spending_utxo_space())
    }

    pub fn get_output_pattern_records(&self) -> Result<Vec<MonitorRecord>, MonitorError> {
        self.records_in(&self.output_pattern_space())
    }

    /// Every record that can track a transaction, one scan per key space. The new block record is left out: it tracks none.
    pub fn get_all_records(&self) -> Result<Vec<MonitorRecord>, MonitorError> {
        let mut records = self.get_transaction_records()?;
        records.extend(self.get_spending_utxo_records()?);
        records.extend(self.get_output_pattern_records()?);

        Ok(records)
    }

    // ================================
    //  CHECK QUEUE
    // ================================

    /// New targets registered for their first special lookup. An empty queue means there is nothing to look up.
    pub fn get_first_check_queue(&self) -> Result<Vec<MonitorTarget>, MonitorError> {
        let key = self.get_key(StoreKey::FirstCheckQueue);
        Ok(self.store.get(key, None)?.unwrap_or_default())
    }

    /// Queues new targets for their first special lookup, in one big read and one big write however many there are.
    /// A target already queued is not added again, but it stills costs one read to find that out.
    pub fn queue_first_checks(&self, targets: &[MonitorTarget]) -> Result<(), MonitorError> {
        let mut queue = self.get_first_check_queue()?;
        let queued = queue.len();

        for target in targets {
            if !queue.contains(target) {
                queue.push(target.clone());
            }
        }

        if queue.len() == queued {
            return Ok(()); // Every one of them was already queued, so nothing is written.
        }

        self.store
            .set(self.get_key(StoreKey::FirstCheckQueue), &queue, None)?;

        Ok(())
    }

    /// Empties the queue, which is how draining it in one pass ends.
    pub fn clear_first_check_queue(&self) -> Result<(), MonitorError> {
        self.store
            .remove(self.get_key(StoreKey::FirstCheckQueue), None)?;
        Ok(())
    }

    // ================================
    //  NEWS
    // ================================

    /// Every pending item, whatever key it is under.
    pub fn get_all_news(&self) -> Result<Vec<MonitorNews>, MonitorError> {
        // Block news lives below this space, so one scan reads both it and the news about transactions.
        let per_key: Vec<Vec<MonitorNews>> = self.store.partial_get(&self.news_space(), None)?;
        Ok(per_key.into_iter().flatten().collect())
    }

    /// Every pending item about one transaction, which is one key and so one read.
    pub fn get_news_of_transaction(&self, tx_id: &Txid) -> Result<Vec<MonitorNews>, MonitorError> {
        let key = self.get_key(StoreKey::TransactionNews(*tx_id));
        Ok(self.store.get(key, None)?.unwrap_or_default())
    }

    /// Appends what a tick decided to report. Items stay in the order they were decided.
    pub fn add_news(&self, news: Vec<MonitorNews>) -> Result<(), MonitorError> {
        // One read and one write per key, however many items went into it.
        for (key, grouped) in self.group_by_key(news)? {
            // What is already there is everything the consumer has not acknowledged
            let mut pending: Vec<MonitorNews> = self.store.get(&key, None)?.unwrap_or_default();

            pending.extend(grouped);
            self.store.set(&key, &pending, None)?;
        }

        Ok(())
    }

    /// Removes these items from the keys they belong to, and a key once nothing is pending under it, so storage
    /// holds exactly what the consumer has not seen.
    pub fn remove_news(&self, news: &[MonitorNews]) -> Result<(), MonitorError> {
        // One read and one write per key, however many of the items sit under it.
        for (key, removing) in self.group_by_key(news.iter())? {
            let mut pending: Vec<MonitorNews> = self.store.get(&key, None)?.unwrap_or_default();
            let stored = pending.len();

            for item in removing {
                // Every copy of it goes (possible in a reorg restoring the very same block).
                pending.retain(|other| other != item);
            }

            if pending.len() == stored {
                continue; // Nothing under this key was there to remove, so it is not rewritten.
            }

            match pending.is_empty() {
                true => self.store.remove(&key, None)?, // Nothing pending under it, so the key goes as well.
                false => self.store.set(&key, &pending, None)?,
            }
        }

        Ok(())
    }

    // =========================================================================
    // Internal
    // =========================================================================

    /// The key each variant is stored under.
    fn get_key(&self, key: StoreKey) -> String {
        match key {
            StoreKey::TransactionMonitor(tx_id) => format!("{}{tx_id}", self.transaction_space()),
            StoreKey::SpendingUtxoMonitor(utxo) => {
                format!("{}{}:{}", self.spending_utxo_space(), utxo.txid, utxo.vout)
            }
            // Every field of the filter is in the key, because a different bound on the outputs is a different rule
            // and gets its own record. That makes the key and the target the same thing, so a record always holds
            // the filter it is stored under and no registration can change another one's bound.
            StoreKey::OutputPatternMonitor(filter) => format!(
                "{}{}:{}:{}",
                self.output_pattern_space(),
                filter.output_index,
                hex::encode(&filter.tag),
                filter.max_outputs.map_or_else(|| "any".to_string(), |max| max.to_string())
            ),
            StoreKey::NewBlockMonitor => format!("{PREFIX}/newblock"),
            StoreKey::FirstCheckQueue => format!("{PREFIX}/first_check"),
            StoreKey::TransactionNews(tx_id) => format!("{}{tx_id}", self.news_space()),
            // Padded so the keys sort by height, which is what keeps block news in the order it happened.
            StoreKey::BlockNews(height) => format!("{}block/{height:010}", self.news_space()),
        }
    }

    /// The key holding a target's record.
    fn record_key(&self, target: &MonitorTarget) -> String {
        self.get_key(match target {
            MonitorTarget::Transaction(tx_id) => StoreKey::TransactionMonitor(*tx_id),
            MonitorTarget::SpendingUtxo(utxo) => StoreKey::SpendingUtxoMonitor(*utxo),
            MonitorTarget::OutputPattern(filter) => StoreKey::OutputPatternMonitor(filter.clone()),
            MonitorTarget::NewBlock => StoreKey::NewBlockMonitor,
        })
    }

    /// The key holding an item of news: the transaction it is about, or the height of the block it reports.
    fn news_key(&self, news: &MonitorNews) -> Result<String, MonitorError> {
        let key = match (&news.kind, &news.target) {
            (NewsKind::Transaction { txid, .. }, _) => StoreKey::TransactionNews(*txid),
            (NewsKind::Block(block), _) => StoreKey::BlockNews(block.height),
            (NewsKind::Unreachable, MonitorTarget::SpendingUtxo(utxo)) => {
                StoreKey::TransactionNews(utxo.txid)
            }
            // Only a UTXO first check can find a spend it will never reach, so anything else is a bug.
            (NewsKind::Unreachable, target) => {
                return Err(MonitorError::InvariantViolation(format!(
                    "{target:?} produced unreachable news, which only a spending UTXO can"
                )))
            }
        };

        Ok(self.get_key(key))
    }

    /// Which of these items belong to which key, in the order they were given. Both adding and removing group
    /// first and touch storage afterwards, so a key that several items land in is read and written once instead
    /// of once per item. Two contexts watching the same transaction is what makes that the normal case.
    fn group_by_key<T: Borrow<MonitorNews>>(
        &self,
        news: impl IntoIterator<Item = T>,
    ) -> Result<Vec<(String, Vec<T>)>, MonitorError> {
        let mut by_key: Vec<(String, Vec<T>)> = Vec::new();

        // Nothing is read or written here: this only works out the grouping.
        for item in news {
            let key = self.news_key(item.borrow())?;

            match by_key.iter_mut().find(|(grouped, _)| grouped == &key) {
                Some((_, grouped)) => grouped.push(item), // A key an earlier item already named.
                None => by_key.push((key, vec![item])),   // The first item for this key.
            }
        }

        Ok(by_key)
    }

    /// Every record in one key space.
    fn records_in(&self, space: &str) -> Result<Vec<MonitorRecord>, MonitorError> {
        Ok(self.store.partial_get(space, None)?)
    }

    fn transaction_space(&self) -> String {
        format!("{PREFIX}/tx/")
    }

    fn spending_utxo_space(&self) -> String {
        format!("{PREFIX}/utxo/")
    }

    fn output_pattern_space(&self) -> String {
        format!("{PREFIX}/pattern/")
    }

    fn news_space(&self) -> String {
        format!("{PREFIX}/news/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{block_ref, outpoint, temp_storage, txid};
    use crate::types::MonitorEntry;
    use bitcoin_indexer::types::TransactionStatus;

    fn store() -> MonitorStore {
        MonitorStore::new(temp_storage())
    }

    fn record(target: MonitorTarget, context: &str) -> MonitorRecord {
        MonitorRecord {
            target,
            entries: vec![MonitorEntry::new(context.to_string(), None, false)],
        }
    }

    // A record is written and read back under its own key, and deleting it leaves nothing behind.
    #[test]
    fn record_round_trip() {
        let store = store();
        let target = MonitorTarget::Transaction(txid(1));

        assert_eq!(store.get_record(&target).unwrap(), None);

        let saved = record(target.clone(), "ctx");
        store.save_record(&saved).unwrap();
        assert_eq!(store.get_record(&target).unwrap(), Some(saved));

        store.delete_record(&target).unwrap();
        assert_eq!(store.get_record(&target).unwrap(), None);
    }

    // Each kind is read by its own key space, so one kind never sees another's records.
    #[test]
    fn records_are_separated_by_kind() {
        let store = store();

        store
            .save_records(&[
                record(MonitorTarget::Transaction(txid(1)), "a"),
                record(MonitorTarget::Transaction(txid(2)), "b"),
                record(MonitorTarget::SpendingUtxo(outpoint(3, 0)), "c"),
                record(MonitorTarget::NewBlock, "d"),
            ])
            .unwrap();

        assert_eq!(store.get_transaction_records().unwrap().len(), 2);
        assert_eq!(store.get_spending_utxo_records().unwrap().len(), 1);
        assert_eq!(store.get_output_pattern_records().unwrap().len(), 0);

        // The new block record has its own fixed key, so it is not part of any kind's scan.
        assert!(store
            .get_record(&MonitorTarget::NewBlock)
            .unwrap()
            .is_some());
    }

    // The queue holds one entry per target however many times it is asked for, and clearing it takes the lot.
    #[test]
    fn first_check_queue_is_deduplicated() {
        let store = store();
        let target = MonitorTarget::Transaction(txid(1));
        let other = MonitorTarget::SpendingUtxo(outpoint(2, 1));

        store
            .queue_first_checks(&[target.clone(), target.clone()])
            .unwrap();
        store.queue_first_checks(&[other.clone()]).unwrap();
        assert_eq!(
            store.get_first_check_queue().unwrap(),
            vec![target, other] // In the order they were asked for, each of them once.
        );

        store.clear_first_check_queue().unwrap();
        assert!(store.get_first_check_queue().unwrap().is_empty());
    }

    fn transaction_news(txid_byte: u8, due_to_reorg: bool) -> MonitorNews {
        MonitorNews {
            target: MonitorTarget::Transaction(txid(txid_byte)),
            context: "ctx".to_string(),
            kind: NewsKind::Transaction {
                txid: txid(txid_byte),
                status: TransactionStatus::NotFound,
                due_to_reorg,
            },
        }
    }

    // News is grouped by the transaction it is about and comes back in the order it was added.
    #[test]
    fn news_is_grouped_and_ordered() {
        let store = store();
        let first = transaction_news(1, false);
        let second = transaction_news(1, true);
        let other = transaction_news(2, false);

        store
            .add_news(vec![first.clone(), second.clone(), other.clone()])
            .unwrap();

        // Inside one key the order is the order they were added, whichever call added them.
        let key = store.news_key(&first).unwrap();
        let pending: Vec<MonitorNews> = store.store.get(&key, None).unwrap().unwrap();
        assert_eq!(pending, vec![first.clone(), second.clone()]);
        assert_eq!(store.get_all_news().unwrap().len(), 3);

        // One key holds every item about one transaction, so that key alone answers for it.
        assert_eq!(
            store.get_news_of_transaction(&txid(1)).unwrap(),
            vec![first.clone(), second.clone()]
        );

        // Taking one out leaves the rest of its key alone.
        store.remove_news(&[first.clone()]).unwrap();
        assert_eq!(store.get_all_news().unwrap().len(), 2);

        // Several going at once is one write per key, and taking the last one removes the key with it.
        store.remove_news(&[second.clone(), other.clone()]).unwrap();
        assert_eq!(store.get_all_news().unwrap(), vec![]);

        // Removing something that is no longer there is not an error.
        store.remove_news(&[second]).unwrap();
    }

    // Each key says what it holds, and two things of one kind never land in the same one.
    #[test]
    fn keys_name_what_they_hold() {
        let store = store();

        assert_eq!(
            store.record_key(&MonitorTarget::Transaction(txid(1))),
            format!("monitor/tx/{}", txid(1))
        );
        assert_eq!(
            store.record_key(&MonitorTarget::NewBlock),
            "monitor/newblock"
        );

        // The vout is part of the key, so two outputs of one transaction are two targets.
        assert_ne!(
            store.record_key(&MonitorTarget::SpendingUtxo(outpoint(2, 0))),
            store.record_key(&MonitorTarget::SpendingUtxo(outpoint(2, 1)))
        );

        // The whole filter is in the key, so two bounds are two targets. The tag is hex, so it can never contain
        // the separator and run into the next component.
        let filter = |max| OutputPatternFilter {
            output_index: 0,
            tag: vec![0xab],
            max_outputs: max,
        };
        assert_eq!(
            store.record_key(&MonitorTarget::OutputPattern(filter(None))),
            "monitor/pattern/0:ab:any"
        );
        assert_eq!(
            store.record_key(&MonitorTarget::OutputPattern(filter(Some(3)))),
            "monitor/pattern/0:ab:3"
        );
    }

    // An item of news is filed under what it is about, and block heights are padded so the keys sort by height.
    #[test]
    fn news_keys_follow_what_they_report() {
        let store = store();

        // The spender names the key, not the outpoint, so every item about that spender stays together.
        let spend = MonitorNews {
            target: MonitorTarget::SpendingUtxo(outpoint(1, 0)),
            ..transaction_news(9, false)
        };
        assert_eq!(
            store.news_key(&spend).unwrap(),
            format!("monitor/news/{}", txid(9))
        );

        // An unreachable spend has no spender, so it goes under the outpoint's own transaction.
        let unreachable = MonitorNews {
            kind: NewsKind::Unreachable,
            ..spend.clone()
        };
        assert_eq!(
            store.news_key(&unreachable).unwrap(),
            format!("monitor/news/{}", txid(1))
        );

        // Nothing but a spending UTXO can be unreachable, so anything else is a bug and says so.
        let impossible = MonitorNews {
            target: MonitorTarget::NewBlock,
            ..unreachable
        };
        assert!(matches!(
            store.news_key(&impossible).unwrap_err(),
            MonitorError::InvariantViolation(_)
        ));

        let block = |height| MonitorNews {
            target: MonitorTarget::NewBlock,
            context: "ctx".to_string(),
            kind: NewsKind::Block(block_ref(height, 0)),
        };
        assert!(store.news_key(&block(99)).unwrap() < store.news_key(&block(100)).unwrap());
    }
}

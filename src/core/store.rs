//! Persistence. It reads and writes records, the first check queue and the pending news.

use bitcoin::{OutPoint, Txid};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::rc::Rc;
use storage_backend::key::StorageKey;
use storage_backend::storage::Storage;

use crate::errors::MonitorError;
use crate::types::{MonitorNews, MonitorRecord, MonitorTarget, NewsKind, OutputPatternFilter};

const PREFIX: &str = "monitor";

// The segment naming each key space.
const TRANSACTION_SPACE: &str = "tx";
const SPENDING_UTXO_SPACE: &str = "utxo";
const OUTPUT_PATTERN_SPACE: &str = "pattern";
const NEWS_SPACE: &str = "news";

/// Every key the monitor owns.
enum StoreKey {
    TransactionMonitor(Txid), // One transaction and every subscription to it.
    SpendingUtxoMonitor(OutPoint), // One outpoint and every subscription to its spending.
    OutputPatternMonitor(OutputPatternFilter), // One pattern and every subscription to it.
    NewBlockMonitor, // The subscriptions to every new block. There is only ever one such record.
    FirstCheckQueue, // The targets waiting for their first lookup, so in the next tick they can be checked in previous blocks.
    TickSeq, // The tick the news added now is stamped with. Every advance and every reorg moves it by one, an idle tick does not.
    TransactionNews(u64, Txid), // The tick_seq where the news was decided, and the transaction it is about.
    BlockNews(u64), // The tick_seq where the news was decided. Only an advance reports a block, so a tick has at most one.
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
        Ok(self.store.get(self.record_key(target)?, None)?)
    }

    /// Writes one record under its own target's key, so it never rewrites another.
    pub fn save_record(&self, record: &MonitorRecord) -> Result<(), MonitorError> {
        self.store
            .set(self.record_key(&record.target)?, record, None)?;
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
        self.store.remove(self.record_key(target)?, None)?;
        Ok(())
    }

    pub fn get_transaction_records(&self) -> Result<Vec<MonitorRecord>, MonitorError> {
        self.records_in(&self.transaction_space()?)
    }

    pub fn get_spending_utxo_records(&self) -> Result<Vec<MonitorRecord>, MonitorError> {
        self.records_in(&self.spending_utxo_space()?)
    }

    pub fn get_output_pattern_records(&self) -> Result<Vec<MonitorRecord>, MonitorError> {
        self.records_in(&self.output_pattern_space()?)
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
        let key = self.get_key(StoreKey::FirstCheckQueue)?;
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
            .set(self.get_key(StoreKey::FirstCheckQueue)?, &queue, None)?;

        Ok(())
    }

    /// Empties the queue, which is how draining it in one pass ends.
    pub fn clear_first_check_queue(&self) -> Result<(), MonitorError> {
        self.store
            .remove(self.get_key(StoreKey::FirstCheckQueue)?, None)?;
        Ok(())
    }

    // ================================
    //  NEWS
    // ================================

    /// The tick the news added now is stamped with. Zero before the first advance.
    pub fn get_tick_seq(&self) -> Result<u64, MonitorError> {
        let key = self.get_key(StoreKey::TickSeq)?;
        Ok(self.store.get(key, None)?.unwrap_or(0))
    }

    /// Moves to the next tick, which is what every advance and every reorg does.
    pub fn next_tick_seq(&self) -> Result<(), MonitorError> {
        let next = self.get_tick_seq()? + 1;
        self.store
            .set(self.get_key(StoreKey::TickSeq)?, next, None)?;
        Ok(())
    }

    /// The pending news under the first `max_keys` keys, or under every key, one list per key, extended to the end of
    /// the last tick it reaches, so no tick comes back cut. Keys start with the tick, so the lists come in tick order,
    /// and inside a tick the block comes before the transactions.
    pub fn get_news_by_key(
        &self,
        max_keys: Option<usize>,
    ) -> Result<Vec<Vec<MonitorNews>>, MonitorError> {
        let mut groups: Vec<Vec<MonitorNews>> =
            self.store
                .partial_get_limited(&self.news_space()?, max_keys, None)?;

        // The limit can stop in the middle of the last tick_seq, and ordering it needs all of it, so that tick is read again whole.
        if let Some(last) = groups.last().map(|group| group[0].tick_seq) {
            groups.retain(|group| group[0].tick_seq != last);
            groups.extend(self.get_news_of_tick(last)?);
        }

        Ok(groups)
    }

    /// Every key one tick wrote, one list per key.
    fn get_news_of_tick(&self, tick_seq: u64) -> Result<Vec<Vec<MonitorNews>>, MonitorError> {
        Ok(self
            .store
            .partial_get_limited(&self.tick_news_space(tick_seq)?, None, None)?)
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
                // Every copy of it goes, should one key ever hold two.
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
    fn get_key(&self, key: StoreKey) -> Result<StorageKey, MonitorError> {
        let key = match key {
            StoreKey::TransactionMonitor(tx_id) => {
                StorageKey::new([PREFIX, TRANSACTION_SPACE, tx_id.to_string().as_str()])
            }
            StoreKey::SpendingUtxoMonitor(utxo) => StorageKey::new([
                PREFIX,
                SPENDING_UTXO_SPACE,
                format!("{}:{}", utxo.txid, utxo.vout).as_str(),
            ]),
            // Every field of the filter is in the key, because a different bound on the outputs is a different rule
            // and gets its own record. That makes the key and the target the same thing, so a record always holds
            // the filter it is stored under and no registration can change another one's bound.
            StoreKey::OutputPatternMonitor(filter) => StorageKey::new([
                PREFIX,
                OUTPUT_PATTERN_SPACE,
                format!(
                    "{}:{}:{}",
                    filter.output_index,
                    hex::encode(&filter.tag),
                    filter
                        .max_outputs
                        .map_or_else(|| "any".to_string(), |max| max.to_string())
                )
                .as_str(),
            ]),
            StoreKey::NewBlockMonitor => StorageKey::new([PREFIX, "newblock"]),
            StoreKey::FirstCheckQueue => StorageKey::new([PREFIX, "first_check"]),
            StoreKey::TickSeq => StorageKey::new([PREFIX, "tick_seq"]),
            StoreKey::TransactionNews(tick_seq, tx_id) => StorageKey::new([
                PREFIX,
                NEWS_SPACE,
                tick_segment(tick_seq).as_str(),
                "tx",
                tx_id.to_string().as_str(),
            ]),
            StoreKey::BlockNews(tick_seq) => {
                StorageKey::new([PREFIX, NEWS_SPACE, tick_segment(tick_seq).as_str(), "block"])
            }
        };
        Ok(key?)
    }

    /// The key holding a target's record.
    fn record_key(&self, target: &MonitorTarget) -> Result<StorageKey, MonitorError> {
        self.get_key(match target {
            MonitorTarget::Transaction(tx_id) => StoreKey::TransactionMonitor(*tx_id),
            MonitorTarget::SpendingUtxo(utxo) => StoreKey::SpendingUtxoMonitor(*utxo),
            MonitorTarget::OutputPattern(filter) => StoreKey::OutputPatternMonitor(filter.clone()),
            MonitorTarget::NewBlock => StoreKey::NewBlockMonitor,
        })
    }

    /// The key holding an item of news: the tick that decided it, and the transaction it is about or its block.
    fn news_key(&self, news: &MonitorNews) -> Result<StorageKey, MonitorError> {
        let tick_seq = news.tick_seq;
        let key = match (&news.kind, &news.target) {
            (NewsKind::Transaction { txid, .. }, _) => StoreKey::TransactionNews(tick_seq, *txid),
            (NewsKind::Block(_), _) => StoreKey::BlockNews(tick_seq),
            (NewsKind::ProbablyUnreachableUTXO, MonitorTarget::SpendingUtxo(utxo)) => {
                StoreKey::TransactionNews(tick_seq, utxo.txid)
            }
            (NewsKind::UnreachableTx, MonitorTarget::Transaction(txid)) => {
                StoreKey::TransactionNews(tick_seq, *txid)
            }
            // Each unreachable kind belongs to the one first check that produces it, so anything else is a bug.
            (kind @ (NewsKind::ProbablyUnreachableUTXO | NewsKind::UnreachableTx), target) => {
                return Err(MonitorError::InvariantViolation(format!(
                    "{target:?} produced {kind:?} news, which only its own kind of target can"
                )))
            }
        };

        self.get_key(key)
    }

    /// Which of these items belong to which key, each key keeping its items in the order they were given. Both adding
    /// and removing group first and touch storage afterwards, so a key that several items land in is read and written
    /// once instead of once per item. Two contexts watching the same transaction is what makes that the normal case.
    fn group_by_key<T: Borrow<MonitorNews>>(
        &self,
        news: impl IntoIterator<Item = T>,
    ) -> Result<HashMap<StorageKey, Vec<T>>, MonitorError> {
        let mut by_key: HashMap<StorageKey, Vec<T>> = HashMap::new();

        // Nothing is read or written here: this only works out the grouping. A map rather than a scan, because a pattern
        // can follow thousands of matches and a scan would compare every item against every key.
        for item in news {
            by_key
                .entry(self.news_key(item.borrow())?)
                .or_default()
                .push(item);
        }

        Ok(by_key)
    }

    /// Every record in one key space.
    fn records_in(&self, space: &str) -> Result<Vec<MonitorRecord>, MonitorError> {
        Ok(self.store.partial_get(space, None)?)
    }

    fn transaction_space(&self) -> Result<String, MonitorError> {
        Ok(StorageKey::new([PREFIX, TRANSACTION_SPACE])?.to_scan_prefix())
    }

    fn spending_utxo_space(&self) -> Result<String, MonitorError> {
        Ok(StorageKey::new([PREFIX, SPENDING_UTXO_SPACE])?.to_scan_prefix())
    }

    fn output_pattern_space(&self) -> Result<String, MonitorError> {
        Ok(StorageKey::new([PREFIX, OUTPUT_PATTERN_SPACE])?.to_scan_prefix())
    }

    fn news_space(&self) -> Result<String, MonitorError> {
        Ok(StorageKey::new([PREFIX, NEWS_SPACE])?.to_scan_prefix())
    }

    fn tick_news_space(&self, tick_seq: u64) -> Result<String, MonitorError> {
        Ok(
            StorageKey::new([PREFIX, NEWS_SPACE, tick_segment(tick_seq).as_str()])?
                .to_scan_prefix(),
        )
    }
}

/// The tick as a key segment, padded so the keys sort by tick.
fn tick_segment(tick_seq: u64) -> String {
    format!("{tick_seq:020}")
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
    fn test_record_round_trip() {
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
    fn test_records_are_separated_by_kind() {
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
    fn test_first_check_queue_is_deduplicated() {
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
        MonitorNews::new(
            MonitorTarget::Transaction(txid(txid_byte)),
            "ctx".to_string(),
            NewsKind::Transaction {
                txid: txid(txid_byte),
                status: TransactionStatus::NotFound,
                due_to_reorg,
            },
        )
    }

    // News is grouped by tick and transaction, comes back in tick order, and inside one key in the order it was added.
    #[test]
    fn test_news_is_grouped_and_ordered() {
        let store = store();

        // The tick starts at zero and every call moves it by one.
        assert_eq!(store.get_tick_seq().unwrap(), 0);
        store.next_tick_seq().unwrap();
        assert_eq!(store.get_tick_seq().unwrap(), 1);

        let first = transaction_news(1, false);
        let second = transaction_news(1, true);
        // A later tick about a transaction whose txid sorts first still comes after.
        let later = MonitorNews {
            tick_seq: 1,
            ..transaction_news(0, false)
        };

        store
            .add_news(vec![first.clone(), second.clone(), later.clone()])
            .unwrap();

        assert_eq!(
            store.get_news_by_key(None).unwrap(),
            vec![vec![first.clone(), second.clone()], vec![later.clone()]]
        );

        // A limit counts keys, and a key comes back whole. More keys than exist is everything.
        assert_eq!(
            store.get_news_by_key(Some(1)).unwrap(),
            vec![vec![first.clone(), second.clone()]]
        );
        assert_eq!(store.get_news_by_key(Some(10)).unwrap().len(), 2);
        assert!(store.get_news_by_key(Some(0)).unwrap().is_empty());

        // One tick is read without the others.
        assert_eq!(
            store.get_news_of_tick(1).unwrap(),
            vec![vec![later.clone()]]
        );

        // Taking one out leaves the rest of its key alone.
        store.remove_news(&[first.clone()]).unwrap();
        assert_eq!(
            store.get_news_by_key(None).unwrap(),
            vec![vec![second.clone()], vec![later.clone()]]
        );

        // Several going at once is one write per key, and taking the last one removes the key with it.
        store.remove_news(&[second.clone(), later]).unwrap();
        assert!(store.get_news_by_key(None).unwrap().is_empty());

        // Removing something that is no longer there is not an error.
        store.remove_news(&[second]).unwrap();
    }

    // Each key says what it holds, and two things of one kind never land in the same one.
    #[test]
    fn test_keys_name_what_they_hold() {
        let store = store();
        let key = |target| store.record_key(&target).unwrap();

        assert_eq!(
            key(MonitorTarget::Transaction(txid(1))),
            StorageKey::new([PREFIX, TRANSACTION_SPACE, txid(1).to_string().as_str()]).unwrap()
        );
        assert_eq!(
            key(MonitorTarget::NewBlock),
            StorageKey::new([PREFIX, "newblock"]).unwrap()
        );

        // The vout is part of the key, so two outputs of one transaction are two targets.
        assert_ne!(
            key(MonitorTarget::SpendingUtxo(outpoint(2, 0))),
            key(MonitorTarget::SpendingUtxo(outpoint(2, 1)))
        );

        // The whole filter is in the key, so two bounds are two targets. The tag is hex, so it can never contain
        // the separator and run into the next component.
        let filter = |max| OutputPatternFilter {
            output_index: 0,
            tag: vec![0xab],
            max_outputs: max,
        };
        assert_eq!(
            key(MonitorTarget::OutputPattern(filter(None))),
            StorageKey::new([PREFIX, OUTPUT_PATTERN_SPACE, "0:ab:any"]).unwrap()
        );
        assert_eq!(
            key(MonitorTarget::OutputPattern(filter(Some(3)))),
            StorageKey::new([PREFIX, OUTPUT_PATTERN_SPACE, "0:ab:3"]).unwrap()
        );
    }

    // An item of news is filed under its tick and what it is about, and the keys sort by tick first.
    #[test]
    fn test_news_keys_follow_what_they_report() {
        let store = store();
        let key = |news: &MonitorNews| store.news_key(news).unwrap();
        let news_of = |tx_id: Txid| {
            StorageKey::new([
                PREFIX,
                NEWS_SPACE,
                tick_segment(0).as_str(),
                "tx",
                tx_id.to_string().as_str(),
            ])
            .unwrap()
        };

        // The spender names the key, not the outpoint, so every item about that spender stays together.
        let spend = MonitorNews {
            target: MonitorTarget::SpendingUtxo(outpoint(1, 0)),
            ..transaction_news(9, false)
        };
        assert_eq!(key(&spend), news_of(txid(9)));

        // A probably unreachable spend has no spender, so it goes under the outpoint's own transaction.
        let unreachable = MonitorNews {
            kind: NewsKind::ProbablyUnreachableUTXO,
            ..spend.clone()
        };
        assert_eq!(key(&unreachable), news_of(txid(1)));

        // An unreachable transaction goes under its own txid.
        let unreachable_tx = MonitorNews {
            target: MonitorTarget::Transaction(txid(2)),
            kind: NewsKind::UnreachableTx,
            ..spend.clone()
        };
        assert_eq!(key(&unreachable_tx), news_of(txid(2)));

        // Each unreachable kind belongs to its own kind of target, so anything else is a bug and says so.
        for impossible in [
            MonitorNews {
                target: MonitorTarget::NewBlock,
                ..unreachable.clone()
            },
            MonitorNews {
                target: MonitorTarget::Transaction(txid(2)),
                ..unreachable
            },
            MonitorNews {
                target: MonitorTarget::SpendingUtxo(outpoint(1, 0)),
                ..unreachable_tx
            },
        ] {
            assert!(matches!(
                store.news_key(&impossible).unwrap_err(),
                MonitorError::InvariantViolation(_)
            ));
        }

        let block = |height, tick_seq| MonitorNews {
            tick_seq,
            ..MonitorNews::new(
                MonitorTarget::NewBlock,
                "ctx".to_string(),
                NewsKind::Block(block_ref(height, 0)),
            )
        };
        // Storage sorts by the joined key. A later tick sorts after, even with a lower height, and inside one tick
        // the block sorts before every transaction.
        assert!(key(&block(100, 0)).joined() < key(&block(99, 1)).joined());
        assert!(key(&block(100, 0)).joined() < key(&spend).joined());
    }
}

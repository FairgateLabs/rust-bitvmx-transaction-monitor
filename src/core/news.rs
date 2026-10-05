//! What is waiting for the consumer. It stores what `Subscriptions` decided to report and hands it back on request
use crate::core::store::MonitorStore;
use crate::errors::MonitorError;
use crate::types::{MonitorNews, MonitorTarget};
use std::slice;

/// The pending news, grouped by the transaction or block each item is about. Acknowledging deletes an item.
pub struct PendingNews {
    store: MonitorStore,
}

impl PendingNews {
    pub fn new(store: MonitorStore) -> Self {
        Self { store }
    }

    /// Adds what a tick decided to report, keeping the items about one transaction in the order they were
    /// decided, so a consumer never reads 20 confirmations before 19.
    pub fn add(&self, items: Vec<MonitorNews>) -> Result<(), MonitorError> {
        self.store.add_news(items)
    }

    /// Everything the consumer has not acknowledged. Order holds inside a key, not between keys.
    pub fn get_all(&self) -> Result<Vec<MonitorNews>, MonitorError> {
        self.store.get_all_news()
    }

    /// Removes the item the consumer acknowledged, by the value it was handed.
    pub fn ack(&self, news: &MonitorNews) -> Result<(), MonitorError> {
        self.store.remove_news(slice::from_ref(news))
    }

    /// Removes what a cancelled subscription had waiting. It is the one thing that deletes unacknowledged news.
    pub fn drop_for(&self, target: &MonitorTarget, context: &str) -> Result<(), MonitorError> {
        let pending = match target {
            // A transaction subscription's news is always about its own transaction, so it is all under one key.
            MonitorTarget::Transaction(tx_id) => self.store.get_news_of_transaction(tx_id)?,
            // The others cannot name theirs. A UTXO's news is under the spender it found, a pattern's under every
            // transaction it matched, and a new block subscription's under each height it has not been
            // acknowledged for. None of those is known from the target, so the whole log is read instead.
            _ => self.store.get_all_news()?,
        };

        let dropped: Vec<MonitorNews> = pending
            .into_iter()
            .filter(|item| &item.target == target && item.context == context)
            .collect();

        self.store.remove_news(&dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{temp_storage, txid};
    use crate::types::NewsKind;
    use bitcoin_indexer::types::TransactionStatus;

    fn news(target_byte: u8, context: &str, due_to_reorg: bool) -> MonitorNews {
        MonitorNews {
            target: MonitorTarget::Transaction(txid(target_byte)),
            context: context.to_string(),
            kind: NewsKind::Transaction {
                txid: txid(target_byte),
                status: TransactionStatus::NotFound,
                due_to_reorg,
            },
        }
    }

    fn pending() -> PendingNews {
        PendingNews::new(MonitorStore::new(temp_storage()))
    }

    // Items about one transaction come back in the order they were added, whichever tick added them.
    #[test]
    fn keeps_order_within_a_transaction() {
        let news_log = pending();
        let first = news(1, "ctx", false);
        let second = news(1, "ctx", true);

        news_log.add(vec![first.clone()]).unwrap();
        news_log.add(vec![second.clone()]).unwrap();

        assert_eq!(news_log.get_all().unwrap(), vec![first, second]);
    }

    // Acknowledging removes that item and leaves the others, and doing it twice is not an error.
    #[test]
    fn ack_removes_one_item() {
        let news_log = pending();
        let first = news(1, "ctx", false);
        let second = news(1, "ctx", true);
        news_log.add(vec![first.clone(), second.clone()]).unwrap();

        news_log.ack(&first).unwrap();
        assert_eq!(news_log.get_all().unwrap(), vec![second.clone()]);

        news_log.ack(&first).unwrap();
        assert_eq!(news_log.get_all().unwrap(), vec![second]);
    }

    // Cancelling takes that subscription's pending news, and only that subscription's.
    #[test]
    fn drop_for_takes_only_its_own() {
        let news_log = pending();
        let mine = news(1, "mine", false);
        let other_context = news(1, "other", false);
        let other_target = news(2, "mine", false);
        news_log
            .add(vec![
                mine.clone(),
                other_context.clone(),
                other_target.clone(),
            ])
            .unwrap();

        news_log
            .drop_for(&MonitorTarget::Transaction(txid(1)), "mine")
            .unwrap();

        let left = news_log.get_all().unwrap();
        assert!(!left.contains(&mine));
        assert!(left.contains(&other_context));
        assert!(left.contains(&other_target));
    }
}

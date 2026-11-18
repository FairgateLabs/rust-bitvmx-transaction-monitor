//! What is waiting for the consumer. It stores what `Subscriptions` decided to report and hands it back on request
use crate::core::store::MonitorStore;
use crate::errors::MonitorError;
use crate::types::{MonitorNews, MonitorTarget, NewsKind};
use bitcoin::Txid;
use std::collections::{HashMap, VecDeque};
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
    /// decided, so a consumer never reads N confirmations before N-1.
    pub fn add(&self, items: Vec<MonitorNews>) -> Result<(), MonitorError> {
        self.store.add_news(items)
    }

    /// Everything the consumer has not acknowledged, or what is pending under the first `max_keys` keys. How many
    /// items that can be is on `Monitor::get_news`, which is where a consumer reads it.
    pub fn get_all(&self, max_keys: Option<usize>) -> Result<Vec<MonitorNews>, MonitorError> {
        // Everything is read and ordered before the limit is applied.
        let ordered = in_dependency_order(self.store.get_news_by_key()?);
        let keys = max_keys.unwrap_or(ordered.len());
        Ok(ordered.into_iter().take(keys).flatten().collect())
    }

    /// Removes the item the consumer acknowledged, by the value it was handed.
    pub fn ack(&self, news: &MonitorNews) -> Result<(), MonitorError> {
        self.store.remove_news(slice::from_ref(news))
    }

    /// Removes what the cancelled subscriptions had waiting. It is the one thing that deletes unacknowledged news.
    pub fn drop_for(&self, targets: &[MonitorTarget], context: &str) -> Result<(), MonitorError> {
        let only_transactions = targets
            .iter()
            .all(|target| matches!(target, MonitorTarget::Transaction(_)));

        let pending = match only_transactions {
            // A transaction subscription's news is always about its own transaction, so it is all under one key.
            true => {
                let mut pending = Vec::new();
                for target in targets {
                    if let MonitorTarget::Transaction(tx_id) = target {
                        pending.extend(self.store.get_news_of_transaction(tx_id)?);
                    }
                }
                pending
            }
            // The others cannot name theirs. A UTXO's news is under the spender it found, a pattern's under every
            // transaction it matched, and a new block subscription's under each height it has not been acknowledged
            // for. None of those is known from the target, so the whole log is read instead.
            false => self.store.get_all_news(None)?,
        };

        let dropped: Vec<MonitorNews> = pending
            .into_iter()
            .filter(|item| item.context == context && targets.contains(&item.target))
            .collect();

        self.store.remove_news(&dropped)
    }
}

/// Puts the news about a transaction after the news about the transactions it spends. A block holds a parent before
/// its child, so this is the order the chain has them in. What spends nothing pending keeps the stored order.
fn in_dependency_order(groups: Vec<Vec<MonitorNews>>) -> Vec<Vec<MonitorNews>> {
    // Where the news about each transaction sits. Block news is about no transaction, so nothing can spend it.
    let position: HashMap<Txid, usize> = groups
        .iter()
        .enumerate()
        .filter_map(|(index, group)| {
            group.iter().find_map(|item| match &item.kind {
                NewsKind::Transaction { txid, .. } => Some((*txid, index)),
                _ => None,
            })
        })
        .collect();

    // Only a confirmed item carries the transaction, and so its inputs. One that is not mined has no place in a
    // block to keep, so it has no parents here.
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); groups.len()];
    let mut parents: Vec<usize> = vec![0; groups.len()];
    for (index, group) in groups.iter().enumerate() {
        let Some(tx) = group.iter().find_map(MonitorNews::confirmed_tx) else {
            continue;
        };
        for input in &tx.input {
            if let Some(&parent) = position.get(&input.previous_output.txid) {
                children[parent].push(index);
                parents[index] += 1;
            }
        }
    }

    // Kahn's algorithm. The groups with no parent go first, in stored order, and a child joins the end of the queue once
    // its last parent is out. Transactions cannot spend each other in a cycle, so every group comes out.
    let mut ready: VecDeque<usize> = (0..groups.len())
        .filter(|&index| parents[index] == 0)
        .collect();
    let mut order = Vec::with_capacity(groups.len());
    while let Some(index) = ready.pop_front() {
        order.push(index);
        for &child in &children[index] {
            parents[child] -= 1;
            if parents[child] == 0 {
                ready.push_back(child);
            }
        }
    }

    let mut groups: Vec<Option<Vec<MonitorNews>>> = groups.into_iter().map(Some).collect();
    order
        .into_iter()
        .filter_map(|index| groups[index].take())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{block_hash, block_ref, outpoint, temp_storage, tx, txid};
    use bitcoin::{OutPoint, Transaction};
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
    fn test_keeps_order_within_a_transaction() {
        let news_log = pending();
        let first = news(1, "ctx", false);
        let second = news(1, "ctx", true);

        news_log.add(vec![first.clone()]).unwrap();
        news_log.add(vec![second.clone()]).unwrap();

        assert_eq!(news_log.get_all(None).unwrap(), vec![first, second]);
    }

    // A transaction comes after the one it spends, even when its txid sorts first and the limit only takes one key.
    #[test]
    fn test_parent_comes_before_child() {
        let confirmed = |tx: &Transaction| MonitorNews {
            target: MonitorTarget::Transaction(tx.compute_txid()),
            context: "ctx".to_string(),
            kind: NewsKind::Transaction {
                txid: tx.compute_txid(),
                status: TransactionStatus::new(tx.clone(), 1, block_hash(1), 1),
                due_to_reorg: false,
            },
        };

        // A child whose txid sorts before its parent's, so the stored order is the wrong one.
        let parent = tx(1, vec![outpoint(9, 0)], vec![]);
        let parent_id = parent.compute_txid();
        let child = (2..)
            .map(|seed| tx(seed, vec![OutPoint::new(parent_id, 0)], vec![]))
            .find(|child| child.compute_txid().to_string() < parent_id.to_string())
            .unwrap();

        let news_log = pending();
        news_log
            .add(vec![confirmed(&child), confirmed(&parent)])
            .unwrap();

        assert_eq!(
            news_log.get_all(None).unwrap(),
            vec![confirmed(&parent), confirmed(&child)]
        );
        assert_eq!(news_log.get_all(Some(1)).unwrap(), vec![confirmed(&parent)]);
    }

    // Acknowledging removes that item and leaves the others, and doing it twice is not an error.
    #[test]
    fn test_ack_removes_one_item() {
        let news_log = pending();
        let first = news(1, "ctx", false);
        let second = news(1, "ctx", true);
        news_log.add(vec![first.clone(), second.clone()]).unwrap();

        news_log.ack(&first).unwrap();
        assert_eq!(news_log.get_all(None).unwrap(), vec![second.clone()]);

        news_log.ack(&first).unwrap();
        assert_eq!(news_log.get_all(None).unwrap(), vec![second]);
    }

    // Cancelling takes those subscriptions' pending news, and only theirs, whether the targets name their keys or not.
    #[test]
    fn test_drop_for_takes_only_its_own() {
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
            .drop_for(&[MonitorTarget::Transaction(txid(1))], "mine")
            .unwrap();

        let left = news_log.get_all(None).unwrap();
        assert!(!left.contains(&mine));
        assert!(left.contains(&other_context));
        assert!(left.contains(&other_target));

        // Two targets in one call, one of which cannot name its keys, so the whole log is read once for both.
        let block = MonitorNews {
            target: MonitorTarget::NewBlock,
            context: "mine".to_string(),
            kind: NewsKind::Block(block_ref(5, 0)),
        };
        let block_other_context = MonitorNews {
            context: "other".to_string(),
            ..block.clone()
        };
        news_log
            .add(vec![block.clone(), block_other_context.clone()])
            .unwrap();

        news_log
            .drop_for(
                &[MonitorTarget::Transaction(txid(2)), MonitorTarget::NewBlock],
                "mine",
            )
            .unwrap();

        let left = news_log.get_all(None).unwrap();
        assert!(!left.contains(&other_target));
        assert!(!left.contains(&block));
        assert!(left.contains(&other_context));
        assert!(left.contains(&block_other_context));
    }
}

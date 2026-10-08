//! What is being watched, and the state of everything it has found. Every rule about when a consumer is told
//! something lives here; storing what it decides is the news log's job, and holding it is the store's.
//!
//! There are five things that happen to a subscription, and one function for each: it is registered, it is
//! cancelled, its past is looked at once, a block arrives, or the chain is reorganised. Nothing is remembered
//! between them beyond the records themselves, which is what keeps the rules short:
//!
//! - without a trigger, every block a tracked transaction is in is reported
//! - with a trigger, the block where the count equals it is reported, and no other. The monitor sees every
//!   block once and in order, so the count rises by exactly one per block and cannot skip past the trigger
//! - a reorg reports a tracked transaction that lost its block, and one whose count fell back below its trigger
//! - at `max_monitoring_confirmations` a transaction stops being tracked, and a subscription that was watching
//!   for that one transaction ends with it

use bitcoin::{OutPoint, Transaction, Txid};
use bitcoin_indexer::config::IndexerSettings;
use bitcoin_indexer::types::TransactionStatus;
use bitcoin_indexer::IndexerType;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use std::collections::HashMap;
use std::rc::Rc;
use tracing::{debug, info, warn};

use crate::config::MonitorSettings;
use crate::core::helper::{is_spending_output, matches_output_pattern};
use crate::core::store::MonitorStore;
use crate::errors::MonitorError;
use crate::types::{
    BlockRef, FullBlock, MonitorEntry, MonitorNews, MonitorRecord, MonitorTarget, NewsKind,
    OutputPatternFilter, TrackedTx,
};

pub struct Subscriptions {
    store: MonitorStore,
    indexer: Rc<IndexerType>,
    settings: MonitorSettings,
}

impl Subscriptions {
    pub fn new(store: MonitorStore, indexer: Rc<IndexerType>, settings: MonitorSettings) -> Self {
        Self {
            store,
            indexer,
            settings,
        }
    }

    /// Subscribes one context to every target given. A target already subscribed under the same context keeps
    /// the transactions it has already found, so a re-registration only changes the parameters.
    pub fn add(
        &self,
        targets: &[MonitorTarget],
        context: String,
        confirmation_trigger: Option<u32>,
        search_in_mempool: bool,
    ) -> Result<(), MonitorError> {
        let maximum = self.settings.max_monitoring_confirmations;
        let finality = self.settings.finality;

        // finality <= trigger <= max_monitoring_confirmations.
        // Finality is the depth at which a block is settled, so a trigger may sit exactly on it: a trigger is reported
        // once and never restated, and from finality on there is no reorg left to undo it. Above the maximum the block
        // of the transaction leaves the indexer's window while it is still tracked, and the block pass would fail.
        if let Some(trigger) = confirmation_trigger {
            if trigger < finality || trigger > maximum {
                return Err(MonitorError::InvalidConfirmationTrigger(
                    trigger, finality, maximum,
                ));
            }
        }

        let mut records = Vec::with_capacity(targets.len()); // All the new records generated from the targets.
        let mut first_check = Vec::new(); // Targets that need a first check (only apply to transactions and UTXOs).

        for target in targets {
            // A parameter the target cannot use is refused, so a consumer never believes it asked for something that is not happening.
            if confirmation_trigger.is_some() && matches!(target, MonitorTarget::NewBlock) {
                return Err(MonitorError::InvalidSubscription(
                    "a block has no confirmations of its own, so a new block subscription cannot have a confirmation trigger".to_string(),
                ));
            }
            if search_in_mempool && !matches!(target, MonitorTarget::Transaction(_)) {
                return Err(MonitorError::InvalidSubscription(format!(
                    "search_in_mempool is only for a transaction target, and {target:?} discovers transactions that are already in a block"
                )));
            }

            let mut record = self
                .store
                .get_record(target)?
                .unwrap_or_else(|| MonitorRecord::new(target.clone()));
            let wanted_mempool = wants_mempool_watch(&record);

            match record.entries.iter_mut().find(|e| e.context == context) {
                // Re-registering a subscription only changes the parameters, and the transactions it already tracks stay tracked.
                Some(existing) => {
                    existing.confirmation_trigger = confirmation_trigger;
                    existing.search_in_mempool = search_in_mempool;
                }
                None => record.entries.push(MonitorEntry::new(
                    context.clone(),
                    confirmation_trigger,
                    search_in_mempool,
                )),
            }

            // Targets that need a first check (only apply to transactions and UTXOs).
            if matches!(
                target,
                MonitorTarget::Transaction(_) | MonitorTarget::SpendingUtxo(_)
            ) {
                first_check.push(target.clone());
            }

            if let MonitorTarget::Transaction(tx_id) = target {
                // Only a transaction target can ask for a mempool watch.
                if search_in_mempool {
                    self.indexer.add_mempool_watch(*tx_id)?;
                }

                // Registering again with the flag off can leave a watch behind that nobody wants any more. The answer
                // comes from the whole record, so another context watching the same transaction keeps the watch alive.
                self.release_mempool_watch(tx_id, wanted_mempool, &record)?;
            }

            records.push(record);
        }

        self.store.queue_first_checks(&first_check)?;
        self.store.save_records(&records)?;

        Ok(())
    }

    /// Cancels one context on every target given, dropping the transactions it was tracking with it.
    pub fn remove(&self, targets: &[MonitorTarget], context: &str) -> Result<(), MonitorError> {
        for target in targets {
            let Some(mut record) = self.store.get_record(target)? else {
                continue; // Nothing is subscribed to it, so there is nothing to cancel.
            };

            let Some(position) = record.entries.iter().position(|e| e.context == context) else {
                continue; // Other contexts watch this target, but not this one.
            };

            // Read before the entry goes, so the watch is released when the last context that wanted it leaves.
            let wanted_mempool = wants_mempool_watch(&record);

            // The entry is gone, and the record is written back with the rest. If it was the last one, the whole record is deleted.
            record.entries.remove(position);
            self.persist(&record)?;

            // Only a transaction target can ask for a mempool watch.
            if let MonitorTarget::Transaction(tx_id) = target {
                self.release_mempool_watch(tx_id, wanted_mempool, &record)?;
            }
        }

        Ok(())
    }

    /// Checks if a transaction was already mined, or a UTXO was already spent when the subscription was made.
    /// It only runs once per target, and is the only path that asks an rpc node. After this function runs for
    /// a target, everything from here on is found only by the new blocks that arrive.
    pub fn run_first_checks(&self) -> Result<Vec<MonitorNews>, MonitorError> {
        let queue = self.store.get_first_check_queue()?;

        if queue.is_empty() {
            return Ok(Vec::new()); // Nothing was registered since the last tick, which is the usual case.
        }

        let mut news = Vec::new();

        for target in &queue {
            let Some(record) = self.store.get_record(target)? else {
                continue; // This subscription was cancelled before its check ever ran.
            };

            news.extend(match target {
                MonitorTarget::Transaction(tx_id) => {
                    self.first_check_transaction(*tx_id, record)?   // A transaction target is always about its own transaction.
                }
                MonitorTarget::SpendingUtxo(outpoint) => {
                    self.first_check_spending_utxo(*outpoint, record)? // A UTXO target is always about the transaction that spent it.
                }
                // A pattern has no past to look at and a new block subscription has nothing to find, so neither is ever queued.
                other => {
                    return Err(MonitorError::InvariantViolation(format!(
                        "{other:?} was queued for a first check, which only a transaction or a spending UTXO is"
                    )))
                }
            });
        }

        // Every target in it was answered, so it is emptied in one write.
        self.store.clear_first_check_queue()?;

        Ok(news)
    }

    /// Processes a new mined block, and reports what it finds to the subscriptions that were waiting for it.
    pub fn process_block(&self, block: &FullBlock) -> Result<Vec<MonitorNews>, MonitorError> {
        let mut news = Vec::new();
        let mut records: Vec<(MonitorRecord, bool)>; // (record, started tracking a transaction in this block)
        records = self
            .store
            .get_all_records()?
            .into_iter()
            .map(|record| (record, false))
            .collect();

        // The block is walked once. A record that finds a transaction it was waiting for is flagged, so it is then written.
        self.track_block_matches(&mut records, block);

        // The records are written back with the transactions they found.
        for (record, started_tracking) in &mut records {
            let wanted_mempool = wants_mempool_watch(record);

            // It checks every tracked transaction in the record, and reports the ones that are at their trigger or have reached the maximum.
            let (reported, stopped_tracking) = self.advance_to_block(record, block.height)?;
            news.extend(reported);

            // Starting and stopping are the only two things a block does to a record. If neither happened, there is nothing to write.
            if !*started_tracking && !stopped_tracking {
                continue;
            }

            // For each transaction in the record, release the mempool watch.
            if let MonitorTarget::Transaction(tx_id) = &record.target {
                self.release_mempool_watch(tx_id, wanted_mempool, record)?;
            }
            self.persist(record)?;
        }

        news.extend(self.notify_block_subscriptions(block)?);

        Ok(news)
    }

    /// Restates what a reorg changed, against the chain left behind.
    pub fn update_after_reorg(&self) -> Result<Vec<MonitorNews>, MonitorError> {
        let mut news = Vec::new();
        let tip = self.indexer.get_indexed_height()?;

        for mut record in self.store.get_all_records()? {
            let target = record.target.clone();
            let mut lost = false;

            for entry in &mut record.entries {
                let trigger = entry.confirmation_trigger;
                let search_in_mempool = entry.search_in_mempool;
                let context = &entry.context;
                let mut kept = Vec::with_capacity(entry.tracked.len());

                for tracked in entry.tracked.drain(..) {
                    // Its block was above the tip that is left, so it went with the unwind.
                    let gone = tracked.confirmed_at.height > tip;

                    // Without a trigger every change is reported.
                    if trigger.is_none() {
                        // Asks the indexer where the transaction is now, and reports it with its new status.
                        let status = self
                            .indexer
                            .get_stored_transaction(&tracked.txid, search_in_mempool)?;

                        news.push(transaction_news(
                            &target,
                            context,
                            tracked.txid,
                            status,
                            true,
                        ));
                    }

                    // Its block went with the reorg, so the subscription goes back to waiting for it.
                    match gone {
                        false => kept.push(tracked),
                        true => lost = true,
                    }
                }

                entry.tracked = kept;
            }

            // A reorg takes tracked transactions. The record keeps every entry it had
            if lost {
                self.store.save_record(&record)?;
            }
        }

        Ok(news)
    }

    // =========================================================================
    // Internal
    // =========================================================================

    /// Check if a transaction was already mined. Asks where the transaction is, and tracks it when it is in a block.
    fn first_check_transaction(
        &self,
        tx_id: Txid,
        record: MonitorRecord,
    ) -> Result<Vec<MonitorNews>, MonitorError> {
        let TransactionStatus::Confirmed {
            tx,
            block_height,
            block_hash,
            confirmations,
        } = self.indexer.get_transaction(&tx_id, false)?
        else {
            return Ok(Vec::new()); // Not in a block, so the blocks that come will find it.
        };

        let confirmed_at = BlockRef {
            height: block_height,
            hash: block_hash,
        };

        self.track_and_report(record, &tx, &confirmed_at, confirmations)
    }

    /// Asks the node whether the outpoint is already spent, and tracks the spender when it is.
    fn first_check_spending_utxo(
        &self,
        outpoint: OutPoint,
        record: MonitorRecord,
    ) -> Result<Vec<MonitorNews>, MonitorError> {
        // TODO: each spent outpoint walks the indexed window on its own, so registering several of them in one call
        // reads and decodes the very same blocks once per outpoint. Collecting the spent ones and walking the window
        // once for all of them would cost one pass per tick instead of one per outpoint, at the price of resolving
        // them after the queue loop rather than inside it.

        // The mempool is left out, so a spend that is only there still reads as unspent and the block that mines it brings the spender.
        if !self
            .indexer
            .rpc_is_utxo_spent(&outpoint.txid, outpoint.vout, false)?
        {
            return Ok(Vec::new()); // Still unspent, so the blocks that come will bring the spender.
        }

        // The output can only be spent in its own block or above it, so that block is as far down as the spender is looked for.
        let TransactionStatus::Confirmed {
            block_height: created_at,
            ..
        } = self.indexer.get_transaction(&outpoint.txid, false)?
        else {
            // The output's transaction is above the indexer or not mined, and so is its spender, which the blocks that come will bring.
            return Ok(Vec::new());
        };

        let (spender, lowest_read) = self.find_utxo_spender(outpoint, created_at)?;

        if let Some((tx, confirmed_at)) = spender {
            let confirmations =
                confirmed_at.confirmations_at(self.indexer.get_indexed_height()?)?;
            return self.track_and_report(record, &tx, &confirmed_at, confirmations);
        }

        // The walk reached the output's own block, so every block the spender could be in up to the cursor was read.
        // It is above the cursor, and the blocks that come will bring it.
        if lowest_read <= created_at {
            return Ok(Vec::new());
        }

        // The output is older than the window, so the spender is either older too or above a cursor that is behind the node, and
        // nothing tells the two apart. The record stays, so a spender the blocks bring later is still reported, and only cancel ends it.
        warn!("Spend of {outpoint} is not in the indexed window, and its output is older than it");
        Ok(probably_unreachable_news(&record))
    }

    /// Tracks what a first check found, in the contexts that were not already tracking it, and tells each of
    /// those that asked to hear about it at this many confirmations.
    fn track_and_report(
        &self,
        mut record: MonitorRecord,
        tx: &Transaction,
        confirmed_at: &BlockRef,
        confirmations: u32,
    ) -> Result<Vec<MonitorNews>, MonitorError> {
        let txid = tx.compute_txid();
        let status = TransactionStatus::new(
            tx.clone(),
            confirmed_at.height,
            confirmed_at.hash,
            confirmations,
        );

        let mut news = Vec::new();
        // Surviving are the entries that will continue to be tracked. The only ones that are dropped are those
        // that are older than the maximum confirmations the monitor tracks.
        let mut surviving = Vec::with_capacity(record.entries.len());
        let mut changed = false; // Whether the record changed and needs to be written back

        for mut entry in record.entries.drain(..) {
            // This context already tracks it, which is what a re-registration runs into, and it has already
            // been told. Another context registering must not make it hear about the same thing twice.
            if entry.tracked.iter().any(|tracked| tracked.txid == txid) {
                surviving.push(entry); // No change to the record, but it is kept for the next block.
                continue;
            }

            // A trigger uses >= here, and not the == a block uses, because the one thing a first check is for
            // is a transaction mined long before the subscription existed.
            if entry
                .confirmation_trigger
                .is_none_or(|trigger| confirmations >= trigger)
            {
                news.push(transaction_news(
                    &record.target,
                    &entry.context,
                    txid,
                    status.clone(),
                    false,
                ));
            }

            match self.nothing_more_to_report(entry.confirmation_trigger, confirmations) {
                false => {
                    entry
                        .tracked
                        .push(TrackedTx::new(txid, confirmed_at.clone()));
                    surviving.push(entry);
                }
                // It has been told everything it asked for, so there is nothing left to track and this subscription is over.
                true => info!(
                    "{:?} was already at {confirmations} confirmations, finishing context {}",
                    record.target, entry.context
                ),
            }

            changed = true;
        }

        record.entries = surviving;

        // Every entry was already tracking it, so the record is exactly as it was and there is nothing to write.
        if changed {
            self.persist(&record)?;
        }

        Ok(news)
    }

    /// Walks the blocks the indexer holds backwards looking for the transaction that spent this outpoint, down to the
    /// block that created the output at the lowest. Returns the spender if found, and the height of the lowest block read.
    fn find_utxo_spender(
        &self,
        outpoint: OutPoint,
        created_at: BlockHeight,
    ) -> Result<(Option<(Transaction, BlockRef)>, BlockHeight), MonitorError> {
        let mut block = self.indexer.get_last_indexed_block()?;

        for _ in 0..self.retention_depth() {
            if let Some(tx) = block.txs.iter().find(|tx| is_spending_output(tx, outpoint)) {
                return Ok((Some((tx.clone(), BlockRef::from(&block))), block.height));
            }

            if block.height <= created_at {
                break; // The output was created in this block, so nothing below it can spend it. This also stops at genesis.
            }

            let Some(previous) = self
                .indexer
                .get_stored_block(block.height - 1, &block.prev_hash)?
            else {
                break; // The window starts here.
            };

            block = previous;
        }

        Ok((None, block.height))
    }

    /// Tracks whatever this block holds for the records given, and flags the ones that started tracking something because of it.
    fn track_block_matches(&self, records: &mut [(MonitorRecord, bool)], block: &FullBlock) {
        let mut by_txid: HashMap<Txid, usize> = HashMap::new();
        let mut by_outpoint: HashMap<OutPoint, usize> = HashMap::new();
        let mut patterns: Vec<(usize, OutputPatternFilter)> = Vec::new(); // Patterns cannot be indexed, because each filter is its own rule.

        for (index, (record, _)) in records.iter().enumerate() {
            match &record.target {
                MonitorTarget::Transaction(tx_id) => {
                    by_txid.insert(*tx_id, index);
                }
                MonitorTarget::SpendingUtxo(outpoint) => {
                    by_outpoint.insert(*outpoint, index);
                }
                MonitorTarget::OutputPattern(filter) => patterns.push((index, filter.clone())),
                // Its record is not among these, and it tracks no transaction: it reports the block itself.
                MonitorTarget::NewBlock => {}
            }
        }

        // The index holds positions and a copy of each filter, so nothing borrows the records any more and the
        // walk can write to them as it goes.
        let confirmed_at = BlockRef::from(block);
        let mut track = |index: usize, txid: Txid| {
            let (record, started_tracking) = &mut records[index];

            if add_tracked_tx(record, txid, &confirmed_at) {
                info!("{:?} found {txid} in block {}", record.target, block.height);
                *started_tracking = true;
            }
        };

        for tx in &block.txs {
            let txid = tx.compute_txid();

            // A subscription to this very transaction.
            if let Some(&index) = by_txid.get(&txid) {
                track(index, txid);
            }

            // A subscription to any outpoint this transaction spends. A block cannot spend one twice, so each
            // input answers for at most one record.
            for input in &tx.input {
                if let Some(&index) = by_outpoint.get(&input.previous_output) {
                    track(index, txid);
                }
            }

            for (index, filter) in &patterns {
                if matches_output_pattern(tx, filter) {
                    track(*index, txid);
                }
            }
        }
    }

    /// One item per context subscribed to every new block, so several consumers can subscribe independently.
    fn notify_block_subscriptions(
        &self,
        block: &FullBlock,
    ) -> Result<Vec<MonitorNews>, MonitorError> {
        let Some(record) = self.store.get_record(&MonitorTarget::NewBlock)? else {
            return Ok(Vec::new()); // Nobody is subscribed to new blocks.
        };

        Ok(record
            .entries
            .iter()
            .map(|entry| MonitorNews {
                target: MonitorTarget::NewBlock,
                context: entry.context.clone(),
                kind: NewsKind::Block(BlockRef::from(block)),
            })
            .collect())
    }

    /// Drops the indexer's mempool watch when the entry that wanted it is gone and no other entry wants it. The
    /// indexer never removes one by itself, so a watch nobody wants would cost a node call on every tick.
    fn release_mempool_watch(
        &self,
        tx_id: &Txid,
        wanted_before: bool,
        record: &MonitorRecord,
    ) -> Result<(), MonitorError> {
        if wanted_before && !wants_mempool_watch(record) {
            self.indexer.remove_mempool_watch(tx_id)?;
        }

        Ok(())
    }

    /// Writes the record back, or removes it when its last subscription is gone. A record with no entries would
    /// be read on every block and answer for nobody, so it does not stay.
    fn persist(&self, record: &MonitorRecord) -> Result<(), MonitorError> {
        match record.entries.is_empty() {
            true => self.store.delete_record(&record.target),
            false => self.store.save_record(record),
        }
    }

    /// How many blocks the indexer keeps, which bounds how far back a first check can look.
    fn retention_depth(&self) -> BlockHeight {
        // Read in place: the settings are only being asked for one number, so there is nothing to copy.
        match &self.settings.indexer_settings {
            Some(settings) => settings.retention_depth,
            None => IndexerSettings::default().retention_depth,
        }
    }

    /// True when a tracked transaction will never be reported again, which is when it stops being tracked. A trigger
    /// is reported once and never restated, so firing it is the end of it. Without one the monitor tracks the
    /// transaction block by block until the maximum confirmations, which is the end of it.
    fn nothing_more_to_report(&self, trigger: Option<u32>, confirmations: u32) -> bool {
        match trigger {
            Some(trigger) => confirmations >= trigger,
            None => confirmations >= self.settings.max_monitoring_confirmations,
        }
    }

    /// Reports every tracked transaction of the record at this block, and drops the ones with nothing left to report.
    /// Returns the news and whether any of them was dropped, which is what can end a subscription.
    fn advance_to_block(
        &self,
        record: &mut MonitorRecord,
        height: BlockHeight,
    ) -> Result<(Vec<MonitorNews>, bool), MonitorError> {
        let target = record.target.clone();
        let mut news = Vec::new();
        let mut surviving = Vec::with_capacity(record.entries.len());
        let mut stopped_tracking = false; // True once some transaction of the record has been tracked as far as it ever will be.

        // A transaction subscription ends with its transaction and a UTXO subscription with its spender, both of
        // which track one at a time. A pattern is a standing rule, so it keeps matching the blocks that come.
        let ends_with_its_transaction = matches!(
            target,
            MonitorTarget::Transaction(_) | MonitorTarget::SpendingUtxo(_)
        );

        for mut entry in record.entries.drain(..) {
            let trigger = entry.confirmation_trigger;
            let context = &entry.context;
            let mut kept = Vec::with_capacity(entry.tracked.len());
            // True once a transaction of this entry has been told everything it was going to be told, which is what
            // both takes it out of the tracked list and can end the subscription that was watching for it.
            let mut finished_one = false;

            for tracked in entry.tracked.drain(..) {
                let confirmations = tracked.confirmed_at.confirmations_at(height)?;

                // Without a trigger every block is reported. With one, the block where the count equals it is.
                if trigger.is_none_or(|trigger| confirmations == trigger) {
                    // Only what is reported is read, and always from indexer storage.
                    let status = self.indexer.get_stored_transaction(&tracked.txid, false)?;

                    // The transaction should be confirmed in the indexer's storage, because it was in the current block.
                    if !matches!(status, TransactionStatus::Confirmed { .. }) {
                        return Err(MonitorError::InvariantViolation(format!(
                            "tracked transaction {} is not in a block the indexer holds",
                            tracked.txid
                        )));
                    }

                    news.push(transaction_news(
                        &target,
                        context,
                        tracked.txid,
                        status,
                        false,
                    ));
                }

                match self.nothing_more_to_report(trigger, confirmations) {
                    false => kept.push(tracked),
                    true => finished_one = true,
                }
            }

            entry.tracked = kept;
            stopped_tracking |= finished_one;

            // What this subscription was watching for is over, so it goes with it.
            if finished_one && ends_with_its_transaction && entry.tracked.is_empty() {
                debug!("{target:?} finished for context {}", entry.context);
                continue;
            }

            surviving.push(entry);
        }

        record.entries = surviving;

        Ok((news, stopped_tracking))
    }
}

/// True when some entry of this record still wants its transaction watched in the node's mempool.
fn wants_mempool_watch(record: &MonitorRecord) -> bool {
    record.entries.iter().any(|entry| entry.search_in_mempool)
}

/// Adds a TrackedTx for this transaction to the record, in every entry that does not already hold it.
fn add_tracked_tx(record: &mut MonitorRecord, txid: Txid, confirmed_at: &BlockRef) -> bool {
    let mut added = false;

    for entry in &mut record.entries {
        // Already there, so nothing is added and nothing about this entry changed. That is what the repeated
        // first check of a re-registration runs into, and a second copy would report the same thing twice.
        if entry.tracked.iter().any(|tracked| tracked.txid == txid) {
            continue;
        }

        entry
            .tracked
            .push(TrackedTx::new(txid, confirmed_at.clone()));
        added = true;
    }

    added
}

/// One item of news about one tracked transaction.
fn transaction_news(
    target: &MonitorTarget,
    context: &str,
    txid: Txid,
    status: TransactionStatus,
    due_to_reorg: bool,
) -> MonitorNews {
    MonitorNews {
        target: target.clone(),
        context: context.to_string(),
        kind: NewsKind::Transaction {
            txid,
            status,
            due_to_reorg,
        },
    }
}

/// One item per context, saying the spend it was watching for is probably older than anything the indexer holds.
fn probably_unreachable_news(record: &MonitorRecord) -> Vec<MonitorNews> {
    record
        .entries
        .iter()
        .map(|entry| MonitorNews {
            target: record.target.clone(),
            context: entry.context.clone(),
            kind: NewsKind::ProbablyUnreachable,
        })
        .collect()
}

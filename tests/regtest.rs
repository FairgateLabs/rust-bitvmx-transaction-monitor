//! Integration tests against a regtest bitcoind in Docker. Each test starts its own node and its own database,
//! and the tests take the node one at a time, so the whole file runs with the default test threads.
mod common;
use common::*;

use bitvmx_transaction_monitor::errors::MonitorError;
use bitvmx_transaction_monitor::types::{MonitorTarget, NewsKind, OutputPatternFilter};
use bitvmx_transaction_monitor::{IndexerError, TransactionStatus};

/// The pattern the output pattern tests watch for: the second output must be an OP_RETURN whose data starts
/// with this tag, and the transaction must have at most three outputs.
fn pattern() -> OutputPatternFilter {
    OutputPatternFilter {
        output_index: 1,
        tag: b"bitvmx".to_vec(),
        max_outputs: Some(3),
    }
}

// =============================================================================
// Startup
// =============================================================================

// Before the first tick the monitor knows nothing, and the first tick places the indexer's cursor. A
// subscription made before any tick is answered by the first one.
#[test]
fn test_startup() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;
    let spender = node.sign_spend(outpoint, 900_000)?;
    let txid = spender.compute_txid();
    node.broadcast(&spender)?;
    node.mine(1)?;

    let tip = node.tip()?;
    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 4, 10)?;

    // Nothing has been read yet, so there is no height to answer with, and the monitor says it is not ready.
    assert!(matches!(
        monitor.get_indexed_height(),
        Err(MonitorError::IndexerError(IndexerError::NotSynced))
    ));
    assert!(!monitor.is_ready()?);
    assert_eq!(monitor.max_monitoring_confirmations(), 4);

    // Registering writes to storage and reads nothing, so it works before the chain has been looked at.
    let target = MonitorTarget::Transaction(txid);
    monitor.monitor(&[target.clone()], "ctx".to_string(), None, false)?;

    // The first tick starts one window below the tip. The transaction is in a block above that, which the
    // indexer has not read yet, and a first check never looks above the indexer's own cursor, so it finds
    // nothing: the past it looks at is the past the indexer already holds.
    monitor.tick()?;
    assert_eq!(monitor.get_indexed_height()?, tip - 10);
    assert!(drain_news(&monitor)?.is_empty());

    // Catching up is what answers it, by the block pass, when the indexer reaches the block it is in. So a
    // subscription made before the monitor has caught up is still answered, one window later.
    node.sync(&monitor)?;
    assert!(monitor.is_ready()?);
    assert_eq!(monitor.get_indexed_height()?, tip);

    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "ctx", txid, 1, false);

    Ok(())
}

// =============================================================================
// A transaction
// =============================================================================

// A transaction subscription from before the transaction exists until it stops being watched: nothing is
// reported while it is unknown or in the mempool, every block is reported once it is mined, and the maximum
// ends both the tracking and the subscription.
#[test]
fn test_transaction_lifecycle() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;
    let spender = node.sign_spend(outpoint, 900_000)?;
    let txid = spender.compute_txid();

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 4, 10)?;
    node.sync(&monitor)?;

    let target = MonitorTarget::Transaction(txid);
    monitor.monitor(&[target.clone()], "ctx".to_string(), None, true)?;

    // The first check runs on the next tick and finds nothing: the transaction is not even broadcast.
    monitor.tick()?;
    assert!(drain_news(&monitor)?.is_empty());
    assert_eq!(monitor.get_tx_status(&txid, true)?, TransactionStatus::NotFound);

    // In the mempool it is still not news. Only a transaction in a block is ever tracked, and the mempool watch
    // only changes what the monitor can say about it, not when it says something.
    node.broadcast(&spender)?;
    monitor.tick()?;
    assert!(drain_news(&monitor)?.is_empty());
    assert_eq!(monitor.get_tx_status(&txid, true)?, TransactionStatus::InMempool);

    // Mined: one item per block from the first one, because this subscription asked for no trigger.
    mine_and_tick(&node, &monitor, 1)?;
    let height = node.height_of(&txid)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "ctx", txid, 1, false);

    match status_of(&news[0]) {
        TransactionStatus::Confirmed {
            tx,
            block_height,
            block_hash,
            ..
        } => {
            // The item carries the transaction itself, so a consumer never has to look anything up.
            assert_eq!(tx.compute_txid(), txid);
            assert_eq!(*block_height, height);
            assert_eq!(*block_hash, node.hash_at(height)?);
        }
        other => panic!("expected a confirmed status, got {other:?}"),
    }

    for expected in 2..=4 {
        mine_and_tick(&node, &monitor, 1)?;
        let news = drain_news(&monitor)?;
        assert_eq!(news.len(), 1, "one item per block at {expected}");
        assert_tx_news(&news[0], &target, "ctx", txid, expected, false);
    }

    // The maximum was reached, so it stopped being watched and the subscription that was watching for it ended.
    mine_and_tick(&node, &monitor, 2)?;
    assert!(drain_news(&monitor)?.is_empty());

    // Registering it again starts over, and the first check answers with the count it has now.
    monitor.monitor(&[target.clone()], "ctx".to_string(), None, true)?;
    monitor.tick()?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "ctx", txid, 6, false);

    // That count is already past the maximum, so there is nothing left to follow: the first check answered and
    // the subscription ended with it, instead of being tracked only to be reported once more and dropped.
    mine_and_tick(&node, &monitor, 2)?;
    assert!(drain_news(&monitor)?.is_empty());

    Ok(())
}

// A trigger is reported at the block where the count equals it and at no other, while a subscription without
// one hears about every block. A trigger that could never be reached, or that is below finality, is refused.
#[test]
fn test_confirmation_trigger() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;
    let spender = node.sign_spend(outpoint, 900_000)?;
    let txid = spender.compute_txid();

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 6, 20)?;
    node.sync(&monitor)?;

    let target = MonitorTarget::Transaction(txid);

    // Past the maximum the block of the transaction would leave the indexer's window while it is still tracked, so
    // such a trigger is refused. The maximum itself is accepted, and is the deepest one that is.
    assert!(matches!(
        monitor.monitor(&[target.clone()], "bad".to_string(), Some(7), false),
        Err(MonitorError::InvalidConfirmationTrigger(_, _, 6))
    ));

    // A trigger is reported once and never restated, so one below finality is refused: a reorg could undo what it
    // promised and the consumer would never hear about it. Finality itself is settled, so it is accepted.
    let finality_storage = TestStorage::new();
    let with_finality = node.monitor_with_finality(finality_storage.storage(), 3, 6, 20)?;
    for trigger in [1, 2] {
        assert!(matches!(
            with_finality.monitor(&[target.clone()], "bad".to_string(), Some(trigger), false),
            Err(MonitorError::InvalidConfirmationTrigger(_, 3, _))
        ));
    }
    with_finality.monitor(&[target.clone()], "at finality".to_string(), Some(3), false)?;
    with_finality.monitor(&[target.clone()], "at the maximum".to_string(), Some(6), false)?;

    monitor.monitor(&[target.clone()], "every".to_string(), None, false)?;
    monitor.monitor(&[target.clone()], "three".to_string(), Some(3), false)?;
    // The maximum is a legal trigger, and the block that reaches it is the last one the transaction is followed in.
    monitor.monitor(&[target.clone()], "at the maximum".to_string(), Some(6), false)?;

    node.broadcast(&spender)?;

    for confirmations in 1..=6 {
        mine_and_tick(&node, &monitor, 1)?;
        let news = drain_news(&monitor)?;

        // Without a trigger, every block.
        let every = of_context(&news, "every");
        assert_eq!(every.len(), 1);
        assert_tx_news(every[0], &target, "every", txid, confirmations, false);

        // With one, the block where the count equals it and no other.
        let three = of_context(&news, "three");
        match confirmations {
            3 => {
                assert_eq!(three.len(), 1);
                assert_tx_news(three[0], &target, "three", txid, 3, false);
            }
            _ => assert!(three.is_empty(), "nothing at {confirmations} confirmations"),
        }

        // A trigger on the maximum itself is reported there, in the same block the transaction stops being followed in.
        let at_max = of_context(&news, "at the maximum");
        match confirmations {
            6 => {
                assert_eq!(at_max.len(), 1);
                assert_tx_news(at_max[0], &target, "at the maximum", txid, 6, false);
            }
            _ => assert!(at_max.is_empty(), "nothing at {confirmations} confirmations"),
        }
    }

    // Everything has been told what it asked for, so nothing is left to report at the next block.
    mine_and_tick(&node, &monitor, 1)?;
    assert!(drain_news(&monitor)?.is_empty());

    Ok(())
}

// A transaction mined before the subscription existed is found by the first check, which reports it whatever
// the count when there is no trigger or the count is already past it, and waits for the block otherwise. One
// lookup answers for every context on that transaction.
#[test]
fn test_first_check_sees_the_past() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;
    let spender = node.sign_spend(outpoint, 900_000)?;
    let txid = spender.compute_txid();

    // Mined three blocks ago, so it already has three confirmations when the subscriptions are made.
    node.broadcast(&spender)?;
    node.mine(3)?;

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 8, 20)?;
    node.sync(&monitor)?;

    let target = MonitorTarget::Transaction(txid);
    monitor.monitor(&[target.clone()], "every".to_string(), None, false)?;
    monitor.monitor(&[target.clone()], "below".to_string(), Some(2), false)?;
    monitor.monitor(&[target.clone()], "above".to_string(), Some(5), false)?;

    // One tick, one lookup, three answers: the two whose trigger is already satisfied hear about it, and the
    // one still waiting for a deeper block does not.
    monitor.tick()?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 2);
    assert_tx_news(of_context(&news, "every")[0], &target, "every", txid, 3, false);
    assert_tx_news(of_context(&news, "below")[0], &target, "below", txid, 3, false);
    assert!(of_context(&news, "above").is_empty());

    // The block pass takes over from there, with the same rules as any other subscription.
    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "every", txid, 4, false);

    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 2);
    assert_tx_news(of_context(&news, "every")[0], &target, "every", txid, 5, false);
    assert_tx_news(of_context(&news, "above")[0], &target, "above", txid, 5, false);

    Ok(())
}

// A transaction whose block has already fallen out of the indexer's window is still answered: the first check asks
// the node, which is the one lookup in the monitor that reaches past what the indexer holds. The count it comes back
// with is past the maximum, so it is reported once and nothing is tracked, which is also the only way the monitor
// ever reports a transaction it cannot read from its own storage.
#[test]
fn test_first_check_asks_the_node_below_the_window() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;
    let spender = node.sign_spend(outpoint, 900_000)?;
    let txid = spender.compute_txid();

    // Mined, then buried under five more blocks, against a window of three.
    node.broadcast(&spender)?;
    node.mine(1)?;
    let height = node.height_of(&txid)?;
    node.mine(5)?;

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 2, 3)?;
    node.sync(&monitor)?;

    // Its block was pruned, which takes the height entry of every transaction in it, so the indexer cannot answer
    // for this one at all. Whatever the subscription is told next can only have come from the node.
    assert_eq!(
        node.stored_tx_status(storage.storage(), &txid)?,
        TransactionStatus::NotFound
    );

    let target = MonitorTarget::Transaction(txid);
    monitor.monitor(&[target.clone()], "ctx".to_string(), None, false)?;

    // One item, from the node, carrying the transaction and the block it is in. Six confirmations, counted from the
    // indexer's cursor like every other answer.
    monitor.tick()?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "ctx", txid, 6, false);

    match status_of(&news[0]) {
        TransactionStatus::Confirmed {
            tx,
            block_height,
            block_hash,
            ..
        } => {
            assert_eq!(tx.compute_txid(), txid);
            assert_eq!(*block_height, height);
            assert_eq!(*block_hash, node.hash_at(height)?);
        }
        other => panic!("expected a confirmed status, got {other:?}"),
    }

    // Six is past the maximum of two, so there was never anything left to follow: no record was written and no block
    // from here on says anything about it again.
    mine_and_tick(&node, &monitor, 2)?;
    assert!(drain_news(&monitor)?.is_empty());

    Ok(())
}

// =============================================================================
// A spent output
// =============================================================================

// A UTXO subscription reports the transaction that spends it, whether the spend comes after the subscription
// or was already in a block the indexer holds, and ignores a transaction that spends something else.
#[test]
fn test_spending_utxo() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let watched = node.fund_utxo(1_000_000)?;
    let already_spent = node.fund_utxo(1_000_000)?;
    let unwatched = node.fund_utxo(1_000_000)?;

    // One of them is spent and mined before the monitor exists.
    let past_spender = node.sign_spend(already_spent, 900_000)?;
    let past_txid = past_spender.compute_txid();
    node.broadcast(&past_spender)?;
    node.mine(2)?;

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 5, 20)?;
    node.sync(&monitor)?;

    let watched_target = MonitorTarget::SpendingUtxo(watched);
    let past_target = MonitorTarget::SpendingUtxo(already_spent);
    monitor.monitor(&[watched_target.clone()], "live".to_string(), None, false)?;
    monitor.monitor(&[watched_target.clone()], "deep".to_string(), Some(3), false)?;
    monitor.monitor(&[past_target.clone()], "past".to_string(), None, false)?;

    // The first check asks the node whether each one is still unspent. The one that is not is answered from the
    // blocks the indexer holds, with the count it has now. The one that is brings nothing.
    assert!(monitor.rpc_is_utxo_unspent(&watched.txid, watched.vout, true)?);
    assert!(!monitor.rpc_is_utxo_unspent(&already_spent.txid, already_spent.vout, true)?);

    monitor.tick()?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &past_target, "past", past_txid, 2, false);

    // A block that holds the spender of a watched output and the spender of an output nobody watches: only the
    // first one is reported, and the news names the outpoint as its target and the spender as its transaction.
    let spender = node.sign_spend(watched, 900_000)?;
    let spender_txid = spender.compute_txid();
    let other = node.sign_spend(unwatched, 900_000)?;
    node.mine_including(&[spender, other])?;
    monitor.tick()?;

    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 2);
    assert_tx_news(
        of_context(&news, "live")[0],
        &watched_target,
        "live",
        spender_txid,
        1,
        false,
    );
    assert_tx_news(of_context(&news, "past")[0], &past_target, "past", past_txid, 3, false);
    assert!(of_context(&news, "deep").is_empty(), "its trigger is three");

    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 2, "both subscriptions without a trigger");
    assert!(of_context(&news, "deep").is_empty());

    // A trigger means the same for a UTXO as for a transaction: the block where the count of the spender
    // equals it. The spend from the past reaches the maximum in the same block.
    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 3);
    assert_tx_news(
        of_context(&news, "deep")[0],
        &watched_target,
        "deep",
        spender_txid,
        3,
        false,
    );
    assert_eq!(confirmations_of(of_context(&news, "past")[0]), 5);

    // A UTXO subscription ends with its spender, like a transaction subscription does with its transaction, so
    // the one that reached the maximum is over while the other goes on.
    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert!(of_context(&news, "past").is_empty(), "the past spend is over");
    assert_eq!(of_context(&news, "live").len(), 1, "the live one goes on");

    Ok(())
}

// A spend in a block older than everything the indexer keeps can never be found, so the subscription is told
// once that it is unreachable and is dropped.
#[test]
fn test_spend_older_than_the_window() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;

    let spender = node.sign_spend(outpoint, 900_000)?;
    node.broadcast(&spender)?;
    node.mine(1)?;

    // Five more blocks, so the block holding the spender is well below a window of three.
    node.mine(5)?;

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 2, 3)?;
    node.sync(&monitor)?;

    let target = MonitorTarget::SpendingUtxo(outpoint);
    monitor.monitor(&[target.clone()], "ctx".to_string(), None, false)?;

    // The node says it is spent, and no block the indexer holds says by what, so there is nothing to wait for.
    monitor.tick()?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0].target, target);
    assert_eq!(news[0].context, "ctx");
    assert!(matches!(news[0].kind, NewsKind::Unreachable));

    // The subscription was dropped with the record, so blocks bring nothing more.
    mine_and_tick(&node, &monitor, 2)?;
    assert!(drain_news(&monitor)?.is_empty());

    // Registering it again is answered the same way, once, rather than silently watching for ever.
    monitor.monitor(&[target.clone()], "ctx".to_string(), None, false)?;
    monitor.tick()?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert!(matches!(news[0].kind, NewsKind::Unreachable));

    Ok(())
}

// =============================================================================
// An output pattern
// =============================================================================

// A pattern matches on what a transaction pays rather than on which transaction it is, so it needs the OP_RETURN
// at the right index, the right tag, and few enough outputs. It is a standing rule: it keeps matching the blocks
// that come, and the bound is part of the rule, so the same tag under another bound is a second subscription.
#[test]
fn test_output_pattern() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let matching = node.fund_utxo(1_000_000)?; // The one that matches: right index, right tag, few enough outputs.
    let wrong_index = node.fund_utxo(1_000_000)?; // Its OP_RETURN sits at index 0 rather than the index watched.
    let wrong_tag = node.fund_utxo(1_000_000)?; // Right shape, another tag.
    let too_many = node.fund_utxo(1_000_000)?; // Right index and tag, so the bound of three against its four outputs is the only thing rejecting it.
    let later = node.fund_utxo(1_000_000)?; // A second match, after the pattern has been registered again.
    let wide_later = node.fund_utxo(1_000_000)?; // A match for both bounds at once, once the unbounded one exists.

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 3, 20)?;
    node.sync(&monitor)?;

    let target = MonitorTarget::OutputPattern(pattern());
    monitor.monitor(&[target.clone()], "ctx".to_string(), None, false)?;
    monitor.monitor(&[target.clone()], "deep".to_string(), Some(2), false)?;

    // The tag is a prefix of the pushed data, so a pattern names a family of transactions rather than one.
    let good = node.sign_spend_to(
        matching,
        outputs_with_op_return(&node, 1, b"bitvmx-step-1", 900_000)?,
    )?;
    let good_txid = good.compute_txid();

    // The OP_RETURN is at index 0 instead of 1, which is a different shape and not a match.
    let at_index_zero =
        node.sign_spend_to(wrong_index, outputs_with_op_return(&node, 0, b"bitvmx", 900_000)?)?;

    // The right shape with another tag.
    let other_tag =
        node.sign_spend_to(wrong_tag, outputs_with_op_return(&node, 1, b"other", 900_000)?)?;

    // The right shape and tag, but above the bound on the outputs: four of them against a maximum of three.
    let mut wide = outputs_with_op_return(&node, 1, b"bitvmx", 800_000)?;
    wide.push(bitcoin::TxOut {
        value: bitcoin::Amount::from_sat(50_000),
        script_pubkey: node.fresh_address()?.script_pubkey(),
    });
    let wide = node.sign_spend_to(too_many, wide)?;

    node.mine_including(&[good, at_index_zero, other_tag, wide])?;
    monitor.tick()?;

    // One block, four candidates, one match. The context with a trigger of two hears nothing yet.
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "ctx", good_txid, 1, false);

    // A trigger means the same for a pattern as for anything else: the block where the count of what it
    // matched equals it, and no other.
    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 2);
    assert_tx_news(of_context(&news, "deep")[0], &target, "deep", good_txid, 2, false);
    assert_eq!(confirmations_of(of_context(&news, "ctx")[0]), 2);

    // The transaction it found stops being watched at the maximum, but the pattern itself does not: it is a
    // standing rule, not a subscription to one transaction.
    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_eq!(confirmations_of(&news[0]), 3);

    mine_and_tick(&node, &monitor, 1)?;
    assert!(drain_news(&monitor)?.is_empty());

    // Registering the same pattern again under the same context finds the subscription that is already there
    // instead of adding a second one, which would match the same blocks and report everything twice.
    monitor.monitor(&[target.clone()], "ctx".to_string(), Some(1), false)?;

    let next = node.sign_spend_to(
        later,
        outputs_with_op_return(&node, 1, b"bitvmx-step-2", 900_000)?,
    )?;
    let next_txid = next.compute_txid();
    node.mine_including(&[next])?;
    monitor.tick()?;

    let news = drain_news(&monitor)?;
    assert_eq!(
        of_context(&news, "ctx").len(),
        1,
        "one subscription, so one item for the one match"
    );
    assert_tx_news(
        of_context(&news, "ctx")[0],
        &target,
        "ctx",
        next_txid,
        1,
        false,
    );

    // The bound is part of what identifies a pattern, so the same index and tag without it is another rule and
    // another subscription. A transaction matching both is reported once for each, under its own target.
    let unbounded = MonitorTarget::OutputPattern(OutputPatternFilter {
        max_outputs: None,
        ..pattern()
    });
    monitor.monitor(&[unbounded.clone()], "ctx".to_string(), None, false)?;

    let both = node.sign_spend_to(
        wide_later,
        outputs_with_op_return(&node, 1, b"bitvmx-step-3", 900_000)?,
    )?;
    let both_txid = both.compute_txid();
    node.mine_including(&[both])?;
    monitor.tick()?;

    let news = drain_news(&monitor)?;
    let reported = of_context(&news, "ctx");
    assert_eq!(reported.len(), 2, "two rules match it, so each one reports it");
    assert_tx_news(reported[0], &target, "ctx", both_txid, 1, false);
    assert_tx_news(reported[1], &unbounded, "ctx", both_txid, 1, false);

    Ok(())
}

// =============================================================================
// New blocks
// =============================================================================

// Every context subscribed to new blocks hears about each block once, with the height and hash of that block,
// and cancelling one context leaves the others alone.
#[test]
fn test_new_block_subscription() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 2, 5)?;
    node.sync(&monitor)?;

    // A block has no confirmations of its own, so a trigger on this target is refused instead of ignored.
    let target = MonitorTarget::NewBlock;
    assert!(matches!(
        monitor.monitor(&[target.clone()], "bad".to_string(), Some(1), false),
        Err(MonitorError::InvalidSubscription(_))
    ));

    monitor.monitor(&[target.clone()], "a".to_string(), None, false)?;
    monitor.monitor(&[target.clone()], "b".to_string(), None, false)?;

    mine_and_tick(&node, &monitor, 1)?;
    let height = node.tip()?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 2);

    for context in ["a", "b"] {
        let items = of_context(&news, context);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].target, target);
        assert_eq!(block_of(items[0]).height, height);
        assert_eq!(block_of(items[0]).hash, node.hash_at(height)?);
    }

    // One context leaving does not take the other's subscription with it.
    monitor.cancel(&[target.clone()], "a")?;
    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0].context, "b");

    // With nobody left, the record goes and blocks bring nothing.
    monitor.cancel(&[target], "b")?;
    mine_and_tick(&node, &monitor, 1)?;
    assert!(drain_news(&monitor)?.is_empty());

    Ok(())
}

// =============================================================================
// Reorgs
// =============================================================================

// A reorg restates what was already reported, against the chain that is left. Only a subscription without a trigger
// hears about it: a trigger is set deeper than a reorg can reach, so once it has fired what it promised stands and
// the subscription is over. The mempool watch is what decides whether a transaction that lost its block is reported
// as pending or as unknown.
#[test]
fn test_reorg_restates() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;
    let spender = node.sign_spend(outpoint, 900_000)?;
    let txid = spender.compute_txid();

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 6, 20)?;
    node.sync(&monitor)?;

    let target = MonitorTarget::Transaction(txid);
    monitor.monitor(&[target.clone()], "every".to_string(), None, true)?;
    monitor.monitor(&[target.clone()], "two".to_string(), Some(2), false)?;
    monitor.monitor(&[target.clone()], "five".to_string(), Some(5), false)?;

    node.broadcast(&spender)?;
    mine_and_tick(&node, &monitor, 1)?;
    let height = node.height_of(&txid)?;
    let first_hash = node.hash_at(height)?;

    // Up to three confirmations: every block for one, the third block for none of the others, and the trigger
    // of two exactly once.
    assert_eq!(drain_news(&monitor)?.len(), 1);
    mine_and_tick(&node, &monitor, 1)?;
    assert_eq!(drain_news(&monitor)?.len(), 2); // "every" at two, and the trigger of two.
    mine_and_tick(&node, &monitor, 1)?;
    assert_eq!(drain_news(&monitor)?.len(), 1);

    // The block holding it is reorged away, which takes the two above it as well. One tick unwinds all three.
    node.invalidate(height)?;
    monitor.tick()?;
    assert_eq!(monitor.get_indexed_height()?, height - 1);

    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);

    // It asked about the mempool, so it is told the transaction is pending rather than unknown.
    let every = of_context(&news, "every");
    assert_eq!(every.len(), 1);
    assert_tx_news(every[0], &target, "every", txid, 0, true);
    assert_eq!(status_of(every[0]), &TransactionStatus::InMempool);

    // A trigger is reported once and never restated: a trigger is set deeper than a reorg can reach, so what it
    // promised stands. The subscription of two ended when it fired and is not here to be told anything.
    assert!(of_context(&news, "two").is_empty());

    // The trigger of five never fired, so there is nothing to restate to it either. It is still waiting, and its
    // transaction went back to being something the blocks that come have to find again.
    assert!(of_context(&news, "five").is_empty());

    // Mined again, into a block of its own at the same height, and from here it is an ordinary discovery
    // rather than a restatement: nothing it is told now carries the reorg flag.
    mine_and_tick(&node, &monitor, 1)?;
    assert_eq!(node.height_of(&txid)?, height);
    assert_ne!(node.hash_at(height)?, first_hash, "a different block");

    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "every", txid, 1, false);

    // The trigger of two is not reported again on the new chain: it fired once, and that is all it ever promised.
    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "every", txid, 2, false);

    mine_and_tick(&node, &monitor, 2)?;
    drain_news(&monitor)?;

    // A reorg that leaves its block alone and only takes blocks above it: the count falls without breaking
    // anything, so only the subscription that hears about every change is told.
    let tip = node.tip()?;
    node.invalidate(tip)?;
    monitor.tick()?;

    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "every", txid, 3, true);
    assert!(status_of(&news[0]).is_confirmed(), "it kept its own block");

    Ok(())
}

// A UTXO subscription watches an output, not a transaction, so a reorg can hand it a different spender than the one
// it found. The first spender is reported, then taken back when its block goes, and the subscription returns to
// waiting and follows whatever spends the output on the new chain, under the same target and context.
#[test]
fn test_reorg_changes_the_spender() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;

    // Two transactions spending the same output, so only one of them can ever be in the chain. The second pays a
    // much larger fee, which is what lets it replace the first in the mempool once the reorg puts it back there.
    let first = node.sign_spend(outpoint, 900_000)?;
    let second = node.sign_spend(outpoint, 700_000)?;
    let first_txid = first.compute_txid();
    let second_txid = second.compute_txid();
    assert_ne!(first_txid, second_txid);

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 4, 20)?;
    node.sync(&monitor)?;

    let target = MonitorTarget::SpendingUtxo(outpoint);
    monitor.monitor(&[target.clone()], "ctx".to_string(), None, false)?;

    // Still unspent when the subscription is made, so the first check finds nothing.
    monitor.tick()?;
    assert!(drain_news(&monitor)?.is_empty());

    node.mine_including(&[first])?;
    let height = node.height_of(&first_txid)?;
    monitor.tick()?;

    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "ctx", first_txid, 1, false);

    // The block holding that spender is reorged away. The subscription hears that the spend it was told about is
    // gone, and goes back to waiting for the output to be spent.
    node.invalidate(height)?;
    monitor.tick()?;

    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "ctx", first_txid, 0, true);
    assert_eq!(status_of(&news[0]), &TransactionStatus::NotFound);

    // The output is spent again on the new chain, by the other transaction. The subscription reports that one as an
    // ordinary discovery: a new spender, not a restatement of the old one.
    node.mine_including(&[second])?;
    monitor.tick()?;

    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "ctx", second_txid, 1, false);
    assert_eq!(txid_of(&news[0]), second_txid, "the spender changed identity");

    // From here the new spender is followed like any other, to the maximum, and the subscription ends with it.
    mine_and_tick(&node, &monitor, 3)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 3);
    assert_tx_news(&news[2], &target, "ctx", second_txid, 4, false);

    mine_and_tick(&node, &monitor, 1)?;
    assert!(drain_news(&monitor)?.is_empty());

    Ok(())
}

// =============================================================================
// Several subscriptions to one target
// =============================================================================

// Contexts on the same target are independent: a new one is answered without telling the others again, one
// leaving takes only its own news, and registering again changes the parameters without losing what was found.
// Cancelling what is not there, a context that never subscribed or a target nobody watches, does nothing at all.
#[test]
fn test_several_contexts_on_one_target() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;
    let spender = node.sign_spend(outpoint, 900_000)?;
    let txid = spender.compute_txid();

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 6, 20)?;
    node.sync(&monitor)?;

    let target = MonitorTarget::Transaction(txid);
    monitor.monitor(&[target.clone()], "first".to_string(), None, false)?;

    node.broadcast(&spender)?;
    mine_and_tick(&node, &monitor, 1)?;
    assert_eq!(drain_news(&monitor)?.len(), 1);

    // A second context subscribes to a target that is already being tracked. Its own first check answers it,
    // and the context that was already there is not told the same thing twice.
    monitor.monitor(&[target.clone()], "second".to_string(), Some(1), false)?;
    monitor.tick()?;

    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1, "only the context that just asked is told");
    assert_tx_news(&news[0], &target, "second", txid, 1, false);

    // Registering again only changes the parameters: what it already tracks stays tracked, so nothing is
    // reported twice and the new trigger applies from here.
    monitor.monitor(&[target.clone()], "second".to_string(), Some(3), false)?;
    monitor.tick()?;
    assert!(drain_news(&monitor)?.is_empty());

    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0].context, "first");

    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 2); // "first" at three, and the new trigger of three.
    assert_eq!(of_context(&news, "second").len(), 1);

    // One context cancelling leaves the other watching the same transaction.
    monitor.cancel(&[target.clone()], "first")?;
    mine_and_tick(&node, &monitor, 1)?;
    assert!(drain_news(&monitor)?.is_empty());

    monitor.monitor(&[target.clone()], "third".to_string(), None, false)?;
    monitor.tick()?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0].context, "third");

    // Cancelling a context that does not watch this target takes nothing away from the one that does.
    monitor.cancel(&[target.clone()], "never subscribed")?;

    // A subscription cancelled before the next tick never has its first check run. The queue still names the target,
    // because cancelling does not touch it, but the record it would answer for is gone. The transaction here is the
    // funding one, already deep in the chain, so a check that did run would have reported it.
    let funding = MonitorTarget::Transaction(outpoint.txid);
    monitor.monitor(&[funding.clone()], "fleeting".to_string(), None, false)?;
    monitor.cancel(&[funding.clone()], "fleeting")?;
    monitor.tick()?;
    assert!(drain_news(&monitor)?.is_empty());

    // Its last context left, so the record went with it, and cancelling a target nobody is subscribed to is not an
    // error either: there is simply nothing to take away.
    monitor.cancel(&[funding], "fleeting")?;

    // None of that disturbed the subscription that was there.
    mine_and_tick(&node, &monitor, 1)?;
    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_eq!(news[0].context, "third");

    Ok(())
}

// The indexer's mempool watch is keyed by the transaction, not by the subscription, so one context leaving must
// not blind another that still wants it, and the last one leaving must take it away. Only a watched transaction 
// can be answered from the indexer's mempool snapshot, and a reorg is where that shows: it reads the snapshot and
// never asks the node.
#[test]
fn test_mempool_watch_outlives_one_of_its_contexts() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;
    let spender = node.sign_spend(outpoint, 900_000)?;
    let txid = spender.compute_txid();

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 4, 20)?;
    node.sync(&monitor)?;

    // Only a transaction can be watched in the mempool, so asking for it on any other target is refused rather
    // than silently dropped, and one bad target in a call takes the whole call with it.
    let utxo_target = MonitorTarget::SpendingUtxo(outpoint);
    assert!(matches!(
        monitor.monitor(&[utxo_target.clone()], "bad".to_string(), None, true),
        Err(MonitorError::InvalidSubscription(_))
    ));

    // Two contexts on one transaction, both asking for mempool answers and both hearing about every block.
    let target = MonitorTarget::Transaction(txid);
    assert!(matches!(
        monitor.monitor(
            &[target.clone(), utxo_target],
            "bad".to_string(),
            None,
            true
        ),
        Err(MonitorError::InvalidSubscription(_))
    ));
    monitor.monitor(&[target.clone()], "keeps".to_string(), None, true)?;
    monitor.monitor(&[target.clone()], "leaves".to_string(), None, true)?;

    node.broadcast(&spender)?;
    mine_and_tick(&node, &monitor, 1)?;
    let height = node.height_of(&txid)?;
    assert_eq!(drain_news(&monitor)?.len(), 2);

    // One of them goes. The watch it wanted is still wanted by the other.
    monitor.cancel(&[target.clone()], "leaves")?;
    assert!(is_mempool_watched(&storage.storage(), &txid)?);

    // The block is reorged away and the transaction falls back into the mempool. The context that stayed is told
    // it is pending, which is only possible while the txid is still watched.
    node.invalidate(height)?;
    monitor.tick()?;

    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "keeps", txid, 0, true);
    assert_eq!(status_of(&news[0]), &TransactionStatus::InMempool);

    // The last context that wanted the watch leaves, and the watch goes with it.
    monitor.cancel(&[target.clone()], "keeps")?;
    assert!(!is_mempool_watched(&storage.storage(), &txid)?);

    Ok(())
}

// Every kind of target in one call, under one context: one block answers all of them, and cancelling the same
// list takes the lot.
#[test]
fn test_many_targets_in_one_call() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let plain = node.fund_utxo(1_000_000)?;
    let spent = node.fund_utxo(1_000_000)?;
    let patterned = node.fund_utxo(1_000_000)?;

    let plain_tx = node.sign_spend(plain, 900_000)?;
    let plain_txid = plain_tx.compute_txid();
    let spender = node.sign_spend(spent, 900_000)?;
    let spender_txid = spender.compute_txid();
    let matching = node.sign_spend_to(
        patterned,
        outputs_with_op_return(&node, 1, b"bitvmx", 900_000)?,
    )?;
    let matching_txid = matching.compute_txid();

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 3, 20)?;
    node.sync(&monitor)?;

    let targets = vec![
        MonitorTarget::Transaction(plain_txid),
        MonitorTarget::SpendingUtxo(spent),
        MonitorTarget::OutputPattern(pattern()),
        MonitorTarget::NewBlock,
    ];
    monitor.monitor(&targets, "ctx".to_string(), None, false)?;

    // The two with a past to look at are checked once each, and none of the transactions exists yet.
    monitor.tick()?;
    assert!(drain_news(&monitor)?.is_empty());

    // One block holding all three transactions: each subscription is answered once, and so is the block.
    node.mine_including(&[plain_tx, spender, matching])?;
    monitor.tick()?;

    let news = drain_news(&monitor)?;
    assert_eq!(news.len(), 4);
    let height = node.tip()?;

    for item in &news {
        match &item.target {
            MonitorTarget::Transaction(_) => {
                assert_tx_news(item, &targets[0], "ctx", plain_txid, 1, false)
            }
            MonitorTarget::SpendingUtxo(_) => {
                assert_tx_news(item, &targets[1], "ctx", spender_txid, 1, false)
            }
            MonitorTarget::OutputPattern(_) => {
                assert_tx_news(item, &targets[2], "ctx", matching_txid, 1, false)
            }
            MonitorTarget::NewBlock => {
                assert_eq!(block_of(item).height, height);
                assert_eq!(block_of(item).hash, node.hash_at(height)?);
            }
        }
    }

    // Cancelling the same list leaves nothing watching, whatever each of them had found.
    monitor.cancel(&targets, "ctx")?;
    mine_and_tick(&node, &monitor, 1)?;
    assert!(drain_news(&monitor)?.is_empty());

    Ok(())
}

// =============================================================================
// The news log
// =============================================================================

// News waits until it is acknowledged, keeps the order it was decided in for one transaction, and is dropped
// for a context that cancels. Acknowledging takes one item and leaves the rest, and reading, with or without a
// limit, consumes nothing.
#[test]
fn test_news_is_kept_until_acknowledged() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;
    let spender = node.sign_spend(outpoint, 900_000)?;
    let txid = spender.compute_txid();

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 6, 20)?;
    node.sync(&monitor)?;

    let target = MonitorTarget::Transaction(txid);
    monitor.monitor(&[target.clone()], "keep".to_string(), None, false)?;
    monitor.monitor(&[target.clone()], "drop".to_string(), None, false)?;

    node.broadcast(&spender)?;
    mine_and_tick(&node, &monitor, 3)?;

    // Nothing was acknowledged, so three blocks are still waiting for each context, deepest last.
    let news = monitor.get_news(None)?;
    assert_eq!(news.len(), 6);

    for context in ["keep", "drop"] {
        let counts: Vec<u32> = of_context(&news, context)
            .iter()
            .map(|item| confirmations_of(item))
            .collect();
        assert_eq!(counts, vec![1, 2, 3], "in the order they were decided");
    }

    // Reading is not consuming: both contexts watch one transaction, so everything pending is under its one key,
    // and asking for one key hands back all six items, as many times as it is asked.
    assert_eq!(monitor.get_news(Some(1))?, news);
    assert_eq!(monitor.get_news(Some(1))?, news);
    assert!(monitor.get_news(Some(0))?.is_empty());

    // Acknowledging takes the one item it was given and nothing else.
    let first = of_context(&news, "keep")[0].clone();
    monitor.ack_news(&first)?;
    assert_eq!(monitor.get_news(None)?.len(), 5);

    // Acknowledging it again is not an error, and takes nothing with it.
    monitor.ack_news(&first)?;
    assert_eq!(monitor.get_news(None)?.len(), 5);

    // Cancelling drops what that context had waiting, which is the only news ever deleted unacknowledged, and
    // leaves every other context's alone.
    monitor.cancel(&[target.clone()], "drop")?;
    let news = monitor.get_news(None)?;
    assert_eq!(news.len(), 2);
    assert!(of_context(&news, "drop").is_empty());
    assert_eq!(of_context(&news, "keep").len(), 2);

    Ok(())
}

// The limit of get_news counts transactions, not items. Everything pending about one transaction is stored
// together and comes back together, so one key can answer with many items while another answers with few.
#[test]
fn test_news_limit_counts_transactions() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let first_outpoint = node.fund_utxo(1_000_000)?;
    let second_outpoint = node.fund_utxo(1_000_000)?;
    let watched = node.sign_spend(first_outpoint, 900_000)?;
    let other = node.sign_spend(second_outpoint, 900_000)?;
    let watched_txid = watched.compute_txid();
    let other_txid = other.compute_txid();

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 6, 20)?;
    node.sync(&monitor)?;

    // Two contexts on one transaction and one on the other, so the two keys hold a different number of items.
    let watched_target = MonitorTarget::Transaction(watched_txid);
    let other_target = MonitorTarget::Transaction(other_txid);
    monitor.monitor(&[watched_target.clone()], "a".to_string(), None, false)?;
    monitor.monitor(&[watched_target.clone()], "b".to_string(), None, false)?;
    monitor.monitor(&[other_target.clone()], "a".to_string(), None, false)?;

    // Both in the same block, then one more block, so every subscription has two items waiting.
    node.mine_including(&[watched, other])?;
    monitor.tick()?;
    mine_and_tick(&node, &monitor, 1)?;

    // Four items about the first transaction, two about the second, none of it acknowledged.
    assert_eq!(monitor.get_news(None)?.len(), 6);

    // One key is one transaction, read whole: every item about whichever of the two txids sorts first, and
    // nothing at all about the other.
    let one_key = monitor.get_news(Some(1))?;
    let served = txid_of(&one_key[0]);
    assert!(one_key.iter().all(|item| txid_of(item) == served));
    assert_eq!(
        one_key.len(),
        match served == watched_txid {
            true => 4, // Two contexts, two blocks each.
            false => 2,
        }
    );

    // Both keys is everything, and a limit of none asks for no key at all.
    assert_eq!(monitor.get_news(Some(2))?.len(), 6);
    assert!(monitor.get_news(Some(0))?.is_empty());

    Ok(())
}

// A reorg that leaves a block and later comes back to it decides the same thing twice, and the two items are
// identical. One acknowledgement clears both, because a consumer cannot tell them apart.
#[test]
fn test_the_same_block_reported_twice() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 2, 10)?;
    node.sync(&monitor)?;

    let target = MonitorTarget::NewBlock;
    monitor.monitor(&[target.clone()], "ctx".to_string(), None, false)?;

    mine_and_tick(&node, &monitor, 1)?;
    let height = node.tip()?;
    let hash = node.hash_at(height)?;
    assert_eq!(monitor.get_news(None)?.len(), 1);

    // The block leaves the chain. Block news is never withdrawn, so what was reported stays pending.
    node.invalidate(height)?;
    monitor.tick()?;
    assert_eq!(monitor.get_indexed_height()?, height - 1);
    assert_eq!(monitor.get_news(None)?.len(), 1);

    // The node switches back to it, so the monitor reads the very same block again and reports it again.
    node.reconsider(&hash)?;
    monitor.tick()?;
    assert_eq!(monitor.get_indexed_height()?, height);

    let news = monitor.get_news(None)?;
    assert_eq!(news.len(), 2);
    assert_eq!(news[0], news[1], "the same block, reported twice");

    // One acknowledgement clears every copy of it.
    monitor.ack_news(&news[0])?;
    assert!(monitor.get_news(None)?.is_empty());

    Ok(())
}

// =============================================================================
// Restart
// =============================================================================

// Everything the monitor knows is in the storage the consumer owns, so a new monitor over the same database picks
// the work up where the old one left it: the news nobody acknowledged is still waiting, and the subscription is
// still following its transaction rather than starting over.
#[test]
fn test_state_survives_a_restart() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;
    let spender = node.sign_spend(outpoint, 900_000)?;
    let txid = spender.compute_txid();

    let storage = TestStorage::new();
    let target = MonitorTarget::Transaction(txid);

    let monitor = node.monitor(storage.storage(), 4, 20)?;
    node.sync(&monitor)?;
    monitor.monitor(&[target.clone()], "ctx".to_string(), None, false)?;

    node.broadcast(&spender)?;
    mine_and_tick(&node, &monitor, 1)?;

    // Read without acknowledging, then stop the monitor with the item still pending.
    let news = monitor.get_news(None)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "ctx", txid, 1, false);
    drop(monitor);

    // A new monitor over the same storage. The unacknowledged item is still there, and acknowledging it through
    // this one is what clears it.
    let resumed = node.monitor(storage.storage(), 4, 20)?;
    let news = resumed.get_news(None)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "ctx", txid, 1, false);

    resumed.ack_news(&news[0])?;
    assert!(resumed.get_news(None)?.is_empty());

    // The subscription came back with it, still following the transaction it had already found: the next block is
    // reported at two confirmations, not as a fresh discovery at one.
    mine_and_tick(&node, &resumed, 1)?;
    let news = drain_news(&resumed)?;
    assert_eq!(news.len(), 1);
    assert_tx_news(&news[0], &target, "ctx", txid, 2, false);

    Ok(())
}

// =============================================================================
// The queries the monitor passes through
// =============================================================================

// What the monitor answers about the chain outside its subscriptions, which is the indexer and the node seen
// through it.
#[test]
fn test_queries() -> anyhow::Result<()> {
    init_trace();
    let node = TestNode::start(101)?;
    let outpoint = node.fund_utxo(1_000_000)?;
    let spender = node.sign_spend(outpoint, 900_000)?;
    let txid = spender.compute_txid();

    let storage = TestStorage::new();
    let monitor = node.monitor(storage.storage(), 2, 10)?;
    node.sync(&monitor)?;

    let tip = node.tip()?;
    assert_eq!(monitor.get_indexed_height()?, tip);
    assert!(monitor.is_ready()?);

    // A block the indexer holds, and a hash it does not have at that height.
    let block = monitor
        .get_block(tip, &node.hash_at(tip)?)?
        .expect("the tip is held");
    assert_eq!(block.height, tip);
    assert_eq!(block.hash, node.hash_at(tip)?);
    assert_eq!(monitor.get_block(tip, &node.hash_at(tip - 1)?)?, None);

    // An unknown transaction, then the same one in the mempool, then in a block.
    assert_eq!(monitor.get_tx_status(&txid, true)?, TransactionStatus::NotFound);
    assert_eq!(monitor.rpc_get_tx_confirmations(&txid)?, None);
    assert!(monitor.rpc_is_utxo_unspent(&outpoint.txid, outpoint.vout, true)?);

    node.broadcast(&spender)?;
    monitor.tick()?;

    // Nothing watches it in the mempool, so the indexer has nothing stored, but the node answers.
    assert_eq!(monitor.rpc_get_tx_confirmations(&txid)?, Some(0));
    assert!(!monitor.rpc_is_utxo_unspent(&outpoint.txid, outpoint.vout, true)?);
    assert!(monitor.rpc_is_utxo_unspent(&outpoint.txid, outpoint.vout, false)?);

    mine_and_tick(&node, &monitor, 1)?;
    let status = monitor.get_tx_status(&txid, false)?;
    assert!(status.is_confirmed());
    assert_eq!(status.confirmations(), 1);
    assert_eq!(monitor.rpc_get_tx_confirmations(&txid)?, Some(1));

    // The fee rate comes from the last block the indexer read, and it is refused when that block holds too few
    // transactions to say anything. Every regtest block here is that thin, so this is the answer to expect.
    assert!(matches!(
        monitor.get_estimated_fee_rate(),
        Err(MonitorError::IndexerError(
            IndexerError::FeeRateNotEstimated
        ))
    ));

    Ok(())
}

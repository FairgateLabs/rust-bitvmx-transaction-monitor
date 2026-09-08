use anyhow::Result;
use bitcoin::Txid;
use bitvmx_transaction_monitor::types::MonitorNews;
use tracing::info;

use crate::utils::{
    ack_spending_utxo_monitor, clear_output, create_and_send_funding_transaction,
    create_and_send_spending_transaction, create_test_setup, mine_blocks, monitor_spending_utxo,
    sync_monitor,
};

mod utils;

/// Asserts that the news is a SpendingUTXOTransaction for the given UTXO and that it carries the spender.
fn assert_spender_news(
    news: &MonitorNews,
    target_txid: Txid,
    target_vout: u32,
    expected_spender: Txid,
    expected_context: &str,
) {
    match news {
        MonitorNews::SpendingUTXOTransaction(tx_id, vout, tx_status, context) => {
            assert_eq!(*tx_id, target_txid, "target txid mismatch");
            assert_eq!(*vout, target_vout, "target vout mismatch");
            assert_eq!(context, expected_context, "context mismatch");

            let reported_spender = tx_status
                .tx
                .as_ref()
                .expect("news must carry the spending transaction")
                .compute_txid();

            assert_eq!(
                reported_spender, expected_spender,
                "expected spender {}, got {}",
                expected_spender, reported_spender
            );
            assert!(
                tx_status.confirmations > 0,
                "spender should be confirmed, got {} confirmations",
                tx_status.confirmations
            );
        }
        other => panic!("Expected SpendingUTXOTransaction news, got {:?}", other),
    }
}

/// A UTXO is spent and buried under further blocks before anyone subscribes to it.
#[test]
fn test_spending_utxo_already_spent_before_subscription() -> Result<()> {
    let max_monitoring_confirmations = 40;
    let (bitcoin_client, monitor, bitcoind) = create_test_setup(max_monitoring_confirmations)?;

    let context = "already_spent_before_subscription";

    // Create the UTXO and confirm it.
    let (_, funding_txid, funding_vout) = create_and_send_funding_transaction(&bitcoin_client)?;
    mine_blocks(&bitcoin_client, 1)?;

    // Spend it and confirm the spender.
    let (_, spending_txid) =
        create_and_send_spending_transaction(&bitcoin_client, funding_txid, funding_vout)?;
    mine_blocks(&bitcoin_client, 1)?;

    mine_blocks(&bitcoin_client, 2)?; // Bury the spend so the tip no longer contains it.
    sync_monitor(&monitor)?; // Advance the monitor past the spend. This is what creates the gap.

    info!(
        "Subscribing to UTXO({}:{}) after it was already spent by Transaction({})",
        funding_txid, funding_vout, spending_txid
    );
    monitor_spending_utxo(&monitor, funding_txid, funding_vout, context, None)?;

    sync_monitor(&monitor)?;

    let news = monitor.get_news()?;
    let spending_news: Vec<_> = news
        .iter()
        .filter(|n| matches!(n, MonitorNews::SpendingUTXOTransaction(..)))
        .collect();

    assert_eq!(
        spending_news.len(),
        1,
        "expected exactly one spending UTXO news, got {:?}",
        spending_news
    );

    assert_spender_news(
        spending_news[0],
        funding_txid,
        funding_vout,
        spending_txid,
        context,
    );

    ack_spending_utxo_monitor(&monitor, funding_txid, funding_vout, context)?;

    bitcoind.stop()?;
    clear_output();

    Ok(())
}

/// The catch up must not disturb the normal case. Subscribing to a UTXO that is still unspent reports nothing.
#[test]
fn test_spending_utxo_still_unspent_at_subscription() -> Result<()> {
    let max_monitoring_confirmations = 40;
    let (bitcoin_client, monitor, bitcoind) = create_test_setup(max_monitoring_confirmations)?;

    let context = "unspent_at_subscription";

    let (_, funding_txid, funding_vout) = create_and_send_funding_transaction(&bitcoin_client)?;
    mine_blocks(&bitcoin_client, 2)?;
    sync_monitor(&monitor)?;

    // The UTXO is still unspent here, so the catch up must find nothing and report nothing.
    monitor_spending_utxo(&monitor, funding_txid, funding_vout, context, None)?;
    sync_monitor(&monitor)?;

    let news = monitor.get_news()?;
    assert!(
        !news
            .iter()
            .any(|n| matches!(n, MonitorNews::SpendingUTXOTransaction(..))),
        "no spending news expected while the UTXO is unspent, got {:?}",
        news
    );

    // Now spend it. The forward path should report it as the block is synced.
    let (_, spending_txid) =
        create_and_send_spending_transaction(&bitcoin_client, funding_txid, funding_vout)?;
    mine_blocks(&bitcoin_client, 1)?;
    sync_monitor(&monitor)?;

    let news = monitor.get_news()?;
    let spending_news: Vec<_> = news
        .iter()
        .filter(|n| matches!(n, MonitorNews::SpendingUTXOTransaction(..)))
        .collect();

    assert_eq!(
        spending_news.len(),
        1,
        "expected exactly one spending UTXO news, got {:?}",
        spending_news
    );

    assert_spender_news(
        spending_news[0],
        funding_txid,
        funding_vout,
        spending_txid,
        context,
    );

    ack_spending_utxo_monitor(&monitor, funding_txid, funding_vout, context)?;

    bitcoind.stop()?;
    clear_output();

    Ok(())
}

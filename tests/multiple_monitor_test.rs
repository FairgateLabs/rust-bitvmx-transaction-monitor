use anyhow::Result;
use bitvmx_bitcoin_rpc::bitcoin_client::BitcoinClientApi;
use bitvmx_transaction_monitor::types::{MonitorNews, TypesToMonitor};
use tracing::info;

use crate::utils::{
    clear_output, create_and_send_a_new_transaction, create_and_send_funding_transaction,
    create_and_send_rsk_pegin_transaction, create_and_send_spending_transaction, create_test_setup,
    mine_blocks, monitor_rsk_pegin, monitor_spending_utxo, monitor_tx, sync_monitor,
};

mod utils;

/// Test that verifies monitoring of multiple transaction types simultaneously.
///
/// This test creates monitors for different transaction types:
/// - Transactions: 4 monitors (2 with trigger, 2 without trigger)
/// - SpendingUTXOTransaction: 4 monitors (2 with trigger, 2 without trigger)
/// - RskPegin: 1 monitor (monitors all RSK pegin transactions, with trigger)
/// - NewBlock: 1 monitor (monitors all new blocks)
///
/// The test then:
/// 1. Creates all the necessary transactions (4 regular tx, 4 funding tx, 4 RSK pegin tx)
/// 2. Sets up all monitors
/// 3. Creates spending transactions for the funding transactions
/// 4. Mines blocks to confirm transactions and trigger news
/// 5. Reviews and verifies all news items after 1 confirmation
#[test]
fn test_multiple_monitors_all_types() -> Result<(), anyhow::Error> {
    // max_monitoring_confirmations should be greater than confirmation_trigger otherwise the monitor will throw an error.
    let big_confirmation_trigger = 35;
    let small_confirmation_trigger = 1;
    let max_monitoring_confirmations = 40;
    let (bitcoin_client, monitor, bitcoind) = create_test_setup(max_monitoring_confirmations)?;

    // ============================================================================
    // PART 1: Create all transactions
    // ============================================================================

    // Create 4 regular transactions
    let (_tx1, tx_id_1) = create_and_send_a_new_transaction(&bitcoin_client)?;
    let (_tx2, tx_id_2) = create_and_send_a_new_transaction(&bitcoin_client)?;
    let (_tx3, tx_id_3) = create_and_send_a_new_transaction(&bitcoin_client)?;
    let (_tx4, tx_id_4) = create_and_send_a_new_transaction(&bitcoin_client)?;

    // Create 4 funding transactions for SpendingUTXO monitoring
    let (_, funding_txid_1, funding_vout_1) = create_and_send_funding_transaction(&bitcoin_client)?;
    let (_, funding_txid_2, funding_vout_2) = create_and_send_funding_transaction(&bitcoin_client)?;
    let (_, funding_txid_3, funding_vout_3) = create_and_send_funding_transaction(&bitcoin_client)?;
    let (_, funding_txid_4, funding_vout_4) = create_and_send_funding_transaction(&bitcoin_client)?;

    // Create 4 RSK pegin transactions
    let (_, rsk_pegin_txid_1) = create_and_send_rsk_pegin_transaction(&bitcoin_client)?;
    let (_, rsk_pegin_txid_2) = create_and_send_rsk_pegin_transaction(&bitcoin_client)?;
    let (_, rsk_pegin_txid_3) = create_and_send_rsk_pegin_transaction(&bitcoin_client)?;
    let (_, rsk_pegin_txid_4) = create_and_send_rsk_pegin_transaction(&bitcoin_client)?;

    // ============================================================================
    // PART 2: Set up all monitors
    // ============================================================================

    // Monitor 4 Transactions: 2 with trigger, 2 without
    monitor_tx(
        &monitor,
        tx_id_1,
        "tx_context_1_with_trigger",
        Some(big_confirmation_trigger),
    )?;
    monitor_tx(
        &monitor,
        tx_id_2,
        "tx_context_2_with_trigger",
        Some(big_confirmation_trigger),
    )?;
    monitor_tx(&monitor, tx_id_3, "tx_context_3_no_trigger", None)?;
    monitor_tx(&monitor, tx_id_4, "tx_context_4_no_trigger", None)?;

    // Monitor 4 SpendingUTXO: 2 with trigger, 2 without
    monitor_spending_utxo(
        &monitor,
        funding_txid_1,
        funding_vout_1,
        "spending_utxo_context_1_with_trigger",
        Some(big_confirmation_trigger),
    )?;
    monitor_spending_utxo(
        &monitor,
        funding_txid_2,
        funding_vout_2,
        "spending_utxo_context_2_with_trigger",
        Some(big_confirmation_trigger),
    )?;
    monitor_spending_utxo(
        &monitor,
        funding_txid_3,
        funding_vout_3,
        "spending_utxo_context_3_no_trigger",
        None,
    )?;
    monitor_spending_utxo(
        &monitor,
        funding_txid_4,
        funding_vout_4,
        "spending_utxo_context_4_no_trigger",
        None,
    )?;

    // Monitor RskPegin: single monitor that detects all RSK pegin transactions (with trigger)
    monitor_rsk_pegin(&monitor, Some(small_confirmation_trigger))?;

    // Monitor NewBlock: single monitor that detects all new blocks
    monitor.monitor(TypesToMonitor::NewBlock, false)?;

    // ============================================================================
    // PART 3: Mine blocks and sync to confirm transactions
    // ============================================================================

    // Create spending transactions for the SpendingUTXO monitors
    let _ = create_and_send_spending_transaction(&bitcoin_client, funding_txid_1, funding_vout_1)?;
    let _ = create_and_send_spending_transaction(&bitcoin_client, funding_txid_2, funding_vout_2)?;
    let (_, spender_txid_3) =
        create_and_send_spending_transaction(&bitcoin_client, funding_txid_3, funding_vout_3)?;
    let (_, spender_txid_4) =
        create_and_send_spending_transaction(&bitcoin_client, funding_txid_4, funding_vout_4)?;

    // Mine another block to confirm spending transactions
    mine_blocks(&bitcoin_client, 1)?;
    sync_monitor(&monitor)?;

    // ============================================================================
    // PART 4: Review news - First check (after 1 confirmation)
    // ============================================================================

    let news = monitor.get_news()?;

    // Count news items by type
    let mut tx_news_count = Vec::new();
    let mut spending_utxo_news_count = Vec::new();
    let mut rsk_pegin_news_count = Vec::new();
    let mut new_block_news_count = 0;

    for news_item in &news {
        match news_item {
            MonitorNews::Transaction(_, _, _) => {
                tx_news_count.push(news_item);
            }
            MonitorNews::SpendingUTXOTransaction(_, _, _, _) => {
                spending_utxo_news_count.push(news_item);
            }
            MonitorNews::RskPeginTransaction(_, _) => {
                rsk_pegin_news_count.push(news_item);
            }
            MonitorNews::NewBlock(_, _) => {
                new_block_news_count += 1;
            }
        }
    }

    // After 1 confirmation, we expect:
    // - 2 Transaction news (only the 2 without trigger appear immediately)
    // - 2 SpendingUTXO news (only the 2 without trigger appear immediately)
    // - 4 RskPegin news (all 4 RSK pegin transactions are detected by the single monitor)
    // - 1 NewBlock news (1 block was mined after the initial setup)
    assert_eq!(
        tx_news_count.len(),
        2,
        "There should be 2 Transaction news items (only those without trigger)"
    );
    assert_eq!(
        spending_utxo_news_count.len(),
        2,
        "There should be 2 SpendingUTXO news items (only those without trigger)"
    );
    assert_eq!(
        rsk_pegin_news_count.len(),
        4,
        "There should be 4 RskPegin news items (all RSK pegin transactions)"
    );
    assert_eq!(
        new_block_news_count, 1,
        "There should be 1 NewBlock news item (1 block mined after setup)"
    );

    // Expected news items: transactions without trigger should appear after 1 confirmation
    let txs_news_should_be = vec![
        (tx_id_3, "tx_context_3_no_trigger"),
        (tx_id_4, "tx_context_4_no_trigger"),
    ];
    let spending_utxo_news_should_be = vec![
        (spender_txid_3, "spending_utxo_context_3_no_trigger"),
        (spender_txid_4, "spending_utxo_context_4_no_trigger"),
    ];
    // All RSK pegin transactions should be detected by the single RskPegin monitor
    let rsk_pegin_news_should_be = vec![
        (rsk_pegin_txid_1, "rsk_pegin_context_1_with_trigger"),
        (rsk_pegin_txid_2, "rsk_pegin_context_2_with_trigger"),
        (rsk_pegin_txid_3, "rsk_pegin_context_3_with_trigger"),
        (rsk_pegin_txid_4, "rsk_pegin_context_4_with_trigger"),
    ];

    // Verify that all expected news items are present by matching txids and contexts.
    // For each news item found, remove the matching entry from the "should be" arrays.
    // At the end, all "should be" arrays should be empty.

    info!("txs_news_should_be: {:?}", txs_news_should_be);

    let mut txs_news_should_be = txs_news_should_be.clone();
    for news in &tx_news_count {
        if let MonitorNews::Transaction(_, transaction_status, context) = news {
            if let Some(pos) = txs_news_should_be.iter().position(|x| {
                x.0 == transaction_status.tx.as_ref().unwrap().compute_txid() && x.1 == context
            }) {
                txs_news_should_be.remove(pos);
            }
        }
    }

    assert!(
        txs_news_should_be.is_empty(),
        "Not all expected Transaction news items were received, remaining: {:?}",
        txs_news_should_be
    );

    let mut spending_utxo_news_should_be = spending_utxo_news_should_be.clone();
    for news in &spending_utxo_news_count {
        if let MonitorNews::SpendingUTXOTransaction(_, _, transaction_status, context) = news {
            if let Some(pos) = spending_utxo_news_should_be.iter().position(|x| {
                x.0 == transaction_status.tx.as_ref().unwrap().compute_txid() && x.1 == context
            }) {
                spending_utxo_news_should_be.remove(pos);
            }
        }
    }

    assert!(
        spending_utxo_news_should_be.is_empty(),
        "Not all expected SpendingUTXO news items were received, remaining: {:?}",
        spending_utxo_news_should_be
    );

    let mut rsk_pegin_news_should_be = rsk_pegin_news_should_be.clone();

    for news in &rsk_pegin_news_count {
        if let MonitorNews::RskPeginTransaction(txid, _) = news {
            if let Some(pos) = rsk_pegin_news_should_be.iter().position(|x| x.0 == *txid) {
                rsk_pegin_news_should_be.remove(pos);
            }
        }
    }
    assert!(
        rsk_pegin_news_should_be.is_empty(),
        "Not all expected RskPegin news items were received, remaining: {:?}",
        rsk_pegin_news_should_be
    );

    // Verify that the NewBlock news matches the current best block height
    if let MonitorNews::NewBlock(block_height, _) = &news[0] {
        let block_height_should_be = bitcoin_client.get_best_block()?;
        assert_eq!(
            block_height_should_be, *block_height,
            "Expected block height {:?} should be {:?}",
            block_height, block_height_should_be
        );
    }

    bitcoind.stop()?;
    clear_output();

    Ok(())
}

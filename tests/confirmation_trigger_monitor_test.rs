use anyhow::Result;
use bitvmx_transaction_monitor::monitor::MonitorApi;

use crate::utils::{
    ack_rsk_pegin_monitor, ack_spending_utxo_monitor, ack_tx_monitor, assert_rsk_pegin_news,
    assert_spending_utxo_news, assert_tx_news, clear_output, create_and_send_a_new_transaction,
    create_and_send_funding_transaction, create_and_send_rsk_pegin_transaction,
    create_and_send_spending_transaction, create_test_setup, mine_blocks, monitor_rsk_pegin,
    monitor_spending_utxo, monitor_tx, sync_monitor,
};

mod utils;

/// Test that verifies the transaction monitor sends news only once when using confirmation_trigger.
///
/// This test ensures that:
/// 1. When monitoring with confirmation_trigger Some(3), news is sent only when confirmations reach 3
/// 2. After sending news at confirmation 3, no more news is sent even when more blocks are mined
/// 3. The monitor remains active (does not deactivate) after the trigger is sent
///
/// Test flow:
/// - Creates and monitors a transaction with confirmation_trigger Some(3)
/// - Mines blocks to reach 3 confirmations
/// - Verifies news is sent at confirmation 3
/// - Mines additional blocks (4, 5, 6 confirmations)
/// - Verifies no additional news is sent
/// - Verifies monitor remains active
#[test]
fn test_transaction_monitor_confirmation_trigger() -> Result<(), anyhow::Error> {
    let confirmation_trigger = 3;
    let max_monitoring_confirmations = 10; // Higher than trigger to ensure monitor doesn't deactivate
    let (bitcoin_client, monitor, bitcoind) = create_test_setup(max_monitoring_confirmations)?;

    // Step 1: Create and send a transaction
    let (_transaction, tx_id) = create_and_send_a_new_transaction(&bitcoin_client)?;

    let extra_data = "context of the transaction".to_string();

    // Step 2: Start monitoring with confirmation_trigger Some(3)
    // News will only be sent when confirmations reach 3
    monitor_tx(&monitor, tx_id, &extra_data, Some(confirmation_trigger))?;

    // Step 3: Sync the monitor to ensure it's up to date
    // Note: create_and_send_a_new_transaction mines 1 block, so the transaction starts with 1 confirmation
    sync_monitor(&monitor)?;

    // Step 4: Verify no news before reaching confirmation_trigger
    // At confirmation 1, no news should be sent (trigger is 3)
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        0,
        "Expected no news at confirmation 1 - trigger is {}",
        confirmation_trigger
    );

    // Step 5: Mine 1 block to reach confirmation 2
    // Still no news should be sent (trigger is 3)
    mine_blocks(&bitcoin_client, 1)?;
    monitor.tick()?;
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        0,
        "Expected no news at confirmation 2 - trigger is {}",
        confirmation_trigger
    );

    // Step 6: Mine 1 block to reach confirmation_trigger (3 confirmations)
    mine_blocks(&bitcoin_client, 1)?;
    monitor.tick()?;

    // Step 7: Verify news is sent at confirmation 3
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        1,
        "Expected exactly 1 news item at confirmation {}",
        confirmation_trigger
    );
    assert_tx_news(&news[0], tx_id, &extra_data, confirmation_trigger)?;

    // Step 8: Acknowledge the news
    ack_tx_monitor(&monitor, tx_id, &extra_data)?;

    // Step 9: Mine additional blocks (4, 5, 6 confirmations)
    // After the trigger is sent, no more news should be generated
    for i in (confirmation_trigger + 1)..=(confirmation_trigger + 3) {
        mine_blocks(&bitcoin_client, 1)?;
        monitor.tick()?;

        // Verify no news is sent for confirmations after the trigger
        let news = monitor.get_news()?;
        assert_eq!(
            news.len(),
            0,
            "Expected no news at confirmation {} - trigger was already sent at confirmation {}",
            i,
            confirmation_trigger
        );
    }

    bitcoind.stop()?;
    clear_output();

    Ok(())
}

/// Test that verifies the SpendingUTXOTransaction monitor sends news only once when using confirmation_trigger.
///
/// This test ensures that:
/// 1. When monitoring with confirmation_trigger Some(3), news is sent only when confirmations reach 3
/// 2. After sending news at confirmation 3, no more news is sent even when more blocks are mined
/// 3. The monitor remains active (does not deactivate) after the trigger is sent
///
/// Test flow:
/// - Creates a funding transaction to produce a UTXO to monitor
/// - Starts monitoring the UTXO with confirmation_trigger Some(3)
/// - Creates and broadcasts a spending transaction
/// - Mines blocks to reach 3 confirmations
/// - Verifies news is sent at confirmation 3
/// - Mines additional blocks and verifies no additional news is sent
#[test]
fn test_spending_utxo_monitor_confirmation_trigger() -> Result<(), anyhow::Error> {
    let confirmation_trigger = 3;
    let max_monitoring_confirmations = 10; // Higher than trigger to ensure monitor doesn't deactivate
    let (bitcoin_client, monitor, bitcoind) = create_test_setup(max_monitoring_confirmations)?;

    // Step 1: Create a funding transaction to get a UTXO that we can monitor
    let (_, funding_txid, funding_vout) = create_and_send_funding_transaction(&bitcoin_client)?;

    let extra_data = "context of the spending utxo monitor".to_string();

    // Step 2: Start monitoring the UTXO with confirmation_trigger Some(3)
    // News will only be sent when the spending transaction reaches 3 confirmations
    monitor_spending_utxo(
        &monitor,
        funding_txid,
        funding_vout,
        &extra_data,
        Some(confirmation_trigger),
    )?;

    // Step 3: Mine a block to confirm the funding transaction
    mine_blocks(&bitcoin_client, 1)?;
    sync_monitor(&monitor)?;

    // Step 4: Verify no news yet - the UTXO exists but hasn't been spent
    let news = monitor.get_news()?;
    assert_eq!(news.len(), 0, "Expected no news before UTXO is spent");

    // Step 5: Create and send a transaction that spends the monitored UTXO
    let (_spending_transaction, spender_txid) =
        create_and_send_spending_transaction(&bitcoin_client, funding_txid, funding_vout)?;

    // Step 6: Mine a block to confirm the spending transaction (gives it 1 confirmation)
    mine_blocks(&bitcoin_client, 1)?;
    sync_monitor(&monitor)?;

    // Step 7: Verify no news before reaching confirmation_trigger
    // At confirmation 1, no news should be sent (trigger is 3)
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        0,
        "Expected no news at confirmation 1 - trigger is {}",
        confirmation_trigger
    );

    // Step 8: Mine 1 block to reach confirmation 2
    // Still no news should be sent (trigger is 3)
    mine_blocks(&bitcoin_client, 1)?;
    monitor.tick()?;
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        0,
        "Expected no news at confirmation 2 - trigger is {}",
        confirmation_trigger
    );

    // Step 9: Mine 1 block to reach confirmation_trigger (3 confirmations)
    mine_blocks(&bitcoin_client, 1)?;
    monitor.tick()?;

    // Step 10: Verify news is sent at confirmation 3
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        1,
        "Expected exactly 1 news item at confirmation {}",
        confirmation_trigger
    );
    assert_spending_utxo_news(
        &news[0],
        funding_txid,
        funding_vout,
        spender_txid,
        &extra_data,
        spender_txid,
        confirmation_trigger,
    )?;

    // Step 11: Acknowledge the news
    ack_spending_utxo_monitor(&monitor, funding_txid, funding_vout, &extra_data)?;

    // Step 12: Mine additional blocks (4, 5, 6 confirmations)
    // After the trigger is sent, no more news should be generated
    for i in (confirmation_trigger + 1)..=(confirmation_trigger + 3) {
        mine_blocks(&bitcoin_client, 1)?;
        monitor.tick()?;

        // Verify no news is sent for confirmations after the trigger
        let news = monitor.get_news()?;
        assert_eq!(
            news.len(),
            0,
            "Expected no news at confirmation {} - trigger was already sent at confirmation {}",
            i,
            confirmation_trigger
        );
    }

    bitcoind.stop()?;
    clear_output();

    Ok(())
}

/// Test that verifies the RskPeginTransaction monitor sends news only once when using confirmation_trigger.
///
/// This test ensures that:
/// 1. When monitoring with confirmation_trigger Some(3), news is sent only when confirmations reach 3
/// 2. After sending news at confirmation 3, no more news is sent even when more blocks are mined
/// 3. The monitor remains active (does not deactivate) after the trigger is sent
///
/// Test flow:
/// - Starts monitoring for RSK pegin transactions with confirmation_trigger Some(3)
/// - Creates and broadcasts an RSK pegin transaction
/// - Mines blocks to reach 3 confirmations
/// - Verifies news is sent at confirmation 3
/// - Mines additional blocks and verifies no additional news is sent
#[test]
fn test_rsk_pegin_monitor_confirmation_trigger() -> Result<(), anyhow::Error> {
    let confirmation_trigger = 3;
    let max_monitoring_confirmations = 10; // Higher than trigger to ensure monitor doesn't deactivate
    let (bitcoin_client, monitor, bitcoind) = create_test_setup(max_monitoring_confirmations)?;

    // Step 1: Start monitoring for RSK pegin transactions with confirmation_trigger Some(3)
    // News will only be sent when an RSK pegin transaction reaches 3 confirmations
    monitor_rsk_pegin(&monitor, Some(confirmation_trigger))?;

    // Step 2: Create and send an RSK pegin transaction
    // Note: create_and_send_rsk_pegin_transaction mines 1 block, so the transaction starts with 1 confirmation
    let (_pegin_transaction, pegin_txid) = create_and_send_rsk_pegin_transaction(&bitcoin_client)?;

    // Step 3: Sync the monitor to detect the transaction
    // At this point, the transaction has 1 confirmation (from the block mined in create_and_send_rsk_pegin_transaction)
    sync_monitor(&monitor)?;

    // Step 4: Verify no news before reaching confirmation_trigger
    // At confirmation 1, no news should be sent (trigger is 3)
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        0,
        "Expected no news at confirmation 1 - trigger is {}",
        confirmation_trigger
    );

    // Step 5: Mine 1 block to reach confirmation 2
    // Still no news should be sent (trigger is 3)
    mine_blocks(&bitcoin_client, 1)?;
    monitor.tick()?;
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        0,
        "Expected no news at confirmation 2 - trigger is {}",
        confirmation_trigger
    );

    // Step 6: Mine 1 block to reach confirmation_trigger (3 confirmations)
    mine_blocks(&bitcoin_client, 1)?;
    monitor.tick()?;

    // Step 7: Verify news is sent at confirmation 3
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        1,
        "Expected exactly 1 news item at confirmation {}",
        confirmation_trigger
    );
    assert_rsk_pegin_news(&news[0], pegin_txid, confirmation_trigger)?;

    // Step 8: Acknowledge the news
    ack_rsk_pegin_monitor(&monitor, pegin_txid)?;

    // Step 9: Mine additional blocks (4, 5, 6 confirmations)
    // After the trigger is sent, no more news should be generated
    for i in (confirmation_trigger + 1)..=(confirmation_trigger + 3) {
        mine_blocks(&bitcoin_client, 1)?;
        monitor.tick()?;

        // Verify no news is sent for confirmations after the trigger
        let news = monitor.get_news()?;
        assert_eq!(
            news.len(),
            0,
            "Expected no news at confirmation {} - trigger was already sent at confirmation {}",
            i,
            confirmation_trigger
        );
    }

    bitcoind.stop()?;
    clear_output();

    Ok(())
}

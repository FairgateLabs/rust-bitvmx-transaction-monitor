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

/// Test that verifies the monitor automatically deactivates after reaching the maximum
/// number of confirmations (max_monitoring_confirmations).
///
/// This test ensures that:
/// 1. The monitor generates news for each confirmation up to max_monitoring_confirmations
/// 2. After reaching max_monitoring_confirmations, the monitor automatically deactivates
/// 3. No further news is generated even when additional blocks are mined
///
/// Note: The test uses 10 confirmations (instead of the default 100) for faster test execution.
#[test]
fn test_transaction_monitor_auto_deactivates_at_max_confirmations() -> Result<(), anyhow::Error> {
    let max_monitoring_confirmations = 10;
    let max_confirmations = max_monitoring_confirmations - 1;
    let (bitcoin_client, monitor, bitcoind) = create_test_setup(max_monitoring_confirmations)?;
    let (_transaction1, tx_id) = create_and_send_a_new_transaction(&bitcoin_client)?;

    let extra_data = "context of the transaction".to_string();
    monitor_tx(&monitor, tx_id, &extra_data, None)?;

    // Sync the monitor to ensure it's up to date with the blockchain state
    sync_monitor(&monitor)?;

    // Iterate through each confirmation from 1 to max_monitoring_confirmations.
    // For each confirmation:
    // - The monitor should generate news about the transaction's confirmation status
    // - We acknowledge the news to clear it
    // - We mine a new block to advance the chain
    // - We tick the monitor to process the new block
    for i in 1..=max_confirmations {
        let news = monitor.get_news()?;
        assert_eq!(news.len(), 1, "Expected 1 news item for confirmation {}", i);
        assert_tx_news(&news[0], tx_id, &extra_data, i)?;
        ack_tx_monitor(&monitor, tx_id, &extra_data)?;
        mine_blocks(&bitcoin_client, 1)?;
        monitor.tick()?;
    }

    // After reaching max_monitoring_confirmations, mine one more block and tick the monitor.
    // At this point, the monitor should have automatically deactivated the transaction monitor
    // (this happens during the last tick when confirmations == max_monitoring_confirmations),
    // so no news should be generated even though a new block was mined.
    mine_blocks(&bitcoin_client, 1)?;
    monitor.tick()?;

    // Verify that no news is generated after deactivation
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        0,
        "Expected no news after monitor deactivation at {} confirmations",
        max_monitoring_confirmations
    );

    bitcoind.stop()?;
    clear_output();

    Ok(())
}

/// Test that verifies the SpendingUTXOTransaction monitor automatically deactivates after
/// reaching the maximum number of confirmations (max_monitoring_confirmations).
///
/// This test ensures that:
/// 1. A UTXO can be monitored using SpendingUTXOTransaction
/// 2. When the UTXO is spent, the monitor generates news for each confirmation up to max_monitoring_confirmations
/// 3. After reaching max_monitoring_confirmations, the monitor automatically deactivates
/// 4. No further news is generated even when additional blocks are mined
///
/// Test flow:
/// - Creates a funding transaction to produce a UTXO to monitor
/// - Starts monitoring the UTXO before it's spent
/// - Creates and broadcasts a spending transaction
/// - Verifies news is generated for each confirmation level
/// - Verifies automatic deactivation occurs at max_monitoring_confirmations
///
/// Note: The test uses 10 confirmations (instead of the default 100) for faster test execution.
#[test]
fn test_spending_utxo_monitor_auto_deactivates_at_max_confirmations() -> Result<(), anyhow::Error> {
    let max_monitoring_confirmations = 10;

    let max_confirmations = max_monitoring_confirmations - 1;
    let (bitcoin_client, monitor, bitcoind) = create_test_setup(max_monitoring_confirmations)?;

    // Step 1: Create a funding transaction to get a UTXO that we can monitor
    let (_, funding_txid, funding_vout) = create_and_send_funding_transaction(&bitcoin_client)?;

    let extra_data = "context of the spending utxo monitor".to_string();

    // Step 2: Start monitoring the UTXO before it's spent
    // The monitor will watch for any transaction that spends this specific UTXO
    monitor_spending_utxo(&monitor, funding_txid, funding_vout, &extra_data, None)?;

    // Step 3: Mine a block to confirm the funding transaction
    // This makes the UTXO available to be spent
    mine_blocks(&bitcoin_client, 1)?;
    sync_monitor(&monitor)?;

    // Step 4: Verify no news yet - the UTXO exists but hasn't been spent
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        0,
        "Expected no news before UTXO is spent - monitoring is active but no spending transaction exists yet"
    );

    // Step 5: Create and send a transaction that spends the monitored UTXO
    // This is the transaction we're waiting for - it consumes the UTXO we're monitoring
    let (_spending_transaction, spender_txid) =
        create_and_send_spending_transaction(&bitcoin_client, funding_txid, funding_vout)?;

    // Step 6: Mine a block to confirm the spending transaction
    // This gives the spending transaction its first confirmation
    mine_blocks(&bitcoin_client, 1)?;
    sync_monitor(&monitor)?;

    // Step 7: Iterate through each confirmation from 1 to max_confirmations (9)
    // For each confirmation level:
    // - The monitor should generate news about the spending transaction's confirmation status
    // - We verify the news contains the correct information (target UTXO, spender txid, confirmations)
    // - We acknowledge the news to clear it from the queue
    // - We mine a new block to advance the chain and increase confirmations
    // - We tick the monitor to process the new block and update confirmation counts
    for i in 1..=max_confirmations {
        let news = monitor.get_news()?;
        assert_eq!(
            news.len(),
            1,
            "Expected exactly 1 news item for confirmation level {}",
            i
        );

        // Verify the news contains correct information about the spending transaction
        assert_spending_utxo_news(
            &news[0],
            funding_txid, // The UTXO we're monitoring (target transaction)
            funding_vout, // The output index of the UTXO
            spender_txid, // The transaction that spent the UTXO
            &extra_data,  // The context/extra data we provided
            spender_txid, // The spending transaction ID (same as spender_txid)
            i,            // Current confirmation count
        )?;

        // Acknowledge the news to remove it from the queue
        ack_spending_utxo_monitor(&monitor, funding_txid, funding_vout, &extra_data)?;

        // Mine a new block to increase the confirmation count
        mine_blocks(&bitcoin_client, 1)?;

        // Process the new block - this will update confirmation counts and check for deactivation
        monitor.tick()?;
    }

    // Step 8: After processing max_confirmations (9), the next tick will reach max_monitoring_confirmations (10)
    // At this point, the monitor should automatically deactivate during the tick() call
    // We mine one more block and tick to verify deactivation occurred
    mine_blocks(&bitcoin_client, 1)?;
    monitor.tick()?;

    // Step 9: Verify that no news is generated after deactivation
    // Even though we mined another block, the monitor should be deactivated and not generate news
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        0,
        "Expected no news after monitor deactivation at {} confirmations - monitor should have auto-deactivated",
        max_monitoring_confirmations
    );

    bitcoind.stop()?;
    clear_output();

    Ok(())
}

/// Test that verifies the RskPeginTransaction monitor automatically deactivates after
/// reaching the maximum number of confirmations (max_monitoring_confirmations).
///
/// This test ensures that:
/// 1. RSK pegin transactions can be monitored
/// 2. When an RSK pegin transaction is detected, the monitor generates news for each confirmation up to max_monitoring_confirmations
/// 3. After reaching max_monitoring_confirmations, the monitor automatically deactivates
/// 4. No further news is generated even when additional blocks are mined
///
/// Test flow:
/// - Starts monitoring for RSK pegin transactions
/// - Creates and broadcasts an RSK pegin transaction
/// - Verifies news is generated for each confirmation level
/// - Verifies automatic deactivation occurs at max_monitoring_confirmations
///
/// Note: The test uses 10 confirmations (instead of the default 100) for faster test execution.
#[test]
fn test_rsk_pegin_monitor_auto_deactivates_at_max_confirmations() -> Result<(), anyhow::Error> {
    let max_monitoring_confirmations = 10;

    let max_confirmations = max_monitoring_confirmations - 1;
    let (bitcoin_client, monitor, bitcoind) = create_test_setup(max_monitoring_confirmations)?;

    // Step 1: Start monitoring for RSK pegin transactions
    // The monitor will automatically detect any RSK pegin transactions in new blocks
    monitor_rsk_pegin(&monitor, None)?;

    // Step 2: Create and send an RSK pegin transaction
    // This transaction will be automatically detected by the monitor when it's included in a block
    let (_pegin_transaction, pegin_txid) = create_and_send_rsk_pegin_transaction(&bitcoin_client)?;

    // Step 3: Mine a block to confirm the RSK pegin transaction
    // This gives the transaction its first confirmation and triggers detection
    sync_monitor(&monitor)?;

    // Step 4: Iterate through each confirmation from 1 to max_confirmations (9)
    // For each confirmation level:
    // - The monitor should generate news about the RSK pegin transaction's confirmation status
    // - We verify the news contains the correct information (txid, confirmations)
    // - We acknowledge the news to clear it from the queue
    // - We mine a new block to advance the chain and increase confirmations
    // - We tick the monitor to process the new block and update confirmation counts
    for i in 1..=max_confirmations {
        let news = monitor.get_news()?;
        assert_eq!(
            news.len(),
            1,
            "Expected exactly 1 news item for confirmation level {}",
            i
        );

        // Verify the news contains correct information about the RSK pegin transaction
        assert_rsk_pegin_news(
            &news[0], pegin_txid, // The RSK pegin transaction ID
            i,          // Current confirmation count
        )?;

        // Acknowledge the news to remove it from the queue
        ack_rsk_pegin_monitor(&monitor, pegin_txid)?;

        // Mine a new block to increase the confirmation count
        mine_blocks(&bitcoin_client, 1)?;

        // Process the new block - this will update confirmation counts and check for deactivation
        monitor.tick()?;
    }

    // Step 5: After processing max_confirmations (9), the next tick will reach max_monitoring_confirmations (10)
    // At this point, the monitor should automatically deactivate during the tick() call
    // We mine one more block and tick to verify deactivation occurred
    mine_blocks(&bitcoin_client, 1)?;
    monitor.tick()?;

    // Step 6: Verify that no news is generated after deactivation
    // Even though we mined another block, the monitor should be deactivated and not generate news
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        0,
        "Expected no news after monitor deactivation at {} confirmations - monitor should have auto-deactivated",
        max_monitoring_confirmations
    );

    bitcoind.stop()?;
    clear_output();

    Ok(())
}

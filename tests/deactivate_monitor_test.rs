use anyhow::Result;
use bitvmx_transaction_monitor::{monitor::MonitorApi, types::MonitorNews};
use tracing::info;

use crate::utils::{
    ack_spending_utxo_monitor, ack_tx_monitor, assert_spending_utxo_news, assert_tx_news,
    clear_output, create_and_send_a_new_transaction, create_and_send_funding_transaction,
    create_and_send_spending_transaction, create_test_setup, mine_blocks, monitor_spending_utxo,
    monitor_tx, sync_monitor,
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
    monitor_tx(&monitor, tx_id, &extra_data)?;

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
/// Note: The test uses 10 confirmations (instead of the default 100) for faster test execution.
#[test]
fn test_spending_utxo_monitor_auto_deactivates_at_max_confirmations() -> Result<(), anyhow::Error> {
    let max_monitoring_confirmations = 10;
    let max_confirmations = max_monitoring_confirmations - 1;
    let (bitcoin_client, monitor, bitcoind) = create_test_setup(max_monitoring_confirmations)?;

    let (_, funding_txid, funding_vout) = create_and_send_funding_transaction(&bitcoin_client)?;

    let extra_data = "context of the spending utxo monitor".to_string();
    // Start monitoring the UTXO
    monitor_spending_utxo(&monitor, funding_txid, funding_vout, &extra_data)?;

    // Mine a block to confirm the funding transaction
    mine_blocks(&bitcoin_client, 1)?;
    sync_monitor(&monitor)?;

    // Verify no news yet (UTXO is not spent)
    let news = monitor.get_news()?;
    assert_eq!(news.len(), 0, "Expected no news before UTXO is spent");

    // Create and send a transaction that spends the monitored UTXO
    let (_spending_transaction, spender_txid) =
        create_and_send_spending_transaction(&bitcoin_client, funding_txid, funding_vout)?;

    // Mine a block to confirm the spending transaction
    mine_blocks(&bitcoin_client, 1)?;
    sync_monitor(&monitor)?;

    // Iterate through each confirmation from 1 to max_monitoring_confirmations.
    // For each confirmation:
    // - The monitor should generate news about the spending transaction's confirmation status
    // - We acknowledge the news to clear it
    // - We mine a new block to advance the chain
    // - We tick the monitor to process the new block
    for i in 1..=max_confirmations {
        let news = monitor.get_news()?;
        assert_eq!(news.len(), 1, "Expected 1 news item for confirmation {}", i);

        assert_spending_utxo_news(
            &news[0],
            funding_txid,
            funding_vout,
            spender_txid,
            &extra_data,
            spender_txid,
            i,
        )?;
        ack_spending_utxo_monitor(&monitor, funding_txid, funding_vout, &extra_data)?;
        mine_blocks(&bitcoin_client, 1)?;
        monitor.tick()?;
    }

    // After reaching max_monitoring_confirmations, mine one more block and tick the monitor.
    // At this point, the monitor should have automatically deactivated the SpendingUTXOTransaction monitor
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

use bitvmx_transaction_monitor::monitor::MonitorApi;
use utils::clear_output;

use crate::utils::{
    ack_tx_monitor, assert_tx_news, create_and_send_a_new_transaction, create_test_setup,
    monitor_tx, sync_monitor,
};

mod utils;

/// Test that verifies the monitor can detect and track multiple transactions simultaneously.
///
/// This test ensures that:
/// 1. Multiple transactions can be monitored at the same time
/// 2. Each transaction reports its correct confirmation count based on when it was mined
/// 3. News is generated for all monitored transactions
/// 4. After acknowledging news, no duplicate news is generated
///
/// The test creates two transactions at different times:
/// - First transaction (tx_id): Created earlier, so it has accumulated more confirmations (4)
/// - Second transaction (tx_id_2): Created later, so it has fewer confirmations (1)
#[test]
fn monitor_txs_detected() -> Result<(), anyhow::Error> {
    let (bitcoin_client, monitor, bitcoind) = create_test_setup(10)?;

    let (_transaction1, tx_id) = create_and_send_a_new_transaction(&bitcoin_client)?;
    let (_transaction2, tx_id_2) = create_and_send_a_new_transaction(&bitcoin_client)?;

    let extra_data_1 = "test".to_string();
    let extra_data_2 = "test 2".to_string();

    // Start monitoring both transactions
    monitor_tx(&monitor, tx_id, &extra_data_1, None)?;
    monitor_tx(&monitor, tx_id_2, &extra_data_2, None)?;

    sync_monitor(&monitor)?;

    // Verify that news was generated for both transactions
    let news = monitor.get_news()?;
    assert_eq!(
        news.len(),
        2,
        "Expected 2 news items, one for each monitored transaction"
    );

    // The first transaction has 4 confirmations because:
    // - It was mined in an earlier block
    // - Additional blocks were mined when creating the second transaction
    // - The sync processed all these blocks, updating the confirmation count
    assert_tx_news(&news[0], tx_id, &extra_data_1, 4)?;

    // The second transaction has 1 confirmation because:
    // - It was just mined in the most recent block
    // - No additional blocks have been mined since then
    assert_tx_news(&news[1], tx_id_2, &extra_data_2, 1)?;

    // Acknowledge the news for both transactions
    ack_tx_monitor(&monitor, tx_id, &extra_data_1)?;
    ack_tx_monitor(&monitor, tx_id_2, &extra_data_2)?;

    // Verify that no new news is generated after acknowledgment
    // (news is only generated once per confirmation level until acknowledged)
    let news = monitor.get_news()?;
    assert_eq!(news.len(), 0, "Expected no news after acknowledgment");

    bitcoind.stop()?;
    clear_output();

    Ok(())
}

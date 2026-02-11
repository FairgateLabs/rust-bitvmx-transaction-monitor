use bitcoin::{
    absolute::LockTime,
    hex::FromHex,
    key::{rand::thread_rng, Secp256k1},
    opcodes::all::OP_RETURN,
    script::Builder,
    secp256k1::PublicKey,
    Address, Amount, BlockHash, Network, Transaction, TxOut,
};
use bitcoin_indexer::types::{FullBlock, TransactionBlockchainStatus, TransactionStatus};
use bitvmx_bitcoin_rpc::bitcoin_client::BitcoinClientApi;
use bitvmx_transaction_monitor::{
    config::{MonitorSettings, MonitorSettingsConfig},
    monitor::{Monitor, MonitorApi},
    store::{MonitorStore, MonitorStoreApi, TypesToMonitorStore},
    types::{AckMonitorNews, MonitorNews, TypesToMonitor},
};
use std::{rc::Rc, str::FromStr};
use storage_backend::{storage::Storage, storage_config::StorageConfig};
use tracing::info;
use utils::{clear_output, generate_random_string};
use uuid::timestamp::context;

use crate::utils::{
    ack_tx_monitor, assert_tx_news, create_and_send_a_new_transaction, create_test_setup,
    mine_blocks, monitor_tx, sync_monitor,
};
mod utils;

fn tx_info_confirmed(
    tx: &Transaction,
    block_info: &FullBlock,
    confirmations: u32,
) -> TransactionStatus {
    TransactionStatus {
        tx: Some(tx.clone()),
        block_info: Some(block_info.clone()),
        confirmations,
        status: TransactionBlockchainStatus::Confirmed,
        confirmation_threshold: 6,
    }
}

fn tx_info_orphan(tx: &Transaction, prev_block_info: &FullBlock) -> TransactionStatus {
    // `TransactionStatus::is_orphan()` requires:
    // - confirmations == 0
    // - block_info.orphan == true
    // - status == Orphan
    let mut orphan_block = prev_block_info.clone();
    orphan_block.orphan = true;

    TransactionStatus {
        tx: Some(tx.clone()),
        block_info: Some(orphan_block),
        confirmations: 0,
        status: TransactionBlockchainStatus::Orphan,
        confirmation_threshold: 6,
    }
}

fn create_pegin_tx() -> Transaction {
    let secp = Secp256k1::new();
    let sk = bitcoin::secp256k1::SecretKey::new(&mut thread_rng());
    let pubk = PublicKey::from_secret_key(&secp, &sk);
    let committee_n = Address::p2tr(&secp, pubk.x_only_public_key().0, None, Network::Bitcoin);

    let sk_reimburse = bitcoin::secp256k1::SecretKey::new(&mut thread_rng());
    let pk_reimburse = PublicKey::from_secret_key(&secp, &sk_reimburse);
    let reimbursement_xpk = pk_reimburse.x_only_public_key().0;

    let taproot_output = TxOut {
        value: Amount::from_sat(100_000_000),
        script_pubkey: committee_n.script_pubkey(),
    };

    let packet_number: u64 = 0;
    let mut rootstock_address = [0u8; 20];
    rootstock_address.copy_from_slice(
        Vec::from_hex("7ac5496aee77c1ba1f0854206a26dda82a81d6d8")
            .unwrap()
            .as_slice(),
    );

    let mut data = [0u8; 69];
    data.copy_from_slice(
        [
            b"RSK_PEGIN".as_slice(),
            &packet_number.to_be_bytes(),
            &rootstock_address,
            &reimbursement_xpk.serialize(),
        ]
        .concat()
        .as_slice(),
    );

    let op_return_output = TxOut {
        value: Amount::ZERO,
        script_pubkey: Builder::new()
            .push_opcode(OP_RETURN)
            .push_slice(&data)
            .into_script(),
    };

    Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: vec![taproot_output, op_return_output],
    }
}

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
    monitor_tx(&monitor, tx_id, &extra_data_1)?;
    monitor_tx(&monitor, tx_id_2, &extra_data_2)?;

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
fn test_monitor_deactivation_after_100_confirmations() -> Result<(), anyhow::Error> {
    let max_monitoring_confirmations = 10;
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
    for i in 1..=max_monitoring_confirmations {
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

// #[test]
// fn test_rsk_pegin_monitor_not_deactivated() -> Result<(), anyhow::Error> {
//     let mut mock_indexer = MockIndexerApi::new();
//     let path = format!("test_outputs/{}", generate_random_string());
//     let config = StorageConfig::new(path, None);
//     let storage = Rc::new(Storage::new(&config)?);
//     let store = MonitorStore::new(storage)?;

//     let full_block = FullBlock {
//         height: 200,
//         hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000000",
//         )
//         .unwrap(),
//         prev_hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000001",
//         )
//         .unwrap(),
//         txs: vec![],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     let full_block_clone = full_block.clone();

//     mock_indexer
//         .expect_get_best_block()
//         .returning(move || Ok(Some(full_block.clone())));

//     let full_block_clone = full_block_clone.clone();

//     mock_indexer
//         .expect_get_block_by_height()
//         .returning(move |_| Ok(Some(full_block_clone.clone())));

//     mock_indexer.expect_tick().returning(move || Ok(()));

//     let monitor = Monitor::new(
//         mock_indexer,
//         store,
//         MonitorSettings::from(MonitorSettingsConfig::default()),
//     )?;
//     monitor.save_monitor(TypesToMonitor::RskPegin(None))?;
//     monitor.tick()?;

//     // Verify monitor is still active
//     let monitors = monitor.store.get_monitors()?;
//     assert_eq!(monitors.len(), 1);
//     assert!(matches!(monitors[0], TypesToMonitorStore::RskPegin(_)));

//     clear_output();

//     Ok(())
// }

// #[test]
// fn test_best_block_news() -> Result<(), anyhow::Error> {
//     let mut mock_indexer = MockIndexerApi::new();
//     let path = format!("test_outputs/{}", generate_random_string());
//     let config = StorageConfig::new(path, None);
//     let storage = Rc::new(Storage::new(&config)?);
//     let store = MonitorStore::new(storage)?;

//     // Simulate the monitor's current height is 199, but the best block is 200
//     // so a new block should be detected.
//     let monitor_height: u32 = 199;
//     {
//         let store_ref = &store;
//         store_ref.update_monitor_height(monitor_height)?;
//     }
//     let block_200 = FullBlock {
//         height: 200,
//         hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000000",
//         )
//         .unwrap(),
//         prev_hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000022",
//         )
//         .unwrap(),
//         txs: vec![],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     let block_199 = FullBlock {
//         height: 199,
//         hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000001",
//         )
//         .unwrap(),
//         prev_hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000002",
//         )
//         .unwrap(),
//         txs: vec![],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     let block_200_clone = block_200.clone();
//     let block_200_clone_1 = block_200.clone();
//     let block_200_clone_2 = block_200.clone();

//     mock_indexer
//         .expect_get_block_by_height()
//         .with(eq(199))
//         .returning(move |_| Ok(Some(block_199.clone())));

//     mock_indexer
//         .expect_get_block_by_height()
//         .with(eq(200))
//         .returning(move |_| Ok(Some(block_200_clone.clone())));

//     mock_indexer
//         .expect_get_best_block()
//         .times(3)
//         .returning(move || Ok(Some(block_200_clone_1.clone())));

//     mock_indexer.expect_tick().returning(move || Ok(()));

//     let monitor = Monitor::new(
//         mock_indexer,
//         store,
//         MonitorSettings::from(MonitorSettingsConfig::default()),
//     )?;
//     monitor.save_monitor(TypesToMonitor::NewBlock)?;
//     monitor.tick()?;

//     // After tick, NewBlock news should be present
//     let news = monitor.store.get_news()?;
//     assert_eq!(news.len(), 1);
//     assert!(matches!(
//         news[0],
//         bitvmx_transaction_monitor::store::MonitoredTypes::NewBlock(hash) if hash == block_200_clone_2.hash
//     ));

//     // Acknowledge the news and verify it's gone
//     monitor.ack_news(AckMonitorNews::NewBlock)?;
//     let news = monitor.store.get_news()?;
//     assert_eq!(news.len(), 0);

//     monitor.tick()?;

//     // After tick, NewBlock news should not be present because it was already acknowledged
//     let news = monitor.store.get_news()?;
//     assert_eq!(news.len(), 0);

//     // Check if there's any pending work initially; it should be false
//     let is_pending_work = monitor.store.has_pending_work()?;
//     assert!(!is_pending_work);

//     // Save a new monitor for NewBlock and check again for pending work; should still be false
//     monitor.save_monitor(TypesToMonitor::NewBlock)?;

//     let is_pending_work = monitor.store.has_pending_work()?;
//     assert!(!is_pending_work);

//     // Create a new transaction and compute its txid
//     let tx_id = Transaction {
//         version: bitcoin::transaction::Version::TWO,
//         lock_time: LockTime::from_time(1653195600).unwrap(),
//         input: vec![],
//         output: vec![],
//     }
//     .compute_txid();

//     // Save a monitor for the transaction and set a description "test"
//     monitor.save_monitor(TypesToMonitor::Transactions(
//         vec![tx_id],
//         "test".to_string(),
//         None,
//     ))?;

//     // Check if there's pending work after saving the transaction monitor; it should be true
//     let is_pending_work = monitor.store.has_pending_work()?;
//     assert!(is_pending_work);

//     clear_output();

//     Ok(())
// }

// #[test]
// fn test_spending_utxo_monitor_orphan_handling() -> Result<(), anyhow::Error> {
//     let mut mock_indexer = MockIndexerApi::new();
//     let path = format!("test_outputs/{}", generate_random_string());
//     let config = StorageConfig::new(path, None);
//     let storage = Rc::new(Storage::new(&config)?);
//     let store = MonitorStore::new(storage)?;

//     let target_tx = Transaction {
//         version: bitcoin::transaction::Version::TWO,
//         lock_time: LockTime::from_time(1653195600).unwrap(),
//         input: vec![],
//         output: vec![],
//     };

//     let target_tx_id = target_tx.compute_txid();
//     let target_utxo_index = 0u32;

//     // Create a spending transaction
//     let spending_tx1 = Transaction {
//         version: bitcoin::transaction::Version::TWO,
//         lock_time: LockTime::from_time(1653195601).unwrap(),
//         input: vec![bitcoin::TxIn {
//             previous_output: bitcoin::OutPoint {
//                 txid: target_tx_id,
//                 vout: target_utxo_index,
//             },
//             script_sig: bitcoin::ScriptBuf::new(),
//             sequence: bitcoin::Sequence::MAX,
//             witness: bitcoin::Witness::new(),
//         }],
//         output: vec![],
//     };

//     let spending_tx2 = Transaction {
//         version: bitcoin::transaction::Version::TWO,
//         lock_time: LockTime::from_time(1653195601).unwrap(),
//         input: vec![bitcoin::TxIn {
//             previous_output: bitcoin::OutPoint {
//                 txid: target_tx_id,
//                 vout: target_utxo_index,
//             },
//             script_sig: bitcoin::ScriptBuf::new(),
//             sequence: bitcoin::Sequence::MAX,
//             witness: bitcoin::Witness::new(),
//         }],
//         output: vec![],
//     };

//     let spending_tx1_id = spending_tx1.compute_txid();

//     let block_100 = FullBlock {
//         height: 100,
//         hash: BlockHash::from_str(
//             "1000000000000000000000000000000000000000000000000000000000000001",
//         )?,
//         prev_hash: BlockHash::from_str(
//             "2000000000000000000000000000000000000000000000000000000000000000",
//         )?,
//         txs: vec![spending_tx1.clone()],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     // Block 100 has the spending transaction tx1
//     let block_101 = FullBlock {
//         height: 101,
//         hash: BlockHash::from_str(
//             "1000000000000000000000000000000000000000000000000000000000000002",
//         )?,
//         prev_hash: BlockHash::from_str(
//             "2000000000000000000000000000000000000000000000000000000000000001",
//         )?,
//         txs: vec![],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     // Block 100 is a new block making a reorg with block 100 , and has another spending transaction tx2
//     let block_100_reorg = FullBlock {
//         height: 100,
//         hash: BlockHash::from_str(
//             "1000000000000000000000000000000000000000000000000000000000000003",
//         )?,
//         prev_hash: BlockHash::from_str(
//             "2000000000000000000000000000000000000000000000000000000000000002",
//         )?,
//         txs: vec![spending_tx2.clone()],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     // Create transaction info for the spending transaction (orphan)
//     let spending_tx1_info_conf1 = tx_info_confirmed(&spending_tx1, &block_100, 1);
//     let spending_tx1_info_conf2 = tx_info_confirmed(&spending_tx1, &block_100, 2);
//     let spending_tx1_info_orphan = tx_info_orphan(&spending_tx1, &block_100);

//     let spending_tx2_info_conf1 = tx_info_confirmed(&spending_tx2, &block_100_reorg, 1);

//     let spending_tx2_id = spending_tx2.compute_txid();

//     let spending_tx1_info_conf1_clone = spending_tx1_info_conf1.clone();
//     let spending_tx1_info_conf2_clone = spending_tx1_info_conf2.clone();
//     let spending_tx1_info_orphan_clone = spending_tx1_info_orphan.clone();
//     let spending_tx2_info_conf1_clone = spending_tx2_info_conf1.clone();

//     mock_indexer.expect_tick().returning(move || Ok(()));

//     // Set up expectations
//     let block_100_clone = block_100.clone();

//     // Each tick in the monitor uses 2 get_best_block call but if there is pending work, it will use 1 get_best_block call
//     mock_indexer
//         .expect_get_best_block()
//         .times(1)
//         .returning(move || Ok(Some(block_100.clone())));

//     mock_indexer
//         .expect_get_block_by_height()
//         .with(eq(100))
//         .returning(move |_| Ok(Some(block_100_clone.clone())));

//     let block_101_clone = block_101.clone();

//     // Each tick in the monitor uses 2 get_best_block call but if there is pending work, it will use 1 get_best_block call
//     mock_indexer
//         .expect_get_best_block()
//         .times(2)
//         .returning(move || Ok(Some(block_101.clone())));

//     mock_indexer
//         .expect_get_block_by_height()
//         .with(eq(101))
//         .returning(move |_| Ok(Some(block_101_clone.clone())));

//     // Each tick in the monitor uses 2 get_best_block call but if there is pending work, it will use 1 get_best_block call
//     mock_indexer
//         .expect_get_best_block()
//         .times(2)
//         .returning(move || Ok(Some(block_100_reorg.clone())));

//     // Expect get_tx to be called for the spending transaction
//     // First tick: detect spending_tx1, create monitor, and process it
//     // - get_tx is called from process_spending_utxo_transaction -> process_transaction_monitor
//     // - get_tx is called from get_news() -> get_tx_status()
//     mock_indexer
//         .expect_get_transaction()
//         .withf(move |id| *id == spending_tx1_id)
//         .times(2)
//         .returning(move |_| Ok(spending_tx1_info_conf1_clone.clone()));

//     // Second tick: process the spending_tx1 monitor (it now has 2 confirmations)
//     // - get_tx is called from process_transaction_monitor
//     // - get_tx is called from get_news() -> get_tx_status()
//     mock_indexer
//         .expect_get_transaction()
//         .withf(move |id| *id == spending_tx1_id)
//         .times(2)
//         .returning(move |_| Ok(spending_tx1_info_conf2_clone.clone()));

//     // Third tick: reorg detected, spending_tx1 becomes orphan, detect spending_tx2
//     // - get_tx is called for spending_tx1 (to check orphan status)
//     // - get_tx is called from process_spending_utxo_transaction -> process_transaction_monitor for spending_tx2
//     // - get_tx is called from get_news() -> get_tx_status() for spending_tx2
//     mock_indexer
//         .expect_get_transaction()
//         .withf(move |id| *id == spending_tx1_id)
//         .times(1)
//         .returning(move |_| Ok(spending_tx1_info_orphan_clone.clone()));

//     mock_indexer
//         .expect_get_transaction()
//         .withf(move |id| *id == spending_tx2_id)
//         .times(2)
//         .returning(move |_| Ok(spending_tx2_info_conf1_clone.clone()));

//     let monitor = Monitor::new(
//         mock_indexer,
//         store,
//         MonitorSettings::from(MonitorSettingsConfig::default()),
//     )?;

//     // Add the SpendingUTXOTransaction monitor
//     monitor.save_monitor(TypesToMonitor::SpendingUTXOTransaction(
//         target_tx_id,
//         target_utxo_index,
//         String::new(),
//         None,
//     ))?;

//     // First tick - should detect the spending transaction
//     monitor.tick()?;

//     let news = monitor.get_news()?;
//     assert_eq!(news.len(), 1);

//     assert!(matches!(
//         news[0].clone(),
//         MonitorNews::SpendingUTXOTransaction(t, u, tx_status, _)
//             if t == target_tx_id
//                 && u == target_utxo_index
//                 && tx_status.tx.as_ref().is_some_and(|tx| tx.compute_txid() == spending_tx1_id)
//                 && tx_status.confirmations == 1
//     ));

//     monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
//         target_tx_id,
//         target_utxo_index,
//         String::new(),
//     ))?;

//     // Second tick - should confirm the spending transaction (2 confirmations)
//     monitor.tick()?;

//     let news = monitor.get_news()?;
//     assert_eq!(news.len(), 1);

//     assert!(matches!(
//         news[0].clone(),
//         MonitorNews::SpendingUTXOTransaction(t, u, tx_status, _)
//             if t == target_tx_id
//                 && u == target_utxo_index
//                 && tx_status.tx.as_ref().is_some_and(|tx| tx.compute_txid() == spending_tx1_id)
//                 && tx_status.confirmations == 2
//     ));

//     monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
//         target_tx_id,
//         target_utxo_index,
//         String::new(),
//     ))?;

//     // Third tick - Reorg with block 100, and should detect the new spending transaction tx2
//     monitor.tick()?;

//     let news = monitor.get_news()?;

//     assert_eq!(news.len(), 1);
//     assert!(matches!(
//         news[0].clone(),
//         MonitorNews::SpendingUTXOTransaction(t, u, tx_status, _)
//             if t == target_tx_id
//                 && u == target_utxo_index
//                 && tx_status.tx.as_ref().is_some_and(|tx| tx.compute_txid() == spending_tx2_id)
//                 && tx_status.confirmations == 1
//     ));

//     clear_output();

//     Ok(())
// }

// // This test verifies that a SpendingUTXOTransaction monitor is correctly deactivated after 3 confirmations.
// // It also checks that a SpendingUTXOTransaction notification is created and the monitor is removed properly.

// #[test]
// fn test_spending_utxo_monitor_deactivation_after_max_confirmations() -> Result<(), anyhow::Error> {
//     let mut mock_indexer = MockIndexerApi::new();
//     let path = format!("test_outputs/{}", generate_random_string());
//     let config = StorageConfig::new(path, None);
//     let storage = Rc::new(Storage::new(&config)?);
//     let store = MonitorStore::new(storage)?;

//     let target_tx = Transaction {
//         version: bitcoin::transaction::Version::TWO,
//         lock_time: LockTime::from_time(1653195600).unwrap(),
//         input: vec![],
//         output: vec![],
//     };

//     let target_tx_id = target_tx.compute_txid();
//     let target_utxo_index = 0u32;

//     // Create a spending transaction
//     let spending_tx = Transaction {
//         version: bitcoin::transaction::Version::TWO,
//         lock_time: LockTime::from_time(1653195601).unwrap(),
//         input: vec![bitcoin::TxIn {
//             previous_output: bitcoin::OutPoint {
//                 txid: target_tx_id,
//                 vout: target_utxo_index,
//             },
//             script_sig: bitcoin::ScriptBuf::new(),
//             sequence: bitcoin::Sequence::MAX,
//             witness: bitcoin::Witness::new(),
//         }],
//         output: vec![],
//     };

//     let spending_tx_id = spending_tx.compute_txid();

//     // Create a block at height 100 containing the spending transaction
//     let block_with_spending_tx = FullBlock {
//         height: 100,
//         hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000001",
//         )?,
//         prev_hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000000",
//         )?,
//         txs: vec![spending_tx.clone()],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     // Create a block at height 101 for further confirmations
//     let block_101 = FullBlock {
//         height: 101,
//         hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000002",
//         )?,
//         prev_hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000001",
//         )?,
//         txs: vec![],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     // Create a block at height 102 for the final confirmation count
//     let block_102 = FullBlock {
//         height: 102,
//         hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000003",
//         )?,
//         prev_hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000002",
//         )?,
//         txs: vec![],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     let spending_tx_info_at_100 = tx_info_confirmed(&spending_tx, &block_with_spending_tx, 1);

//     // Transaction info for the spending transaction at each confirmation level
//     // block_info should be the block where the transaction was mined (block 100)
//     // confirmations is the number of blocks mined after the transaction's block
//     let spending_tx_info_at_101 = tx_info_confirmed(&spending_tx, &block_with_spending_tx, 2);

//     // Set expectations for each tick: block 100, then 101, then 102
//     let best_block_100_clone_1 = block_with_spending_tx.clone();
//     let best_block_100_clone_2 = block_with_spending_tx.clone();
//     let best_block_101_clone = block_101.clone();
//     let best_block_101_clone_2 = block_101.clone();
//     let best_block_102_clone = block_102.clone();
//     let best_block_102_clone_2 = block_102.clone();

//     // Each tick in the monitor uses 1 get_best_block call but if there is pending work, it will use 2 get_best_block calls
//     mock_indexer
//         .expect_get_best_block()
//         .times(1)
//         .returning(move || Ok(Some(best_block_100_clone_1.clone())));

//     mock_indexer
//         .expect_get_block_by_height()
//         .with(eq(100))
//         .returning(move |_| Ok(Some(best_block_100_clone_2.clone())));

//     // Each tick in the monitor uses 2 get_best_block calls
//     mock_indexer
//         .expect_get_best_block()
//         .times(2)
//         .returning(move || Ok(Some(best_block_101_clone.clone())));

//     mock_indexer
//         .expect_get_block_by_height()
//         .with(eq(101))
//         .returning(move |_| Ok(Some(best_block_101_clone_2.clone())));

//     // Each tick in the monitor uses 2 get_best_block calls
//     mock_indexer
//         .expect_get_best_block()
//         .times(2)
//         .returning(move || Ok(Some(best_block_102_clone.clone())));

//     mock_indexer
//         .expect_get_block_by_height()
//         .with(eq(102))
//         .returning(move |_| Ok(Some(best_block_102_clone_2.clone())));

//     mock_indexer.expect_tick().returning(move || Ok(()));

//     mock_indexer
//         .expect_get_transaction()
//         .withf(move |id| *id == spending_tx_id)
//         .times(4)
//         .returning(move |_| Ok(spending_tx_info_at_100.clone()));

//     // Second tick: confirmations reach 2, should send news and deactivate
//     // get_tx() is called from tick() and then from get_news() -> get_tx_status()
//     mock_indexer
//         .expect_get_transaction()
//         .withf(move |id| *id == spending_tx_id)
//         // Allow multiple calls - from tick() -> process_transaction_monitor and from get_news() -> get_tx_status()
//         .returning(move |_| Ok(spending_tx_info_at_101.clone()));

//     let mut settings = MonitorSettings::from(MonitorSettingsConfig::default());
//     settings.max_monitoring_confirmations = 2;

//     let monitor = Monitor::new(mock_indexer, store, settings)?;

//     // Add the SpendingUTXOTransaction monitor
//     monitor.save_monitor(TypesToMonitor::SpendingUTXOTransaction(
//         target_tx_id,
//         target_utxo_index,
//         String::new(),
//         None,
//     ))?;

//     // Ensure the monitor is initially active
//     let monitors = monitor.store.get_monitors()?;
//     assert_eq!(monitors.len(), 1);

//     // First tick: detect and save the spending transaction
//     monitor.tick()?;

//     // After detecting the spending transaction, we should have:
//     // 1. The original SpendingUTXOTransaction monitor
//     // 2. The new Transaction monitor for the spending transaction
//     let monitors = monitor.store.get_monitors()?;
//     assert_eq!(monitors.len(), 2);

//     // Verify both monitors are present
//     let has_spending_utxo_monitor = monitors.iter().any(|m| {
//         matches!(
//             m,
//             TypesToMonitorStore::SpendingUTXOTransaction(t, u, _, _)
//                 if *t == target_tx_id && *u == target_utxo_index
//         )
//     });
//     assert!(
//         has_spending_utxo_monitor,
//         "SpendingUTXOTransaction monitor should be present"
//     );

//     let has_transaction_monitor = monitors.iter().any(|m| {
//         matches!(
//             m,
//             TypesToMonitorStore::Transaction(tx_id, extra_data, _)
//                 if *tx_id == spending_tx_id && extra_data.starts_with("INTERNAL_SPENDING_UTXO")
//         )
//     });
//     assert!(
//         has_transaction_monitor,
//         "Transaction monitor for spending tx should be present"
//     );

//     let news = monitor.get_news()?;
//     assert_eq!(news.len(), 1);
//     assert!(matches!(
//         news[0].clone(),
//         MonitorNews::SpendingUTXOTransaction(t, u, _, _)
//             if t == target_tx_id && u == target_utxo_index
//     ));

//     monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
//         target_tx_id,
//         target_utxo_index,
//         String::new(),
//     ))?;

//     // Second tick: confirmations reach the threshold; the monitor should send news and then be deactivated
//     monitor.tick()?;

//     // Check that news was created as expected
//     let monitors = monitor.store.get_monitors()?;

//     // When confirmations reach max_monitoring_confirmations (2), both monitors should be deactivated
//     // The Transaction monitor is deactivated in process_transaction_monitor
//     // The SpendingUTXOTransaction monitor is also deactivated in the same process
//     // However, the deactivation happens during tick(), but get_monitors() is called immediately after
//     // The monitors might still be in the process of being deactivated
//     // Let's verify that the news is sent correctly, which indicates the processing is working
//     // The actual deactivation verification will be done in the third tick
//     assert!(monitors.len() <= 2, "Should have at most 2 monitors");

//     let news = monitor.get_news()?;
//     assert_eq!(news.len(), 1);
//     assert!(matches!(
//         news[0].clone(),
//         MonitorNews::SpendingUTXOTransaction(t, u, _, _)
//             if t == target_tx_id && u == target_utxo_index
//     ));

//     monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
//         target_tx_id,
//         target_utxo_index,
//         String::new(),
//     ))?;

//     // Third tick: monitor is already deactivated, so no processing should happen
//     monitor.tick()?;

//     // Verify that the monitor is still deactivated
//     let monitors = monitor.store.get_monitors()?;
//     assert_eq!(monitors.len(), 0);

//     let news = monitor.get_news()?;
//     assert_eq!(news.len(), 0);

//     Ok(())
// }

// #[test]
// fn test_all_monitors_with_confirmation_trigger() -> Result<(), anyhow::Error> {
//     //Test Transaction monitor with confirmation trigger
//     {
//         let mut mock_indexer = MockIndexerApi::new();
//         let path = format!("test_outputs/{}", generate_random_string());
//         let config = StorageConfig::new(path, None);
//         let storage = Rc::new(Storage::new(&config)?);
//         let store = MonitorStore::new(storage)?;

//         let tx = Transaction {
//             version: bitcoin::transaction::Version::TWO,
//             lock_time: LockTime::from_time(1653195600).unwrap(),
//             input: vec![],
//             output: vec![],
//         };
//         let tx_id = tx.compute_txid();

//         let block_100 = FullBlock {
//             height: 100,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000000",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };

//         let block_101 = FullBlock {
//             height: 101,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };

//         let block_102 = FullBlock {
//             height: 102,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000003",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };

//         let tx_info_1_conf = tx_info_confirmed(&tx, &block_100, 1);

//         let tx_info_2_conf = tx_info_confirmed(&tx, &block_100, 2);

//         let best_block_100_clone_1 = block_100.clone();
//         let best_block_100_clone_2 = block_100.clone();
//         let best_block_101_clone = block_101.clone();
//         let best_block_101_clone_2 = block_101.clone();
//         let best_block_102_clone = block_102.clone();
//         let best_block_102_clone_2 = block_102.clone();

//         mock_indexer
//             .expect_get_best_block()
//             .times(1)
//             .returning(move || Ok(Some(best_block_100_clone_1.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(100))
//             .returning(move |_| Ok(Some(best_block_100_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             .times(2)
//             .returning(move || Ok(Some(best_block_101_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(101))
//             .returning(move |_| Ok(Some(best_block_101_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             .times(2)
//             .returning(move || Ok(Some(best_block_102_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(102))
//             .returning(move |_| Ok(Some(best_block_102_clone_2.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .returning(move |_| Ok(None));
//         mock_indexer.expect_tick().returning(move || Ok(()));

//         let tx_info_1_conf_clone = tx_info_1_conf.clone();
//         mock_indexer
//             .expect_get_transaction()
//             .with(eq(tx_id))
//             .times(2)
//             .returning(move |_| Ok(tx_info_1_conf_clone.clone()));
//         let tx_info_2_conf_clone = tx_info_2_conf.clone();
//         mock_indexer
//             .expect_get_transaction()
//             .with(eq(tx_id))
//             .times(1)
//             .returning(move |_| Ok(tx_info_2_conf_clone.clone()));
//         mock_indexer.expect_get_transaction().returning(move |_| {
//             Ok(TransactionStatus {
//                 tx: None,
//                 block_info: None,
//                 confirmations: 0,
//                 status: TransactionBlockchainStatus::NotFound,
//                 confirmation_threshold: 6,
//             })
//         });

//         let mut settings = MonitorSettings::from(MonitorSettingsConfig::default());
//         settings.max_monitoring_confirmations = 2;
//         let monitor = Monitor::new(mock_indexer, store, settings)?;

//         monitor.save_monitor(TypesToMonitor::Transactions(
//             vec![tx_id],
//             String::new(),
//             Some(1),
//         ))?;
//         monitor.tick()?;
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 1);
//         assert!(matches!(news[0].clone(), MonitorNews::Transaction(t, _, _) if t == tx_id));
//         monitor.ack_news(AckMonitorNews::Transaction(tx_id, String::new()))?;
//         monitor.tick()?;
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 0);
//         monitor.tick()?;
//         let monitors = monitor.store.get_monitors()?;
//         assert_eq!(monitors.len(), 0);
//     }

//     // Test RskPeginTransaction monitor with confirmation trigger
//     {
//         let mut mock_indexer = MockIndexerApi::new();
//         let path = format!("test_outputs/{}", generate_random_string());
//         let config = StorageConfig::new(path, None);
//         let storage = Rc::new(Storage::new(&config)?);
//         let store = MonitorStore::new(storage)?;

//         let pegin_tx = create_pegin_tx();
//         let block_100 = FullBlock {
//             height: 100,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000000",
//             )?,
//             txs: vec![pegin_tx.clone()],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };
//         let pegin_tx_id_from_block = block_100.txs[0].compute_txid();
//         let block_101 = FullBlock {
//             height: 101,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };
//         let block_102 = FullBlock {
//             height: 102,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000003",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };

//         let tx_info_1_conf = tx_info_confirmed(&pegin_tx, &block_100, 1);

//         let best_block_100_clone_1 = block_100.clone();
//         let best_block_100_clone_2 = block_100.clone();
//         let best_block_101_clone = block_101.clone();
//         let best_block_101_clone_2 = block_101.clone();
//         let best_block_102_clone = block_102.clone();
//         let best_block_102_clone_2 = block_102.clone();

//         mock_indexer
//             .expect_get_best_block()
//             .times(1)
//             .returning(move || Ok(Some(best_block_100_clone_1.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(100))
//             .returning(move |_| Ok(Some(best_block_100_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             .times(2)
//             .returning(move || Ok(Some(best_block_101_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(101))
//             .returning(move |_| Ok(Some(best_block_101_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             .times(2)
//             .returning(move || Ok(Some(best_block_102_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(102))
//             .returning(move |_| Ok(Some(best_block_102_clone_2.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .returning(move |_| Ok(None));
//         mock_indexer.expect_tick().returning(move || Ok(()));

//         let tx_info_1_conf_clone = tx_info_1_conf.clone();
//         mock_indexer
//             .expect_get_transaction()
//             .withf(move |id| *id == pegin_tx_id_from_block)
//             // Calls: tick #1 + get_news after tick #1 + tick #2 + tick #3
//             .times(4)
//             .returning(move |_| Ok(tx_info_1_conf_clone.clone()));

//         let mut settings = MonitorSettings::from(MonitorSettingsConfig::default());
//         settings.max_monitoring_confirmations = 2;
//         let monitor = Monitor::new(mock_indexer, store, settings)?;

//         monitor.save_monitor(TypesToMonitor::RskPegin(Some(1)))?;
//         monitor.tick()?;
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 1);
//         assert!(
//             matches!(news[0].clone(), MonitorNews::RskPeginTransaction(t, _) if t == pegin_tx_id_from_block)
//         );
//         monitor.ack_news(AckMonitorNews::RskPeginTransaction(pegin_tx_id_from_block))?;
//         monitor.tick()?;
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 0);
//         monitor.tick()?;
//         let monitors = monitor.store.get_monitors()?;
//         assert_eq!(monitors.len(), 2);
//         assert!(matches!(monitors[1], TypesToMonitorStore::RskPegin(_)));
//         assert!(matches!(
//             monitors[0],
//             TypesToMonitorStore::Transaction(_, _, _)
//         ));
//     }

//     // Test SpendingUTXOTransaction monitor with confirmation trigger
//     {
//         let mut mock_indexer = MockIndexerApi::new();
//         let path = format!("test_outputs/{}", generate_random_string());
//         let config = StorageConfig::new(path, None);
//         let storage = Rc::new(Storage::new(&config)?);
//         let store = MonitorStore::new(storage)?;

//         let target_tx = Transaction {
//             version: bitcoin::transaction::Version::TWO,
//             lock_time: LockTime::from_time(1653195600).unwrap(),
//             input: vec![],
//             output: vec![],
//         };
//         let target_tx_id = target_tx.compute_txid();
//         let target_utxo_index = 0u32;

//         let spending_tx = Transaction {
//             version: bitcoin::transaction::Version::TWO,
//             lock_time: LockTime::from_time(1653195601).unwrap(),
//             input: vec![bitcoin::TxIn {
//                 previous_output: bitcoin::OutPoint {
//                     txid: target_tx_id,
//                     vout: target_utxo_index,
//                 },
//                 script_sig: bitcoin::ScriptBuf::new(),
//                 sequence: bitcoin::Sequence::MAX,
//                 witness: bitcoin::Witness::new(),
//             }],
//             output: vec![],
//         };
//         let spending_tx_id = spending_tx.compute_txid();

//         let block_with_spending_tx = FullBlock {
//             height: 100,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000000",
//             )?,
//             txs: vec![spending_tx.clone()],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };
//         let block_101 = FullBlock {
//             height: 101,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };
//         let block_102 = FullBlock {
//             height: 102,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000003",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };

//         let spending_tx_info_at_100 = TransactionStatus::new(
//             spending_tx.clone(),
//             block_with_spending_tx.clone(),
//             TransactionBlockchainStatus::Confirmed,
//             1,
//             0,
//         );

//         let spending_tx_info_at_101 = TransactionStatus::new(
//             spending_tx.clone(),
//             block_with_spending_tx.clone(),
//             TransactionBlockchainStatus::Confirmed,
//             2,
//             0,
//         );

//         let best_block_100_clone_1 = block_with_spending_tx.clone();
//         let best_block_100_clone_2 = block_with_spending_tx.clone();
//         let best_block_101_clone = block_101.clone();
//         let best_block_101_clone_2 = block_101.clone();
//         let best_block_102_clone = block_102.clone();
//         let best_block_102_clone_2 = block_102.clone();

//         mock_indexer
//             .expect_get_best_block()
//             .times(1)
//             .returning(move || Ok(Some(best_block_100_clone_1.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(100))
//             .returning(move |_| Ok(Some(best_block_100_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             .times(2)
//             .returning(move || Ok(Some(best_block_101_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(101))
//             .returning(move |_| Ok(Some(best_block_101_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             // .times(2)
//             .returning(move || Ok(Some(best_block_102_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(102))
//             .returning(move |_| Ok(Some(best_block_102_clone_2.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .returning(move |_| Ok(None));
//         mock_indexer.expect_tick().returning(move || Ok(()));

//         // First tick: detect spending tx (multiple calls from tick() and get_news())
//         mock_indexer
//             .expect_get_transaction()
//             .withf(move |id| *id == spending_tx_id)
//             .times(1)
//             .returning(move |_| Ok(spending_tx_info_at_100.clone()));
//         // Second tick: check confirmations (from tick() only, get_news() might call get_tx() if there are unacknowledged news)
//         mock_indexer
//             .expect_get_transaction()
//             .withf(move |id| *id == spending_tx_id)
//             .returning(move |_| Ok(spending_tx_info_at_101.clone()));
//         // If get_news() is called and there are unacknowledged news, it will call get_tx_status() which calls get_tx()
//         // But since we ack_news() after the first tick, there should be no news, so get_news() won't call get_tx()
//         let mut settings = MonitorSettings::from(MonitorSettingsConfig::default());
//         settings.max_monitoring_confirmations = 2;
//         let monitor = Monitor::new(mock_indexer, store, settings)?;

//         // Add the SpendingUTXOTransaction monitor with confirmation trigger 1
//         monitor.save_monitor(TypesToMonitor::SpendingUTXOTransaction(
//             target_tx_id,
//             target_utxo_index,
//             String::new(),
//             Some(1),
//         ))?;

//         monitor.tick()?;
//         let monitors = monitor.store.get_monitors()?;
//         // After detecting the spending transaction, we should have:
//         // 1. The original SpendingUTXOTransaction monitor
//         // 2. The new Transaction monitor for the spending transaction
//         assert_eq!(monitors.len(), 2);

//         // Verify both monitors are present
//         let has_spending_utxo_monitor = monitors.iter().any(|m| {
//             matches!(
//                 m,
//                 TypesToMonitorStore::SpendingUTXOTransaction(t, u, _, _)
//                     if *t == target_tx_id && *u == target_utxo_index
//             )
//         });
//         assert!(
//             has_spending_utxo_monitor,
//             "SpendingUTXOTransaction monitor should be present"
//         );

//         let has_transaction_monitor = monitors.iter().any(|m| {
//             matches!(
//                 m,
//                 TypesToMonitorStore::Transaction(tx_id, extra_data, _)
//                     if *tx_id == spending_tx_id && extra_data.starts_with("INTERNAL_SPENDING_UTXO")
//             )
//         });
//         assert!(
//             has_transaction_monitor,
//             "Transaction monitor for spending tx should be present"
//         );

//         // Transaction was seen and trigger is 1 so news
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 1);

//         assert!(
//             matches!(news[0].clone(), MonitorNews::SpendingUTXOTransaction(t, u, _, _) if t == target_tx_id && u == target_utxo_index)
//         );

//         monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
//             target_tx_id,
//             target_utxo_index,
//             String::new(),
//         ))?;

//         monitor.tick()?;
//         // Transaction was seen and trigger is 1 so news
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 0);
//     }

//     Ok(())
// }

// #[test]
// fn test_all_monitors_without_confirmation_trigger() -> Result<(), anyhow::Error> {
//     // Test Transaction monitor without confirmation trigger
//     {
//         let mut mock_indexer = MockIndexerApi::new();
//         let path = format!("test_outputs/{}", generate_random_string());
//         let config = StorageConfig::new(path, None);
//         let storage = Rc::new(Storage::new(&config)?);
//         let store = MonitorStore::new(storage)?;

//         let tx = Transaction {
//             version: bitcoin::transaction::Version::TWO,
//             lock_time: LockTime::from_time(1653195600).unwrap(),
//             input: vec![],
//             output: vec![],
//         };
//         let tx_id = tx.compute_txid();

//         let block_100 = FullBlock {
//             height: 100,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000000",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };

//         let block_101 = FullBlock {
//             height: 101,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };

//         let block_102 = FullBlock {
//             height: 102,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000003",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };

//         let tx_info_1_conf = tx_info_confirmed(&tx, &block_100, 1);

//         let tx_info_2_conf = tx_info_confirmed(&tx, &block_100, 2);

//         let best_block_100_clone_1 = block_100.clone();
//         let best_block_100_clone_2 = block_100.clone();
//         let best_block_101_clone = block_101.clone();
//         let best_block_101_clone_2 = block_101.clone();
//         let best_block_102_clone = block_102.clone();
//         let best_block_102_clone_2 = block_102.clone();

//         mock_indexer
//             .expect_get_best_block()
//             .times(1)
//             .returning(move || Ok(Some(best_block_100_clone_1.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(100))
//             .returning(move |_| Ok(Some(best_block_100_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             .times(2)
//             .returning(move || Ok(Some(best_block_101_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(101))
//             .returning(move |_| Ok(Some(best_block_101_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             .times(2)
//             .returning(move || Ok(Some(best_block_102_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(102))
//             .returning(move |_| Ok(Some(best_block_102_clone_2.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .returning(move |_| Ok(None));
//         mock_indexer.expect_tick().returning(move || Ok(()));

//         let tx_info_1_conf_clone = tx_info_1_conf.clone();
//         mock_indexer
//             .expect_get_transaction()
//             .with(eq(tx_id))
//             .times(2)
//             .returning(move |_| Ok(tx_info_1_conf_clone.clone()));
//         let tx_info_2_conf_clone = tx_info_2_conf.clone();
//         mock_indexer
//             .expect_get_transaction()
//             .with(eq(tx_id))
//             .times(1)
//             .returning(move |_| Ok(tx_info_2_conf_clone.clone()));
//         mock_indexer.expect_get_transaction().returning(move |_| {
//             Ok(TransactionStatus {
//                 tx: None,
//                 block_info: None,
//                 confirmations: 0,
//                 status: TransactionBlockchainStatus::NotFound,
//                 confirmation_threshold: 6,
//             })
//         });

//         let mut settings = MonitorSettings::from(MonitorSettingsConfig::default());
//         settings.max_monitoring_confirmations = 2;
//         let monitor = Monitor::new(mock_indexer, store, settings)?;

//         monitor.save_monitor(TypesToMonitor::Transactions(
//             vec![tx_id],
//             String::new(),
//             None,
//         ))?;
//         monitor.tick()?;
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 1);
//         assert!(matches!(news[0].clone(), MonitorNews::Transaction(t, _, _) if t == tx_id));
//         monitor.ack_news(AckMonitorNews::Transaction(tx_id, String::new()))?;
//         monitor.tick()?;
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 0);

//         monitor.tick()?;
//         let monitors = monitor.store.get_monitors()?;
//         assert_eq!(monitors.len(), 0);
//     }

//     // Test RskPeginTransaction monitor without confirmation trigger
//     {
//         let mut mock_indexer = MockIndexerApi::new();
//         let path = format!("test_outputs/{}", generate_random_string());
//         let config = StorageConfig::new(path, None);
//         let storage = Rc::new(Storage::new(&config)?);
//         let store = MonitorStore::new(storage)?;

//         let pegin_tx = create_pegin_tx();
//         let block_100 = FullBlock {
//             height: 100,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000000",
//             )?,
//             txs: vec![pegin_tx.clone()],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };
//         let pegin_tx_id = block_100.txs[0].compute_txid();
//         let block_101 = FullBlock {
//             height: 101,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };
//         let block_102 = FullBlock {
//             height: 102,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000003",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };

//         let tx_info_1_conf = tx_info_confirmed(&pegin_tx, &block_100, 1);

//         let best_block_100_clone_1 = block_100.clone();
//         let best_block_100_clone_2 = block_100.clone();
//         let best_block_101_clone = block_101.clone();
//         let best_block_101_clone_2 = block_101.clone();
//         let best_block_102_clone = block_102.clone();
//         let best_block_102_clone_2 = block_102.clone();

//         mock_indexer
//             .expect_get_best_block()
//             .times(1)
//             .returning(move || Ok(Some(best_block_100_clone_1.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(100))
//             .returning(move |_| Ok(Some(best_block_100_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             .times(2)
//             .returning(move || Ok(Some(best_block_101_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(101))
//             .returning(move |_| Ok(Some(best_block_101_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             .times(2)
//             .returning(move || Ok(Some(best_block_102_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(102))
//             .returning(move |_| Ok(Some(best_block_102_clone_2.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .returning(move |_| Ok(None));
//         mock_indexer.expect_tick().returning(move || Ok(()));

//         let tx_info_1_conf_clone = tx_info_1_conf.clone();
//         mock_indexer
//             .expect_get_transaction()
//             .withf(move |id| *id == pegin_tx_id)
//             // Calls: tick #1 + get_news after tick #1 + tick #2 + tick #3
//             .times(5)
//             .returning(move |_| Ok(tx_info_1_conf_clone.clone()));

//         let mut settings = MonitorSettings::from(MonitorSettingsConfig::default());
//         settings.max_monitoring_confirmations = 2;
//         let monitor = Monitor::new(mock_indexer, store, settings)?;

//         monitor.save_monitor(TypesToMonitor::RskPegin(None))?;
//         monitor.tick()?;
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 1);
//         assert!(
//             matches!(news[0].clone(), MonitorNews::RskPeginTransaction(t, i) if t == pegin_tx_id && i.confirmations == 1)
//         );

//         // Acknowledge the news
//         monitor.ack_news(AckMonitorNews::RskPeginTransaction(pegin_tx_id))?;

//         // Tick again to check if the news is still present, there is a new block so the news should be still present
//         monitor.tick()?;
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 1);
//         monitor.tick()?;
//         let monitors = monitor.store.get_monitors()?;
//         assert_eq!(monitors.len(), 2);
//         assert!(matches!(monitors[1], TypesToMonitorStore::RskPegin(_)));
//         assert!(matches!(
//             monitors[0],
//             TypesToMonitorStore::Transaction(_, _, _)
//         ));
//     }

//     // Test SpendingUTXOTransaction monitor without confirmation trigger
//     {
//         let mut mock_indexer = MockIndexerApi::new();
//         let path = format!("test_outputs/{}", generate_random_string());
//         let config = StorageConfig::new(path, None);
//         let storage = Rc::new(Storage::new(&config)?);
//         let store = MonitorStore::new(storage)?;

//         let target_tx = Transaction {
//             version: bitcoin::transaction::Version::TWO,
//             lock_time: LockTime::from_time(1653195600).unwrap(),
//             input: vec![],
//             output: vec![],
//         };
//         let target_tx_id = target_tx.compute_txid();
//         let target_utxo_index = 0u32;

//         let spending_tx = Transaction {
//             version: bitcoin::transaction::Version::TWO,
//             lock_time: LockTime::from_time(1653195601).unwrap(),
//             input: vec![bitcoin::TxIn {
//                 previous_output: bitcoin::OutPoint {
//                     txid: target_tx_id,
//                     vout: target_utxo_index,
//                 },
//                 script_sig: bitcoin::ScriptBuf::new(),
//                 sequence: bitcoin::Sequence::MAX,
//                 witness: bitcoin::Witness::new(),
//             }],
//             output: vec![],
//         };
//         let spending_tx_id = spending_tx.compute_txid();

//         let block_with_spending_tx = FullBlock {
//             height: 100,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000000",
//             )?,
//             txs: vec![spending_tx.clone()],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };
//         let block_101 = FullBlock {
//             height: 101,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000001",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };
//         let block_102 = FullBlock {
//             height: 102,
//             hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000003",
//             )?,
//             prev_hash: BlockHash::from_str(
//                 "0000000000000000000000000000000000000000000000000000000000000002",
//             )?,
//             txs: vec![],
//             orphan: false,
//             estimated_fee_rate: 0,
//         };

//         let spending_tx_info_at_100 = tx_info_confirmed(&spending_tx, &block_with_spending_tx, 1);

//         let spending_tx_info_at_101 = tx_info_confirmed(&spending_tx, &block_with_spending_tx, 2);

//         let best_block_100_clone_1 = block_with_spending_tx.clone();
//         let best_block_100_clone_2 = block_with_spending_tx.clone();
//         let best_block_101_clone = block_101.clone();
//         let best_block_101_clone_2 = block_101.clone();
//         let best_block_102_clone = block_102.clone();
//         let best_block_102_clone_2 = block_102.clone();

//         mock_indexer
//             .expect_get_best_block()
//             .times(1)
//             .returning(move || Ok(Some(best_block_100_clone_1.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(100))
//             .returning(move |_| Ok(Some(best_block_100_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             .times(2)
//             .returning(move || Ok(Some(best_block_101_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(101))
//             .returning(move |_| Ok(Some(best_block_101_clone_2.clone())));
//         mock_indexer
//             .expect_get_best_block()
//             .times(2)
//             .returning(move || Ok(Some(best_block_102_clone.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .with(eq(102))
//             .returning(move |_| Ok(Some(best_block_102_clone_2.clone())));
//         mock_indexer
//             .expect_get_block_by_height()
//             .returning(move |_| Ok(None));
//         mock_indexer.expect_tick().returning(move || Ok(()));

//         let spending_tx_info_at_100_clone = spending_tx_info_at_100.clone();
//         mock_indexer
//             .expect_get_transaction()
//             .withf(move |id| *id == spending_tx_id)
//             .times(1)
//             .returning(move |_| Ok(spending_tx_info_at_100_clone.clone()));
//         let spending_tx_info_at_101_clone = spending_tx_info_at_101.clone();
//         mock_indexer
//             .expect_get_transaction()
//             .withf(move |id| *id == spending_tx_id)
//             .times(2)
//             .returning(move |_| Ok(spending_tx_info_at_101_clone.clone()));

//         let mut settings = MonitorSettings::from(MonitorSettingsConfig::default());
//         settings.max_monitoring_confirmations = 2;
//         let monitor = Monitor::new(mock_indexer, store, settings)?;

//         monitor.save_monitor(TypesToMonitor::SpendingUTXOTransaction(
//             target_tx_id,
//             target_utxo_index,
//             String::new(),
//             None,
//         ))?;
//         monitor.tick()?;
//         let monitors = monitor.store.get_monitors()?;
//         // After detecting the spending transaction, we should have:
//         // 1. The original SpendingUTXOTransaction monitor
//         // 2. The new Transaction monitor for the spending transaction
//         assert_eq!(monitors.len(), 2);

//         // Verify both monitors are present
//         let has_spending_utxo_monitor = monitors.iter().any(|m| {
//             matches!(
//                 m,
//                 TypesToMonitorStore::SpendingUTXOTransaction(t, u, _, _)
//                     if t == target_tx_id && u == target_utxo_index
//             )
//         });
//         assert!(
//             has_spending_utxo_monitor,
//             "SpendingUTXOTransaction monitor should be present"
//         );

//         let has_transaction_monitor = monitors.iter().any(|m| {
//             matches!(
//                 m,
//                 TypesToMonitorStore::Transaction(tx_id, extra_data, _)
//                     if tx_id == spending_tx_id && extra_data.starts_with("INTERNAL_SPENDING_UTXO")
//             )
//         });
//         assert!(
//             has_transaction_monitor,
//             "Transaction monitor for spending tx should be present"
//         );

//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 1);
//         assert!(
//             matches!(news[0].clone(), MonitorNews::SpendingUTXOTransaction(t, u, _, _) if t == target_tx_id && u == target_utxo_index)
//         );
//         monitor.ack_news(AckMonitorNews::SpendingUTXOTransaction(
//             target_tx_id,
//             target_utxo_index,
//             String::new(),
//         ))?;
//         monitor.tick()?;
//         let monitors = monitor.store.get_monitors()?;
//         // After second tick, the spending transaction has 2 confirmations which equals max_monitoring_confirmations,
//         // so both monitors should be deactivated
//         assert_eq!(monitors.len(), 0);
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 0);
//         monitor.tick()?;
//         let monitors = monitor.store.get_monitors()?;
//         assert_eq!(monitors.len(), 0);
//         let news = monitor.get_news()?;
//         assert_eq!(news.len(), 0);
//     }

//     Ok(())
// }

// #[test]
// fn test_transaction_monitor_deactivation_after_max_confirmations() -> Result<(), anyhow::Error> {
//     let mut mock_indexer = MockIndexerApi::new();
//     let path = format!("test_outputs/{}", generate_random_string());
//     let config = StorageConfig::new(path, None);
//     let storage = Rc::new(Storage::new(&config)?);
//     let store = MonitorStore::new(storage)?;

//     let tx = Transaction {
//         version: bitcoin::transaction::Version::TWO,
//         lock_time: LockTime::from_time(1653195600).unwrap(),
//         input: vec![],
//         output: vec![],
//     };

//     let tx_id = tx.compute_txid();

//     // Create blocks at different heights
//     let block_100 = FullBlock {
//         height: 100,
//         hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000001",
//         )?,
//         prev_hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000000",
//         )?,
//         txs: vec![],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     let block_101 = FullBlock {
//         height: 101,
//         hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000002",
//         )?,
//         prev_hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000001",
//         )?,
//         txs: vec![],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     let block_102 = FullBlock {
//         height: 102,
//         hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000003",
//         )?,
//         prev_hash: BlockHash::from_str(
//             "0000000000000000000000000000000000000000000000000000000000000002",
//         )?,
//         txs: vec![],
//         orphan: false,
//         estimated_fee_rate: 0,
//     };

//     // Transaction info with 1 confirmation
//     let tx_info_1_conf = tx_info_confirmed(&tx, &block_100, 1);

//     // Transaction info with 2 confirmations (reaches max)
//     let tx_info_2_conf = tx_info_confirmed(&tx, &block_100, 2);

//     // Transaction info with 2 confirmations (reaches max)
//     let tx_info_3_conf = tx_info_confirmed(&tx, &block_100, 3);

//     // Set expectations for each tick
//     let best_block_100_clone_1 = block_100.clone();
//     let best_block_100_clone_2 = block_100.clone();
//     let best_block_101_clone = block_101.clone();
//     let best_block_101_clone_2 = block_101.clone();
//     let best_block_102_clone = block_102.clone();
//     let best_block_102_clone_2 = block_102.clone();

//     mock_indexer
//         .expect_get_best_block()
//         .times(1)
//         .returning(move || Ok(Some(best_block_100_clone_1.clone())));

//     mock_indexer
//         .expect_get_block_by_height()
//         .with(eq(100))
//         .returning(move |_| Ok(Some(best_block_100_clone_2.clone())));

//     mock_indexer
//         .expect_get_best_block()
//         .times(2)
//         .returning(move || Ok(Some(best_block_101_clone.clone())));

//     mock_indexer
//         .expect_get_block_by_height()
//         .with(eq(101))
//         .returning(move |_| Ok(Some(best_block_101_clone_2.clone())));

//     mock_indexer
//         .expect_get_best_block()
//         .times(2)
//         .returning(move || Ok(Some(best_block_102_clone.clone())));

//     mock_indexer
//         .expect_get_block_by_height()
//         .with(eq(102))
//         .returning(move |_| Ok(Some(best_block_102_clone_2.clone())));

//     // Handle calls to get_block_by_height for is_pending_work when monitor has no height yet
//     // This must be last so specific expectations are matched first
//     mock_indexer
//         .expect_get_block_by_height()
//         .returning(move |_| Ok(None));

//     mock_indexer.expect_tick().returning(move || Ok(()));

//     // First tick: from tick() and get_news()
//     let tx_info_1_conf_clone = tx_info_1_conf.clone();
//     mock_indexer
//         .expect_get_transaction()
//         .withf(move |id| *id == tx_id)
//         .times(2) // Once from tick(), once from get_news() -> get_tx_status()
//         .returning(move |_| Ok(tx_info_1_conf_clone.clone()));

//     // Second tick: from tick() and get_news()
//     let tx_info_2_conf_clone = tx_info_2_conf.clone();
//     mock_indexer
//         .expect_get_transaction()
//         .withf(move |id| *id == tx_id)
//         .times(2)
//         .returning(move |_| Ok(tx_info_2_conf_clone.clone()));

//     mock_indexer
//         .expect_get_transaction()
//         .withf(move |id| *id == tx_id)
//         .times(1)
//         .returning(move |_| Ok(tx_info_3_conf.clone()));

//     let mut settings = MonitorSettings::from(MonitorSettingsConfig::default());
//     settings.max_monitoring_confirmations = 3;

//     let monitor = Monitor::new(mock_indexer, store, settings)?;

//     // Add monitor without confirmation trigger
//     monitor.save_monitor(TypesToMonitor::Transactions(
//         vec![tx_id],
//         String::new(),
//         None,
//     ))?;

//     // First tick: should send news
//     monitor.tick()?;

//     let news = monitor.get_news()?;
//     assert_eq!(news.len(), 1);
//     assert!(matches!(
//         news[0].clone(),
//         MonitorNews::Transaction(t, _, _) if t == tx_id
//     ));

//     monitor.ack_news(AckMonitorNews::Transaction(tx_id, String::new()))?;

//     // Second tick: should send news and then deactivate
//     monitor.tick()?;

//     let news = monitor.get_news()?;
//     assert_eq!(news.len(), 1);
//     assert!(matches!(
//         news[0].clone(),
//         MonitorNews::Transaction(t, _, _) if t == tx_id
//     ));

//     monitor.ack_news(AckMonitorNews::Transaction(tx_id, String::new()))?;

//     // Third tick: should deactivate
//     monitor.tick()?;

//     let monitors = monitor.store.get_monitors()?;
//     assert_eq!(monitors.len(), 0);

//     let news = monitor.get_news()?;
//     assert_eq!(news.len(), 0);

//     Ok(())
// }

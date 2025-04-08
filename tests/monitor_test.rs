use bitcoin::{absolute::LockTime, BlockHash, Transaction};
use bitcoin_indexer::indexer::MockIndexerApi;
use bitvmx_bitcoin_rpc::types::{FullBlock, TransactionInfo};
use bitvmx_transaction_monitor::{
    monitor::Monitor,
    store::{MockMonitorStore, TransactionMonitorType},
    types::{ExtraData, TransactionMonitor},
};
use mockall::predicate::*;
use std::str::FromStr;

#[test]
fn no_monitors() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_monitor_store = MockMonitorStore::new();

    let best_block_100 = FullBlock {
        height: 100,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap(),
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap(),
        txs: vec![],
        orphan: false,
    };

    let block_100_height = best_block_100.height;

    mock_indexer
        .expect_tick()
        .returning(move |_| Ok(block_100_height + 1));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(best_block_100.clone())));

    // Return nothing to monitor
    mock_monitor_store
        .expect_get_monitors()
        .with(eq(block_100_height))
        .times(1)
        .returning(|_| Ok(vec![]));

    mock_monitor_store
        .expect_set_monitor_height()
        .returning(|_| Ok(()));

    mock_monitor_store
        .expect_get_monitor_height()
        .returning(|| Ok(100));

    let monitor = Monitor::new(mock_indexer, mock_monitor_store, Some(block_100_height), 6)?;

    monitor.tick()?;

    Ok(())
}

#[test]
fn monitor_tx_detected() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_monitor_store = MockMonitorStore::new();

    let block_200 = FullBlock {
        height: 200,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap(),
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap(),
        txs: vec![],
        orphan: false,
    };

    let block_height_200 = block_200.height;

    let tx_to_seen = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195601).unwrap(),
        input: vec![],
        output: vec![],
    };

    let monitors = vec![
        (
            TransactionMonitor::Transactions(vec![tx.compute_txid()], ExtraData::None),
            180,
        ),
        (
            TransactionMonitor::Transactions(vec![tx_to_seen.compute_txid()], ExtraData::None),
            180,
        ),
    ];

    let hash_150 =
        BlockHash::from_str("12efaa3528db3845a859c470a525f1b8b4643b0d561f961ab395a9db778c204d")?;

    let hash_190 =
        BlockHash::from_str("23efda3528db3845a859c470a525f1b8b4643b0d561f961ab395a9db778c204d")?;

    let tx_to_seen_info = TransactionInfo {
        tx: tx_to_seen.clone(),
        block_hash: hash_150,
        orphan: false,
        block_height: 150,
        confirmations: 10,
    };

    let tx_info = TransactionInfo {
        tx: tx.clone(),
        block_hash: hash_190,
        orphan: false,
        block_height: 190,
        confirmations: 10,
    };

    mock_indexer
        .expect_tick()
        .returning(move |_| Ok(block_height_200 + 1));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(block_200.clone())));

    // Convert TransactionMonitor to TransactionMonitorType for the mock
    let monitor_types = monitors
        .iter()
        .map(|(monitor, _)| match monitor {
            TransactionMonitor::Transactions(txids, extra_data) => {
                TransactionMonitorType::Transaction(txids[0], extra_data.clone())
            }
            TransactionMonitor::SpendingUTXOTransaction(txid, utxo_index, extra_data) => {
                TransactionMonitorType::SpendingUTXOTransaction(
                    *txid,
                    *utxo_index,
                    extra_data.clone(),
                )
            }
            TransactionMonitor::RskPeginTransaction => TransactionMonitorType::RskPeginTransaction,
        })
        .collect::<Vec<_>>();

    mock_monitor_store
        .expect_get_monitors()
        .with(eq(block_height_200))
        .times(1)
        .returning(move |_| Ok(monitor_types.clone()));

    // Tx was found by the indexer and is already in the blockchain.
    mock_indexer
        .expect_get_tx()
        .with(eq(tx_to_seen.compute_txid()))
        .times(1)
        .returning(move |_| Ok(Some(tx_to_seen_info.clone())));

    mock_indexer
        .expect_get_tx()
        .with(eq(tx.compute_txid()))
        .times(1)
        .returning(move |_| Ok(Some(tx_info.clone())));

    mock_monitor_store
        .expect_set_monitor_height()
        .returning(|_| Ok(()));

    mock_monitor_store
        .expect_get_monitor_height()
        .returning(|| Ok(200));

    let monitor = Monitor::new(mock_indexer, mock_monitor_store, Some(block_height_200), 6)?;

    monitor.tick()?;

    Ok(())
}

#[test]
fn monitor_tx_already_detected() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_monitor_store = MockMonitorStore::new();

    let block_200 = FullBlock {
        height: 200,
        hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap(),
        prev_hash: BlockHash::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap(),
        txs: vec![],
        orphan: false,
    };

    let block_height_200 = block_200.height;

    let tx_to_seen = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    let monitors = vec![(
        TransactionMonitor::Transactions(vec![tx_to_seen.compute_txid()], ExtraData::None),
        180,
    )];

    mock_indexer.expect_tick().returning(move |_| Ok(201));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(block_200.clone())));

    let hash_100 =
        BlockHash::from_str("12efaa3528db3845a859c470a525f1b8b4643b0d561f961ab395a9db778c204d")?;

    let tx_info = TransactionInfo {
        tx: tx_to_seen.clone(),
        block_hash: hash_100,
        orphan: false,
        block_height: 100,
        confirmations: 10,
    };

    mock_indexer
        .expect_get_tx()
        .with(eq(tx_to_seen.compute_txid()))
        .times(1)
        .returning(move |_| Ok(Some(tx_info.clone())));

    // Convert TransactionMonitor to TransactionMonitorType for the mock
    let monitor_types = monitors
        .iter()
        .map(|(monitor, _)| match monitor {
            TransactionMonitor::Transactions(txids, extra_data) => {
                TransactionMonitorType::Transaction(txids[0], extra_data.clone())
            }
            TransactionMonitor::SpendingUTXOTransaction(txid, utxo_index, extra_data) => {
                TransactionMonitorType::SpendingUTXOTransaction(
                    *txid,
                    *utxo_index,
                    extra_data.clone(),
                )
            }
            TransactionMonitor::RskPeginTransaction => TransactionMonitorType::RskPeginTransaction,
        })
        .collect::<Vec<_>>();

    mock_monitor_store
        .expect_get_monitors()
        .with(eq(block_height_200))
        .times(1)
        .returning(move |_| Ok(monitor_types.clone()));

    mock_monitor_store
        .expect_set_monitor_height()
        .returning(|_| Ok(()));

    mock_monitor_store
        .expect_get_monitor_height()
        .returning(|| Ok(200));

    let monitor = Monitor::new(mock_indexer, mock_monitor_store, Some(block_height_200), 6)?;

    monitor.tick()?;

    Ok(())
}

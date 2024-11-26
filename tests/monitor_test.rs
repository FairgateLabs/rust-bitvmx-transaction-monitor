use bitcoin::{absolute::LockTime, BlockHash, Transaction};
use bitcoin_indexer::{indexer::MockIndexerApi, types::TransactionInfo};
use bitvmx_transaction_monitor::{
    bitvmx_store::{BitvmxStore, MockBitvmxStore},
    monitor::Monitor,
    types::{BitvmxInstance, BlockInfo, TxStatus},
};
use mockall::predicate::*;
use std::str::FromStr;

use rand::{thread_rng, RngCore};
use std::env;

fn temp_storage() -> String {
    let dir = env::temp_dir().to_str().unwrap().to_string();
    let mut rng = thread_rng();
    let index = rng.next_u32();
    format!("{}/bitvmx_store_{}", dir, index)
}

#[test]
fn no_instances() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_bitvmx_store = MockBitvmxStore::new();

    let block_100 = 100;

    mock_indexer
        .expect_tick()
        .returning(move |_| Ok(block_100 + 1));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(block_100)));

    // Return an empty bitvmx array
    mock_bitvmx_store
        .expect_get_instances_ready_to_track()
        .with(eq(block_100))
        .times(1)
        .returning(|_| Ok(vec![]));

    // Then we never call update_bitvmx_tx_confirmations
    mock_bitvmx_store.expect_update_news().times(0);

    // Then we never call update_bitvmx_tx_seen
    mock_bitvmx_store.expect_update_instance_tx_seen().times(0);

    let mut monitor = Monitor::new(mock_indexer, mock_bitvmx_store, Some(block_100), 6);

    monitor.tick()?;

    assert_eq!(monitor.get_current_height(), block_100 + 1);

    Ok(())
}

#[test]
fn instance_tx_detected() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_bitvmx_store = MockBitvmxStore::new();

    let block_200 = 200;
    let instance_id = 2;

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
    let instances = vec![BitvmxInstance {
        id: instance_id,
        txs: vec![
            TxStatus {
                tx_id: tx.compute_txid(),
                tx: None,
                block_info: Some(BlockInfo {
                    block_height: 190,
                    block_hash: BlockHash::from_str(
                        "12efaa3528db3845a859c470a525f1b8b4643b0d561f961ab395a9db778c204d",
                    )
                    .unwrap(),
                    is_orphan: false,
                }),
            },
            TxStatus {
                tx_id: tx_to_seen.compute_txid(),
                tx: None,
                block_info: None,
            },
        ],
        start_height: 180,
    }];

    let hash_150 =
        BlockHash::from_str("12efaa3528db3845a859c470a525f1b8b4643b0d561f961ab395a9db778c204d")?;

    let hash_190 =
        BlockHash::from_str("23efda3528db3845a859c470a525f1b8b4643b0d561f961ab395a9db778c204d")?;

    let tx_to_seen_info = TransactionInfo {
        tx: tx_to_seen.clone(),
        block_hash: hash_150,
        orphan: false,
        block_height: 150,
    };

    let tx_info = TransactionInfo {
        tx: tx.clone(),
        block_hash: hash_190,
        orphan: false,
        block_height: 190,
    };

    mock_indexer
        .expect_tick()
        .returning(move |_| Ok(block_200 + 1));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(block_200)));

    mock_bitvmx_store
        .expect_get_instances_ready_to_track()
        .with(eq(block_200))
        .times(1)
        .returning(move |_| Ok(instances.clone()));

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

    // The first time was seen the tx should not call update_bitvmx_tx_confirmations
    mock_bitvmx_store.expect_update_news().times(0);

    // Then call update_bitvmx_tx_seen for the first time
    mock_bitvmx_store
        .expect_update_instance_tx_seen()
        .with(
            eq(instance_id),
            eq(tx_to_seen),
            eq(150),
            eq(hash_150),
            eq(false),
        )
        .times(1)
        .returning(|_, _, _, _, _| Ok(()));

    let mut monitor = Monitor::new(mock_indexer, mock_bitvmx_store, Some(block_200), 6);

    monitor.tick()?;

    assert_eq!(monitor.get_current_height(), block_200 + 1);

    Ok(())
}

#[test]
fn instance_tx_already_detected_increase_confirmation() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_bitvmx_store = MockBitvmxStore::new();

    let block_200 = 200;
    let intance_id = 2;

    let tx_to_seen = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    let instances = vec![BitvmxInstance {
        id: intance_id,
        txs: vec![TxStatus {
            tx_id: tx_to_seen.compute_txid(),
            tx: None,
            block_info: Some(BlockInfo {
                block_height: 200,
                block_hash: BlockHash::from_str(
                    "12efaa3528db3845a859c470a525f1b8b4643b0d561f961ab395a9db778c204d",
                )
                .unwrap(),
                is_orphan: false,
            }),
        }],
        start_height: 180,
    }];

    mock_indexer.expect_tick().returning(move |_| Ok(201));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(block_200)));

    let hash_100 =
        BlockHash::from_str("12efaa3528db3845a859c470a525f1b8b4643b0d561f961ab395a9db778c204d")?;

    let tx_info = TransactionInfo {
        tx: tx_to_seen.clone(),
        block_hash: hash_100,
        orphan: false,
        block_height: 100,
    };
    // Tx was found by the indexer and is already in the blockchain.
    mock_indexer
        .expect_get_tx()
        .with(eq(tx_to_seen.compute_txid()))
        .times(1)
        .returning(move |_| Ok(Some(tx_info.clone())));

    mock_bitvmx_store
        .expect_get_instances_ready_to_track()
        .with(eq(block_200))
        .times(1)
        .returning(move |_| Ok(instances.clone()));

    // Do no Increase confirmations given the block is the same were was found
    mock_bitvmx_store.expect_update_news().times(0);

    // Also the update_bitvmx_tx_seen is not call
    mock_bitvmx_store
        .expect_update_instance_tx_seen()
        .times(0);

    let mut monitor = Monitor::new(mock_indexer, mock_bitvmx_store, Some(block_200), 6);

    monitor.tick()?;

    assert_eq!(monitor.get_current_height(), 201);

    Ok(())
}

#[test]
fn tx_got_caught_in_reorganisation() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let bitvmx_store = BitvmxStore::new_with_path(&temp_storage())?;

    let block_200 = 200;
    let instance_id = 2;

    let tx_to_seen =
        Txid::from_str(&"3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f")
            .unwrap();

    let instances = vec![BitvmxInstance {
        id: instance_id,
        txs: vec![
            TxStatus {
                tx_id: tx_to_seen.clone(),
                tx_hex: None,
                block_info: None,
            },
        ],
        start_height: 180,
    }];

    mock_indexer
    .expect_index_height()
    .returning(move |_| Ok(201));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(block_200)));

    let hash_190 =
        BlockHash::from_str("12efaa3528db3845a859c470a525f1b8b4643b0d561f961ab395a9db778c204d")?;

    let tx_info = TransactionInfo {
        tx_id: tx_to_seen,
        block_hash: hash_190,
        orphan: false,
        block_height: 190,
    };

    let tx_info_2 = TransactionInfo {
        tx_id: tx_to_seen,
        block_hash: hash_190,
        orphan: true,
        block_height: 190,
    };

    mock_indexer
        .expect_get_tx_info()
        .with(eq(tx_to_seen.clone()))
        .times(1)
        .returning(move |_| Ok(Some(tx_info.clone())))
        .once();

    mock_indexer.
        expect_get_tx_info()
        .with(eq(tx_to_seen.clone()))
        .times(1)
        .returning(move |_| Ok(Some(tx_info_2.clone())));
    
    mock_indexer
        .expect_get_tx()
        .times(1)
        .returning(move |_| Ok("0x123".to_string()));

    let mut monitor = Monitor::new(mock_indexer, bitvmx_store, Some(block_200), 6);

    monitor.save_instances_for_tracking(instances)?;
    
    monitor.tick()?;

    assert_eq!(monitor.get_current_height(), 201);

    monitor.tick()?;

    assert_eq!(monitor.get_current_height(), 201);
    
    Ok(())
}

#[test]
fn orphan_tx_has_confirmations_equals_zero() -> Result<(), anyhow::Error>{

    // WIP
    // let mut mock_indexer = MockIndexerApi::new();
    // let bitvmx_store = BitvmxStore::new_with_path(&temp_storage())?;

    // let block_200 = 200;
    // let instance_id = 2;

    // let tx_to_seen =
    //     Txid::from_str(&"3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f")
    //         .unwrap();

    // let instances = vec![BitvmxInstance {
    //     id: instance_id,
    //     txs: vec![
    //         TxStatus {
    //             tx_id: tx_to_seen.clone(),
    //             tx_hex: None,
    //             block_info: None,
    //         },
    //     ],
    //     start_height: 180,
    // }];

    // mock_indexer
    // .expect_index_height()
    // .returning(move |_| Ok(201));

    // mock_indexer
    //     .expect_get_best_block()
    //     .returning(move || Ok(Some(block_200)));

    // let hash_190 =
    //     BlockHash::from_str("12efaa3528db3845a859c470a525f1b8b4643b0d561f961ab395a9db778c204d")?;

    // let tx_info = TransactionInfo {
    //     tx_id: tx_to_seen,
    //     block_hash: hash_190,
    //     orphan: false,
    //     block_height: 190,
    // };

    // let tx_info_2 = TransactionInfo {
    //     tx_id: tx_to_seen,
    //     block_hash: hash_190,
    //     orphan: false,
    //     block_height: 190,
    // };

    // let tx_info_3 = TransactionInfo {
    //     tx_id: tx_to_seen,
    //     block_hash: hash_190,
    //     orphan: true,
    //     block_height: 190,
    // };

    // mock_indexer
    //     .expect_get_tx_info()
    //     .with(eq(tx_to_seen.clone()))
    //     .times(2)
    //     .returning(move |_| Ok(Some(tx_info.clone())));

    // mock_indexer.
    //     expect_get_tx_info()
    //     .with(eq(tx_to_seen.clone()))
    //     .times(1)
    //     .returning(move |_| Ok(Some(tx_info_2.clone())));
    
    // mock_indexer
    //     .expect_get_tx()
    //     .times(1)
    //     .returning(move |_| Ok("0x123".to_string()));

    // let mut monitor = Monitor::new(mock_indexer, bitvmx_store, Some(block_200), 6);

    // monitor.save_instances_for_tracking(instances)?;
    
    // monitor.tick()?;

    // assert_eq!(monitor.get_current_height(), 201);

    // monitor.tick()?;

    // assert_eq!(monitor.get_current_height(), 202);

    // monitor.tick()?;

    // assert_eq!(monitor.get_current_height(), 202);
    
    Ok(())
}

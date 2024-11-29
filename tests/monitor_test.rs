use bitcoin::{
    absolute::{self, LockTime},
    key::{rand, Secp256k1},
    secp256k1::{All, SecretKey},
    transaction, Amount, BlockHash, Network, Transaction, TxOut,
};
use bitcoin::{Address, CompressedPublicKey};
use bitcoin_indexer::{
    indexer::MockIndexerApi,
    types::{FullBlock, TransactionInfo},
};
use bitvmx_transaction_monitor::{
    bitvmx_store::MockBitvmxStore,
    monitor::Monitor,
    types::{BitvmxInstance, BlockInfo, TxStatus},
};
use mockall::predicate::*;
use std::str::FromStr;

#[test]
fn no_instances() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_bitvmx_store = MockBitvmxStore::new();

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

    mock_bitvmx_store
        .expect_get_addresses()
        .returning(|| Ok(vec![]));

    // Return an empty bitvmx array
    mock_bitvmx_store
        .expect_get_instances_ready_to_track()
        .with(eq(block_100_height))
        .times(1)
        .returning(|_| Ok(vec![]));

    // Then we never call update_bitvmx_tx_confirmations
    mock_bitvmx_store.expect_update_instance_news().times(0);

    // Then we never call update_bitvmx_tx_seen
    mock_bitvmx_store.expect_update_instance_tx_seen().times(0);

    let mut monitor = Monitor::new(mock_indexer, mock_bitvmx_store, Some(block_100_height), 6);

    monitor.tick()?;

    assert_eq!(monitor.get_current_height(), block_100_height + 1);

    Ok(())
}

#[test]
fn instance_tx_detected() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_bitvmx_store = MockBitvmxStore::new();

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
        .returning(move |_| Ok(block_height_200 + 1));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(block_200.clone())));

    mock_bitvmx_store
        .expect_get_instances_ready_to_track()
        .with(eq(block_height_200))
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
    mock_bitvmx_store.expect_update_instance_news().times(0);

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

    mock_bitvmx_store
        .expect_get_addresses()
        .returning(|| Ok(vec![]));

    let mut monitor = Monitor::new(mock_indexer, mock_bitvmx_store, Some(block_height_200), 6);

    monitor.tick()?;

    assert_eq!(monitor.get_current_height(), block_height_200 + 1);

    Ok(())
}

#[test]
fn instance_tx_already_detected_increase_confirmation() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_bitvmx_store = MockBitvmxStore::new();

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
        .returning(move || Ok(Some(block_200.clone())));

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
        .with(eq(block_height_200))
        .times(1)
        .returning(move |_| Ok(instances.clone()));

    mock_bitvmx_store
        .expect_get_addresses()
        .returning(|| Ok(vec![]));

    // Do no Increase confirmations given the block is the same were was found
    mock_bitvmx_store.expect_update_instance_news().times(0);

    // Also the update_bitvmx_tx_seen is not call
    mock_bitvmx_store.expect_update_instance_tx_seen().times(0);

    let mut monitor = Monitor::new(mock_indexer, mock_bitvmx_store, Some(block_height_200), 6);

    monitor.tick()?;

    assert_eq!(monitor.get_current_height(), 201);

    Ok(())
}

#[test]
fn detect_address_in_tx() -> Result<(), anyhow::Error> {
    let to = get_address();
    let to_clone = to.clone();

    // The spend output is locked to a key controlled by the receiver.
    let spend = TxOut {
        value: Amount::default(),
        script_pubkey: to.script_pubkey(),
    };

    // The transaction we want to sign and broadcast.
    let unsigned_tx = Transaction {
        version: transaction::Version::TWO,  // Post BIP-68.
        lock_time: absolute::LockTime::ZERO, // Ignore the locktime.
        input: vec![],                       // Input goes into index 0.
        output: vec![spend],                 // cpfp output is always index 0.
    };

    let mock_indexer = MockIndexerApi::new();
    let mock_bitvmx_store = MockBitvmxStore::new();
    let monitor = Monitor::new(mock_indexer, mock_bitvmx_store, None, 6);
    let matched = monitor.address_exist_in_output(to_clone, &unsigned_tx);

    assert!(matched);

    Ok(())
}

fn get_address() -> Address {
    let secp: Secp256k1<All> = Secp256k1::new();
    let sk = SecretKey::new(&mut rand::thread_rng());
    let pk = bitcoin::PublicKey::new(sk.public_key(&secp));
    let compressed = CompressedPublicKey::try_from(pk).unwrap();
    let to = Address::p2wpkh(&compressed, Network::Bitcoin);
    to
}

use bitcoin::Txid;
use bitcoin_indexer::indexer::MockIndexerApi;
use bitvmx_transaction_monitor::{
    bitvmx_store::MockBitvmxStore,
    monitor::Monitor,
    types::{BitvmxInstance, TxStatus},
};
use mockall::predicate::*;
use std::str::FromStr;

#[test]
fn no_instances() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_bitvmx_store = MockBitvmxStore::new();

    let block_100 = 100;

    mock_indexer
        .expect_index_height()
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
    mock_bitvmx_store
        .expect_update_instance_tx_confirmations()
        .times(0);

    // Then we never call update_bitvmx_tx_seen
    mock_bitvmx_store.expect_update_instance_tx_seen().times(0);

    let mut monitor = Monitor::new(mock_indexer, mock_bitvmx_store, Some(block_100));

    monitor.detect_instances()?;

    assert_eq!(monitor.get_current_height(), block_100 + 1);

    Ok(())
}

#[test]
fn instance_tx_detected() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_bitvmx_store = MockBitvmxStore::new();

    let block_200 = 200;
    let intance_id = 2;

    let tx_to_seen =
        Txid::from_str(&"3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f")
            .unwrap();
    let txid = Txid::from_str(&"e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b")
        .unwrap();

    let instances = vec![BitvmxInstance {
        id: intance_id,
        txs: vec![
            TxStatus {
                tx_id: txid,
                tx_hex: None,
                tx_was_seen: true,
                height_tx_seen: Some(190),
                confirmations: 10,
            },
            TxStatus {
                tx_id: tx_to_seen.clone(),
                tx_hex: None,
                tx_was_seen: false,
                height_tx_seen: None,
                confirmations: 0,
            },
        ],
        start_height: 180,
    }];

    mock_indexer
        .expect_index_height()
        .returning(move |_| Ok(block_200 + 1));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(block_200)));

    mock_indexer
        .expect_get_tx()
        .times(1)
        .returning(move |_| Ok("0x123".to_string()));

    mock_bitvmx_store
        .expect_get_instances_ready_to_track()
        .with(eq(block_200))
        .times(1)
        .returning(move |_| Ok(instances.clone()));

    // Tx was found by the indexer and is already in the blockchain.
    mock_indexer
        .expect_tx_exists()
        .with(eq(tx_to_seen.clone()))
        .times(1)
        .returning(|_| Ok((true, Some(150))));

    // The first time was seen the tx should not call update_bitvmx_tx_confirmations
    mock_bitvmx_store
        .expect_update_instance_tx_confirmations()
        .times(0);

    // Then call update_bitvmx_tx_seen for the first time
    mock_bitvmx_store
        .expect_update_instance_tx_seen()
        .with(eq(intance_id), eq(tx_to_seen), eq(150), eq("0x123"))
        .times(1)
        .returning(|_, _, _, _| Ok(()));

    let mut monitor = Monitor::new(mock_indexer, mock_bitvmx_store, Some(block_200));

    monitor.detect_instances()?;

    assert_eq!(monitor.get_current_height(), block_200 + 1);

    Ok(())
}

#[test]
fn instance_tx_already_detected_increase_confirmation() -> Result<(), anyhow::Error> {
    let mut mock_indexer = MockIndexerApi::new();
    let mut mock_bitvmx_store = MockBitvmxStore::new();

    let block_200 = 200;
    let intance_id = 2;

    let tx_to_seen =
        Txid::from_str(&"3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f")
            .unwrap();

    let confirmations = 1;
    let instances = vec![BitvmxInstance {
        id: intance_id,
        txs: vec![TxStatus {
            tx_id: tx_to_seen.clone(),
            tx_hex: None,
            tx_was_seen: true,
            height_tx_seen: Some(200),
            confirmations,
        }],
        start_height: 180,
    }];

    mock_indexer
        .expect_index_height()
        .returning(move |_| Ok(201));

    mock_indexer
        .expect_get_best_block()
        .returning(move || Ok(Some(block_200)));

    // Tx was found by the indexer and is already in the blockchain.
    mock_indexer
        .expect_tx_exists()
        .with(eq(tx_to_seen.clone()))
        .times(1)
        .returning(|_| Ok((true, Some(100))));

    mock_bitvmx_store
        .expect_get_instances_ready_to_track()
        .with(eq(block_200))
        .times(1)
        .returning(move |_| Ok(instances.clone()));

    // Do no Increase confirmations given the block is the same were was found
    mock_bitvmx_store
        .expect_update_instance_tx_confirmations()
        .times(0);

    // Also the update_bitvmx_tx_seen is not call
    mock_bitvmx_store.expect_update_instance_tx_seen().times(0);

    let mut monitor = Monitor::new(mock_indexer, mock_bitvmx_store, Some(block_200));

    monitor.detect_instances()?;

    assert_eq!(monitor.get_current_height(), 201);

    Ok(())
}

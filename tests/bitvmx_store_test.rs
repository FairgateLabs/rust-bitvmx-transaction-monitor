use bitcoin::Txid;
use bitvmx_transaction_monitor::{
    bitvmx_store::{BitvmxApi, BitvmxStore},
    types::{BitvmxInstance, BitvmxTxData},
};

use std::str::FromStr;

fn get_mock_bitvmx_instances_already_stated() -> Vec<BitvmxInstance> {
    let txid = Txid::from_str(&"e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b")
        .unwrap();

    let txid2 = Txid::from_str(&"3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f")
        .unwrap();

    let instances = vec![BitvmxInstance {
        id: 2,
        txs: vec![
            BitvmxTxData {
                tx_id: txid,
                tx_hex: None,
                tx_was_seen: true,
                height_tx_seen: Some(190),
                confirmations: 10,
            },
            BitvmxTxData {
                tx_id: txid2,
                tx_hex: None,
                tx_was_seen: false,
                height_tx_seen: None,
                confirmations: 0,
            },
        ],
        start_height: 180,
    }];

    instances
}

fn get_mock_bitvmx_instances_no_started() -> Vec<BitvmxInstance> {
    let txid = Txid::from_str(&"6fe2aef3426a6b9d4b9a774b58dafe7b736e7a67998ab54b53cf6e82df1a28b8")
        .unwrap();

    let txid2 = Txid::from_str(&"3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f")
        .unwrap();

    let instances = vec![BitvmxInstance {
        id: 3,
        txs: vec![
            BitvmxTxData {
                tx_id: txid,
                tx_hex: None,
                tx_was_seen: false,
                height_tx_seen: None,
                confirmations: 0,
            },
            BitvmxTxData {
                tx_id: txid2,
                tx_hex: None,
                tx_was_seen: false,
                height_tx_seen: None,
                confirmations: 0,
            },
        ],
        start_height: 1000,
    }];

    instances
}

fn get_all_mock_bitvmx_instances() -> Vec<BitvmxInstance> {
    let mut all_instances = Vec::new();

    all_instances.extend(get_mock_bitvmx_instances_already_stated());
    all_instances.extend(get_mock_bitvmx_instances_no_started());

    all_instances
}

#[test]
fn get_bitvmx_instances() -> Result<(), anyhow::Error> {
    let file_path = String::from("test_outputs/test_one");
    let bitvmx_store = BitvmxStore::new_with_path(&file_path)?;

    let instances: Vec<BitvmxInstance> = get_all_mock_bitvmx_instances();

    bitvmx_store.save_instances(&instances)?;

    let instances = bitvmx_store.get_pending_instances(0)?;

    assert_eq!(instances.len(), 0);

    let instances = bitvmx_store.get_pending_instances(50)?;
    assert_eq!(instances.len(), 0);

    let instances = bitvmx_store.get_pending_instances(2000)?;

    assert_eq!(instances.len(), 2);

    Ok(())
}

#[test]
fn update_bitvmx_tx() -> Result<(), anyhow::Error> {
    let block_300 = 300;

    let txid = Txid::from_str(&"e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b")
        .unwrap();

    let tx_id_not_seen =
        Txid::from_str(&"3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f")
            .unwrap();

    let instances = vec![BitvmxInstance {
        id: 2,
        txs: vec![
            BitvmxTxData {
                tx_id: txid,
                tx_hex: None,
                tx_was_seen: true,
                height_tx_seen: Some(190),
                confirmations: 10,
            },
            BitvmxTxData {
                tx_id: tx_id_not_seen,
                tx_hex: None,
                tx_was_seen: false,
                height_tx_seen: None,
                confirmations: 0,
            },
        ],
        start_height: 180,
    }];

    let bitvmx_store = BitvmxStore::new_with_path("test_outputs/test_two")?;
    bitvmx_store.save_instances(&instances)?;

    // Getting from a block in the future
    let instances = bitvmx_store.get_pending_instances(100000)?;
    assert_eq!(instances.len(), 1);

    // Tx 2 was seen in block_300
    bitvmx_store.update_instance_tx_seen(instances[0].id, &tx_id_not_seen, block_300, "")?;

    let instances = bitvmx_store.get_pending_instances(100000)?;

    // Once a transaction is seen in a block, the number of confirmations is 1 at that point.
    assert_eq!(instances[0].txs[1].confirmations, 1);

    // First block seen should be block_300
    assert_eq!(instances[0].txs[1].height_tx_seen, Some(block_300));

    let block_400 = 400;
    //Update again but in another block
    bitvmx_store.update_instance_tx_confirmations(instances[0].id, &tx_id_not_seen, block_400)?;

    // This will return instances are not confirmed > 6
    let data = bitvmx_store.get_pending_instances(100000)?;

    // There is not pending instances.
    assert_eq!(data.len(), 1);

    // Now get all the data, with the finished instances
    let instances = bitvmx_store.get_instances_for_tracking()?;

    // First block seen should be block_300, never change
    assert_eq!(instances[0].txs[1].height_tx_seen, Some(block_300));

    // Once a transaction is seen in a block, the number of confirmations is last_block_height - firt_height_seen.
    assert_eq!(instances[0].txs[1].confirmations, block_400 - block_300);

    Ok(())
}

#[test]
fn update_bitvmx_tx_confirmation() -> Result<(), anyhow::Error> {
    let txid = Txid::from_str(&"e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b")
        .unwrap();

    let instances = vec![BitvmxInstance {
        id: 2,
        txs: vec![BitvmxTxData {
            tx_id: txid,
            tx_hex: None,
            tx_was_seen: true,
            height_tx_seen: Some(190),
            confirmations: 1,
        }],
        start_height: 180,
    }];

    let bitvmx_store = BitvmxStore::new_with_path("test_outputs/test_three")?;
    bitvmx_store.save_instances(&instances)?;

    let instances = bitvmx_store.get_pending_instances(100000)?;

    // Tx 2 was seen in block_300
    bitvmx_store.update_instance_tx_confirmations(instances[0].id, &txid, 1000)?;

    //The instance is not pending anymore
    let instances = bitvmx_store.get_pending_instances(instances[0].id)?;
    assert_eq!(instances.len(), 0);

    // Check the confirmations
    let instances = bitvmx_store.get_instances_for_tracking()?;
    assert_eq!(instances[0].txs[0].confirmations, 1000 - 190);

    Ok(())
}

#[test]
fn save_tx_for_tranking() -> Result<(), anyhow::Error> {
    let tx_id = Txid::from_str(&"e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b")
        .unwrap();

    let tx_id_to_add =
        Txid::from_str(&"8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115bec")
            .unwrap();

    let instances = vec![BitvmxInstance {
        id: 2,
        txs: vec![BitvmxTxData {
            tx_id: tx_id,
            tx_hex: None,
            tx_was_seen: true,
            height_tx_seen: Some(190),
            confirmations: 1,
        }],
        start_height: 180,
    }];

    let bitvmx_store = BitvmxStore::new_with_path("test_outputs/test_four")?;
    bitvmx_store.save_instances(&instances)?;
    bitvmx_store.save_transaction(instances[0].id, tx_id_to_add)?;

    let instances = bitvmx_store.get_pending_instances(100000)?;

    assert_eq!(instances[0].txs.len(), 2);

    // Verify the properties of the newly added transaction
    let new_tx = instances[0]
        .txs
        .iter()
        .find(|tx| tx.tx_id == tx_id_to_add)
        .unwrap();
    assert_eq!(new_tx.tx_hex, None);
    assert_eq!(new_tx.tx_was_seen, false);
    assert_eq!(new_tx.height_tx_seen, None);
    assert_eq!(new_tx.confirmations, 0);

    Ok(())
}

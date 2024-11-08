use bitcoin::Txid;
use bitvmx_transaction_monitor::{
    bitvmx_store::{BitvmxApi, BitvmxStore},
    types::{BitvmxInstance, TxStatus},
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
            TxStatus {
                tx_id: txid,
                tx_hex: None,
                height_tx_seen: Some(190),
                confirmations: 10,
            },
            TxStatus {
                tx_id: txid2,
                tx_hex: None,
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
            TxStatus {
                tx_id: txid,
                tx_hex: None,
                height_tx_seen: None,
                confirmations: 0,
            },
            TxStatus {
                tx_id: txid2,
                tx_hex: None,
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

    let instances = bitvmx_store.get_instances_ready_to_track(0)?;

    assert_eq!(instances.len(), 0);

    let instances = bitvmx_store.get_instances_ready_to_track(50)?;
    assert_eq!(instances.len(), 0);

    let instances = bitvmx_store.get_instances_ready_to_track(2000)?;

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
            TxStatus {
                tx_id: txid,
                tx_hex: None,
                height_tx_seen: Some(190),
                confirmations: 10,
            },
            TxStatus {
                tx_id: tx_id_not_seen,
                tx_hex: None,
                height_tx_seen: None,
                confirmations: 0,
            },
        ],
        start_height: 180,
    }];

    let bitvmx_store = BitvmxStore::new_with_path("test_outputs/test_two")?;
    bitvmx_store.save_instances(&instances)?;

    // Getting from a block in the future
    let instances = bitvmx_store.get_instances_ready_to_track(100000)?;
    assert_eq!(instances.len(), 1);

    // Tx 2 was seen in block_300
    bitvmx_store.update_instance_tx_seen(instances[0].id, &tx_id_not_seen, block_300, "")?;

    let instances = bitvmx_store.get_instances_ready_to_track(100000)?;

    // Once a transaction is seen in a block, the number of confirmations is 1 at that point.
    assert_eq!(instances[0].txs[1].confirmations, 1);

    // First block seen should be block_300
    assert_eq!(instances[0].txs[1].height_tx_seen, Some(block_300));

    let block_400 = 400;
    //Update again but in another block
    bitvmx_store.update_instance_tx_confirmations(instances[0].id, &tx_id_not_seen, block_400)?;

    // This will return instances are not confirmed > 6
    let data = bitvmx_store.get_instances_ready_to_track(100000)?;

    // There is not pending instances.
    assert_eq!(data.len(), 1);

    // Now get all the data, with the finished instances
    let instances = bitvmx_store.get_all_instances_for_tracking()?;

    // First block seen should be block_300, never change
    assert_eq!(instances[0].txs[1].height_tx_seen, Some(block_300));

    // Once a transaction is seen in a block, the number of confirmations is last_block_height - firt_height_seen.
    assert_eq!(instances[0].txs[1].confirmations, block_400 - block_300 + 1);

    Ok(())
}

#[test]
fn update_bitvmx_tx_confirmation() -> Result<(), anyhow::Error> {
    let txid = Txid::from_str(&"e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b")
        .unwrap();

    let instances = vec![BitvmxInstance {
        id: 2,
        txs: vec![TxStatus {
            tx_id: txid,
            tx_hex: None,
            height_tx_seen: Some(190),
            confirmations: 1,
        }],
        start_height: 180,
    }];

    let bitvmx_store = BitvmxStore::new_with_path("test_outputs/test_three")?;
    bitvmx_store.save_instances(&instances)?;

    let instances = bitvmx_store.get_instances_ready_to_track(100000)?;

    // Tx 2 was seen in block_300
    bitvmx_store.update_instance_tx_confirmations(instances[0].id, &txid, 1000)?;

    //The instance is not pending anymore
    let instances = bitvmx_store.get_instances_ready_to_track(instances[0].id)?;
    assert_eq!(instances.len(), 0);

    // Check the confirmations
    let instances = bitvmx_store.get_all_instances_for_tracking()?;
    assert_eq!(instances[0].txs[0].confirmations, 1000 - 189);

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
        txs: vec![TxStatus {
            tx_id: tx_id,
            tx_hex: None,
            height_tx_seen: Some(190),
            confirmations: 1,
        }],
        start_height: 180,
    }];

    let bitvmx_store = BitvmxStore::new_with_path("test_outputs/test_four")?;
    bitvmx_store.save_instances(&instances)?;
    bitvmx_store.save_transaction(instances[0].id, &tx_id_to_add)?;

    let instances = bitvmx_store.get_instances_ready_to_track(100000)?;

    assert_eq!(instances[0].txs.len(), 2);

    // Verify the properties of the newly added transaction
    let new_tx = instances[0]
        .txs
        .iter()
        .find(|tx| tx.tx_id == tx_id_to_add)
        .unwrap();
    assert_eq!(new_tx.tx_hex, None);
    assert_eq!(new_tx.height_tx_seen, None);
    assert_eq!(new_tx.confirmations, 0);

    Ok(())
}

#[test]
fn get_instance_news() -> Result<(), anyhow::Error> {
    let bitvmx_store = BitvmxStore::new_with_path("test_outputs/test_five")?;
    let tx_id = Txid::from_str(&"e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b")
        .unwrap();

    //remove all the news
    bitvmx_store.acknowledge_instance_tx_news(2, &tx_id)?;

    let instance_news = bitvmx_store.get_instance_news()?;

    //assert There is no news
    assert_eq!(instance_news.len(), 0);

    // Add a new instance with one tx
    let instances = vec![BitvmxInstance {
        id: 2,
        txs: vec![TxStatus {
            tx_id: tx_id,
            tx_hex: None,
            height_tx_seen: Some(190),
            confirmations: 1,
        }],
        start_height: 180,
    }];

    // Add the instance to the store
    bitvmx_store.save_instances(&instances)?;

    // update the tx with a confirmation
    bitvmx_store.update_instance_tx_confirmations(instances[0].id, &tx_id, 1000)?;

    //get and check news
    let instance_news = bitvmx_store.get_instance_news()?;
    assert_eq!(instance_news.len(), 1);
    //check news is the instance with id 2 and tx with id tx_id
    assert_eq!(instance_news[0].0, 2);
    assert_eq!(instance_news[0].1.len(), 1);
    assert!(instance_news[0].1.contains(&tx_id));

    // update the tx with a confirmation in another block
    bitvmx_store.update_instance_tx_seen(instances[0].id, &tx_id, 1000, "123")?;

    // Get the news
    let instance_news = bitvmx_store.get_instance_news()?;

    assert_eq!(instance_news.len(), 1);
    //check news is the instance with id 2 and tx with id tx_id
    assert_eq!(instance_news[0].0, 2);
    assert_eq!(instance_news[0].1.len(), 1);
    //assert!(instance_news[0].1[0], &tx_id);

    //remove news
    bitvmx_store.acknowledge_instance_tx_news(2, &tx_id)?;

    let instance_news = bitvmx_store.get_instance_news()?;
    assert_eq!(instance_news.len(), 0);

    //update tx with a confirmation in another block
    bitvmx_store.update_instance_tx_confirmations(instances[0].id, &tx_id, 1000)?;

    // Get the news
    let instance_news = bitvmx_store.get_instance_news()?;
    assert_eq!(instance_news.len(), 1);
    //check news is the instance with id 2 and tx with id tx_id
    assert_eq!(instance_news[0].0, 2);
    assert_eq!(instance_news[0].1.len(), 1);
    assert!(instance_news[0].1.contains(&tx_id));

    Ok(())
}

#[test]
fn get_instance_news_multiple_instances() -> Result<(), anyhow::Error> {
    let bitvmx_store = BitvmxStore::new_with_path("test_outputs/test_multiple_instances")?;
    // Create two instances with one transaction each
    let tx_id_1 =
        Txid::from_str("e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b")?;
    let tx_id_2 =
        Txid::from_str("8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115bec")?;
    let tx_id_3 =
        Txid::from_str("8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115bec")?;

    //remove all the news
    bitvmx_store.acknowledge_instance_tx_news(1, &tx_id_1)?;
    bitvmx_store.acknowledge_instance_tx_news(1, &tx_id_3)?;
    bitvmx_store.acknowledge_instance_tx_news(2, &tx_id_2)?;

    let instances = vec![
        BitvmxInstance {
            id: 1,
            txs: vec![
                TxStatus {
                    tx_id: tx_id_1,
                    tx_hex: None,
                    height_tx_seen: None,
                    confirmations: 0,
                },
                TxStatus {
                    tx_id: tx_id_3,
                    tx_hex: None,
                    height_tx_seen: None,
                    confirmations: 0,
                },
            ],
            start_height: 100,
        },
        BitvmxInstance {
            id: 2,
            txs: vec![TxStatus {
                tx_id: tx_id_2,
                tx_hex: None,
                height_tx_seen: None,
                confirmations: 0,
            }],
            start_height: 200,
        },
    ];

    // Save instances
    bitvmx_store.save_instances(&instances)?;

    // Verify no news initially
    let instance_news = bitvmx_store.get_instance_news()?;
    assert_eq!(instance_news.len(), 0);

    // Update transactions in both instances
    bitvmx_store.update_instance_tx_seen(1, &tx_id_3, 100, "tx_hex_3")?;
    bitvmx_store.update_instance_tx_seen(1, &tx_id_1, 150, "tx_hex_1")?;
    bitvmx_store.update_instance_tx_seen(2, &tx_id_2, 250, "tx_hex_2")?;
    // update each tx with confirms
    bitvmx_store.update_instance_tx_confirmations(1, &tx_id_1, 1000)?;
    bitvmx_store.update_instance_tx_confirmations(2, &tx_id_2, 2000)?;
    bitvmx_store.update_instance_tx_confirmations(1, &tx_id_3, 1000)?;

    // Get and verify news
    let instance_news = bitvmx_store.get_instance_news()?;
    assert_eq!(instance_news.len(), 2);

    // Acknowledge news for instance 1
    bitvmx_store.acknowledge_instance_tx_news(2, &tx_id_2)?;

    // Verify only news for instance 2 remains
    let instance_news = bitvmx_store.get_instance_news()?;

    assert_eq!(instance_news.len(), 1);
    assert_eq!(instance_news[0].0, 1);
    assert_eq!(instance_news[0].1.len(), 2);
    assert!(instance_news[0].1.contains(&tx_id_1));
    assert!(instance_news[0].1.contains(&tx_id_3));

    // Acknowledge news for instance 2
    bitvmx_store.acknowledge_instance_tx_news(1, &tx_id_1)?;
    bitvmx_store.acknowledge_instance_tx_news(1, &tx_id_3)?;

    // Verify no news remains
    let instance_news = bitvmx_store.get_instance_news()?;
    assert_eq!(instance_news.len(), 0);

    Ok(())
}

#[test]
fn remove_instance() -> Result<(), anyhow::Error> {
    let bitvmx_store = BitvmxStore::new_with_path("test_outputs/test_remove_instances")?;
    // Create two instances with one transaction each
    let tx_id_1 =
        Txid::from_str("e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b")?;
    let tx_id_2 =
        Txid::from_str("8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115bec")?;

    let instances = vec![BitvmxInstance {
        id: 1,
        txs: vec![
            TxStatus {
                tx_id: tx_id_1,
                tx_hex: None,
                height_tx_seen: None,
                confirmations: 0,
            },
            TxStatus {
                tx_id: tx_id_2,
                tx_hex: None,
                height_tx_seen: None,
                confirmations: 0,
            },
        ],
        start_height: 100,
    }];

    // Save instances
    bitvmx_store.save_instances(&instances)?;

    Ok(())
}

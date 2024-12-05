use bitcoin::{absolute::LockTime, Transaction, Txid};
use bitvmx_transaction_monitor::{
    bitvmx_store::{BitvmxApi, BitvmxStore},
    types::{BitvmxInstance, TransactionStatus},
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
            TransactionStatus {
                tx_id: txid,
                tx: None,
            },
            TransactionStatus {
                tx_id: txid2,
                tx: None,
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
            TransactionStatus {
                tx_id: txid,
                tx: None,
            },
            TransactionStatus {
                tx_id: txid2,
                tx: None,
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
fn save_tx_for_tranking() -> Result<(), anyhow::Error> {
    let tx_id = Txid::from_str(&"e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b")
        .unwrap();

    let tx_id_to_add =
        Txid::from_str(&"8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115bec")
            .unwrap();

    let instances = vec![BitvmxInstance {
        id: 2,
        txs: vec![TransactionStatus {
            tx_id: tx_id,
            tx: None,
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
    assert_eq!(new_tx.tx, None);

    Ok(())
}

#[test]
fn get_instance_news() -> Result<(), anyhow::Error> {
    let bitvmx_store = BitvmxStore::new_with_path("test_outputs/test_five")?;
    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };

    //remove all the news
    bitvmx_store.acknowledge_instance_tx_news(2, &tx.compute_txid())?;

    let instance_news = bitvmx_store.get_instance_news()?;

    //assert There is no news
    assert_eq!(instance_news.len(), 0);

    // Add a new instance with one tx
    let instances = vec![BitvmxInstance {
        id: 2,
        txs: vec![TransactionStatus {
            tx_id: tx.compute_txid(),
            tx: None,
            // block_info: Some(BlockInfo {
            //     block_height: 190,
            //     block_hash: BlockHash::from_str(
            //         "12efaa3528db3845a859c470a525f1b8b4643b0d561f961ab395a9db778c204d",
            //     )
            //     .unwrap(),
            //     is_orphan: false,
            // }),
        }],
        start_height: 180,
    }];

    // Add the instance to the store
    bitvmx_store.save_instances(&instances)?;

    // update the tx with a confirmation
    bitvmx_store.update_instance_news(instances[0].id, tx.compute_txid())?;

    //get and check news
    let instance_news = bitvmx_store.get_instance_news()?;
    assert_eq!(instance_news.len(), 1);
    //check news is the instance with id 2 and tx with id tx_id
    assert_eq!(instance_news[0].0, 2);
    assert_eq!(instance_news[0].1.len(), 1);
    assert!(instance_news[0].1.contains(&tx.compute_txid()));

    // update the tx with a confirmation in another block
    bitvmx_store.update_instance_news(instances[0].id, tx.compute_txid())?;

    // Get the news
    let instance_news = bitvmx_store.get_instance_news()?;

    assert_eq!(instance_news.len(), 1);
    //check news is the instance with id 2 and tx with id tx_id
    assert_eq!(instance_news[0].0, 2);
    assert_eq!(instance_news[0].1.len(), 1);
    //assert!(instance_news[0].1[0], &tx_id);

    //remove news
    bitvmx_store.acknowledge_instance_tx_news(2, &tx.compute_txid())?;

    let instance_news = bitvmx_store.get_instance_news()?;
    assert_eq!(instance_news.len(), 0);

    //update tx with a confirmation in another block
    bitvmx_store.update_instance_news(instances[0].id, tx.compute_txid())?;

    // Get the news
    let instance_news = bitvmx_store.get_instance_news()?;
    assert_eq!(instance_news.len(), 1);
    //check news is the instance with id 2 and tx with id tx_id
    assert_eq!(instance_news[0].0, 2);
    assert_eq!(instance_news[0].1.len(), 1);
    assert!(instance_news[0].1.contains(&tx.compute_txid()));

    Ok(())
}

#[test]
fn get_instance_news_multiple_instances() -> Result<(), anyhow::Error> {
    let bitvmx_store = BitvmxStore::new_with_path("test_outputs/test_multiple_instances")?;
    // Create two instances with one transaction each
    let tx_1 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195600).unwrap(),
        input: vec![],
        output: vec![],
    };
    let tx_2 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195601).unwrap(),
        input: vec![],
        output: vec![],
    };
    let tx_3 = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::from_time(1653195602).unwrap(),
        input: vec![],
        output: vec![],
    };

    //remove all the news
    bitvmx_store.acknowledge_instance_tx_news(1, &tx_1.compute_txid())?;
    bitvmx_store.acknowledge_instance_tx_news(1, &tx_3.compute_txid())?;
    bitvmx_store.acknowledge_instance_tx_news(2, &tx_2.compute_txid())?;

    let instances = vec![
        BitvmxInstance {
            id: 1,
            txs: vec![
                TransactionStatus {
                    tx_id: tx_1.compute_txid(),
                    tx: None,
                },
                TransactionStatus {
                    tx_id: tx_3.compute_txid(),
                    tx: None,
                },
            ],
            start_height: 100,
        },
        BitvmxInstance {
            id: 2,
            txs: vec![TransactionStatus {
                tx_id: tx_2.compute_txid(),
                tx: None,
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
    bitvmx_store.update_instance_news(1, tx_3.compute_txid())?;
    bitvmx_store.update_instance_news(1, tx_1.compute_txid())?;
    bitvmx_store.update_instance_news(2, tx_2.compute_txid())?;
    // update each tx with confirms
    bitvmx_store.update_instance_news(1, tx_1.compute_txid())?;
    bitvmx_store.update_instance_news(2, tx_2.compute_txid())?;
    bitvmx_store.update_instance_news(1, tx_3.compute_txid())?;

    // Get and verify news
    let instance_news = bitvmx_store.get_instance_news()?;
    assert_eq!(instance_news.len(), 2);

    // Acknowledge news for instance 1
    bitvmx_store.acknowledge_instance_tx_news(2, &tx_2.compute_txid())?;

    // Verify only news for instance 2 remains
    let instance_news = bitvmx_store.get_instance_news()?;

    assert_eq!(instance_news.len(), 1);
    assert_eq!(instance_news[0].0, 1);
    assert_eq!(instance_news[0].1.len(), 2);
    assert!(instance_news[0].1.contains(&tx_1.compute_txid()));
    assert!(instance_news[0].1.contains(&tx_3.compute_txid()));

    // Acknowledge news for instance 2
    bitvmx_store.acknowledge_instance_tx_news(1, &tx_1.compute_txid())?;
    bitvmx_store.acknowledge_instance_tx_news(1, &tx_3.compute_txid())?;

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
            TransactionStatus {
                tx_id: tx_id_1,
                tx: None,
            },
            TransactionStatus {
                tx_id: tx_id_2,
                tx: None,
            },
        ],
        start_height: 100,
    }];

    // Save instances
    bitvmx_store.save_instances(&instances)?;

    Ok(())
}

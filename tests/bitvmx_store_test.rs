use bitcoin::Txid;
use bitvmx_transaction_monitor::{
    bitvmx_store::{BitvmxApi, BitvmxStore},
    types::{BitvmxInstance, BitvmxTxData},
};

use std::{
    fs::{self, File},
    io::Write,
    str::FromStr,
};

fn get_mock_bitvmx_instances_already_stated() -> Vec<BitvmxInstance> {
    let txid = Txid::from_str(&"e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b")
        .unwrap();

    let txid2 = Txid::from_str(&"3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f")
        .unwrap();

    let instances = vec![BitvmxInstance {
        id: 2,
        txs: vec![
            BitvmxTxData {
                txid: txid,
                tx_hex: None,
                tx_was_seen: true,
                height_tx_seen: Some(190),
                confirmations: 10,
            },
            BitvmxTxData {
                txid: txid2,
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
                txid: txid,
                tx_hex: None,
                tx_was_seen: false,
                height_tx_seen: None,
                confirmations: 0,
            },
            BitvmxTxData {
                txid: txid2,
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

fn setup_bitvmx_instances(
    file_path: &str,
    instances: Vec<BitvmxInstance>,
) -> Result<(), anyhow::Error> {
    let json_data = serde_json::to_string_pretty(&instances)?;
    let mut file = File::create(file_path)?;
    file.write_all(json_data.as_bytes())?;

    Ok(())
}

#[test]
fn get_bitvmx_instances() -> Result<(), anyhow::Error> {
    let file_path = String::from("test1.json");
    let instances: Vec<BitvmxInstance> = get_all_mock_bitvmx_instances();
    setup_bitvmx_instances(&file_path, instances)?;

    let bitvmx_store = BitvmxStore::new(&file_path)?;
    let data = bitvmx_store.get_pending_instances(0)?;

    assert_eq!(data.len(), 0);

    let data = bitvmx_store.get_pending_instances(50)?;
    assert_eq!(data.len(), 0);

    let data = bitvmx_store.get_pending_instances(200)?;
    assert_eq!(data.len(), 1);

    let data = bitvmx_store.get_pending_instances(2000)?;
    assert_eq!(data.len(), 2);

    fs::remove_file(file_path)?;
    Ok(())
}

#[test]
fn update_bitvmx_tx() -> Result<(), anyhow::Error> {
    let file_path = String::from("test2.json");

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
                txid: txid,
                tx_hex: None,
                tx_was_seen: true,
                height_tx_seen: Some(190),
                confirmations: 10,
            },
            BitvmxTxData {
                txid: tx_id_not_seen,
                tx_hex: None,
                tx_was_seen: false,
                height_tx_seen: None,
                confirmations: 0,
            },
        ],
        start_height: 180,
    }];

    setup_bitvmx_instances(&file_path, instances)?;

    let bitvmx_store = BitvmxStore::new(&file_path)?;

    // Getting from a block in the future
    let data = bitvmx_store.get_pending_instances(100000)?;

    assert_eq!(data.len(), 1);

    // Tx 2 was seen in block_300
    bitvmx_store.update_instance_tx_seen(data[0].id, &tx_id_not_seen, block_300, "")?;

    let data = bitvmx_store.get_pending_instances(100000)?;

    // Once a transaction is seen in a block, the number of confirmations is 1 at that point.
    assert_eq!(data[0].txs[1].confirmations, 1);

    // First block seen should be block_300
    assert_eq!(data[0].txs[1].height_tx_seen, Some(block_300));

    let block_400 = 400;
    //Update again but in another block
    bitvmx_store.update_instance_tx_confirmations(data[0].id, &tx_id_not_seen, block_400)?;

    // This will return instances are not confirmed > 6
    let data = bitvmx_store.get_pending_instances(100000)?;

    // There is not pending instances.
    assert_eq!(data.len(), 0);

    // Now get all the data, with the finished instances
    let data = bitvmx_store.get_data()?;

    // First block seen should be block_300, never change
    assert_eq!(data[0].txs[1].height_tx_seen, Some(block_300));

    // Once a transaction is seen in a block, the number of confirmations is last_block_height - firt_height_seen.
    assert_eq!(data[0].txs[1].confirmations, block_400 - block_300);

    fs::remove_file(file_path)?;

    Ok(())
}

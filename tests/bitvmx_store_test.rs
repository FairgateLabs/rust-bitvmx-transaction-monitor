use rust_bitcoin_tx_monitor::{
    stores::bitvmx_store::{BitvmxApi, BitvmxStore},
    types::{BitvmxInstance, BitvmxTxData},
};

use std::{
    fs::{self, File},
    io::Write,
};

fn get_mock_bitvmx_instances_finished() -> Vec<BitvmxInstance> {
    let instances = vec![BitvmxInstance {
        id: 1,
        txs: vec![
            BitvmxTxData {
                txid: "4b8e07b98e23ab6a7e8ff2d2a4846c607d97ab3e51d6a6896a1eeb0d0b1fc63a"
                    .to_string(),
                tx_was_seen: true,
                fist_height_tx_seen: Some(95),
                confirmations: 10,
            },
            BitvmxTxData {
                txid: "4a5e1e4baab89f3a32518a88b87bedd5a19d2b260bba7e560f7a28a4a6a6e4f4"
                    .to_string(),
                tx_was_seen: true,
                fist_height_tx_seen: Some(100),
                confirmations: 10,
            },
        ],
        start_height: 40,
        finished: true,
    }];

    instances
}

fn get_mock_bitvmx_instances_already_stated() -> Vec<BitvmxInstance> {
    let instances = vec![BitvmxInstance {
        id: 2,
        txs: vec![
            BitvmxTxData {
                txid: "e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b"
                    .to_string(),
                tx_was_seen: true,
                fist_height_tx_seen: Some(190),
                confirmations: 10,
            },
            BitvmxTxData {
                txid: "3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f"
                    .to_string(),
                tx_was_seen: false,
                fist_height_tx_seen: None,
                confirmations: 0,
            },
        ],
        start_height: 180,
        finished: false,
    }];

    instances
}

fn get_mock_bitvmx_instances_no_started() -> Vec<BitvmxInstance> {
    let instances = vec![BitvmxInstance {
        id: 3,
        txs: vec![
            BitvmxTxData {
                txid: "6fe2aef3426a6b9d4b9a774b58dafe7b736e7a67998ab54b53cf6e82df1a28b8"
                    .to_string(),
                tx_was_seen: false,
                fist_height_tx_seen: None,
                confirmations: 0,
            },
            BitvmxTxData {
                txid: "5a675f5d26b09cf9a41d93f5a12d2e5730c8e4cdbb1fbb7e20c4c7881a8e1f9d"
                    .to_string(),
                tx_was_seen: false,
                fist_height_tx_seen: None,
                confirmations: 0,
            },
        ],
        start_height: 1000,
        finished: false,
    }];

    instances
}

fn get_all_mock_bitvmx_instances() -> Vec<BitvmxInstance> {
    let mut all_instances = Vec::new();

    all_instances.extend(get_mock_bitvmx_instances_finished());
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

    let mut bitvmx_store = BitvmxStore::new(&file_path)?;
    let data = bitvmx_store.get_pending_bitvmx_instances(0)?;

    assert_eq!(data.len(), 0);

    let data = bitvmx_store.get_pending_bitvmx_instances(50)?;
    assert_eq!(data.len(), 0);

    let data = bitvmx_store.get_pending_bitvmx_instances(200)?;
    assert_eq!(data.len(), 1);

    let data = bitvmx_store.get_pending_bitvmx_instances(2000)?;
    assert_eq!(data.len(), 2);

    fs::remove_file(file_path)?;
    Ok(())
}

#[test]
fn update_bitvmx_tx() -> Result<(), anyhow::Error> {
    let file_path = String::from("test2.json");

    let tx_id_not_seen = "3a3f8d147abf0b9b9d25b07de7a16a4db96bda3e474ceab4c4f9e8e107d5b02f";
    let block_300 = 300;
    let instances = vec![BitvmxInstance {
        id: 2,
        txs: vec![
            BitvmxTxData {
                txid: "e9b7ad71b2f0bbce7165b5ab4a3c1e17e9189f2891650e3b7d644bb7e88f200b"
                    .to_string(),
                tx_was_seen: true,
                fist_height_tx_seen: Some(190),
                confirmations: 10,
            },
            BitvmxTxData {
                txid: tx_id_not_seen.to_string(),
                tx_was_seen: false,
                fist_height_tx_seen: None,
                confirmations: 0,
            },
        ],
        start_height: 180,
        finished: false,
    }];

    setup_bitvmx_instances(&file_path, instances)?;

    let mut bitvmx_store = BitvmxStore::new(&file_path)?;

    // Getting from a block in the future
    let data = bitvmx_store.get_pending_bitvmx_instances(100000)?;

    assert_eq!(data.len(), 1);

    // Tx 2 was seen in block_300
    bitvmx_store.update_bitvmx_tx_seen(data[0].id, &tx_id_not_seen.to_string(), block_300)?;

    let data = bitvmx_store.get_pending_bitvmx_instances(100000)?;

    //All txns were seen but are not confirm with more than 6 blocks
    assert_eq!(data[0].finished, false);

    // Once a transaction is seen in a block, the number of confirmations is 1 at that point.
    assert_eq!(data[0].txs[1].confirmations, 1);

    // First block seen should be block_300
    assert_eq!(data[0].txs[1].fist_height_tx_seen, Some(block_300));

    let block_400 = 400;
    //Update again but in another block
    bitvmx_store.update_bitvmx_tx_confirmations(
        data[0].id,
        &tx_id_not_seen.to_string(),
        block_400,
    )?;

    // This will return instances are not finished
    let data = bitvmx_store.get_pending_bitvmx_instances(100000)?;

    // There is not pending instances.
    assert_eq!(data.len(), 0);

    // Now get all the data, with the finished instances
    let data = bitvmx_store.get_data()?;

    // First block seen should be block_300, never change
    assert_eq!(data[0].txs[1].fist_height_tx_seen, Some(block_300));

    // Once a transaction is seen in a block, the number of confirmations is last_block_height - firt_height_seen.
    assert_eq!(data[0].txs[1].confirmations, block_400 - block_300);

    //All txns were seen and confirmed with more than 6 blocks
    assert_eq!(data[0].finished, true);

    fs::remove_file(file_path)?;

    Ok(())
}

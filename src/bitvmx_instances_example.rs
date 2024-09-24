use crate::types::{BitvmxInstance, TxStatus};
use bitcoin::Txid;
use std::str::FromStr;

pub fn get_bitvmx_instances_example() -> Vec<BitvmxInstance> {
    vec![
        BitvmxInstance {
            id: 1,
            txs: vec![
                TxStatus {
                    tx_id: Txid::from_str(
                        "8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115be1",
                    )
                    .unwrap(),
                    tx_hex: None,
                    tx_was_seen: false,
                    height_tx_seen: None,
                    confirmations: 0,
                },
                TxStatus {
                    tx_id: Txid::from_str(
                        "8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115bea",
                    )
                    .unwrap(),
                    tx_hex: None,
                    tx_was_seen: false,
                    height_tx_seen: None,
                    confirmations: 0,
                },
            ],
            start_height: 1,
        },
        BitvmxInstance {
            id: 2,
            txs: vec![
                TxStatus {
                    tx_id: Txid::from_str(
                        "8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115bed",
                    )
                    .unwrap(),
                    tx_hex: None,
                    tx_was_seen: false,
                    height_tx_seen: None,
                    confirmations: 0,
                },
                TxStatus {
                    tx_id: Txid::from_str(
                        "8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115bec",
                    )
                    .unwrap(),
                    tx_hex: None,
                    tx_was_seen: false,
                    height_tx_seen: None,
                    confirmations: 0,
                },
            ],
            start_height: 180,
        },
        BitvmxInstance {
            id: 3,
            txs: vec![
                TxStatus {
                    tx_id: Txid::from_str(
                        "3c2d0b8d3052af2423f7c93450473aeacfb47e7aa3f0b0ae63f3e240a15496b1",
                    )
                    .unwrap(),
                    tx_hex: None,
                    tx_was_seen: false,
                    height_tx_seen: None,
                    confirmations: 0,
                },
                TxStatus {
                    tx_id: Txid::from_str(
                        "3c2d0b8d3052af2423f7c93450473aeacfb47e7aa3f0b0ae63f3e240a15496b2",
                    )
                    .unwrap(),
                    tx_hex: None,
                    tx_was_seen: false,
                    height_tx_seen: None,
                    confirmations: 0,
                },
            ],
            start_height: 1000,
        },
    ]
}

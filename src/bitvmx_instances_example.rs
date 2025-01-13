use crate::types::{BitvmxInstance, TransactionStore};
use bitcoin::Txid;
use std::str::FromStr;
use uuid::Uuid;

pub fn get_bitvmx_instances_example() -> Vec<BitvmxInstance> {
    vec![
        BitvmxInstance {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            txs: vec![
                TransactionStore {
                    tx_id: Txid::from_str(
                        "8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115be1",
                    )
                    .unwrap(),
                    tx: None,
                },
                TransactionStore {
                    tx_id: Txid::from_str(
                        "8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115bea",
                    )
                    .unwrap(),
                    tx: None,
                },
            ],
            start_height: 1,
        },
        BitvmxInstance {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            txs: vec![
                TransactionStore {
                    tx_id: Txid::from_str(
                        "8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115bed",
                    )
                    .unwrap(),
                    tx: None,
                },
                TransactionStore {
                    tx_id: Txid::from_str(
                        "8904aba41b91cc59eea5f5767bf8fbd5f8d861629885267379cae615c8115bec",
                    )
                    .unwrap(),
                    tx: None,
                },
            ],
            start_height: 180,
        },
        BitvmxInstance {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap(),
            txs: vec![
                TransactionStore {
                    tx_id: Txid::from_str(
                        "3c2d0b8d3052af2423f7c93450473aeacfb47e7aa3f0b0ae63f3e240a15496b1",
                    )
                    .unwrap(),
                    tx: None,
                },
                TransactionStore {
                    tx_id: Txid::from_str(
                        "3c2d0b8d3052af2423f7c93450473aeacfb47e7aa3f0b0ae63f3e240a15496b2",
                    )
                    .unwrap(),
                    tx: None,
                },
            ],
            start_height: 1000,
        },
    ]
}

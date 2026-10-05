//! Fixtures shared by the unit tests, and by the integration tests that drive a monitor against regtest.

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use bitcoin::hashes::Hash;
use bitcoin::{absolute, transaction, BlockHash, Network, OutPoint, Transaction, TxOut, Txid};
use bitcoin_indexer::indexer::Indexer;
use bitcoin_indexer::IndexerType;
use bitvmx_bitcoin_rpc::bitcoin_client::BitcoinClient;
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use bitvmx_bitcoin_rpc::types::BlockHeight;
use storage_backend::storage::Storage;
use storage_backend::storage_config::StorageConfig;

use crate::types::{BlockRef, FullBlock};

/// A txid that is unique per `seed`, for tests that only need to tell two of them apart.
pub fn txid(seed: u8) -> Txid {
    Txid::from_byte_array([seed; 32])
}

pub fn block_hash(seed: u8) -> BlockHash {
    BlockHash::from_byte_array([seed; 32])
}

pub fn outpoint(seed: u8, vout: u32) -> OutPoint {
    OutPoint::new(txid(seed), vout)
}

pub fn block_ref(height: BlockHeight, hash_seed: u8) -> BlockRef {
    BlockRef {
        height,
        hash: block_hash(hash_seed),
    }
}

/// A transaction that is unique per `seed`, spending the given outpoints and paying the given outputs.
pub fn tx(seed: u32, inputs: Vec<OutPoint>, outputs: Vec<TxOut>) -> Transaction {
    Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::from_consensus(seed),
        input: inputs
            .into_iter()
            .map(|previous_output| bitcoin::TxIn {
                previous_output,
                ..Default::default()
            })
            .collect(),
        output: outputs,
    }
}

/// A block holding the given transactions, with a hash that follows its height unless one is given.
pub fn block(height: BlockHeight, hash_seed: u8, txs: Vec<Transaction>) -> FullBlock {
    FullBlock {
        height,
        hash: block_hash(hash_seed),
        prev_hash: block_hash(hash_seed.wrapping_sub(1)),
        txs,
        estimated_fee_rate: 0,
    }
}

/// An indexer wired to a port nothing listens on. Building one reads nothing from a node, so it is enough for
/// the tests that only exercise the rules and never reach the chain. A test that does reach it fails on the
/// connection rather than on a type, which is the price of the monitor sharing one handle with its indexer.
pub fn offline_indexer(storage: Rc<Storage>) -> Rc<IndexerType> {
    let config = RpcConfig::new(
        Network::Regtest,
        "http://127.0.0.1:1".to_string(),
        "user".to_string(),
        "password".to_string(),
        String::new(),
    );
    let client = BitcoinClient::new_from_config(&config).expect("test bitcoin client");

    Rc::new(Indexer::new(client, storage, None).expect("test indexer"))
}

/// A database backed by a fresh directory under the system temp folder, so tests never share state.
pub fn temp_storage() -> Rc<Storage> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let unique = format!(
        "bitvmx_monitor_test_{}_{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let path = std::env::temp_dir().join(unique);
    let _ = std::fs::remove_dir_all(&path);

    let config = StorageConfig::new(path.to_string_lossy().to_string(), None);
    Rc::new(Storage::new(&config).expect("test storage"))
}

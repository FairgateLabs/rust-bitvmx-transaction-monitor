use bitcoin::Txid;
// use bitcoin::hash_types::Txid;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TxStatus {
    pub tx_id: Txid,

    pub tx_hex: Option<String>,

    //If transaction was seen in the blockchain then true
    pub tx_was_seen: bool,

    //Firt block height seen in the blockchain data
    // TODO: this should have more information about the block. block hash.
    pub height_tx_seen: Option<BlockHeight>,

    // Number of blocks that have passed since the transaction was identified in Bitcoin.
    pub confirmations: u32,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BitvmxInstance {
    //bitvmx instance id
    pub id: InstanceId,

    //bitvmx linked transactions data + speed up transactions data
    pub txs: Vec<TxStatus>,

    //First height to start searching the bitvmx instance in the blockchain
    pub start_height: BlockHeight,
}

pub type BlockHeight = u32;
pub type InstanceId = u32;

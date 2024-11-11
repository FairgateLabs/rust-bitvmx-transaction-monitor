use bitcoin::{BlockHash, Txid};
// use bitcoin::hash_types::Txid;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TxStatus {
    pub tx_id: Txid,

    pub tx_hex: Option<String>,

    //Firt block height seen in the blockchain data
    // TODO: this should have more information about the block. block hash
    pub block_info: Option<BlockInfo>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct BlockInfo {
    pub block_height: BlockHeight,
    pub block_hash: BlockHash,
    pub is_orphan: bool, 
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
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct InstanceData {
    //bitvmx instance id
    pub instance_id: InstanceId,

    //bitvmx linked transactions data
    pub txs: Vec<Txid>,
}

pub type BlockHeight = u32;
pub type InstanceId = u32;

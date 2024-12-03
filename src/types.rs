use bitcoin::{BlockHash, Transaction, Txid};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TxStatus {
    pub tx_id: Txid,
    pub tx: Option<Transaction>,
    pub block_info: Option<BlockInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub struct AddressStatus {
    pub tx: Option<Transaction>,
    pub block_info: Option<BlockInfo>,
}

impl TxStatus {
    pub fn was_seen(&self) -> bool {
        self.block_info.is_some()
    }

    pub fn get_confirmations(&self, current_height: u32) -> u32 {
        if !self.was_seen() {
            return 0;
        }

        let height_seen = self.block_info.as_ref().unwrap().block_height;

        if current_height >= height_seen {
            current_height - height_seen + 1
        } else {
            //TODO: Review this, this could implies that if indexer runs again in a checkpoint older, transaction status should no be in the database.
            // This is a special case where either the indexer is backward or there is a reorganization
            // that reorganizes blocks and the winning chain is shorter, leaving out the transaction that was seen.
            // Default case for now is 0
            0
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TxStatusResponse {
    pub tx_id: Txid,
    pub tx: Option<Transaction>,
    pub block_info: Option<BlockInfo>,
    pub confirmations: u32,
}

impl TxStatusResponse {
    pub fn is_confirmed(&self) -> bool {
        self.block_info.is_some() && self.confirmations > 0
    }

    pub fn is_orphan(&self) -> bool {
        //Orphan should have:
        //  block_info because it was mined time before.
        //  confirmation == 0 , this is just a validation, orphan should be moved as confirmation 0.
        //  is_orphan = true
        self.block_info.is_some()
            && self.confirmations == 0
            && self.block_info.as_ref().unwrap().is_orphan
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct BlockInfo {
    pub block_height: BlockHeight,
    pub block_hash: BlockHash,
    pub is_orphan: bool,
}

pub struct BlockAgragatedInfo {
    pub block_height: BlockHeight,
    pub block_hash: BlockHash,
    pub confirmations: u32,
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

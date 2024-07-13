use bitcoin::hash_types::Txid;

pub struct Operation {
    pub id: u64,
    pub transaction_ids: Vec<Txid>,
    pub start_height: BlockHeight,
    pub last_verified_height: Option<BlockHeight>,
    pub tx_was_seen: bool,
    pub block_tx_seen: Option<BlockHeight>,
    pub block_confirmations: Option<u32>,
}

pub type BlockHeight = u64;

pub struct TxData {}

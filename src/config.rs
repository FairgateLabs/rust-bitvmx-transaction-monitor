use bitvmx_bitcoin_rpc::{rpc_config::RpcConfig, types::BlockHeight};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct ConfigMonitor {
    /// Path to the file containing BitVMX instances to monitor. This file is used when running 
    /// the monitor directly with `cargo run` rather than as a library.
    pub bitvmx_file_path: String,

    /// Bitcoin height to start indexing from
    pub checkpoint_height: Option<BlockHeight>,

    pub confirmation_threshold: u32,

    pub db_file_path: String,

    pub rpc: RpcConfig,

    pub log_level: Option<String>,
}

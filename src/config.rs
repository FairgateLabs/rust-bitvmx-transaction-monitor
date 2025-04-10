use bitvmx_bitcoin_rpc::{rpc_config::RpcConfig, types::BlockHeight};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct ConfigMonitor {
    pub database: DatabaseConfig,
    pub rpc: RpcConfig,
    pub monitor: MonitorConfig,
}

#[derive(Deserialize, Debug)]
pub struct DatabaseConfig {
    pub file_path: String,
}

#[derive(Deserialize, Debug)]
pub struct MonitorConfig {
    pub checkpoint_height: BlockHeight,
    pub confirmation_threshold: u32,
}

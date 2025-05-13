use bitvmx_bitcoin_rpc::{rpc_config::RpcConfig, types::BlockHeight};
use serde::Deserialize;
use storage_backend::storage_config::StorageConfig;

#[derive(Deserialize, Debug)]
pub struct ConfigMonitor {
    pub storage: StorageConfig,
    pub bitcoin: RpcConfig,
    pub monitor: MonitorConfig,
}

#[derive(Deserialize, Debug)]
pub struct MonitorConfig {
    pub checkpoint_height: BlockHeight,
    pub confirmation_threshold: u32,
}

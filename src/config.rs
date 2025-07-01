use crate::constants::{DEFAULT_CONFIRMATION_THRESHOLD, DEFAULT_MAX_MONITORING_CONFIRMATIONS};
use bitcoin_indexer::config::IndexerConstants;
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use serde::Deserialize;
use storage_backend::storage_config::StorageConfig;

#[derive(Deserialize, Debug)]
pub struct MonitorConfig {
    pub storage: StorageConfig,
    pub bitcoin: RpcConfig,
    pub constants: Option<MonitorConstants>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MonitorConstants {
    pub confirmation_threshold: u32,
    pub max_monitoring_confirmations: u32,
    pub indexer_constants: Option<IndexerConstants>,
}

impl MonitorConstants {
    pub fn new(
        confirmation_threshold: u32,
        max_monitoring_confirmations: u32,
        indexer_constants: Option<IndexerConstants>,
    ) -> Self {
        Self {
            confirmation_threshold,
            max_monitoring_confirmations,
            indexer_constants,
        }
    }
}

impl Default for MonitorConstants {
    fn default() -> Self {
        MonitorConstants {
            confirmation_threshold: DEFAULT_CONFIRMATION_THRESHOLD,
            max_monitoring_confirmations: DEFAULT_MAX_MONITORING_CONFIRMATIONS,
            indexer_constants: Some(IndexerConstants::default()),
        }
    }
}

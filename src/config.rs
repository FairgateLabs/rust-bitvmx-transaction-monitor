use crate::settings::{DEFAULT_CONFIRMATION_THRESHOLD, DEFAULT_MAX_MONITORING_CONFIRMATIONS};
use bitcoin_indexer::config::IndexerSettings;
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use serde::Deserialize;
use storage_backend::storage_config::StorageConfig;

#[derive(Deserialize, Debug)]
pub struct MonitorConfig {
    pub storage: StorageConfig,
    pub bitcoin: RpcConfig,
    pub settings: Option<MonitorSettings>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MonitorSettings {
    pub confirmation_threshold: u32,
    pub max_monitoring_confirmations: u32,
    pub indexer_settings: Option<IndexerSettings>,
}

impl MonitorSettings {
    pub fn new(
        confirmation_threshold: u32,
        max_monitoring_confirmations: u32,
        indexer_settings: Option<IndexerSettings>,
    ) -> Self {
        Self {
            confirmation_threshold,
            max_monitoring_confirmations,
            indexer_settings,
        }
    }
}

impl Default for MonitorSettings {
    fn default() -> Self {
        MonitorSettings {
            confirmation_threshold: DEFAULT_CONFIRMATION_THRESHOLD,
            max_monitoring_confirmations: DEFAULT_MAX_MONITORING_CONFIRMATIONS,
            indexer_settings: Some(IndexerSettings::default()),
        }
    }
}

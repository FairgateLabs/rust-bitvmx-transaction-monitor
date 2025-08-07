use crate::settings::{DEFAULT_CONFIRMATION_THRESHOLD, DEFAULT_MAX_MONITORING_CONFIRMATIONS};
use bitcoin_indexer::config::IndexerSettings;
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use serde::Deserialize;
use storage_backend::storage_config::StorageConfig;

#[derive(Deserialize, Debug)]
pub struct MonitorConfig {
    pub storage: StorageConfig,
    pub bitcoin: RpcConfig,
    pub settings: Option<MonitorSettingsConfig>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MonitorSettingsConfig {
    pub confirmation_threshold: Option<u32>,
    pub max_monitoring_confirmations: Option<u32>,
    pub indexer_settings: Option<IndexerSettings>,
}

impl Default for MonitorSettingsConfig {
    fn default() -> Self {
        Self {
            confirmation_threshold: Some(DEFAULT_CONFIRMATION_THRESHOLD),
            max_monitoring_confirmations: Some(DEFAULT_MAX_MONITORING_CONFIRMATIONS),
            indexer_settings: Some(IndexerSettings::default()),
        }
    }
}

impl From<MonitorSettingsConfig> for MonitorSettings {
    fn from(monitor_settings: MonitorSettingsConfig) -> Self {
        MonitorSettings {
            confirmation_threshold: monitor_settings
                .confirmation_threshold
                .unwrap_or(DEFAULT_CONFIRMATION_THRESHOLD),
            max_monitoring_confirmations: monitor_settings
                .max_monitoring_confirmations
                .unwrap_or(DEFAULT_MAX_MONITORING_CONFIRMATIONS),
            indexer_settings: monitor_settings.indexer_settings,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct MonitorSettings {
    pub confirmation_threshold: u32,
    pub max_monitoring_confirmations: u32,
    pub indexer_settings: Option<IndexerSettings>,
}

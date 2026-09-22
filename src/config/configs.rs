use crate::{config::settings::*, errors::MonitorError};
use bitcoin_indexer::config::IndexerSettings;
use bitvmx_bitcoin_rpc::rpc_config::RpcConfig;
use bitvmx_settings::settings::load_config_file;
use serde::Deserialize;
use storage_backend::storage_config::StorageConfig;

macro_rules! ensure {
    ($cond:expr, $msg:expr) => {
        if !($cond) {
            return Err(MonitorError::InvalidConfiguration($msg.to_string()));
        }
    };
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)] // Enforce fields.
pub struct MonitorConfig {
    pub storage: StorageConfig,
    pub rpc: RpcConfig,

    #[serde(default)]
    pub settings: MonitorSettingsConfig,
}

impl MonitorConfig {
    pub fn load_config(path: &str) -> Result<Self, MonitorError> {
        let config = load_config_file::<Self>(Some(path.to_string()))
            .map_err(|e| MonitorError::InvalidConfiguration(e.to_string()))?;
        MonitorSettings::from(config.settings.clone()).validate()?;

        Ok(config)
    }
}

/// The settings as they are read from a file, where anything left out takes its default.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)] // Enforce fields.
pub struct MonitorSettingsConfig {
    #[serde(default)]
    pub max_monitoring_confirmations: Option<u32>,

    #[serde(default)]
    pub indexer_settings: Option<IndexerSettings>,
}

impl Default for MonitorSettingsConfig {
    fn default() -> Self {
        Self {
            max_monitoring_confirmations: Some(DEFAULT_MAX_MONITORING_CONFIRMATIONS),
            indexer_settings: Some(IndexerSettings::default()),
        }
    }
}

impl From<MonitorSettingsConfig> for MonitorSettings {
    fn from(monitor_settings: MonitorSettingsConfig) -> Self {
        MonitorSettings {
            max_monitoring_confirmations: monitor_settings
                .max_monitoring_confirmations
                .unwrap_or(DEFAULT_MAX_MONITORING_CONFIRMATIONS),
            indexer_settings: monitor_settings.indexer_settings,
        }
    }
}

/// The settings the monitor runs with, with every default already resolved.
#[derive(Deserialize, Debug, Clone)]
pub struct MonitorSettings {
    pub max_monitoring_confirmations: u32,
    pub indexer_settings: Option<IndexerSettings>,
}

impl MonitorSettings {
    /// Validates the settings that can be checked without touching the chain.
    pub fn validate(&self) -> Result<(), MonitorError> {
        ensure!(
            self.max_monitoring_confirmations >= MIN_MAX_MONITORING_CONFIRMATIONS,
            "max_monitoring_confirmations must be at least 2 blocks, so a reorg right after the first news is reported"
        );

        let indexer_settings = self.indexer_settings.clone().unwrap_or_default();

        ensure!(
            indexer_settings.retention_depth >= self.max_monitoring_confirmations,
            "retention_depth must be at least max_monitoring_confirmations, so the block of a watched transaction is never deleted while it is watched"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The defaults pass validation, and both bounds are enforced.
    #[test]
    fn validate() {
        let defaults = MonitorSettings::from(MonitorSettingsConfig::default());
        assert!(defaults.validate().is_ok());
        assert_eq!(
            defaults.max_monitoring_confirmations,
            DEFAULT_MAX_MONITORING_CONFIRMATIONS
        );

        for confirmations in 0..MIN_MAX_MONITORING_CONFIRMATIONS {
            let settings = MonitorSettings {
                max_monitoring_confirmations: confirmations,
                indexer_settings: None,
            };
            assert!(matches!(
                settings.validate().unwrap_err(),
                MonitorError::InvalidConfiguration(_)
            ));
        }

        // A window shorter than the confirmations being monitored is rejected.
        let settings = MonitorSettings {
            max_monitoring_confirmations: 50,
            indexer_settings: Some(IndexerSettings::new(49, true)),
        };
        assert!(matches!(
            settings.validate().unwrap_err(),
            MonitorError::InvalidConfiguration(_)
        ));

        // The same depth as the confirmations being monitored is enough.
        let settings = MonitorSettings {
            max_monitoring_confirmations: 50,
            indexer_settings: Some(IndexerSettings::new(50, true)),
        };
        assert!(settings.validate().is_ok());
    }

    // Settings left out take their defaults, and a setting that no longer exists fails to parse.
    #[test]
    fn parse() {
        let config: MonitorSettingsConfig =
            serde_json::from_str(r#"{"max_monitoring_confirmations": 6}"#).unwrap();
        assert_eq!(config.max_monitoring_confirmations, Some(6));
        assert!(config.indexer_settings.is_none());

        let err = serde_json::from_str::<MonitorSettingsConfig>(r#"{"checkpoint_height": 10}"#)
            .unwrap_err();
        assert!(err.to_string().contains("checkpoint_height"));
    }

    // The development config loads and validates, and a missing file is an invalid configuration.
    #[test]
    fn load_config() {
        let config = MonitorConfig::load_config("config/monitor_config.yaml").unwrap();
        assert!(MonitorSettings::from(config.settings).validate().is_ok());

        let err = MonitorConfig::load_config("config/does_not_exist.yaml").unwrap_err();
        assert!(matches!(err, MonitorError::InvalidConfiguration(_)));
    }
}

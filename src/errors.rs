use bitcoin_indexer::errors::IndexerError;
use bitvmx_bitcoin_rpc::errors::BitcoinClientError;
use storage_backend::error::StorageError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MonitorError {
    #[error("Error with Indexer: {0}")]
    IndexerError(#[from] IndexerError),

    #[error("Error with Internal Storage: {0}")]
    StorageError(#[from] StorageError),

    #[error("Bitcoin Client Error: {0}")]
    BitcoinClientError(#[from] BitcoinClientError),

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),

    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),

    #[error("Invalid confirmation trigger: requested {0}, max allowed {1}")]
    InvalidConfirmationTrigger(u32, u32),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
}

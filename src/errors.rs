use bitcoin_indexer::errors::IndexerError;
use bitvmx_bitcoin_rpc::errors::BitcoinClientError;
use storage_backend::error::StorageError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MonitorError {
    #[error("Error with Indexer: {0}")]
    IndexerError(#[from] IndexerError),

    #[error("Error with Monitor Store: {0}")]
    MonitorStoreError(#[from] MonitorStoreError),

    #[error("Bitcoin Client Error: {0}")]
    BitcoinClientError(#[from] BitcoinClientError),

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),
}

#[derive(Error, Debug)]
pub enum MonitorStoreError {
    #[error("Error with Internal Storage: {0}")]
    InternalStorageError(#[from] StorageError),

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),

    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),
}

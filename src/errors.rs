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

    #[error("Invalid confirmation trigger {0}: it must be at least the finality of {1} and at most the maximum of {2}")]
    InvalidConfirmationTrigger(u32, u32, u32),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    /// A parameter that the target it was registered with can never use.
    #[error("Invalid subscription: {0}")]
    InvalidSubscription(String),

    /// Something the monitor guarantees about its own state turned out not to hold. A bug to find and fix.
    #[error("Invariant violated: {0}")]
    InvariantViolation(String),
}

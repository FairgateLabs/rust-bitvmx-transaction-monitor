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

impl MonitorError {
    /// True when the error came from the Bitcoin node client, here or inside the indexer.
    pub fn is_bitcoin_client_error(&self) -> bool {
        match self {
            MonitorError::BitcoinClientError(_) => true,
            MonitorError::IndexerError(e) => e.is_bitcoin_client_error(),
            _ => false,
        }
    }

    /// True when the error came from the storage backend, here or inside the indexer.
    pub fn is_storage_error(&self) -> bool {
        match self {
            MonitorError::StorageError(_) => true,
            MonitorError::IndexerError(e) => e.is_storage_error(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_classification() {
        let client = MonitorError::from(BitcoinClientError::InvalidHeight);
        assert!(client.is_bitcoin_client_error());
        assert!(!client.is_storage_error());

        let storage = MonitorError::from(StorageError::WriteError);
        assert!(storage.is_storage_error());
        assert!(!storage.is_bitcoin_client_error());

        // Errors wrapped by the indexer keep their classification.
        let indexer_client =
            MonitorError::from(IndexerError::from(BitcoinClientError::InvalidHeight));
        assert!(indexer_client.is_bitcoin_client_error());
        let indexer_storage = MonitorError::from(IndexerError::from(StorageError::WriteError));
        assert!(indexer_storage.is_storage_error());

        let other = MonitorError::from(IndexerError::NotSynced);
        assert!(!other.is_bitcoin_client_error());
        assert!(!other.is_storage_error());
    }
}

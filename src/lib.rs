pub mod config;
#[doc(hidden)]
pub mod core; // Not part of the API: the monitor's state is written only through Monitor. It stays reachable for the benches.
pub mod errors;
pub mod monitor;
#[doc(hidden)]
pub mod test_utils; // Fixtures for the tests and benches, not part of the API.
pub mod types;

pub use bitcoin_indexer::errors::IndexerError;
pub use bitcoin_indexer::types::TransactionStatus;

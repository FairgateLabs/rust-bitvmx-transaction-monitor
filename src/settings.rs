/// The maximum number of confirmations to monitor for a transaction.
/// After this number of confirmations, the monitor will stop tracking the transaction for further updates.
pub const DEFAULT_MAX_MONITORING_CONFIRMATIONS: u32 = 100;

/// The default number of confirmations required for a transaction to be considered final.
/// This is the minimum number of blocks that must be mined on top of a transaction's block before it is considered Finalized.
pub const DEFAULT_CONFIRMATION_THRESHOLD: u32 = 6;

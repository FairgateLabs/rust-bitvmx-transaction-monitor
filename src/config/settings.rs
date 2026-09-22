// Default Monitor constants

/// Number of confirmations a transaction is monitored for. Once a transaction reaches it the monitor stops watching it,
/// so it is also the reorg depth the monitor can still report on.
pub const DEFAULT_MAX_MONITORING_CONFIRMATIONS: u32 = 100;

/// A transaction must stay monitored for at least one block after its first news. With fewer, a transaction is dropped
/// as soon as it is first confirmed and a reorg that removes it right afterwards is never reported.
pub const MIN_MAX_MONITORING_CONFIRMATIONS: u32 = 2;

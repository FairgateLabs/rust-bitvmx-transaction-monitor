//! # Helper Module
//!
//! This module provides utility functions for transaction validation and detection.
//! It includes functions for matching output patterns and checking UTXO spending.

use bitcoin::script::Instruction;
use bitcoin::{BlockHash, OutPoint, Script, Transaction};

use crate::types::{
    MonitorEntry, NewsAck, OutputPatternFilter, OutputPatternMonitor, SpendingUtxoMonitor,
    SpendingUtxoMonitorEntry, TransactionMonitor, TransactionMonitorEntry,
};

/// Extracts pushed data from a Bitcoin script.
///
/// This function iterates through script instructions and collects all pushed byte data,
/// which is useful for extracting OP_RETURN data.
///
/// # Arguments
/// * `script` - The Bitcoin script to extract data from
///
/// # Returns
/// A vector of byte vectors containing all pushed data from the script
pub fn extract_output_data(script: &Script) -> Vec<Vec<u8>> {
    // Iterate over script instructions to find pushed data
    let instructions = script.instructions_minimal();
    let mut result = Vec::new();

    for inst in instructions.flatten() {
        if let Instruction::PushBytes(data) = inst {
            result.push(data.as_bytes().to_vec());
        }
    }

    result
}

/// Returns `true` if `tx` matches the given output pattern filter:
/// - If `filter.max_outputs` is set, the transaction must not exceed that many outputs.
/// - The output at `filter.output_index` must be an OP_RETURN whose pushed data starts
///   with `filter.tag`.
pub fn matches_output_pattern(tx: &Transaction, filter: &OutputPatternFilter) -> bool {
    if let Some(max) = filter.max_outputs {
        if tx.output.len() > max {
            return false;
        }
    }

    if let Some(output) = tx.output.get(filter.output_index) {
        if output.script_pubkey.is_op_return() {
            let data = extract_output_data(&output.script_pubkey);
            if let Some(first) = data.first() {
                return first.starts_with(filter.tag.as_slice());
            }
        }
    }

    false
}

/// Checks if a transaction spends a specific UTXO.
///
/// # Arguments
/// * `tx` - The transaction to check
/// * `outpoint` - The UTXO being checked
///
/// # Returns
/// `true` if the transaction spends the specified UTXO, `false` otherwise
pub fn is_spending_output(tx: &Transaction, outpoint: OutPoint) -> bool {
    tx.input
        .iter()
        .any(|input| input.previous_output == outpoint)
}

impl NewsAck {
    pub fn new(block_hash: BlockHash, acknowledged: bool) -> Self {
        Self {
            block_hash,
            acknowledged,
        }
    }
}

impl MonitorEntry {
    pub fn new(
        context: String,
        confirmation_trigger: Option<u32>,
        search_in_mempool: bool,
    ) -> Self {
        Self {
            context,
            confirmation_trigger,
            search_in_mempool,
        }
    }
}

impl TransactionMonitor {
    /// Adds a subscription to this transaction, or replaces the one under the same context.
    /// `first_check_done` is true when the caller already knows where the transaction is.
    pub fn add_or_replace(&mut self, entry: MonitorEntry, first_check_done: bool) {
        match self
            .entries
            .iter_mut()
            .find(|e| e.entry.context == entry.context)
        {
            // Only the parameters change.
            Some(existing) => existing.entry = entry,
            // A new subscription.
            None => self.entries.push(TransactionMonitorEntry {
                entry,
                notified_block_hash: None,
                first_check_done,
            }),
        }
    }
}

impl SpendingUtxoMonitor {
    /// Adds a subscription to the spending of this UTXO, or replaces the one under the same context.
    pub fn add_or_replace(&mut self, entry: MonitorEntry) {
        match self
            .entries
            .iter_mut()
            .find(|e| e.entry.context == entry.context)
        {
            // Only the parameters change, and a subscription that already asked about the past stays checked.
            Some(existing) => existing.entry = entry,
            // A new subscription has not asked whether the UTXO was already spent.
            None => self.entries.push(SpendingUtxoMonitorEntry {
                entry,
                first_check_done: false,
            }),
        }
    }
}

impl OutputPatternMonitor {
    /// Adds a subscription to this output pattern, or replaces the one under the same context.
    pub fn add_or_replace(&mut self, entry: MonitorEntry) {
        match self.entries.iter().position(|e| e.context == entry.context) {
            Some(pos) => self.entries[pos] = entry,
            None => self.entries.push(entry),
        }
    }
}

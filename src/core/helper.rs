//! What the monitor's types can do. The types themselves are declared in `types.rs`, which holds no behaviour.

use bitcoin::script::Instruction;
use bitcoin::{OutPoint, Script, Transaction, Txid};
use bitvmx_bitcoin_rpc::types::BlockHeight;

use crate::errors::MonitorError;
use crate::types::{
    BlockRef, FullBlock, MonitorEntry, MonitorRecord, MonitorTarget, OutputPatternFilter, TrackedTx,
};

impl MonitorRecord {
    /// An empty record for a target nobody has subscribed to yet.
    pub fn new(target: MonitorTarget) -> Self {
        Self {
            target,
            entries: Vec::new(),
        }
    }
}

impl MonitorEntry {
    /// A subscription that has not found anything yet.
    pub fn new(
        context: String,
        confirmation_trigger: Option<u32>,
        search_in_mempool: bool,
    ) -> Self {
        Self {
            context,
            confirmation_trigger,
            search_in_mempool,
            tracked: Vec::new(),
        }
    }
}

impl TrackedTx {
    /// A transaction found in a block, which is the only place one is ever tracked from.
    pub fn new(txid: Txid, confirmed_at: BlockRef) -> Self {
        Self { txid, confirmed_at }
    }
}

impl BlockRef {
    /// How many confirmations a transaction in this block has at `height`, counting this block as the first.
    pub fn confirmations_at(&self, height: BlockHeight) -> Result<u32, MonitorError> {
        // A tracked transaction cannot sit above the height being counted from.
        let depth = height.checked_sub(self.height).ok_or_else(|| {
            MonitorError::InvariantViolation(format!(
                "a transaction confirmed at {} is above the processed height {height}",
                self.height
            ))
        })?;

        Ok(depth + 1)
    }
}

impl From<&FullBlock> for BlockRef {
    fn from(block: &FullBlock) -> Self {
        Self {
            height: block.height,
            hash: block.hash,
        }
    }
}

/// Every byte string pushed by a script, which is how the payload of an OP_RETURN is read.
pub fn extract_output_data(script: &Script) -> Vec<Vec<u8>> {
    let mut result = Vec::new();

    for instruction in script.instructions_minimal().flatten() {
        if let Instruction::PushBytes(data) = instruction {
            result.push(data.as_bytes().to_vec());
        }
    }

    result
}

/// True when the transaction matches the filter: at most `max_outputs` outputs when that is set, and the output
/// at `output_index` an OP_RETURN whose pushed data starts with the tag.
pub fn matches_output_pattern(tx: &Transaction, filter: &OutputPatternFilter) -> bool {
    // An upper bound on the outputs is optional, and only narrows the match.
    if filter.max_outputs.is_some_and(|max| tx.output.len() > max) {
        return false;
    }

    let Some(output) = tx.output.get(filter.output_index) else {
        return false; // The transaction has no output at that index.
    };

    if !output.script_pubkey.is_op_return() {
        return false;
    }

    // Only the first push carries the tag, so a script that pushes nothing cannot match.
    extract_output_data(&output.script_pubkey)
        .first()
        .is_some_and(|data| data.starts_with(&filter.tag))
}

/// True when the transaction spends this outpoint.
pub fn is_spending_output(tx: &Transaction, outpoint: OutPoint) -> bool {
    tx.input
        .iter()
        .any(|input| input.previous_output == outpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{outpoint, tx};
    use bitcoin::script::PushBytesBuf;
    use bitcoin::{Amount, ScriptBuf, TxOut};

    fn op_return(data: &[u8]) -> TxOut {
        let mut pushed = PushBytesBuf::new();
        pushed.extend_from_slice(data).unwrap();

        TxOut {
            value: Amount::ZERO,
            script_pubkey: ScriptBuf::new_op_return(pushed),
        }
    }

    fn anything_else() -> TxOut {
        TxOut {
            value: Amount::ZERO,
            script_pubkey: ScriptBuf::new(),
        }
    }

    fn filter(output_index: usize, tag: &[u8], max_outputs: Option<usize>) -> OutputPatternFilter {
        OutputPatternFilter {
            output_index,
            tag: tag.to_vec(),
            max_outputs,
        }
    }

    // The tag is a prefix of the pushed data, at the index the filter names and nowhere else.
    #[test]
    fn test_a_tag_matches_where_the_filter_looks() {
        let matching = tx(1, vec![], vec![op_return(b"tagAndMore")]);
        assert!(matches_output_pattern(&matching, &filter(0, b"tag", None)));

        // A different tag at the right index does not match.
        assert!(!matches_output_pattern(
            &matching,
            &filter(0, b"other", None)
        ));

        // The right tag at an index the transaction does not have does not match either.
        assert!(!matches_output_pattern(&matching, &filter(1, b"tag", None)));

        // The tag has to be a prefix, not appear anywhere in the data.
        let suffix = tx(2, vec![], vec![op_return(b"prefixTag")]);
        assert!(!matches_output_pattern(&suffix, &filter(0, b"Tag", None)));
    }

    // Only an OP_RETURN carries a tag, and the bound on the outputs narrows an otherwise good match.
    #[test]
    fn test_a_match_needs_an_op_return_within_the_bound() {
        let not_op_return = tx(1, vec![], vec![anything_else()]);
        assert!(!matches_output_pattern(
            &not_op_return,
            &filter(0, b"tag", None)
        ));

        let two_outputs = tx(2, vec![], vec![op_return(b"tag"), anything_else()]);
        assert!(matches_output_pattern(
            &two_outputs,
            &filter(0, b"tag", Some(2))
        ));
        assert!(!matches_output_pattern(
            &two_outputs,
            &filter(0, b"tag", Some(1))
        ));
    }

    // Spending is decided by the outpoint a transaction consumes, not by anything it pays.
    #[test]
    fn test_spending_is_decided_by_the_input() {
        let spent = outpoint(7, 0);
        let spender = tx(1, vec![spent], vec![]);

        assert!(is_spending_output(&spender, spent));

        // The same transaction, a different output of the same transaction.
        assert!(!is_spending_output(&spender, outpoint(7, 1)));
        assert!(!is_spending_output(&tx(2, vec![], vec![]), spent));
    }
}

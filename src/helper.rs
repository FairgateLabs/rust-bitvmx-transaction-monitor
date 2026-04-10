use bitcoin::script::Instruction;
use bitcoin::{OutPoint, Script, Transaction, Txid};

use crate::types::OutputPatternFilter;

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

pub fn is_spending_output(tx: &Transaction, target_txid: Txid, target_vout: u32) -> bool {
    tx.input.iter().any(|input| {
        input.previous_output
            == OutPoint {
                txid: target_txid,
                vout: target_vout,
            }
    })
}

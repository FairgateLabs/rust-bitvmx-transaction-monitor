//! # Helper Module
//!
//! This module provides utility functions for transaction validation and detection.
//! It includes functions for identifying RSK pegin transactions and checking UTXO spending.

use bitcoin::script::Instruction;
use bitcoin::secp256k1::ffi::{
    secp256k1_context_no_precomp, secp256k1_xonly_pubkey_parse, XOnlyPublicKey,
};
use bitcoin::{Address, Network, OutPoint, Script, Transaction, Txid};

/// Validates the OP_RETURN data to ensure it contains valid RSK pegin data.
///
/// The expected format is: "RSK_PEGIN" (9 bytes) + packet_number (8 bytes) +
/// RSK address (20 bytes) + Bitcoin xOnlyPublicKey (32 bytes) = 69 bytes total.
///
/// # Arguments
/// * `data` - Vector of byte vectors extracted from the OP_RETURN script
///
/// # Returns
/// `true` if the data matches the expected RSK pegin format, `false` otherwise
pub fn is_valid_op_return_rsk_data(data: Vec<Vec<u8>>) -> bool {
    if data.len() != 1 {
        return false;
    }
    let rest = &data[0];
    // Expected OP_RETURN format: "RSK_PEGIN N A R"
    if rest.len() != 69 {
        return false;
    }
    // First part should be "RSK_PEGIN"
    let (first_part, rest) = rest.split_at(9);
    if String::from_utf8_lossy(first_part) != "RSK_PEGIN" {
        return false;
    }

    // Second part should be a number for the packet number (8 bytes)
    let (second_part, rest) = rest.split_at(8);
    if second_part.len() != 8 {
        return false;
    }
    // let _packet_number = u64::from_be_bytes(second_part.try_into().unwrap());

    // Third part should be RSK address (20 bytes)
    let (third_part, rest) = rest.split_at(20);
    if third_part.len() != 20 {
        return false;
    }
    if !is_valid_rsk_address(&hex::encode(third_part)) {
        return false;
    }

    // Fourth part should be Bitcoin xOnlyPublicKey (32 bytes)
    let fourth_part = rest;
    if fourth_part.len() != 32 {
        return false;
    }

    // Fourth part should be Bitcoin xOnlyPublicKey
    unsafe {
        let mut x_only_public_key = XOnlyPublicKey::new();
        let fourth_part = secp256k1_xonly_pubkey_parse(
            secp256k1_context_no_precomp,
            &mut x_only_public_key as *mut _,
            fourth_part.as_ptr(),
        );

        if fourth_part != 1 {
            return false;
        }
    };

    true
}

/// Validates if a string is a valid RSK address.
///
/// RSK addresses are 40-character hexadecimal strings (20 bytes in hex format).
///
/// # Arguments
/// * `address` - The address string to validate
///
/// # Returns
/// `true` if the address is a valid 40-character hex string, `false` otherwise
pub fn is_valid_rsk_address(address: &str) -> bool {
    address.len() == 40 && address.chars().all(|c| c.is_ascii_hexdigit())
}

/// Validates if a transaction is a valid peg-in transaction by checking:
/// 1. The first output matches the given committee address (N)
/// 2. The second output is a valid OP_RETURN containing:
///    - "RSK_PEGIN" identifier
///    - Packet number
///    - RSK destination address
///    - Bitcoin reimbursement address (R)
pub fn is_a_pegin_tx(tx: &Transaction) -> bool {
    // Ensure at least 2 outputs exist
    if tx.output.len() < 2 {
        return false;
    }

    // Check the first output for the matching address
    let mut first_output_match = false;

    if let Some(first_output) = tx.output.first() {
        // Note: RSK pegin transactions are specific to Bitcoin mainnet, so we use Network::Bitcoin.
        // The network could be obtained from RpcConfig in the future if support for testnet/regtest
        // RSK pegin transactions is needed, but currently this is not required.
        if Address::from_script(&first_output.script_pubkey, Network::Bitcoin).is_ok() {
            first_output_match = true;
        }
    }

    if !first_output_match {
        return false;
    }

    // Check the second output for the OP_RETURN structure
    if let Some(op_return_output) = tx.output.get(1) {
        if op_return_output.script_pubkey.is_op_return() {
            let data = extract_output_data(&op_return_output.script_pubkey);

            if is_valid_op_return_rsk_data(data) {
                return true; // OP_RETURN has valid format
            }
        }
    }

    false
}

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

/// Checks if a transaction spends a specific UTXO.
///
/// # Arguments
/// * `tx` - The transaction to check
/// * `target_txid` - The transaction ID of the UTXO being checked
/// * `target_vout` - The output index (vout) of the UTXO being checked
///
/// # Returns
/// `true` if the transaction spends the specified UTXO, `false` otherwise
pub fn is_spending_output(tx: &Transaction, target_txid: Txid, target_vout: u32) -> bool {
    tx.input.iter().any(|input| {
        input.previous_output
            == OutPoint {
                txid: target_txid,
                vout: target_vout,
            }
    })
}

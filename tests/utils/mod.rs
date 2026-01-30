use anyhow::Result;
use bitcoin::{Amount, Transaction, Txid};
use bitcoincore_rpc::RpcApi;
use bitvmx_bitcoin_rpc::bitcoin_client::BitcoinClient;

use bitcoin::key::rand;
use rand::Rng;

pub fn generate_random_string() -> String {
    let mut rng = rand::thread_rng();
    (0..10).map(|_| rng.gen_range('a'..='z')).collect()
}

pub fn clear_output() {
    let _ = std::fs::remove_dir_all("test_outputs");
}

/// Creates and sends a transaction that spends a specific UTXO.
/// Returns the decoded transaction and its txid.
pub fn create_and_send_spending_transaction(
    bitcoin_client: &BitcoinClient,
    utxo_txid: Txid,
    utxo_vout: u32,
    spending_amount: Amount,
) -> Result<(Transaction, Txid)> {
    // Get a new address to send to
    let recipient_address = bitcoin_client
        .client
        .get_new_address(None, Some(bitcoincore_rpc::json::AddressType::Bech32))?;

    // Create a raw transaction that spends the UTXO
    let inputs = vec![bitcoincore_rpc::json::CreateRawTransactionInput {
        txid: utxo_txid,
        vout: utxo_vout,
        sequence: None,
    }];

    let mut outputs = std::collections::HashMap::new();
    // Convert address to string for the output map
    let address_str = format!("{}", recipient_address.assume_checked());
    outputs.insert(address_str, spending_amount);

    let raw_tx = bitcoin_client
        .client
        .create_raw_transaction(&inputs, &outputs, None, None)?;

    // Sign the transaction with the wallet
    let signed_tx = bitcoin_client
        .client
        .sign_raw_transaction_with_wallet(&raw_tx, None, None)?;

    if !signed_tx.complete {
        return Err(anyhow::anyhow!(
            "Transaction signing incomplete: {:?}",
            signed_tx.errors
        ));
    }

    // Decode the signed transaction to get the txid
    // signed_tx.hex is Vec<u8>
    let transaction: Transaction =
        bitcoin::consensus::Decodable::consensus_decode(&mut &signed_tx.hex[..])?;
    let txid = transaction.compute_txid();

    // Send the transaction to the network
    bitcoin_client.client.send_raw_transaction(&signed_tx.hex)?;

    Ok((transaction, txid))
}

use bitcoin::{
    absolute::LockTime, hex::{ DisplayHex, FromHex}, key::{rand::thread_rng, Secp256k1}, opcodes::all::OP_RETURN, script::Builder, secp256k1::{PublicKey, SecretKey}, transaction::Version
};
use bitcoin::{Address, Amount, Network, Transaction, TxOut};
use bitcoin_indexer::indexer::MockIndexerApi;
use bitcoincore_rpc::RawTx;
use bitvmx_transaction_monitor::{monitor::Monitor, store::MockMonitorStore};

#[test]
fn test_pegin_address_detection1() -> Result<(), anyhow::Error> {
    let secp = Secp256k1::new();

    // Generate committee N address (taproot internal key)
    let sk = SecretKey::new(&mut thread_rng());
    let pubk = PublicKey::from_secret_key(&secp, &sk);

    // TODO: we can generate taproot sepending tree. instead a empty tree.
    let committee_n = Address::p2tr(&secp, pubk.x_only_public_key().0, None, Network::Bitcoin);

    // Generate reimbursement address (R)
    let sk_reimburse = SecretKey::new(&mut thread_rng());
    let pk_reimburse = PublicKey::from_secret_key(&secp, &sk_reimburse);
    let reimbursement_addr = Address::p2tr(
        &secp,
        pk_reimburse.x_only_public_key().0,
        None,
        Network::Bitcoin,
    );

    // Create the taproot output
    let taproot_output = TxOut {
        value: Amount::from_sat(100_000_000), // 1 BTC
        script_pubkey: committee_n.script_pubkey(),
    };

    let packet_number: u64 = 0;
    let mut rootstock_address = [0u8; 20];
    rootstock_address.copy_from_slice(Vec::from_hex("7ac5496aee77c1ba1f0854206a26dda82a81d6d8").unwrap().as_slice());

    let reimbursement_addr_data = reimbursement_addr.to_string();
    let mut reimbursement_addr_bytes = [0u8; 62];
    let bytes = reimbursement_addr_data.as_bytes();
    reimbursement_addr_bytes[..bytes.len()].copy_from_slice(bytes);

    // Create the OP_RETURN output
    let op_return_output = TxOut {
        value: Amount::ZERO,
        script_pubkey: Builder::new()
            .push_opcode(OP_RETURN)
            .push_slice(b"RSK_PEGIN")
            .push_slice(packet_number.to_be_bytes()) // packet_number
            .push_slice(rootstock_address)
            .push_slice(reimbursement_addr_bytes)
            .into_script(),
    };

    // Create the Peg-In transaction
    let pegin_tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![], // Inputs would be added by the user
        output: vec![taproot_output.clone(), op_return_output.clone()],
    };

    println!("=======PegInTx raw: {}", pegin_tx.raw_hex());
    println!("======= txId: {}", pegin_tx.compute_txid());
    println!("======= wTxId: {}", pegin_tx.compute_wtxid());
    println!("======= taproot_output: amount:{} size:{} script_pubkey:{}", taproot_output.value, taproot_output.size(), taproot_output.script_pubkey.as_bytes().to_hex_string(bitcoin::hex::Case::Lower));
    println!("======= op_return_output: amount:{} size:{} script_pubkey:{}, script_pubkey_hex:{}", op_return_output.value, op_return_output.size(), op_return_output.script_pubkey, op_return_output.script_pubkey.as_bytes().to_hex_string(bitcoin::hex::Case::Lower));

    // Create a mock monitor to test address detection
    let mock_indexer = MockIndexerApi::new();
    let mut mock_store = MockMonitorStore::new();
    mock_store.expect_set_current_block_height().returning(|_| Ok(()));
    
    let monitor = Monitor::new(mock_indexer, mock_store, None, 6)?;

    // Validate that the committee address (N) is detected
    assert!(monitor.is_a_pegin_tx(committee_n.clone(), &pegin_tx));

    Ok(())
}

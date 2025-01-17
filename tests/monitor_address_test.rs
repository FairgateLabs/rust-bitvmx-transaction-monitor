use bitcoin::{
    absolute::LockTime,
    key::{rand::thread_rng, Secp256k1},
    opcodes::all::OP_RETURN,
    script::Builder,
    secp256k1::{PublicKey, SecretKey},
    transaction::Version,
};
use bitcoin::{Address, Amount, Network, Transaction, TxOut};
use bitcoin_indexer::indexer::MockIndexerApi;
use bitvmx_transaction_monitor::{monitor::Monitor, store::MockMonitorStore};

#[test]
fn test_pegin_address_detection() -> Result<(), anyhow::Error> {
    let secp = Secp256k1::new();

    // Generate committee N address (taproot internal key)
    let sk = SecretKey::new(&mut thread_rng());
    let pubk = PublicKey::from_secret_key(&secp, &sk);

    // TODO:  generate a merkel with 2 leaves.
    let committee_n = Address::p2tr(&secp, pubk.x_only_public_key().0, None, Network::Bitcoin);

    // Generate reimbursement address (R)
    // let sk_reimburse = SecretKey::new(&mut thread_rng());
    // let pk_reimburse = PublicKey::from_secret_key(&secp, &sk_reimburse);
    // let reimbursement_addr = Address::p2tr(
    //     &secp,
    //     pk_reimburse.x_only_public_key().0,
    //     None,
    //     Network::Bitcoin,
    // );

    // Create the taproot output
    let taproot_output = TxOut {
        value: Amount::from_sat(100_000_000), // 1 BTC
        script_pubkey: committee_n.script_pubkey(),
    };

    // let rootstock_address = "0x1234567890abcdef".to_string();
    // let mut rootstock_address_bytes = [0u8; 40];
    // let bytes = rootstock_address.as_bytes();
    // rootstock_address_bytes[..bytes.len()].copy_from_slice(bytes);

    // let reimbursement_addr_data = reimbursement_addr.to_string();
    // let mut reimbursement_addr_bytes = [0u8; 62];
    // let bytes = reimbursement_addr_data.as_bytes();
    // reimbursement_addr_bytes[..bytes.len()].copy_from_slice(bytes);

    let data_bytes = b"RSK_PEGIN 1 0x1234567890abcdef 1BitcoinReimbursementAddress";

    // Create the OP_RETURN output
    // TODO:  not sure how this script will be created. For now we just push the data bytes.
    let op_return_output = TxOut {
        value: Amount::ZERO,
        script_pubkey: Builder::new()
            .push_opcode(OP_RETURN)
            .push_slice(data_bytes)
            // .push_slice(b"RSK_PEGIN")
            // .push_slice(b"1") // packet_number
            // .push_slice(rootstock_address_bytes)
            // .push_slice(reimbursement_addr_bytes)
            .into_script(),
    };

    // Create the Peg-In transaction
    let pegin_tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![], // Inputs would be added by the user
        output: vec![taproot_output, op_return_output],
    };

    // Create a mock monitor to test address detection
    let mock_indexer = MockIndexerApi::new();
    let mock_store = MockMonitorStore::new();
    let monitor = Monitor::new(mock_indexer, mock_store, None, 6);

    // Validate that the committee address (N) is detected
    assert!(monitor.address_exist_in_output(committee_n.clone(), &pegin_tx));

    Ok(())
}

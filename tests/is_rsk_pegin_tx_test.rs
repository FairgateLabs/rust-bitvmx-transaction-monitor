use bitcoin::{
    absolute::LockTime,
    hex::{DisplayHex, FromHex},
    key::{rand::thread_rng, Secp256k1},
    opcodes::all::OP_RETURN,
    script::Builder,
    secp256k1::{PublicKey, SecretKey},
    transaction::Version,
};
use bitcoin::{Address, Amount, Network, Transaction, TxOut};
use bitcoincore_rpc::RawTx;
use bitvmx_transaction_monitor::helper::is_a_pegin_tx;

#[test]
fn test_pegin_tx_detection() -> Result<(), anyhow::Error> {
    let secp = Secp256k1::new();

    // Generate committee N address (taproot internal key)
    let sk = SecretKey::new(&mut thread_rng());
    let pubk = PublicKey::from_secret_key(&secp, &sk);

    // TODO: we can generate taproot sepending tree. instead a empty tree.
    let committee_n = Address::p2tr(&secp, pubk.x_only_public_key().0, None, Network::Bitcoin);

    // Generate reimbursement address (R)
    let sk_reimburse = SecretKey::new(&mut thread_rng());
    let pk_reimburse = PublicKey::from_secret_key(&secp, &sk_reimburse);
    let reimbursement_xpk = pk_reimburse.x_only_public_key().0;

    // Create the taproot output
    let taproot_output = TxOut {
        value: Amount::from_sat(100_000_000), // 1 BTC
        script_pubkey: committee_n.script_pubkey(),
    };

    let packet_number: u64 = 0;
    let mut rootstock_address = [0u8; 20];
    rootstock_address.copy_from_slice(
        Vec::from_hex("7ac5496aee77c1ba1f0854206a26dda82a81d6d8")
            .unwrap()
            .as_slice(),
    );

    let mut data = [0u8; 69];
    data.copy_from_slice(
        [
            b"RSK_PEGIN".as_slice(),
            &packet_number.to_be_bytes(),
            &rootstock_address,
            &reimbursement_xpk.serialize(),
        ]
        .concat()
        .as_slice(),
    );

    // Create the OP_RETURN output
    let op_return_output = TxOut {
        value: Amount::ZERO,
        script_pubkey: Builder::new()
            .push_opcode(OP_RETURN)
            .push_slice(&data)
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

    println!(
        "======= taproot_output: amount:{} size:{} script_pubkey:{}",
        taproot_output.value,
        taproot_output.size(),
        taproot_output
            .script_pubkey
            .as_bytes()
            .to_hex_string(bitcoin::hex::Case::Lower)
    );
    println!(
        "======= op_return_output: amount:{} size:{} script_pubkey:{}, script_pubkey_hex:{}",
        op_return_output.value,
        op_return_output.size(),
        op_return_output.script_pubkey,
        op_return_output
            .script_pubkey
            .as_bytes()
            .to_hex_string(bitcoin::hex::Case::Lower)
    );

    println!("=======PegInTx raw: {}", pegin_tx.raw_hex());
    println!("======= txId: {}", pegin_tx.compute_txid());
    println!("======= wTxId: {}", pegin_tx.compute_wtxid());
    println!(
        "======= taproot_output: amount:{} size:{} script_pubkey:{}",
        taproot_output.value,
        taproot_output.size(),
        taproot_output
            .script_pubkey
            .as_bytes()
            .to_hex_string(bitcoin::hex::Case::Lower)
    );
    println!(
        "======= op_return_output: amount:{} size:{} script_pubkey:{}, script_pubkey_hex:{}",
        op_return_output.value,
        op_return_output.size(),
        op_return_output.script_pubkey,
        op_return_output
            .script_pubkey
            .as_bytes()
            .to_hex_string(bitcoin::hex::Case::Lower)
    );

    // Validate that the committee address (N) is detected
    assert!(is_a_pegin_tx(&pegin_tx));

    Ok(())
}

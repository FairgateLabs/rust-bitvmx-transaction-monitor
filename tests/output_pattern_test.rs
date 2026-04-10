use bitcoin::{
    absolute::LockTime,
    opcodes::all::OP_RETURN,
    script::{Builder, PushBytesBuf},
    transaction::Version,
    Amount, Transaction, TxOut,
};
use bitvmx_transaction_monitor::{helper::matches_output_pattern, types::OutputPatternFilter};

fn make_tx_with_op_return(output_index: usize, data: &[u8], total_outputs: usize) -> Transaction {
    let push_data =
        PushBytesBuf::try_from(data.to_vec()).expect("data too large for push_slice");
    let op_return_output = TxOut {
        value: Amount::ZERO,
        script_pubkey: Builder::new()
            .push_opcode(OP_RETURN)
            .push_slice(push_data.as_push_bytes())
            .into_script(),
    };

    let dummy_output = TxOut {
        value: Amount::from_sat(1000),
        script_pubkey: Builder::new().into_script(),
    };

    let mut outputs = vec![dummy_output.clone(); total_outputs];
    if output_index < total_outputs {
        outputs[output_index] = op_return_output;
    }

    Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: outputs,
    }
}

#[test]
fn test_matches_output_pattern_rsk_tag() {
    let mut data = [0u8; 69];
    data[..9].copy_from_slice(b"RSK_PEGIN");

    let tx = make_tx_with_op_return(1, &data, 2);

    let filter = OutputPatternFilter {
        output_index: 1,
        tag: b"RSK_PEGIN".to_vec(),
        max_outputs: None,
    };

    assert!(matches_output_pattern(&tx, &filter));
}

#[test]
fn test_matches_output_pattern_wrong_tag() {
    let mut data = [0u8; 69];
    data[..9].copy_from_slice(b"RSK_PEGIN");

    let tx = make_tx_with_op_return(1, &data, 2);

    let filter = OutputPatternFilter {
        output_index: 1,
        tag: b"OTHER_TAG".to_vec(),
        max_outputs: None,
    };

    assert!(!matches_output_pattern(&tx, &filter));
}

#[test]
fn test_matches_output_pattern_wrong_output_index() {
    let mut data = [0u8; 69];
    data[..9].copy_from_slice(b"RSK_PEGIN");

    let tx = make_tx_with_op_return(1, &data, 3);

    let filter = OutputPatternFilter {
        output_index: 0,
        tag: b"RSK_PEGIN".to_vec(),
        max_outputs: None,
    };

    // Output 0 is not an OP_RETURN in this tx
    assert!(!matches_output_pattern(&tx, &filter));
}

#[test]
fn test_matches_output_pattern_max_outputs_ok() {
    let mut data = [0u8; 10];
    data[..4].copy_from_slice(b"TEST");

    let tx = make_tx_with_op_return(0, &data, 2);

    let filter = OutputPatternFilter {
        output_index: 0,
        tag: b"TEST".to_vec(),
        max_outputs: Some(3),
    };

    assert!(matches_output_pattern(&tx, &filter));
}

#[test]
fn test_matches_output_pattern_max_outputs_exceeded() {
    let mut data = [0u8; 10];
    data[..4].copy_from_slice(b"TEST");

    let tx = make_tx_with_op_return(0, &data, 5);

    let filter = OutputPatternFilter {
        output_index: 0,
        tag: b"TEST".to_vec(),
        max_outputs: Some(3),
    };

    assert!(!matches_output_pattern(&tx, &filter));
}

#[test]
fn test_matches_output_pattern_no_outputs() {
    let tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: vec![],
    };

    let filter = OutputPatternFilter {
        output_index: 0,
        tag: b"TEST".to_vec(),
        max_outputs: None,
    };

    assert!(!matches_output_pattern(&tx, &filter));
}

use super::*;
use slimchain_common::{
    create_address, create_state_key, create_tx_read_data, create_tx_write_set,
};

#[cfg(all(feature = "read", feature = "write"))]
#[test]
fn test_read_write() {
    let write1 = create_tx_write_set! {
        "0000000000000000000000000000000000000000" => {
            nonce: 1,
        },
        "0000000000000000000000000000000000000010" => {
            nonce: 2,
            reset_values: true,
            code: b"code",
            values: {
                "0000000000000000000000000000000000000000000000000000000000000001" => 1,
                "0000000000000000000000000000000000000000000000000000000000000002" => 2,
                "0000000000000000000000000000000000000000000000000000000000001001" => 3,
                "0000000000000000000000000000000000000000000000000000000001001002" => 4,
            }
        },
    };

    let mut state = MemTxState::new();
    let update1 = update_tx_state(&state.state_view(), state.state_root(), &write1).unwrap();
    state.apply_update(update1).unwrap();

    let read1 = create_tx_read_data! {
        "0000000000000000000000000000000000000000" => {
            nonce: 1,
            code: b"",
            values: {
                "0000000000000000000000000000000000000000000000000000000000000001" => 0,
            }
        },
        "0000000000000000000000000000000000000010" => {
            code: b"code",
            values: {
                "0000000000000000000000000000000000000000000000000000000000000001" => 1,
                "0000000000000000000000000000000000000000000000000000000000001001" => 3,
            }
        },
        "0000000000000000000000000000000000000100" => {
            nonce: 0,
        }
    };

    let mut read_ctx = TxStateReadContext::new(state.state_view(), state.state_root());
    let acc_addr1 = create_address!("0000000000000000000000000000000000000000");
    let acc_addr2 = create_address!("0000000000000000000000000000000000000010");
    let acc_addr3 = create_address!("0000000000000000000000000000000000000100");
    assert_eq!(read_ctx.get_nonce(acc_addr1).unwrap(), 1.into());
    assert_eq!(read_ctx.get_code_len(acc_addr1).unwrap(), 0);
    assert_eq!(
        read_ctx
            .get_value(
                acc_addr1,
                create_state_key!(
                    "0000000000000000000000000000000000000000000000000000000000000001"
                )
            )
            .unwrap(),
        0.into()
    );
    assert_eq!(
        read_ctx.get_code(acc_addr2).unwrap(),
        b"code".to_vec().into()
    );
    assert_eq!(
        read_ctx
            .get_value(
                acc_addr2,
                create_state_key!(
                    "0000000000000000000000000000000000000000000000000000000000000001"
                )
            )
            .unwrap(),
        1.into()
    );
    assert_eq!(
        read_ctx
            .get_value(
                acc_addr2,
                create_state_key!(
                    "0000000000000000000000000000000000000000000000000000000000001001"
                )
            )
            .unwrap(),
        3.into()
    );
    assert_eq!(read_ctx.get_nonce(acc_addr3).unwrap(), 0.into());

    let read_proof1 = read_ctx.generate_proof().unwrap();
    assert!(read_proof1.verify(&read1, state.state_root()).is_ok());

    let write2 = create_tx_write_set! {
        "0000000000000000000000000000000000000000" => {
            nonce: 2,
        },
        "0000000000000000000000000000000000000010" => {
            values: {
                "0000000000000000000000000000000000000000000000000000000000000002" => 7,
                "1000000000000000000000000000000000000000000000000000000000000000" => 8,
            }
        },
    };
    let update2 = update_tx_state(&state.state_view(), state.state_root(), &write2).unwrap();
    state.apply_update(update2).unwrap();

    let read2 = create_tx_read_data! {
        "0000000000000000000000000000000000000000" => {
            nonce: 2,
        },
        "0000000000000000000000000000000000000010" => {
            values: {
                "0000000000000000000000000000000000000000000000000000000000000002" => 7,
                "1000000000000000000000000000000000000000000000000000000000000000" => 8,
            }
        },
    };
    let mut read_ctx = TxStateReadContext::new(state.state_view(), state.state_root());
    assert_eq!(read_ctx.get_nonce(acc_addr1).unwrap(), 2.into());
    assert_eq!(
        read_ctx
            .get_value(
                acc_addr2,
                create_state_key!(
                    "0000000000000000000000000000000000000000000000000000000000000002"
                )
            )
            .unwrap(),
        7.into()
    );
    assert_eq!(
        read_ctx
            .get_value(
                acc_addr2,
                create_state_key!(
                    "1000000000000000000000000000000000000000000000000000000000000000"
                )
            )
            .unwrap(),
        8.into()
    );
    let read_proof2 = read_ctx.generate_proof().unwrap();
    assert!(read_proof2.verify(&read2, state.state_root()).is_ok());

    let read3 = create_tx_read_data! {
        "0000000000000000000000000000000000000010" => {
            nonce: 2,
        },
    };
    let mut read_ctx = TxStateReadContext::new(state.state_view(), state.state_root());
    assert_eq!(read_ctx.get_nonce(acc_addr2).unwrap(), 2.into());
    let read_proof3 = read_ctx.generate_proof().unwrap();
    assert!(read_proof3.verify(&read3, state.state_root()).is_ok());
}

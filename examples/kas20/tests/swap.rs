use argent::build_file;
use argent_runtime::{
    BuilderError, EntryCall, TxBuilder, TxContext, args, execute_transaction_with_covenants,
};
use kas20::token_state;
use kaspa_consensus_core::{
    Hash,
    tx::{CovenantBinding, ScriptPublicKey, TransactionOutput, UtxoEntry},
};
use kcc20_reference::demo_support::{demo_keys, demo_outpoint, sign_input};

// Synthetic existing UTXOs: no genesis provenance, node or mempool claim.
#[test]
fn two_asset_swap_and_signed_payment_controls() {
    let artifact = build_file(
        concat!(env!("CARGO_MANIFEST_DIR"), "/contracts/swap_test.ag"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/build/swap-test"),
    )
    .unwrap();
    let builder = TxBuilder::new(&artifact).unwrap();
    let (alice, a) = demo_keys(0xaa);
    let (bob, b) = demo_keys(0xbb);
    let ka = CovenantBinding::new(0, Hash::from_bytes([0x11; 32]));
    let kb = CovenantBinding::new(1, Hash::from_bytes([0x22; 32]));
    let sa = token_state(&a, 100);
    let sb = token_state(&b, 200);
    let oa = token_state(&b, 100);
    let ob = token_state(&a, 200);
    let ua = builder
        .covenant_utxo("KAS20", sa.clone(), 100, 1, false, Some(ka.covenant_id))
        .unwrap();
    let ub = builder
        .covenant_utxo("KCC20", sb.clone(), 1000, 1, false, Some(kb.covenant_id))
        .unwrap();
    let funding = UtxoEntry::new(10, ScriptPublicKey::from_vec(0, vec![0x51]), 1, false, None);
    let paid_to_bob = builder
        .covenant_utxo(
            "KAS20",
            token_state(&b, 100),
            100,
            0,
            false,
            Some(ka.covenant_id),
        )
        .unwrap()
        .script_public_key;
    let paid_to_alice = builder
        .covenant_utxo(
            "KCC20",
            token_state(&a, 200),
            1000,
            0,
            false,
            Some(kb.covenant_id),
        )
        .unwrap()
        .script_public_key;
    let aa = oa.clone();
    let bb = ob.clone();
    let mut tx = builder
        .build(
            &TxContext::new()
                .actor_input(
                    "KAS20",
                    sa,
                    EntryCall::new("transfer").args_with(move |tx, i| {
                        let mut w = vec![0];
                        w.extend(sign_input(tx, i, &alice));
                        args!(vec![aa.clone()], w)
                    }),
                    demo_outpoint(1, 0),
                    ua.clone(),
                    0,
                )
                .actor_input(
                    "KCC20",
                    sb,
                    EntryCall::new("transfer").args_with(move |tx, i| {
                        let mut w = vec![0];
                        w.extend(sign_input(tx, i, &bob));
                        args!(vec![bb.clone()], w)
                    }),
                    demo_outpoint(2, 0),
                    ub.clone(),
                    0,
                )
                .input(demo_outpoint(3, 0), funding.clone(), vec![], 0)
                .actor_output("KAS20", oa, ka, 100)
                .actor_output("KCC20", ob, kb, 1000)
                .output(ScriptPublicKey::from_vec(0, vec![0x51]), None, 7),
        )
        .unwrap();
    let entries = vec![ua, ub, funding];
    execute_transaction_with_covenants(&mut tx, entries.clone()).unwrap();
    assert_eq!(tx.outputs[0].script_public_key, paid_to_bob); // Bob receives 100 KAS20
    assert_eq!(tx.outputs[1].script_public_key, paid_to_alice); // Alice receives 200 KCC20
    assert_eq!(tx.outputs[0].value, 100); // exact KAS20 backing
    assert_eq!(tx.outputs[1].value, 1000); // other asset deposit preserved
    assert_eq!(1110 - tx.outputs.iter().map(|o| o.value).sum::<u64>(), 3); // explicit fee funding
    for position in [0, 1] {
        let mut changed = tx.clone();
        changed.outputs[position].value -= 1;
        let error = execute_transaction_with_covenants(&mut changed, entries.clone()).unwrap_err();
        assert!(matches!(error, BuilderError::InputScript { .. }), "{error}");
        let mut removed = tx.clone();
        removed.outputs.remove(position);
        let error = execute_transaction_with_covenants(&mut removed, entries.clone()).unwrap_err();
        assert!(matches!(error, BuilderError::InputScript { .. }), "{error}");
        let mut redirected = tx.clone();
        redirected.outputs[position] = TransactionOutput::new(
            tx.outputs[position].value,
            ScriptPublicKey::from_vec(0, vec![0x51]),
        );
        let error =
            execute_transaction_with_covenants(&mut redirected, entries.clone()).unwrap_err();
        assert!(matches!(error, BuilderError::InputScript { .. }), "{error}");
    }
    for position in [0, 1] {
        let mut unsigned = tx.clone();
        unsigned.inputs[position].signature_script.clear();
        let error = execute_transaction_with_covenants(&mut unsigned, entries.clone()).unwrap_err();
        assert!(
            matches!(error, BuilderError::InputScript { input_index, .. } if input_index == position),
            "{error}"
        );
    }
}

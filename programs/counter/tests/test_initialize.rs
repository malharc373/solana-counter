//! LiteSVM tests: an in-process Solana VM, no validator needed.
//! Run `anchor build` first (the tests load target/deploy/counter.so).

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    counter::{accounts, instruction, Counter, COUNTER_SEED},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const SOL: u64 = 1_000_000_000;

fn setup() -> LiteSVM {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/counter.so"
    ));
    svm.add_program(counter::id(), bytes).unwrap();
    svm
}

fn user(svm: &mut LiteSVM) -> Keypair {
    let kp = Keypair::new();
    svm.airdrop(&kp.pubkey(), SOL).unwrap();
    kp
}

fn counter_pda(authority: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[COUNTER_SEED, authority.as_ref()], &counter::id()).0
}

/// Signs and sends one instruction; returns Err with the logs on failure.
fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> Result<(), String> {
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    let res = svm
        .send_transaction(tx)
        .map(|_| ())
        .map_err(|e| e.meta.logs.join("\n"));
    svm.expire_blockhash(); // so identical txs aren't rejected as duplicates
    res
}

fn ix_initialize(authority: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        counter::id(),
        &instruction::Initialize {}.data(),
        accounts::Initialize {
            authority: *authority,
            counter: counter_pda(authority),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_update(data: Vec<u8>, counter: Pubkey, authority: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        counter::id(),
        &data,
        accounts::Update {
            counter,
            authority: *authority,
        }
        .to_account_metas(None),
    )
}

fn ix_increment(authority: &Pubkey) -> Instruction {
    ix_update(
        instruction::Increment {}.data(),
        counter_pda(authority),
        authority,
    )
}

fn ix_decrement(authority: &Pubkey) -> Instruction {
    ix_update(
        instruction::Decrement {}.data(),
        counter_pda(authority),
        authority,
    )
}

fn ix_close(authority: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        counter::id(),
        &instruction::Close {}.data(),
        accounts::Close {
            counter: counter_pda(authority),
            authority: *authority,
        }
        .to_account_metas(None),
    )
}

fn read_counter(svm: &LiteSVM, authority: &Pubkey) -> Counter {
    let acc = svm.get_account(&counter_pda(authority)).unwrap();
    Counter::try_deserialize(&mut acc.data.as_slice()).unwrap()
}

#[test]
fn initialize_increment_decrement() {
    let mut svm = setup();
    let alice = user(&mut svm);

    send(&mut svm, ix_initialize(&alice.pubkey()), &alice).unwrap();
    let c = read_counter(&svm, &alice.pubkey());
    assert_eq!(c.count, 0);
    assert_eq!(c.authority, alice.pubkey());

    send(&mut svm, ix_increment(&alice.pubkey()), &alice).unwrap();
    send(&mut svm, ix_increment(&alice.pubkey()), &alice).unwrap();
    assert_eq!(read_counter(&svm, &alice.pubkey()).count, 2);

    send(&mut svm, ix_decrement(&alice.pubkey()), &alice).unwrap();
    assert_eq!(read_counter(&svm, &alice.pubkey()).count, 1);
}

#[test]
fn each_user_has_their_own_counter() {
    let mut svm = setup();
    let alice = user(&mut svm);
    let bob = user(&mut svm);

    send(&mut svm, ix_initialize(&alice.pubkey()), &alice).unwrap();
    send(&mut svm, ix_initialize(&bob.pubkey()), &bob).unwrap();
    send(&mut svm, ix_increment(&alice.pubkey()), &alice).unwrap();

    assert_ne!(counter_pda(&alice.pubkey()), counter_pda(&bob.pubkey()));
    assert_eq!(read_counter(&svm, &alice.pubkey()).count, 1);
    assert_eq!(read_counter(&svm, &bob.pubkey()).count, 0);
}

#[test]
fn cannot_initialize_twice() {
    let mut svm = setup();
    let alice = user(&mut svm);
    send(&mut svm, ix_initialize(&alice.pubkey()), &alice).unwrap();
    assert!(send(&mut svm, ix_initialize(&alice.pubkey()), &alice).is_err());
}

#[test]
fn decrement_below_zero_fails() {
    let mut svm = setup();
    let alice = user(&mut svm);
    send(&mut svm, ix_initialize(&alice.pubkey()), &alice).unwrap();

    let logs = send(&mut svm, ix_decrement(&alice.pubkey()), &alice).unwrap_err();
    assert!(logs.contains("Underflow"), "unexpected logs:\n{logs}");
}

#[test]
fn attacker_cannot_touch_someone_elses_counter() {
    let mut svm = setup();
    let alice = user(&mut svm);
    let mallory = user(&mut svm);
    send(&mut svm, ix_initialize(&alice.pubkey()), &alice).unwrap();

    // Mallory passes Alice's counter but signs as herself. The PDA seeds
    // (derived from the signer) no longer match, so Anchor rejects it.
    let ix = ix_update(
        instruction::Increment {}.data(),
        counter_pda(&alice.pubkey()),
        &mallory.pubkey(),
    );
    assert!(send(&mut svm, ix, &mallory).is_err());
    assert_eq!(read_counter(&svm, &alice.pubkey()).count, 0);
}

#[test]
fn close_refunds_rent() {
    let mut svm = setup();
    let alice = user(&mut svm);
    send(&mut svm, ix_initialize(&alice.pubkey()), &alice).unwrap();

    let rent = svm
        .get_account(&counter_pda(&alice.pubkey()))
        .unwrap()
        .lamports;
    let before = svm.get_balance(&alice.pubkey()).unwrap();
    send(&mut svm, ix_close(&alice.pubkey()), &alice).unwrap();
    let after = svm.get_balance(&alice.pubkey()).unwrap();

    assert!(svm
        .get_account(&counter_pda(&alice.pubkey()))
        .is_none_or(|a| a.lamports == 0));
    // Alice gets the rent back, minus the 5000-lamport tx fee.
    assert_eq!(after, before + rent - 5000);
}

/// Sends one instruction that must succeed; returns the transaction's compute units.
fn compute_units(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> u64 {
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    let meta = svm
        .send_transaction(tx)
        .unwrap_or_else(|e| panic!("{:?}\n{}", e.err, e.meta.logs.join("\n")));
    svm.expire_blockhash();
    meta.compute_units_consumed
}

/// A fixed wallet gives the same PDA and bump, so the same compute units, every run.
#[test]
fn cu_report() {
    let mut svm = setup();
    let wallet = Keypair::new_from_array([1; 32]);
    let key = wallet.pubkey();
    svm.airdrop(&key, SOL).unwrap();
    let mut report = cu_report::Report::new();
    for (name, ix) in [
        ("initialize", ix_initialize(&key)),
        ("increment", ix_increment(&key)),
        ("decrement", ix_decrement(&key)),
        ("close", ix_close(&key)),
    ] {
        report.record(name, compute_units(&mut svm, ix, &wallet));
    }
    report.write(cu_report::default_path!()).unwrap();
}

# 01-counter

[![CI](https://github.com/malharc373/solana-counter/actions/workflows/ci.yml/badge.svg)](https://github.com/malharc373/solana-counter/actions/workflows/ci.yml)

A per-user counter on Solana. Every wallet gets its own counter account, a PDA derived from the wallet's address. The wallet can increment it, decrement it, or close it to get its rent back.

Built with **Anchor 1.1.2**, tested with **LiteSVM**, and called from a **@solana/kit** client.

| | |
|---|---|
| Program ID (devnet) | [`HoBHp5ZXWSwcVP7BH15K1qnn7Zn8mhgTZSEYUJLmwmeW`](https://explorer.solana.com/address/HoBHp5ZXWSwcVP7BH15K1qnn7Zn8mhgTZSEYUJLmwmeW?cluster=devnet) |
| Example counter PDA | [`Dn64tu67WGaqgfMeii6FdzKdXqqrYP4uDieh5T5X3HPA`](https://explorer.solana.com/address/Dn64tu67WGaqgfMeii6FdzKdXqqrYP4uDieh5T5X3HPA?cluster=devnet) |
| Example increment tx | [`5xi6Tx…TMNseV`](https://explorer.solana.com/tx/5xi6TxFpuWdpBHapx4Z76w2cmWj8B8VseC47G93dXSFwQJHMN2KvNHVN4Rk1qkhfGwgHmQ5znwahxrq8SiTMNseV?cluster=devnet) |

## Instructions

| Instruction | Accounts | What it does | Compute units (devnet) |
|---|---|---|---|
| `initialize` | authority (signer, mut), counter (PDA, init), system_program | Creates `["counter", authority]` with count = 0 and stores the bump | 11,463 |
| `increment` | counter (mut), authority (signer) | `count + 1`, checked | 4,011 |
| `decrement` | counter (mut), authority (signer) | `count - 1`; fails with `Underflow` at 0 | 4,036 |
| `close` | counter (mut), authority (signer, mut) | Deletes the account and refunds its rent (~0.0009 SOL) | n/a |

Account layout (49 bytes + 8-byte discriminator): `authority: Pubkey · count: u64 · bump: u8`

## Security properties (each one is tested)
- **One counter per wallet.** The PDA seeds include the signer's key, so users can't collide.
- **Only the owner can change it.** The seeds are re-derived from the signer, and `has_one = authority` must match. An attacker who passes someone else's counter is rejected.
- **No double init.** Anchor's `init` fails if the account already exists.
- **No overflow or underflow.** `checked_add` and `checked_sub` with custom errors.
- **Clean close.** `close = authority` zeroes the data and returns every lamport of rent.

## Run it

```bash
anchor build
```

```bash
cargo test
```

On each pull request, [solana-cu-guard](https://github.com/malharc373/solana-cu-guard) runs the `cu_report` test on the base commit and on the change, and fails the check if any instruction costs more.

```bash
cd client && npm install && npm run counter -- show
```

The client also takes `init`, `inc`, `dec` and `close`. It uses devnet and `~/.config/solana/id.json` by default; override them with `RPC_URL` and `KEYPAIR`.

Deploy:

```bash
anchor deploy --provider.cluster devnet
```

## Tests (LiteSVM, in-process, about 0.07 s)
`initialize_increment_decrement` · `each_user_has_their_own_counter` · `cannot_initialize_twice` · `decrement_below_zero_fails` · `attacker_cannot_touch_someone_elses_counter` · `close_refunds_rent`

## What I learned
- **Code and state are separate.** The program is stateless, and every account it touches has to be passed in and declared writable or a signer up front. That's what lets Solana run transactions in parallel.
- **PDAs give deterministic addresses with no private key.** `find_program_address` searches for a bump that pushes the address off the ed25519 curve. Storing that bump means later instructions verify it with a single hash instead of searching again.
- **Rent is a deposit, not a fee.** A 57-byte account locks about 0.0009 SOL, and `close` returns all of it.
- **Anchor constraints are the security layer.** `seeds`, `bump`, `has_one` and `close` replace dozens of lines of manual checks. Each test above maps to one constraint.
- **Clients only need the discriminator and the account metas.** The kit client builds instructions by hand from the IDL (`sha256("global:<name>")[..8]`) to show what Anchor's generated clients do under the hood.
- Anchor 1.x writes the IDL into an on-chain metadata account at deploy time.

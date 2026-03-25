# Development notes

This page summarizes how the standalone workspace is organized and how to build
and test it.

## Workspace layout

`tribute_to_talk` is an independent Cargo workspace at the repository
root. It does not modify the main repository workspace manifests.

The workspace has three main crates:

- `core`: shared types and limits for instructions, payment note data,
  receiving addresses, and receipts
- `methods` and `methods/guest`: the embedded Risc0 guest program and its
  generated method bindings
- `cli`: the operator-facing binary that deploys the program, creates receiving
  addresses, sends payments, and verifies receipts

## Build and test

Run these commands from the repository root:

```bash
cargo check --workspace --manifest-path tribute_to_talk/Cargo.toml
cargo test --workspace --manifest-path tribute_to_talk/Cargo.toml
cargo run --manifest-path tribute_to_talk/cli/Cargo.toml -- --help
```

`methods/build.rs` calls `risc0_build::embed_methods()`, so normal Cargo builds
embed the guest method into the CLI binary.

If you need the standalone CLI installed globally:

```bash
cargo install --path tribute_to_talk/cli --force
```

## User-visible types

The workspace exposes a few types that matter outside the code:

- `InstructionV1`
  - `Init`
  - `Send { amount: u128, message: Vec<u8> }`
- `PaymentNoteDataV1`
  - `version: u8`
  - `message: Vec<u8>`
- `ReceivingAddressV1`
  - `version: u8`
  - `npk: [u8; 32]`
  - `vpk: Vec<u8>`
- `ReceiptV1`
  - `version: u8`
  - `program_id: [u32; 8]`
  - `tx_hash: [u8; 32]`
  - `privacy_tx_borsh_base64: String`
  - `created_at: u64`

Current protocol defaults:

- receiving address HRP: `lssa_recv`
- receiving address version: `1`
- payment note version: `1`
- receipt version: `1`
- maximum message length: `4096` bytes

## Guest program behavior

The guest binary handles two instructions.

### `Init`

`Init` expects exactly one authorized private account. The account must still be
equal to `Account::default()`. The guest writes an empty `PaymentNoteDataV1`
payload into the account data and claims the account for this program.

### `Send`

`Send` expects exactly two private accounts:

- an authorized sender account already owned by this program
- an uninitialized recipient account

The guest:

1. Checks authorization and account shape.
2. Rejects reused recipient accounts.
3. Rejects oversized messages through `PaymentNoteDataV1::new`.
4. Subtracts `amount` from the sender balance.
5. Resets the sender note data to an empty payload.
6. Sets the recipient balance to `amount`.
7. Stores the human message inside the recipient's encrypted note data.
8. Claims the recipient account for this program.

## Verification model

The CLI verifies receipts locally by:

1. Checking the receipt version and embedded program ID.
2. Recomputing the transaction hash from the serialized payload.
3. Decrypting candidate private outputs with the receiver's wallet keys.
4. Selecting the decrypted note owned by the Tribute to Talk program.
5. Parsing `PaymentNoteDataV1`.
6. Rebuilding the privacy-preserving circuit output.
7. Verifying the Risc0 receipt against `PRIVACY_PRESERVING_CIRCUIT_ID`.

`--check-chain` adds a live sequencer lookup after those local checks.

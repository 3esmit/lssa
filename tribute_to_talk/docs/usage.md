# Usage guide

This guide shows how to run `tribute_to_talk_cli` end to end.

## Prerequisites

Before you use the CLI, make sure:

1. You installed the repository dependencies described in the root
   [project README](../../README.md#install-dependencies).
2. You started the chain services described in
   [run the sequencer and node](../../README.md#run-the-sequencer-and-node).
3. You have a wallet home directory with working config and keys.
4. You exported `NSSA_WALLET_HOME_DIR` so both `wallet` and
   `tribute_to_talk_cli` use the same local wallet storage.

For example:

```bash
export NSSA_WALLET_HOME_DIR=/absolute/path/to/your/wallet-home
wallet check-health
```

Install the CLI if you want a standalone binary:

```bash
cargo install --path tribute_to_talk/cli --force
```

If you prefer to run it without installing, use:

```bash
cargo run --manifest-path tribute_to_talk/cli/Cargo.toml -- <command>
```

The installed binary name is `tribute_to_talk_cli`.

## Command reference

### Deploy the program

Use `deploy` once per environment to submit the embedded guest program.

```bash
tribute_to_talk_cli deploy
```

The command prints:

- The embedded program ID as a hex string
- The deployment transaction hash

`verify` later checks that a receipt was created with the same embedded program
ID as the binary you are running.

### Create a receiving address

Use `address new` in the receiver's wallet.

```bash
tribute_to_talk_cli address new
```

The command:

- Creates a new private account in wallet storage
- Persists that account locally
- Prints the new `Private/...` account ID
- Prints a Bech32m receiving address with the `lssa_recv` HRP
- Prints the raw `npk` and `vpk` values

Example output:

```text
Generated receiving address for account_id
Private/7EDHyxejuynBpmbLuiEym9HMUyCYxZDuF8X3B89ADeMr at path /0
Address lssa_recv1...
npk 8c5f...
vpk 03ab...
```

Create a new receiving address for every payment request. Once a payment uses
that address, the underlying private account is no longer uninitialized.

### Initialize a sender account

`send` only works from a private account that is already owned by this program.
Initialize that account once before you fund it.

```bash
tribute_to_talk_cli account init \
  --account-id Private/<sender-account-id>
```

Important details:

- The account ID must include the `Private/` prefix.
- The account must still be uninitialized when you run this command.
- The command updates the local wallet store after it fetches and decodes the
  resulting private transaction.

### Fund the sender account

After initialization, fund the sender account with the wallet's authenticated
transfer flow.

Example from a public funded account:

```bash
wallet auth-transfer send \
  --from Public/<funded-public-account-id> \
  --to Private/<sender-account-id> \
  --amount 500
```

You can also fund it from another private account if you already control one.
If you need a broader wallet walkthrough, see the
[program deployment tutorial](../../examples/program_deployment/README.md).

### Send a payment and write a receipt

Use `send` in the sender's wallet:

```bash
tribute_to_talk_cli send \
  --from Private/<sender-account-id> \
  --to lssa_recv1<receiver-address> \
  --amount 125 \
  --message "Invoice March 2026 #1234" \
  --receipt-out ./payment-receipt.json
```

The command:

- Decodes the receiving address into the receiver's public keys
- Builds a private-to-private payment transaction
- Proves the transaction locally
- Polls the sequencer for the full privacy-preserving transaction
- Updates the sender's local private account state
- Writes a pretty-printed `ReceiptV1` JSON file

Constraints:

- `--from` must be a `Private/...` account already initialized under this
  program
- `--to` must be a fresh `lssa_recv1...` address
- `--amount` is a native-balance `u128` amount
- `--message` must be `4096` bytes or smaller

Output:

```text
Transaction hash: <hash>
Receipt written to ./payment-receipt.json
```

### Verify a receipt

Use `verify` in the receiver's wallet:

```bash
tribute_to_talk_cli verify \
  --receipt ./payment-receipt.json \
  --account-id Private/<receiver-account-id>
```

Add `--check-chain` if you also want a sequencer lookup by transaction hash:

```bash
tribute_to_talk_cli verify \
  --receipt ./payment-receipt.json \
  --account-id Private/<receiver-account-id> \
  --check-chain
```

The command:

- Reads the receipt JSON
- Checks the receipt version and embedded program ID
- Recomputes the transaction hash from the embedded Borsh payload
- Uses the receiver's wallet keys to decrypt the matching private output
- Parses the decrypted payment note
- Re-verifies the privacy-preserving proof locally
- Optionally looks up the transaction hash on the sequencer

Successful output looks like:

```text
Receipt verified for Private/<receiver-account-id>
Amount: 125
Message: Invoice March 2026 #1234
Transaction hash: <hash>
Chain inclusion: confirmed by transaction lookup
```

The verification wallet must contain the same private account that created the
receiving address. A different wallet cannot decrypt the note.

## Worked example

This example shows the complete happy path with placeholders you can replace.

1. Create a receiver address:

   ```bash
   tribute_to_talk_cli address new
   ```

   Save:

   - `Private/<receiver-account-id>`
   - `lssa_recv1<receiver-address>`

2. Create and initialize a sender account:

   ```bash
   wallet account new private

   tribute_to_talk_cli account init \
     --account-id Private/<sender-account-id>
   ```

3. Fund the sender account:

   ```bash
   wallet auth-transfer send \
     --from Public/<funded-public-account-id> \
     --to Private/<sender-account-id> \
     --amount 500
   ```

4. Send the payment:

   ```bash
   tribute_to_talk_cli send \
     --from Private/<sender-account-id> \
     --to lssa_recv1<receiver-address> \
     --amount 125 \
     --message "Invoice March 2026 #1234" \
     --receipt-out ./payment-receipt.json
   ```

5. Share `payment-receipt.json` with the receiver.

6. Verify the receipt:

   ```bash
   tribute_to_talk_cli verify \
     --receipt ./payment-receipt.json \
     --account-id Private/<receiver-account-id> \
     --check-chain
   ```

## Operational notes

- The first build and the first proving run can take time because Risc0 methods
  are compiled and proofs are generated locally.
- `verify --check-chain` confirms that the sequencer can still retrieve the
  transaction by hash. It does not add a separate sender identity check.
- If you need to inspect receipt contents, see
  [receipt and verification](receipt-and-verification.md).

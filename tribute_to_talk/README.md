# Tribute to Talk

`tribute_to_talk` is a standalone LEZ program and CLI for sending
private-to-private native-balance payments with a human message and a portable
receipt.

The receiver creates a one-time receiving address. The sender pays that address
with a message. The CLI writes a JSON receipt that the sender can pass through
email or any other channel. The receiver later verifies that receipt offline
with the wallet keys that created the receiving address.

This tool is for private account flows only in v1. It does not add a separate
sender signature layer beyond the payment proof itself.

## Product context

Tribute to Talk is inspired by one of Satoshi Nakamoto's original suggested
use cases for Bitcoin: an economics-based anti-spam filter for receiving
messages and cold contact requests.

This MVP intentionally uses the lightest version of that idea. Instead of
placing a tribute into escrow and waiting for the receiver to accept it, the
sender bypasses the tribute paywall by making a private transaction directly.
The receiver does not need to participate in that payment flow. Once tribute is
paid, the line of communication is opened.

This simplification was chosen both to ship an initial version under current
technical constraints and to preserve plausible deniability around the payment
flow, rather than creating a separate acceptance event on-chain. The longer
term direction is a richer anonymous escrow design so users can do more than
filter spam. The goal is to let them also monetize their time and attention
through future versions of Tribute to Talk.

## How it works

1. The receiver runs `address new` to create a fresh private wallet account and
   encode its nullifier and viewing public keys as a Bech32m
   `lssa_recv1...` address.
2. The sender initializes a private account under this program and funds that
   account through the existing wallet native-balance transfer flow.
3. The sender runs `send` with the receiving address, amount, and message.
4. The CLI fetches the privacy-preserving transaction and writes a portable
   `ReceiptV1` JSON file.
5. The receiver runs `verify` with the receipt and the original private
   account. The CLI decrypts the matching note locally and re-verifies the
   privacy proof.

## Quickstart

The commands below assume you are in the repository root.

1. Install the repository prerequisites.

   Follow the dependency setup in the root
   [project README](../README.md#install-dependencies), including Rust and
   Risc0.

2. Install the required CLIs.

   ```bash
   cargo install --path wallet --force
   cargo install --path tribute_to_talk/cli --force
   ```

   If you prefer not to install the Tribute to Talk binary, replace
   `tribute_to_talk_cli` below with:

   ```bash
   cargo run --manifest-path tribute_to_talk/cli/Cargo.toml --
   ```

3. Start the chain services and point both CLIs at the same wallet home.

   Follow the root
   [run the sequencer and node](../README.md#run-the-sequencer-and-node)
   instructions, then set:

   ```bash
   export NSSA_WALLET_HOME_DIR=/absolute/path/to/your/wallet-home
   wallet check-health
   ```

4. Deploy the Tribute to Talk program.

   ```bash
   tribute_to_talk_cli deploy
   ```

5. Create a receiving address in the receiver's wallet.

   ```bash
   tribute_to_talk_cli address new
   ```

   Save both the `Private/...` account ID and the `lssa_recv1...` address. The
   address is single-use.

6. Create and initialize a sender account.

   ```bash
   wallet account new private
   tribute_to_talk_cli account init \
     --account-id Private/<sender-account-id>
   ```

   Run `account init` before funding the sender account. It only succeeds for
   an uninitialized private account.

7. Fund the sender account with the wallet's authenticated transfer flow.

   ```bash
   wallet auth-transfer send \
     --from Public/<funded-public-account-id> \
     --to Private/<sender-account-id> \
     --amount 500
   ```

   If you need a walkthrough for creating and syncing private accounts, see the
   private execution sections in the
   [program deployment tutorial](../examples/program_deployment/README.md).

8. Send the payment and write a receipt.

   ```bash
   tribute_to_talk_cli send \
     --from Private/<sender-account-id> \
     --to lssa_recv1<receiver-address> \
     --amount 125 \
     --message "Invoice March 2026 #1234" \
     --receipt-out ./payment-receipt.json
   ```

9. Send `payment-receipt.json` to the receiver through your preferred channel.

10. Verify the receipt in the receiver's wallet.

    ```bash
    tribute_to_talk_cli verify \
      --receipt ./payment-receipt.json \
      --account-id Private/<receiver-account-id> \
      --check-chain
    ```

    `--check-chain` performs a sequencer lookup for the transaction hash after
    local verification succeeds.

## Key defaults and limits

- Payments are private-to-private only in v1.
- The receiving address HRP is `lssa_recv`.
- Messages are UTF-8 text and must be `4096` bytes or smaller.
- Receiving addresses are single-use and must point to an uninitialized private
  account.
- The receiver must verify with the same wallet keys that created the receiving
  address.

## More documentation

- [Usage guide](docs/usage.md)
- [Receipt and verification](docs/receipt-and-verification.md)
- [Privacy model and limitations](docs/privacy-and-limitations.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Development notes](docs/development.md)

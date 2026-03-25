# Privacy model and limitations

This page explains what the Tribute to Talk flow hides, what it does
not hide, and what limits apply in v1.

## Privacy model

The program uses LEZ private accounts and privacy-preserving transactions. The
sender executes the payment locally, generates a proof, and submits encrypted
private post-states instead of plaintext account values.

The receiver shares a one-time receiving address that contains:

- A nullifier public key
- A viewing public key
- A version byte encoded under the `lssa_recv` Bech32m HRP

The sender uses those public keys to create an encrypted recipient note. The
human message is stored inside that note, not in public account data.

## What v1 keeps private

In the intended v1 flow:

- The sender spends from a private account
- The receiver is represented by a one-time private receiving address
- The human message is encrypted inside the receiver's private note
- Receipt verification requires the receiver's local wallet keys

This keeps sender and receiver account contents private at the protocol level.

## What v1 does not hide

This tool does not attempt to hide everything:

- The receipt file itself can be copied, forwarded, logged, or stored by any
  system that handles it
- Whoever receives the receipt gains the full encrypted transaction bundle in
  `privacy_tx_borsh_base64`
- The communication channel used to share the receipt, such as email, remains
  outside the scope of this program
- Email headers, inbox logs, attachment names, and similar metadata are not
  protected by the blockchain transaction
- `verify --check-chain` reveals the transaction hash to the sequencer during
  lookup

The receipt alone does not reveal the plaintext message unless the holder also
controls the correct receiver keys, but it is still sensitive material.

## Why the MVP uses direct payment instead of escrow

Tribute to Talk is inspired by the idea of using economics as an anti-spam
filter for unsolicited messages and cold contact requests.

The current MVP is deliberately simpler than a full escrow design. Instead of
locking a tribute in escrow until the receiver accepts it, the sender makes a
private payment directly to open the communication path. The receiver does not
need to take any action during that payment step.

This choice keeps the first release light, fits current technical constraints,
and avoids a separate public acceptance event that would reduce plausible
deniability around the payment flow. Future iterations can move toward an
anonymous escrow model that supports stronger monetization of time and
attention, not just spam filtering.

## Limits in v1

Version 1 intentionally keeps the scope narrow:

- Private-to-private native-balance payments only
- Single-use receiving addresses only
- Message size capped at `4096` bytes
- No token support
- No reusable sender identity layer
- No extra sender signature artifact beyond the payment proof

The sender account must be initialized under this program before it is funded.
`account init` only accepts an uninitialized private account.

## Operational guidance

Use these habits if you want the cleanest privacy properties:

1. Create a fresh receiving address for every payment request.
2. Share receipts only with the intended receiver.
3. Keep `NSSA_WALLET_HOME_DIR` private and backed up like any other wallet
   secret material.
4. Treat the receipt as sensitive, even though the message itself stays
   encrypted for non-receivers.
5. Use `--check-chain` only when you want a live sequencer lookup in addition
   to local proof verification.

# Receipt and verification

The sender-side `send` command writes a portable `ReceiptV1` JSON file. The
receiver can carry that file across machines or communication channels and
verify it later with local wallet keys.

## What the receipt contains

`ReceiptV1` has five fields:

- `version`: receipt schema version. The current value is `1`.
- `program_id`: the Tribute to Talk program ID as eight `u32` words.
- `tx_hash`: the transaction hash as 32 raw bytes.
- `privacy_tx_borsh_base64`: the full privacy-preserving transaction,
  Borsh-serialized and Base64-encoded.
- `created_at`: the local creation time in Unix milliseconds.

The receipt is not just a summary. It carries the full privacy-preserving
transaction payload needed for offline verification.

## Example receipt

This example is shortened for readability, but the field names match the actual
JSON output:

```jsonc
{
  "version": 1,
  "program_id": [2712847312, 18273645, 0, 0, 0, 0, 0, 0],
  "tx_hash": [
    203, 71, 19, 88, 14, 117, 91, 42,
    9, 166, 18, 210, 44, 8, 77, 191,
    123, 54, 11, 93, 17, 220, 201, 5,
    44, 7, 65, 39, 80, 91, 201, 13
  ],
  "privacy_tx_borsh_base64": "AAABAAIA...redacted...",
  "created_at": 1774348800123
}
```

## What verification does

When the receiver runs:

```bash
tribute_to_talk_cli verify \
  --receipt ./payment-receipt.json \
  --account-id Private/<receiver-account-id> \
  --check-chain
```

the CLI performs these checks:

1. It parses the receipt JSON and checks `version == 1`.
2. It checks that `program_id` matches the embedded Tribute to Talk
   program in the current binary.
3. It decodes `privacy_tx_borsh_base64`, deserializes the private transaction,
   and recomputes the transaction hash.
4. It loads the receiver's key material from local wallet storage.
5. It scans the encrypted private outputs, tries the matching receiver view tag,
   and decrypts candidate notes with the receiver's shared secret.
6. It keeps only the decrypted note owned by the receipt's program ID.
7. It parses `PaymentNoteDataV1` from the decrypted account data.
8. It rebuilds the privacy-preserving circuit output and verifies the Risc0
   proof locally with `PRIVACY_PRESERVING_CIRCUIT_ID`.
9. If `--check-chain` is set, it looks up `tx_hash` on the sequencer and
   confirms the transaction is retrievable.

## What verification proves

If verification succeeds, the receiver has strong local evidence that:

- The receipt was not altered after it was created
- The receipt payload matches the transaction hash it claims
- The receiver can decrypt one output with the specified private account
- The decrypted output belongs to the Tribute to Talk program
- The note contains the displayed amount and message
- The privacy-preserving proof validates against the embedded circuit ID

With `--check-chain`, the receiver also learns that the sequencer can still
return a transaction for that hash.

## What verification does not prove

Verification does not:

- Add a separate sender signature outside the privacy-preserving transaction
- Reveal the sender's private account ID
- Protect the transport channel that carried the receipt
- Guarantee anything about email metadata or message delivery
- Report deeper finality metadata beyond transaction lookup

## Related types

These user-visible types appear in the receipt flow:

- `ReceivingAddressV1`: the Bech32m payment request shared by the receiver
- `PaymentNoteDataV1`: the encrypted note payload that stores the message
- `ReceiptV1`: the portable JSON wrapper around the serialized private
  transaction

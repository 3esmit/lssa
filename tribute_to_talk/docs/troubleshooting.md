# Troubleshooting

This page covers the most common problems when you deploy, send, and verify
Tribute to Talk.

## Invalid receiving address

Symptom:

```text
Invalid receiving address: ...
```

Common causes:

- The address was copied with missing characters
- The address does not start with `lssa_recv`
- The address is from a different format or version

What to do:

1. Ask the receiver to generate a fresh address with
   `tribute_to_talk_cli address new`.
2. Copy the full Bech32m string again.
3. Avoid editing whitespace or line wrapping by hand.

## Message exceeds the maximum size

Symptom:

```text
Message exceeds maximum allowed length of 4096 bytes
```

What to do:

1. Shorten the message.
2. Keep in mind that the limit is bytes, not characters.
3. If you need to send longer context, put a short identifier in the payment
   note and share the larger document separately.

## Sender has insufficient balance

Typical symptom:

The `send` command fails while proving or submitting the transaction because the
guest program rejects the sender balance change.

What to do:

1. Confirm that the sender account was funded after `account init`.
2. Check the sender account state in the wallet.
3. Fund the account again with `wallet auth-transfer send` if needed.

## Recipient address was already used

Typical symptom:

The payment fails because the recipient account is no longer uninitialized.

What to do:

1. Do not reuse a receiving address after one successful payment.
2. Ask the receiver to run `address new` again.

This is expected behavior. The program only accepts fresh recipient accounts.

## `account init` fails

Typical causes:

- The account ID is missing the `Private/` prefix
- The account already has local state or balance
- The account was already initialized under some program

What to do:

1. Create a new private account with `wallet account new private`.
2. Run `tribute_to_talk_cli account init` before any funding transfer.
3. Use the exact `Private/...` account ID printed by the wallet.

## Receipt cannot be decrypted by the receiver

Typical symptoms:

```text
Private account not found in wallet storage
```

or:

```text
Receipt could not be decrypted with the provided receiver account
```

What to do:

1. Verify with the same wallet home that created the receiving address.
2. Make sure `NSSA_WALLET_HOME_DIR` points to that wallet.
3. Confirm that `--account-id` is the original receiver `Private/...` account.

## Receipt transaction hash mismatch

Symptom:

```text
Receipt transaction hash mismatch
```

What it means:

The serialized transaction inside the receipt no longer matches the hash stored
next to it. The file may be truncated, corrupted, or modified.

What to do:

1. Ask the sender for a fresh copy of the receipt file.
2. Compare checksums if you are moving the file across systems.
3. Do not edit the JSON by hand.

## Receipt program ID does not match the binary

Symptom:

```text
Receipt program id ... does not match embedded Tribute to Talk program ...
```

What it means:

The binary used for `verify` embeds a different guest program than the one that
created the receipt.

What to do:

1. Verify with the same `tribute_to_talk_cli` build that the sender
   used.
2. Reinstall the CLI from the same repository revision if necessary.

## `--check-chain` cannot find the transaction

Typical symptoms:

```text
Failed to query sequencer for transaction hash
```

or:

```text
Transaction hash not found on sequencer
```

What to do:

1. Confirm the sequencer is running and reachable.
2. Confirm `NSSA_WALLET_HOME_DIR` points at a wallet config for the same
   environment where the payment was sent.
3. Retry without `--check-chain` if you only need offline verification.

Remember that local receipt verification and sequencer lookup are separate
steps. The local proof and decryption checks can still succeed even if the live
lookup fails.

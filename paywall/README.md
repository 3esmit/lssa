# Paywall: Anonymous Fee-Gated Messenger for LSSA

This standalone `paywall/` workspace implements a fee-gated messaging protocol:

1. An artist publishes a public endpoint with a fixed fee.
2. A payer privately pays that fee and exports a signed message proof bundle (`JSON`).
3. The artist verifies the bundle offline against on-chain state.
4. The artist privately redeems the bundle and withdraws to a private recipient account.

The system is built on chained calls to LSSA native token transfer (`authenticated_transfer`) and uses a global shared pool account for anonymity.

## Build and Deploy

Build the guest program from repository root:

```bash
cargo risczero build --manifest-path paywall/methods/guest/Cargo.toml
```

Deploy it to the sequencer:

```bash
wallet deploy-program paywall/target/riscv32im-risc0-zkvm-elf/docker/paywall.bin
```

## CLI Commands

All flows are exposed through `paywall_cli`:

```bash
cargo run --manifest-path paywall/Cargo.toml --bin paywall_cli -- <COMMAND> [ARGS...]
```

### 1) Artist: Initialize State + Pool

```bash
cargo run --manifest-path paywall/Cargo.toml --bin paywall_cli -- \
  artist-init \
  --state-account Public/<STATE_ACCOUNT>
```

If `--state-account` is omitted, the CLI creates one. If `--pool-account` is omitted, the CLI derives the pool PDA from paywall program ID and fixed seed.

### 2) Artist: Create Fee Endpoint

```bash
cargo run --manifest-path paywall/Cargo.toml --bin paywall_cli -- \
  artist-create-endpoint \
  --state-account Public/<STATE_ACCOUNT> \
  --fee <FEE> \
  --descriptor-out ./endpoint.json \
  --endpoint-secret-out ./endpoint-secret.json
```

Outputs:
- `endpoint.json`: public descriptor for payers.
- `endpoint-secret.json`: private material required for artist verification/redeem.

### 3) Payer: Privately Pay + Sign Message

```bash
cargo run --manifest-path paywall/Cargo.toml --bin paywall_cli -- \
  payer-pay-and-sign \
  --descriptor ./endpoint.json \
  --payer-account Private/<PAYER_ACCOUNT> \
  --message "<MESSAGE_TEXT>" \
  --bundle-out ./proof-bundle.json
```

This submits a private pay transaction and exports `proof-bundle.json` for off-chain delivery to the artist.

### 4) Artist: Verify Bundle Offline Against On-Chain State

```bash
cargo run --manifest-path paywall/Cargo.toml --bin paywall_cli -- \
  artist-verify-bundle \
  --state-account Public/<STATE_ACCOUNT> \
  --endpoint-secret ./endpoint-secret.json \
  --bundle ./proof-bundle.json
```

Checks:
- endpoint exists,
- bundle signature is valid,
- commitment is present,
- nullifier is not yet consumed.

### 5) Artist: Redeem Bundle Privately

```bash
cargo run --manifest-path paywall/Cargo.toml --bin paywall_cli -- \
  artist-redeem-bundle \
  --state-account Public/<STATE_ACCOUNT> \
  --endpoint-secret ./endpoint-secret.json \
  --bundle ./proof-bundle.json \
  --recipient-private-account Private/<ARTIST_PRIVATE_ACCOUNT>
```

This consumes the ticket and withdraws fee from the shared pool to a private recipient account.

### 6) Wallet Private Sync Helper

```bash
cargo run --manifest-path paywall/Cargo.toml --bin paywall_cli -- wallet-sync-private
```

Run this after private transactions to refresh local private account state.

## Privacy Model Summary

- Payer -> artist payment is private (no public linkage from payer wallet to endpoint).
- Artist payout destination is private.
- Artist learns message + valid paid ticket, not payer wallet identity.
- Replay is prevented by on-chain nullifiers.

## Development Checks

```bash
cargo check --manifest-path paywall/Cargo.toml
cargo test --manifest-path paywall/Cargo.toml
cargo risczero build --manifest-path paywall/methods/guest/Cargo.toml
```

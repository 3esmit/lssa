# Paywall Program Specification

## Goal

Provide an anonymous fee-gated messaging primitive on LSSA:

1. Artist publishes a public endpoint (`endpoint_address`) with fixed `fee`.
2. Payer privately pays fee and creates a message proof bundle.
3. Artist verifies bundle and privately redeems to a private account.

## State

```rust
struct PaywallState {
    pool_account_id: AccountId,
    endpoints: Vec<EndpointConfig>,
    commitments: Vec<[u8; 32]>,
    nullifiers: Vec<[u8; 32]>,
}

struct EndpointConfig {
    endpoint_address: [u8; 32],
    fee: u128,
}
```

## Hash Domains

- Endpoint address:
  - `H("LSSA_PAYWALL_V1_ENDPOINT" || endpoint_secret)`
- Ticket commitment:
  - `H("LSSA_PAYWALL_V1_TICKET" || payment_secret || endpoint_address || fee || message_hash || payer_pseudonym_pubkey)`
- Nullifier:
  - `H("LSSA_PAYWALL_V1_NULLIFIER" || payment_secret || endpoint_secret)`

## Instructions

## `Initialize { pool_account_id }`

- Accounts: `[state_account]`
- Behavior:
  - requires empty `state_account.data`
  - stores initial `PaywallState`
  - claims `state_account` if default owner.

## `RegisterEndpoint { endpoint_address, fee }`

- Accounts: `[state_account]`
- Behavior:
  - requires `fee > 0`
  - requires unique `endpoint_address`
  - appends endpoint config.

## `PayForMessage { endpoint_address, payment_secret, message_hash, payer_pseudonym_pubkey }`

- Accounts: `[state_account, payer_private_account, pool_public_account]`
- Behavior:
  - resolves `fee` from endpoint registry
  - validates `pool_public_account == state.pool_account_id`
  - computes and appends commitment
  - emits chained call to sender program owner (expected `authenticated_transfer`) to transfer `fee` from payer private account to pool public account.

## `ConsumeAndWithdraw { endpoint_secret, payment_secret, message_hash, payer_pseudonym_pubkey }`

- Accounts: `[state_account, pool_public_account, artist_private_recipient_account]`
- Behavior:
  - derives endpoint address from `endpoint_secret`
  - resolves `fee` from endpoint registry
  - recomputes commitment and requires membership
  - recomputes nullifier and requires absence
  - appends nullifier
  - emits chained call to sender program owner (expected `authenticated_transfer`) to transfer `fee` from pool public account to artist private recipient account
  - pool sender authorization is granted via fixed PDA seed.

## On-Chain/Off-Chain Interfaces

### Endpoint descriptor (public JSON)

```json
{
  "state_account_id": "<BASE58_ACCOUNT_ID>",
  "pool_account_id": "<BASE58_ACCOUNT_ID>",
  "endpoint_address_hex": "<64-hex>",
  "fee": 123
}
```

### Endpoint secret material (private JSON)

```json
{
  "endpoint_secret_hex": "<64-hex>",
  "endpoint_address_hex": "<64-hex>"
}
```

### Proof bundle (off-chain JSON)

```json
{
  "endpoint_address_hex": "<64-hex>",
  "message": "<text>",
  "message_hash_hex": "<64-hex>",
  "payment_secret_hex": "<64-hex>",
  "payer_pseudonym_pubkey_hex": "<64-hex>",
  "payer_signature_hex": "<128-hex>"
}
```

## Invariants

1. Replay resistance: each ticket can be redeemed once (`nullifiers`).
2. Fee correctness: commitment includes endpoint fee from on-chain registry.
3. Decoupled identities: endpoint identity is not artist payout wallet; payer uses one-time pseudonym key.
4. Conservation of funds delegated to `authenticated_transfer` chained call.

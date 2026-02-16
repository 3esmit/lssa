# Paywall Design Decisions

## 1. Standalone Workspace Scope

Decision:
- Keep implementation self-contained under `paywall/`.

Why:
- Allows independent iteration/testing without changing root workspace membership.

## 2. Global Shared Pool (Not Per-Artist)

Decision:
- Use one global pool account configured in paywall state.

Why:
- Larger anonymity set: deposits from many endpoints share the same liquidity surface.

Trade-off:
- Shared operational risk and larger state coordination requirements.

## 3. Native Token Movement via Chained `authenticated_transfer`

Decision:
- Do not mutate balances directly in paywall guest.
- Emit chained calls to transfer native token balance.

Why:
- Reuses canonical transfer and authorization semantics already enforced by LSSA.

## 4. Pool Authorization by PDA Seed

Decision:
- Redemption path authorizes pool sender using fixed PDA seed in chained call.

Why:
- Allows program-controlled spending of pool funds without exposing a private key.

Constraint:
- Pool account must be the expected PDA for this paywall program ID.

## 5. Endpoint Identity Decoupled from Artist Payout Wallet

Decision:
- Artist publishes endpoint address derived from endpoint secret.
- Artist redeems to a separate private recipient account.

Why:
- Prevents direct endpoint-to-wallet linkage.

## 6. Off-Chain Message Transport with Signed JSON Bundle

Decision:
- Message and payment proof are transported off-chain via JSON (`ProofBundle`).
- Bundle is signed by one-time payer pseudonym key.

Why:
- Keeps message transport simple while preserving cryptographic binding to paid ticket inputs.

## 7. Replay Protection with On-Chain Nullifiers

Decision:
- Store consumed nullifiers in paywall state vectors.

Why:
- Enforces one-time redemption for each `(payment_secret, endpoint_secret)` pair.

Trade-off:
- Vector-backed state grows over time; acceptable for MVP.

## 8. Data Structures: Vector Registries (MVP)

Decision:
- Use `Vec` for endpoints/commitments/nullifiers.

Why:
- Simple implementation and auditability for initial release.

Trade-off:
- No logarithmic proofs yet (future move to Merkle structures).

## 9. Verification Direction

Decision:
- Payer proves payment to artist.
- No artist acknowledgment flow back to payer.

Why:
- Matches main use case: artist filters messages by verified fee payment.

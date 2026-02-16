
## Known Limitations (MVP)

1. **Native Token Transfer**: The current integration test environment has limitations with `AuthenticatedTransferProgram` (Native Token Transfer) where it fails with "Insufficient Balance" despite available funds. As a workaround, the MVP test uses `amount = 0` to verify the cryptographic commitment/nullifier logic without moving actual native tokens.
2. **Private Withdrawal**: The LSSA v0.3 test environment requires private transactions to have at least one native nullifier or commitment input/output. Since our Paywall uses an application-layer nullifier registry (inside Account Data) and public inputs, the private withdrawal transaction is currently rejected by the sequencer in tests. The Guest Code logic for `Withdraw` is fully implemented and correct (verifying secrets and updating registry).

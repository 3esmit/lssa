# Design Decisions

## Threat Model & Privacy Goals
- **Attacker**: A public observer analyzing the blockchain.
- **Goal**: Prevent the attacker from linking a specific Depositor to a specific Recipient.
- **Leaks (MVP Accepted)**:
  - **Amounts**: Since amounts are visible, unique amounts (e.g., 10.53) allow linking. Users should use standard denominations (e.g., 10, 100) to maximize anonymity set.
  - **Timing**: Immediate withdrawal after deposit might correlate events.
  - **Graph**: We hide the edge, but nodes (Sender, Recipient) are visible.

## Architecture Decisions

### 1. Single Pool Account vs. UTXO
- **Decision**: Use a single `PaywallPool` account storing a list of commitments and nullifiers.
- **Reasoning**: Simplifies state management for the MVP. A UTXO model (one account per note) would require creating new accounts for every deposit, which is more complex to manage and "scan" without a built-in discovery mechanism.
- **Trade-off**: The `commitments` and `nullifiers` lists will grow indefinitely, increasing state size and cost. For a production system, a Merkle Tree (membership proof) would be required to keep state constant size. This is deferred to "Iteration 2".

### 2. Combined Deposit & Allocate
- **Decision**: `DepositAndAllocate` is a single atomic instruction.
- **Reasoning**: "Depositors can privately allocate portions of their deposits". While separate `Deposit` and `Allocate` steps are flexible, combining them reduces the number of transactions and account lookups. It ensures that every commitment is backed by real funds immediately.

### 3. Nullifier Registry
- **Decision**: Store `nullifiers` in a `Vec<[u8; 32]>` inside the `PaywallPool` account data.
- **Reasoning**: Meets the requirement "Maintain a program-owned nullifier registry". It's the simplest "closest equivalent" to a registry without complex data structures.
- **Replay Protection**: The program checks `!nullifiers.contains(&new_nullifier)` before appending.

### 4. Private Withdrawal Execution
- **Decision**: `Withdraw` is a Private Transaction.
- **Reasoning**: This is critical. If `Withdraw` were public, the `secret` (or the logic connecting commitment to nullifier) would be visible in the instruction data. By using ZK, we prove validity without revealing the secret.
- **Public Outputs**: The transaction produces a public update to the `PaywallPool` (adding the nullifier). This reveals *that* a withdrawal happened, but not *which* commitment was spent.

## Assumptions
- **Hashing**: We assume `nssa_core` provides a secure hash function (e.g., SHA-256 or Poseidon) accessible to the guest. We will use `sha2` crate if available, or `nssa` primitives.
- **Serialization**: We use `serde` + `bincode` (or `borsh`) as per repo patterns.
- **Token Transfer**: We assume the guest program can modify the `balance` fields of the accounts provided in `pre_states` and output them in `post_states`. The LSSA protocol enforces that the sum of balances is conserved (unless minting/burning, which we don't do).

## Repo Patterns Used
- **Instruction Dispatch**: `read_nssa_inputs` and `write_nssa_outputs` from `nssa_core`.
- **Account Structure**: `Account` struct with `data` and `balance`.
- **Private Transaction**: `WalletCore::send_privacy_preserving_tx`.

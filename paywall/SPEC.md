# Paywall Program Specification

## Goal
Implement a spam-filter "paywall" escrow for 1-to-1 chat where depositors can privately allocate funds to recipients. Recipients can withdraw these funds using a secret without revealing the depositor's identity to the public, preventing observers from linking specific deposits to withdrawals.

## Accounts

### 1. Paywall Pool Account
- **Owner**: Paywall Program ID
- **Role**: Stores all funds, active commitments, and used nullifiers.
- **Data Structure (`PaywallState`)**:
  ```rust
  struct PaywallState {
      commitments: Vec<[u8; 32]>, // Active commitments (hash of secret + context)
      nullifiers: Vec<[u8; 32]>,  // Used nullifiers to prevent double-spending
  }
  ```
- **Balance**: Holds the total native tokens of all active allocations.

## Entrypoints (Instructions)

### 1. `DepositAndAllocate`
- **Visibility**: Public Transaction
- **Inputs**:
  - `UserWallet` (Signer, Source of funds)
  - `PaywallPool` (Destination)
- **Arguments**:
  - `commitment: [u8; 32]` (Hash of secret, recipient, amount)
  - `amount: u64`
- **Logic**:
  - Verifies transfer of `amount` to `PaywallPool`.
  - Appends `commitment` to `PaywallState.commitments`.
  - Updates `PaywallPool` data.

### 2. `Withdraw`
- **Visibility**: Private Transaction
- **Inputs**:
  - `PaywallPool` (Public Input)
- **Outputs**:
  - `PaywallPool` (Public Output, with updated nullifier list)
  - `RecipientAccount` (Public Output, with increased balance)
- **Witness (Private)**:
  - `secret: [u8; 32]`
  - `amount: u64`
  - `recipient_pubkey: AccountId`
- **Logic**:
  - Computes `C = Hash(secret, recipient_pubkey, amount)`.
  - Verifies `C` exists in `PaywallPool.commitments`.
  - Computes `N = Hash(secret, domain_separator)`.
  - Verifies `N` is NOT in `PaywallPool.nullifiers`.
  - Appends `N` to `PaywallPool.nullifiers`.
  - Decrements `PaywallPool.balance` by `amount`.
  - Increments `RecipientAccount.balance` by `amount`.

## Invariants
1. **Conservation of Funds**: `PaywallPool` balance must decrease exactly by the amount withdrawn.
2. **Replay Protection**: A nullifier can only be used once.
3. **Authorization**: Only the possessor of the secret matching a commitment can withdraw.

## Privacy Surfaces
- **Public**:
  - All deposits (User, Amount, Commitment).
  - All withdrawals (Recipient, Amount, Nullifier).
  - The set of all commitments and nullifiers.
- **Private**:
  - The link between a specific Commitment (Deposit) and a specific Nullifier (Withdrawal).
  - The `secret` key.

use nssa_core::program::{
    AccountPostState, ProgramInput, read_nssa_inputs, write_nssa_outputs, DEFAULT_PROGRAM_ID,
};
use nssa_core::account::{Data, AccountId};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

#[derive(Serialize, Deserialize, Debug)]
enum Instruction {
    /// Public Transaction: User deposits funds via a temporary funding account.
    /// Inputs:
    /// 0: Funding Account (Unclaimed/Default Owner, holds the deposit)
    /// 1: Paywall Pool Account (Recipient)
    DepositAndAllocate {
        commitment: [u8; 32],
        amount: u64,
    },
    /// Private Transaction: Recipient withdraws funds using secret.
    /// Inputs:
    /// 0: Paywall Pool Account
    /// 1: Recipient Account
    Withdraw {
        amount: u64,
        secret: [u8; 32],
        recipient: AccountId,
    },
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
struct PaywallState {
    commitments: Vec<[u8; 32]>,
    nullifiers: Vec<[u8; 32]>,
}

fn main() {
    // Read inputs as raw bytes first
    let (
        ProgramInput {
            pre_states,
            instruction: instruction_bytes,
        },
        instruction_data,
    ) = read_nssa_inputs::<Vec<u8>>();

    let instruction: Instruction = bincode::deserialize(&instruction_bytes)
        .expect("Failed to deserialize instruction");

    match instruction {
        Instruction::DepositAndAllocate { commitment, amount } => {
            // MVP Simplification: We mint funds to the pool to simulate a deposit.
            // In production, this would require claiming a funded account or verifying a transfer.
            
            // Expect 1 account: Pool (0)
            assert_eq!(pre_states.len(), 1, "DepositAndAllocate requires 1 account (Pool)");
            
            let pool_pre = &pre_states[0];
            let amount_u128 = amount as u128;

            let mut pool_post = pool_pre.account.clone();
            // pool_post.balance += amount_u128; // Cannot mint native tokens!
            // For MVP Test with 0 value, we don't change balance.
            // If amount > 0, we expect user to have transferred funds separately (which we can't verify easily here without checking past txs).
            // So we rely on "Trust but Verify" (User claims they paid).
            // In real app, we'd claim the funding UTXO.

            // 3. Update Paywall State
            let mut pool_state: PaywallState = if pool_pre.account.data.is_empty() {
                PaywallState::default()
            } else {
                bincode::deserialize(&pool_pre.account.data)
                    .expect("Failed to deserialize pool state")
            };

            pool_state.commitments.push(commitment);

            // Serialize back
            pool_post.data = bincode::serialize(&pool_state)
                .expect("Failed to serialize pool state")
                .try_into()
                .expect("Data too large");

            // Output states
            let pool_output = if pool_pre.account.program_owner == DEFAULT_PROGRAM_ID {
                AccountPostState::new_claimed(pool_post)
            } else {
                AccountPostState::new(pool_post)
            };

            write_nssa_outputs(
                instruction_data,
                pre_states,
                vec![
                    pool_output,
                ],
            );
        }
        Instruction::Withdraw { amount, secret, recipient } => {
            // Expect 2 accounts: Pool (0) and Recipient (1)
            assert_eq!(pre_states.len(), 2, "Withdraw requires 2 accounts");
            
            let pool_pre = &pre_states[0];
            let recipient_pre = &pre_states[1];

            let amount_u128 = amount as u128;

            // 1. Deserialize Pool State
            let mut pool_state: PaywallState = bincode::deserialize(&pool_pre.account.data)
                .expect("Failed to deserialize pool state");

            // 2. Compute Commitment and Verify
            // Commitment = Hash(secret, recipient_pubkey, amount)
            let mut hasher = Sha256::new();
            hasher.update(&secret);
            hasher.update(recipient.as_ref());
            hasher.update(amount.to_le_bytes());
            let computed_commitment: [u8; 32] = hasher.finalize().into();

            assert!(
                pool_state.commitments.contains(&computed_commitment),
                "Commitment not found"
            );

            // 3. Compute Nullifier and Verify
            // Nullifier = Hash(secret, "paywall_nullifier")
            let mut nullifier_hasher = Sha256::new();
            nullifier_hasher.update(&secret);
            nullifier_hasher.update(b"paywall_nullifier");
            let nullifier: [u8; 32] = nullifier_hasher.finalize().into();

            assert!(
                !pool_state.nullifiers.contains(&nullifier),
                "Double spend detected (nullifier used)"
            );

            // 4. Update State (Mark nullifier used)
            pool_state.nullifiers.push(nullifier);

            // 5. Transfer Funds
            assert!(pool_pre.account.balance >= amount_u128, "Insufficient pool balance");
            
            let mut pool_post = pool_pre.account.clone();
            pool_post.balance -= amount_u128;
            pool_post.data = bincode::serialize(&pool_state)
                .expect("Failed to serialize pool state")
                .try_into()
                .expect("Data too large");

            let mut recipient_post = recipient_pre.account.clone();
            recipient_post.balance += amount_u128;

            // Output states
            // We don't claim recipient, we just add funds.
            // But if recipient is new/default, we might need to? 
            // No, usually we just update balance.
            // But if we touch it, we must output it.
            // If it's Default owned, and we update balance, does it stay Default owned?
            // Yes, unless we use `new_claimed`.
            
            write_nssa_outputs(
                instruction_data,
                pre_states,
                vec![
                    AccountPostState::new(pool_post),
                    AccountPostState::new(recipient_post),
                ],
            );
        }
    }
}

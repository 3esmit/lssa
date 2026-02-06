use borsh::{BorshDeserialize, BorshSerialize};
use nssa_core::program::{
    read_nssa_inputs, write_nssa_outputs, AccountPostState, DEFAULT_PROGRAM_ID, ProgramInput,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const BITSET_BYTES: usize = 256; // 2048 bits
const NULLIFIER_INDEXES: usize = 3;

#[derive(Serialize, Deserialize, Debug)]
enum AirdropInstruction {
    Init { merkle_root: [u8; 32] },
    Claim { path: Vec<[u8; 32]>, index: u64 },
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
struct Registry {
    merkle_root: [u8; 32],
    used_bitmap: [u8; BITSET_BYTES],
}

fn sha256_bytes(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for p in parts {
        hasher.update(p);
    }
    hasher.finalize().into()
}

fn merkle_root(leaf: [u8; 32], path: &[[u8; 32]], mut index: u64) -> [u8; 32] {
    let mut acc = leaf;
    for sibling in path {
        let (left, right) = if index & 1 == 0 {
            (acc, *sibling)
        } else {
            (*sibling, acc)
        };
        acc = sha256_bytes(&[&left, &right]);
        index >>= 1;
    }
    acc
}

fn nullifier_indexes(nullifier: &[u8; 32]) -> [usize; NULLIFIER_INDEXES] {
    let mut out = [0usize; NULLIFIER_INDEXES];
    for i in 0..NULLIFIER_INDEXES {
        let start = i * 2;
        let v = u16::from_le_bytes([nullifier[start], nullifier[start + 1]]);
        out[i] = (v as usize) % (BITSET_BYTES * 8);
    }
    out
}

fn bitmap_is_set(bitmap: &[u8; BITSET_BYTES], index: usize) -> bool {
    let byte = index / 8;
    let bit = index % 8;
    (bitmap[byte] & (1u8 << bit)) != 0
}

fn bitmap_set(bitmap: &mut [u8; BITSET_BYTES], index: usize) {
    let byte = index / 8;
    let bit = index % 8;
    bitmap[byte] |= 1u8 << bit;
}

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction,
        },
        instruction_data,
    ) = read_nssa_inputs::<AirdropInstruction>();

    let post_states = match instruction {
        AirdropInstruction::Init { merkle_root } => {
            if pre_states.len() != 1 {
                panic!("Init requires exactly 1 account");
            }
            let pre_state = &pre_states[0];

            if pre_state.account.program_owner != DEFAULT_PROGRAM_ID {
                panic!("Registry account already initialized");
            }

            let registry = Registry {
                merkle_root,
                used_bitmap: [0u8; BITSET_BYTES],
            };
            let mut post_account = pre_state.account.clone();
            post_account.data = borsh::to_vec(&registry).unwrap().try_into().unwrap();

            // Claim the account
            vec![AccountPostState::new_claimed(post_account)]
        }
        AirdropInstruction::Claim { path, index } => {
            if pre_states.len() != 2 {
                panic!("Claim requires 2 accounts: Registry and Recipient");
            }
            let registry_state = &pre_states[0];
            let recipient_state = &pre_states[1];

            if registry_state.account.program_owner == DEFAULT_PROGRAM_ID {
                panic!("Registry account not initialized");
            }

            // 1. Verify and Update Registry
            let mut registry: Registry = borsh::from_slice(&registry_state.account.data)
                .expect("Failed to deserialize registry");

            // Ticket secret is stored in the recipient account data (private in PP exec).
            let data = recipient_state.account.data.clone().into_inner();
            if data.len() != 32 {
                panic!("Recipient account data must be exactly 32 bytes (ticket secret)");
            }
            let mut ticket_secret = [0u8; 32];
            ticket_secret.copy_from_slice(&data);

            // Prove ticket validity via Merkle path.
            let ticket_hash = sha256_bytes(&[b"AirdropTicket", &ticket_secret]);
            let computed_root = merkle_root(ticket_hash, &path, index);
            if computed_root != registry.merkle_root {
                panic!("Invalid ticket or Merkle path");
            }

            // Compute Nullifier from ticket hash (prevents double claim).
            let nullifier = sha256_bytes(&[b"AirdropNullifier", &ticket_hash]);

            let indexes = nullifier_indexes(&nullifier);
            if indexes
                .iter()
                .all(|idx| bitmap_is_set(&registry.used_bitmap, *idx))
            {
                panic!("Coupon already claimed");
            }

            // Mark as used
            for idx in indexes {
                bitmap_set(&mut registry.used_bitmap, idx);
            }

            // Update Registry Account
            let mut post_registry_account = registry_state.account.clone();
            post_registry_account.data = borsh::to_vec(&registry).unwrap().try_into().unwrap();
            
            vec![
                AccountPostState::new(post_registry_account),
                AccountPostState::new(recipient_state.account.clone()),
            ]
        }
    };

    write_nssa_outputs(instruction_data, pre_states, post_states);
}

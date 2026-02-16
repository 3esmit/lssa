use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata},
    program::{
        AccountPostState, ChainedCall, DEFAULT_PROGRAM_ID, PdaSeed, ProgramInput,
        read_nssa_inputs, write_nssa_outputs, write_nssa_outputs_with_chained_call,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const ENDPOINT_DOMAIN: &[u8] = b"LSSA_PAYWALL_V1_ENDPOINT";
const TICKET_DOMAIN: &[u8] = b"LSSA_PAYWALL_V1_TICKET";
const NULLIFIER_DOMAIN: &[u8] = b"LSSA_PAYWALL_V1_NULLIFIER";
const POOL_PDA_SEED: PdaSeed = PdaSeed::new([91; 32]);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EndpointConfig {
    pub endpoint_address: [u8; 32],
    pub fee: u128,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct PaywallState {
    pub pool_account_id: AccountId,
    pub endpoints: Vec<EndpointConfig>,
    pub commitments: Vec<[u8; 32]>,
    pub nullifiers: Vec<[u8; 32]>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Instruction {
    Initialize {
        pool_account_id: AccountId,
    },
    RegisterEndpoint {
        endpoint_address: [u8; 32],
        fee: u128,
    },
    PayForMessage {
        endpoint_address: [u8; 32],
        payment_secret: [u8; 32],
        message_hash: [u8; 32],
        payer_pseudonym_pubkey: [u8; 32],
    },
    ConsumeAndWithdraw {
        endpoint_secret: [u8; 32],
        payment_secret: [u8; 32],
        message_hash: [u8; 32],
        payer_pseudonym_pubkey: [u8; 32],
    },
}

fn hash_parts(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn derive_endpoint_address(endpoint_secret: [u8; 32]) -> [u8; 32] {
    hash_parts(&[ENDPOINT_DOMAIN, &endpoint_secret])
}

fn derive_payment_commitment(
    payment_secret: [u8; 32],
    endpoint_address: [u8; 32],
    fee: u128,
    message_hash: [u8; 32],
    payer_pseudonym_pubkey: [u8; 32],
) -> [u8; 32] {
    hash_parts(&[
        TICKET_DOMAIN,
        &payment_secret,
        &endpoint_address,
        &fee.to_le_bytes(),
        &message_hash,
        &payer_pseudonym_pubkey,
    ])
}

fn derive_nullifier(payment_secret: [u8; 32], endpoint_secret: [u8; 32]) -> [u8; 32] {
    hash_parts(&[NULLIFIER_DOMAIN, &payment_secret, &endpoint_secret])
}

fn decode_state(account: &Account) -> PaywallState {
    if account.data.is_empty() {
        panic!("paywall state account is not initialized");
    }

    bincode::deserialize(&account.data).expect("failed to deserialize paywall state")
}

fn encode_state(account: &mut Account, state: &PaywallState) {
    account.data = bincode::serialize(state)
        .expect("failed to serialize paywall state")
        .try_into()
        .expect("paywall state too large")
}

fn transfer_program_id_for(sender: &AccountWithMetadata) -> nssa_core::program::ProgramId {
    let program_id = sender.account.program_owner;
    if program_id == DEFAULT_PROGRAM_ID {
        panic!("sender must be owned by a transfer-capable program");
    }
    program_id
}

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction: instruction_bytes,
        },
        instruction_data,
    ) = read_nssa_inputs::<Vec<u8>>();

    let instruction: Instruction =
        bincode::deserialize(&instruction_bytes).expect("failed to deserialize instruction");

    match instruction {
        Instruction::Initialize { pool_account_id } => {
            assert_eq!(
                pre_states.len(),
                1,
                "Initialize expects [state_account] as inputs"
            );

            let state_pre = &pre_states[0];
            let mut state_account = state_pre.account.clone();

            if !state_account.data.is_empty() {
                panic!("state account already initialized");
            }

            let state = PaywallState {
                pool_account_id,
                ..PaywallState::default()
            };
            encode_state(&mut state_account, &state);

            write_nssa_outputs(
                instruction_data,
                pre_states,
                vec![AccountPostState::new_claimed_if_default(state_account)],
            );
        }
        Instruction::RegisterEndpoint {
            endpoint_address,
            fee,
        } => {
            assert_eq!(
                pre_states.len(),
                1,
                "RegisterEndpoint expects [state_account] as inputs"
            );

            if fee == 0 {
                panic!("endpoint fee must be greater than zero");
            }

            let state_pre = &pre_states[0];
            let mut state_account = state_pre.account.clone();
            let mut state = decode_state(&state_account);

            if state
                .endpoints
                .iter()
                .any(|endpoint| endpoint.endpoint_address == endpoint_address)
            {
                panic!("endpoint already registered");
            }

            state.endpoints.push(EndpointConfig {
                endpoint_address,
                fee,
            });
            encode_state(&mut state_account, &state);

            write_nssa_outputs(
                instruction_data,
                pre_states,
                vec![AccountPostState::new(state_account)],
            );
        }
        Instruction::PayForMessage {
            endpoint_address,
            payment_secret,
            message_hash,
            payer_pseudonym_pubkey,
        } => {
            assert_eq!(
                pre_states.len(),
                3,
                "PayForMessage expects [state_account, payer_private_account, pool_public_account]"
            );

            let state_pre = pre_states[0].clone();
            let payer_pre = pre_states[1].clone();
            let pool_pre = pre_states[2].clone();

            let mut state_account = state_pre.account.clone();
            let mut state = decode_state(&state_account);

            if pool_pre.account_id != state.pool_account_id {
                panic!("provided pool account does not match initialized pool account");
            }

            let fee = state
                .endpoints
                .iter()
                .find(|endpoint| endpoint.endpoint_address == endpoint_address)
                .map(|endpoint| endpoint.fee)
                .unwrap_or_else(|| panic!("endpoint not registered"));

            let commitment = derive_payment_commitment(
                payment_secret,
                endpoint_address,
                fee,
                message_hash,
                payer_pseudonym_pubkey,
            );

            if state.commitments.contains(&commitment) {
                panic!("ticket commitment already exists");
            }

            state.commitments.push(commitment);
            encode_state(&mut state_account, &state);

            let chained_call = ChainedCall::new(
                transfer_program_id_for(&payer_pre),
                vec![payer_pre.clone(), pool_pre.clone()],
                &fee,
            );

            write_nssa_outputs_with_chained_call(
                instruction_data,
                pre_states,
                vec![
                    AccountPostState::new(state_account),
                    AccountPostState::new(payer_pre.account),
                    AccountPostState::new(pool_pre.account),
                ],
                vec![chained_call],
            );
        }
        Instruction::ConsumeAndWithdraw {
            endpoint_secret,
            payment_secret,
            message_hash,
            payer_pseudonym_pubkey,
        } => {
            assert_eq!(
                pre_states.len(),
                3,
                "ConsumeAndWithdraw expects [state_account, pool_public_account, artist_private_recipient_account]",
            );

            let state_pre = pre_states[0].clone();
            let pool_pre = pre_states[1].clone();
            let recipient_pre = pre_states[2].clone();

            let mut state_account = state_pre.account.clone();
            let mut state = decode_state(&state_account);

            if pool_pre.account_id != state.pool_account_id {
                panic!("provided pool account does not match initialized pool account");
            }

            let endpoint_address = derive_endpoint_address(endpoint_secret);
            let fee = state
                .endpoints
                .iter()
                .find(|endpoint| endpoint.endpoint_address == endpoint_address)
                .map(|endpoint| endpoint.fee)
                .unwrap_or_else(|| panic!("endpoint does not exist"));

            let commitment = derive_payment_commitment(
                payment_secret,
                endpoint_address,
                fee,
                message_hash,
                payer_pseudonym_pubkey,
            );

            if !state.commitments.contains(&commitment) {
                panic!("ticket commitment not found");
            }

            let nullifier = derive_nullifier(payment_secret, endpoint_secret);
            if state.nullifiers.contains(&nullifier) {
                panic!("ticket already consumed");
            }
            state.nullifiers.push(nullifier);
            encode_state(&mut state_account, &state);

            let mut pool_sender_for_tail_call = pool_pre.clone();
            pool_sender_for_tail_call.is_authorized = true;

            let chained_call = ChainedCall::new(
                transfer_program_id_for(&pool_pre),
                vec![pool_sender_for_tail_call, recipient_pre.clone()],
                &fee,
            )
            .with_pda_seeds(vec![POOL_PDA_SEED]);

            write_nssa_outputs_with_chained_call(
                instruction_data,
                pre_states,
                vec![
                    AccountPostState::new(state_account),
                    AccountPostState::new(pool_pre.account),
                    AccountPostState::new(recipient_pre.account),
                ],
                vec![chained_call],
            );
        }
    }
}

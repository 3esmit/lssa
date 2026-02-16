use anyhow::{bail, Context, Result};
use nssa::{AccountId, PrivateKey, ProgramId, PublicKey, Signature};
use nssa_core::program::PdaSeed;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const ENDPOINT_DOMAIN: &[u8] = b"LSSA_PAYWALL_V1_ENDPOINT";
const TICKET_DOMAIN: &[u8] = b"LSSA_PAYWALL_V1_TICKET";
const NULLIFIER_DOMAIN: &[u8] = b"LSSA_PAYWALL_V1_NULLIFIER";
const BUNDLE_SIGNATURE_DOMAIN: &[u8] = b"LSSA_PAYWALL_V1_BUNDLE_SIG";

pub const POOL_PDA_SEED_BYTES: [u8; 32] = [91; 32];

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EndpointConfig {
    pub endpoint_address: [u8; 32],
    pub fee: u128,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct PaywallState {
    pub pool_account_id: AccountId,
    pub endpoints: Vec<EndpointConfig>,
    pub commitments: Vec<[u8; 32]>,
    pub nullifiers: Vec<[u8; 32]>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EndpointDescriptor {
    pub state_account_id: AccountId,
    pub pool_account_id: AccountId,
    pub endpoint_address_hex: String,
    pub fee: u128,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EndpointSecretMaterial {
    pub endpoint_secret_hex: String,
    pub endpoint_address_hex: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProofBundle {
    pub endpoint_address_hex: String,
    pub message: String,
    pub message_hash_hex: String,
    pub payment_secret_hex: String,
    pub payer_pseudonym_pubkey_hex: String,
    pub payer_signature_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProofBundle {
    pub endpoint_address: [u8; 32],
    pub message_hash: [u8; 32],
    pub payment_secret: [u8; 32],
    pub payer_pseudonym_pubkey: [u8; 32],
    pub payer_signature: Signature,
}

pub fn encode_instruction(instruction: &Instruction) -> Result<Vec<u8>> {
    bincode::serialize(instruction).context("failed to serialize paywall instruction")
}

pub fn decode_state(data: &[u8]) -> Result<PaywallState> {
    if data.is_empty() {
        bail!("paywall state account data is empty")
    }

    bincode::deserialize(data).context("failed to deserialize paywall state")
}

pub fn derive_pool_account_id(paywall_program_id: ProgramId) -> AccountId {
    AccountId::from((&paywall_program_id, &PdaSeed::new(POOL_PDA_SEED_BYTES)))
}

pub fn derive_endpoint_address(endpoint_secret: [u8; 32]) -> [u8; 32] {
    hash_parts(&[ENDPOINT_DOMAIN, &endpoint_secret])
}

pub fn derive_payment_commitment(
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

pub fn derive_nullifier(payment_secret: [u8; 32], endpoint_secret: [u8; 32]) -> [u8; 32] {
    hash_parts(&[NULLIFIER_DOMAIN, &payment_secret, &endpoint_secret])
}

pub fn hash_message(message: &str) -> [u8; 32] {
    hash_parts(&[message.as_bytes()])
}

pub fn bundle_signature_payload_hash(
    endpoint_address: [u8; 32],
    message_hash: [u8; 32],
    payment_secret: [u8; 32],
    payer_pseudonym_pubkey: [u8; 32],
) -> [u8; 32] {
    hash_parts(&[
        BUNDLE_SIGNATURE_DOMAIN,
        &endpoint_address,
        &message_hash,
        &payment_secret,
        &payer_pseudonym_pubkey,
    ])
}

pub fn build_bundle(
    endpoint_address: [u8; 32],
    message: String,
    payment_secret: [u8; 32],
    payer_pseudonym_private_key: &PrivateKey,
) -> ProofBundle {
    let message_hash = hash_message(&message);
    let payer_pseudonym_pubkey = PublicKey::new_from_private_key(payer_pseudonym_private_key);
    let payload_hash = bundle_signature_payload_hash(
        endpoint_address,
        message_hash,
        payment_secret,
        *payer_pseudonym_pubkey.value(),
    );
    let signature = Signature::new(payer_pseudonym_private_key, &payload_hash);

    ProofBundle {
        endpoint_address_hex: hex::encode(endpoint_address),
        message,
        message_hash_hex: hex::encode(message_hash),
        payment_secret_hex: hex::encode(payment_secret),
        payer_pseudonym_pubkey_hex: hex::encode(payer_pseudonym_pubkey.value()),
        payer_signature_hex: hex::encode(signature.value),
    }
}

pub fn parse_and_verify_bundle(bundle: &ProofBundle) -> Result<ParsedProofBundle> {
    let endpoint_address = decode_hex_array::<32>(&bundle.endpoint_address_hex)
        .context("invalid endpoint_address_hex in proof bundle")?;
    let message_hash = decode_hex_array::<32>(&bundle.message_hash_hex)
        .context("invalid message_hash_hex in proof bundle")?;
    let payment_secret = decode_hex_array::<32>(&bundle.payment_secret_hex)
        .context("invalid payment_secret_hex in proof bundle")?;
    let payer_pseudonym_pubkey = decode_hex_array::<32>(&bundle.payer_pseudonym_pubkey_hex)
        .context("invalid payer_pseudonym_pubkey_hex in proof bundle")?;
    let payer_signature_value = decode_hex_array::<64>(&bundle.payer_signature_hex)
        .context("invalid payer_signature_hex in proof bundle")?;

    let recomputed_message_hash = hash_message(&bundle.message);
    if recomputed_message_hash != message_hash {
        bail!("message_hash_hex does not match message content")
    }

    let payer_pseudonym_pubkey =
        PublicKey::try_new(payer_pseudonym_pubkey).context("invalid pseudonym public key")?;

    let signature = Signature {
        value: payer_signature_value,
    };

    let payload_hash = bundle_signature_payload_hash(
        endpoint_address,
        message_hash,
        payment_secret,
        *payer_pseudonym_pubkey.value(),
    );

    if !signature.is_valid_for(&payload_hash, &payer_pseudonym_pubkey) {
        bail!("proof bundle signature is invalid")
    }

    Ok(ParsedProofBundle {
        endpoint_address,
        message_hash,
        payment_secret,
        payer_pseudonym_pubkey: *payer_pseudonym_pubkey.value(),
        payer_signature: signature,
    })
}

pub fn compute_bundle_commitment(parsed_bundle: &ParsedProofBundle, fee: u128) -> [u8; 32] {
    derive_payment_commitment(
        parsed_bundle.payment_secret,
        parsed_bundle.endpoint_address,
        fee,
        parsed_bundle.message_hash,
        parsed_bundle.payer_pseudonym_pubkey,
    )
}

pub fn compute_bundle_nullifier(
    parsed_bundle: &ParsedProofBundle,
    endpoint_secret: [u8; 32],
) -> [u8; 32] {
    derive_nullifier(parsed_bundle.payment_secret, endpoint_secret)
}

pub fn endpoint_fee(state: &PaywallState, endpoint_address: [u8; 32]) -> Option<u128> {
    state
        .endpoints
        .iter()
        .find(|endpoint| endpoint.endpoint_address == endpoint_address)
        .map(|endpoint| endpoint.fee)
}

pub fn decode_hex_array<const N: usize>(value: &str) -> Result<[u8; N]> {
    let bytes = hex::decode(value).context("failed to decode hex")?;
    let bytes_len = bytes.len();
    let array = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected {N} bytes, got {bytes_len}"))?;
    Ok(array)
}

fn hash_parts(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_derivation_is_deterministic() {
        let endpoint_secret = [11; 32];
        let payment_secret = [22; 32];
        let message_hash = [33; 32];
        let payer_pseudonym_pubkey = [44; 32];

        let endpoint_address_1 = derive_endpoint_address(endpoint_secret);
        let endpoint_address_2 = derive_endpoint_address(endpoint_secret);
        assert_eq!(endpoint_address_1, endpoint_address_2);

        let commitment_1 = derive_payment_commitment(
            payment_secret,
            endpoint_address_1,
            123,
            message_hash,
            payer_pseudonym_pubkey,
        );
        let commitment_2 = derive_payment_commitment(
            payment_secret,
            endpoint_address_2,
            123,
            message_hash,
            payer_pseudonym_pubkey,
        );
        assert_eq!(commitment_1, commitment_2);

        let nullifier_1 = derive_nullifier(payment_secret, endpoint_secret);
        let nullifier_2 = derive_nullifier(payment_secret, endpoint_secret);
        assert_eq!(nullifier_1, nullifier_2);
    }

    #[test]
    fn bundle_sign_and_verify_success() {
        let endpoint_address = [55; 32];
        let payment_secret = [66; 32];
        let payer_pseudonym_private_key = PrivateKey::new_os_random();

        let bundle = build_bundle(
            endpoint_address,
            "hello artist".to_string(),
            payment_secret,
            &payer_pseudonym_private_key,
        );

        let parsed = parse_and_verify_bundle(&bundle).unwrap();
        assert_eq!(parsed.endpoint_address, endpoint_address);
        assert_eq!(parsed.payment_secret, payment_secret);
    }

    #[test]
    fn bundle_tamper_message_detected() {
        let endpoint_address = [77; 32];
        let payment_secret = [88; 32];
        let payer_pseudonym_private_key = PrivateKey::new_os_random();

        let mut bundle = build_bundle(
            endpoint_address,
            "original message".to_string(),
            payment_secret,
            &payer_pseudonym_private_key,
        );
        bundle.message = "tampered message".to_string();

        assert!(parse_and_verify_bundle(&bundle).is_err());
    }

    #[test]
    fn bundle_tamper_signature_detected() {
        let endpoint_address = [99; 32];
        let payment_secret = [111; 32];
        let payer_pseudonym_private_key = PrivateKey::new_os_random();

        let mut bundle = build_bundle(
            endpoint_address,
            "message".to_string(),
            payment_secret,
            &payer_pseudonym_private_key,
        );
        bundle.payer_signature_hex = hex::encode([0u8; 64]);

        assert!(parse_and_verify_bundle(&bundle).is_err());
    }
}

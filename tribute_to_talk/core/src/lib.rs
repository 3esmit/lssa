use base64::Engine as _;
use bech32::{Bech32m, Hrp};
use borsh::{BorshDeserialize, BorshSerialize};
use nssa_core::account::Data;
use serde::{Deserialize, Serialize};

pub const RECEIVING_ADDRESS_HRP: &str = "lssa_recv";
pub const RECEIVING_ADDRESS_VERSION: u8 = 1;
pub const RECEIPT_VERSION: u8 = 1;
pub const PAYMENT_NOTE_VERSION: u8 = 1;
pub const MAX_MESSAGE_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstructionV1 {
    Init,
    Send { amount: u128, message: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PaymentNoteDataV1 {
    pub version: u8,
    pub message: Vec<u8>,
}

impl PaymentNoteDataV1 {
    pub fn new(message: Vec<u8>) -> Result<Self, MessageError> {
        if message.len() > MAX_MESSAGE_BYTES {
            return Err(MessageError::TooLong {
                actual: message.len(),
                max: MAX_MESSAGE_BYTES,
            });
        }

        Ok(Self {
            version: PAYMENT_NOTE_VERSION,
            message,
        })
    }

    pub fn empty() -> Self {
        Self {
            version: PAYMENT_NOTE_VERSION,
            message: Vec::new(),
        }
    }

    pub fn into_account_data(self) -> Result<Data, MessageError> {
        let bytes = borsh::to_vec(&self).map_err(|err| MessageError::Encode(err.to_string()))?;
        Data::try_from(bytes).map_err(|err| MessageError::Encode(err.to_string()))
    }

    pub fn from_account_data(data: &Data) -> Result<Self, MessageError> {
        borsh::from_slice(data).map_err(|err| MessageError::Decode(err.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceivingAddressV1 {
    pub version: u8,
    pub npk: [u8; 32],
    pub vpk: Vec<u8>,
}

impl ReceivingAddressV1 {
    pub fn new(npk: [u8; 32], vpk: Vec<u8>) -> Result<Self, AddressError> {
        if vpk.len() != 33 {
            return Err(AddressError::WrongVpkLength(vpk.len()));
        }

        Ok(Self {
            version: RECEIVING_ADDRESS_VERSION,
            npk,
            vpk,
        })
    }

    pub fn encode(&self) -> Result<String, AddressError> {
        let hrp = Hrp::parse(RECEIVING_ADDRESS_HRP)
            .map_err(|err| AddressError::Encoding(err.to_string()))?;
        let mut bytes = Vec::with_capacity(66);
        bytes.push(self.version);
        bytes.extend_from_slice(&self.npk);
        bytes.extend_from_slice(&self.vpk);
        bech32::encode::<Bech32m>(hrp, &bytes)
            .map_err(|err| AddressError::Encoding(err.to_string()))
    }

    pub fn decode(value: &str) -> Result<Self, AddressError> {
        let (hrp, payload) =
            bech32::decode(value).map_err(|err| AddressError::Encoding(err.to_string()))?;
        if hrp.as_str() != RECEIVING_ADDRESS_HRP {
            return Err(AddressError::WrongHrp {
                expected: RECEIVING_ADDRESS_HRP,
                actual: hrp.to_string(),
            });
        }

        if payload.len() != 66 {
            return Err(AddressError::WrongLength(payload.len()));
        }

        let version = payload[0];
        if version != RECEIVING_ADDRESS_VERSION {
            return Err(AddressError::WrongVersion(version));
        }

        let mut npk = [0_u8; 32];
        npk.copy_from_slice(&payload[1..33]);

        let vpk = payload[33..66].to_vec();

        Ok(Self { version, npk, vpk })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptV1 {
    pub version: u8,
    pub program_id: [u32; 8],
    pub tx_hash: [u8; 32],
    pub privacy_tx_borsh_base64: String,
    pub created_at: u64,
}

impl ReceiptV1 {
    pub fn new(
        program_id: [u32; 8],
        tx_hash: [u8; 32],
        tx_bytes: Vec<u8>,
        created_at: u64,
    ) -> Self {
        Self {
            version: RECEIPT_VERSION,
            program_id,
            tx_hash,
            privacy_tx_borsh_base64: base64::engine::general_purpose::STANDARD.encode(tx_bytes),
            created_at,
        }
    }

    pub fn tx_bytes(&self) -> Result<Vec<u8>, ReceiptError> {
        base64::engine::general_purpose::STANDARD
            .decode(&self.privacy_tx_borsh_base64)
            .map_err(|err| ReceiptError::Decode(err.to_string()))
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum MessageError {
    #[error("message exceeds maximum allowed length of {max} bytes (got {actual})")]
    TooLong { actual: usize, max: usize },
    #[error("failed to encode payment note data: {0}")]
    Encode(String),
    #[error("failed to decode payment note data: {0}")]
    Decode(String),
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum AddressError {
    #[error("receiving address version mismatch: {0}")]
    WrongVersion(u8),
    #[error("receiving address payload has invalid length: {0}")]
    WrongLength(usize),
    #[error("receiving address VPK must be 33 bytes, got {0}")]
    WrongVpkLength(usize),
    #[error("receiving address HRP mismatch: expected {expected}, got {actual}")]
    WrongHrp { expected: &'static str, actual: String },
    #[error("failed to encode or decode receiving address: {0}")]
    Encoding(String),
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ReceiptError {
    #[error("failed to decode receipt payload: {0}")]
    Decode(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payment_note_roundtrip() {
        let note = PaymentNoteDataV1::new(b"hello".to_vec()).unwrap();
        let data = note.clone().into_account_data().unwrap();
        let decoded = PaymentNoteDataV1::from_account_data(&data).unwrap();
        assert_eq!(decoded, note);
    }

    #[test]
    fn message_limit_rejected() {
        let err = PaymentNoteDataV1::new(vec![0_u8; MAX_MESSAGE_BYTES + 1]).unwrap_err();
        assert!(matches!(err, MessageError::TooLong { .. }));
    }

    #[test]
    fn receiving_address_roundtrip() {
        let address = ReceivingAddressV1::new([7_u8; 32], vec![9_u8; 33]).unwrap();
        let encoded = address.encode().unwrap();
        let decoded = ReceivingAddressV1::decode(&encoded).unwrap();
        assert_eq!(decoded, address);
    }

    #[test]
    fn receipt_roundtrip() {
        let receipt = ReceiptV1::new([1_u32; 8], [2_u8; 32], vec![1, 2, 3], 44);
        let json = serde_json::to_string(&receipt).unwrap();
        let decoded: ReceiptV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, receipt);
        assert_eq!(decoded.tx_bytes().unwrap(), vec![1, 2, 3]);
    }
}

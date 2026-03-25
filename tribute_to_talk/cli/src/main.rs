use std::{path::PathBuf, str::FromStr as _, time::{SystemTime, UNIX_EPOCH}};

use tribute_to_talk_core::{
    InstructionV1, MAX_MESSAGE_BYTES, PaymentNoteDataV1, RECEIPT_VERSION, ReceiptV1,
    ReceivingAddressV1,
};
use anyhow::{Context as _, Result, anyhow, bail, ensure};
use borsh::BorshDeserialize as _;
use clap::{Parser, Subcommand};
use common::{HashType, transaction::NSSATransaction};
use key_protocol::key_management::KeyChain;
use nssa::{
    AccountId, PRIVACY_PRESERVING_CIRCUIT_ID, PrivacyPreservingTransaction,
    ProgramDeploymentTransaction, program::Program, program_deployment_transaction,
    privacy_preserving_transaction::message::EncryptedAccountData,
};
use nssa_core::{
    EncryptionScheme, NullifierPublicKey, PrivacyPreservingCircuitOutput,
    account::Account,
    encryption::shared_key_derivation::Secp256k1Point,
    program::ProgramId,
};
use risc0_zkvm::{InnerReceipt, Receipt};
use sequencer_service_rpc::RpcClient as _;
use wallet::{AccDecodeData, ExecutionFailureKind, PrivacyPreservingAccount, WalletCore};

#[derive(Parser)]
#[command(version, about = "Tribute to Talk CLI")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Deploy,
    #[command(subcommand)]
    Address(AddressCommand),
    #[command(subcommand)]
    Account(AccountCommand),
    Send {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        amount: u128,
        #[arg(long)]
        message: String,
        #[arg(long)]
        receipt_out: PathBuf,
    },
    Verify {
        #[arg(long)]
        receipt: PathBuf,
        #[arg(long)]
        account_id: String,
        #[arg(long)]
        check_chain: bool,
    },
}

#[derive(Subcommand)]
enum AddressCommand {
    New,
}

#[derive(Subcommand)]
enum AccountCommand {
    Init {
        #[arg(long)]
        account_id: String,
    },
}

#[derive(Debug, Clone)]
struct VerifiedNote {
    balance: u128,
    message: Vec<u8>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Deploy => deploy().await,
        Command::Address(command) => match command {
            AddressCommand::New => address_new().await,
        },
        Command::Account(command) => match command {
            AccountCommand::Init { account_id } => account_init(&account_id).await,
        },
        Command::Send {
            from,
            to,
            amount,
            message,
            receipt_out,
        } => send_payment(&from, &to, amount, &message, &receipt_out).await,
        Command::Verify {
            receipt,
            account_id,
            check_chain,
        } => verify_receipt(&receipt, &account_id, check_chain).await,
    }
}

async fn deploy() -> Result<()> {
    let wallet = WalletCore::from_env().context("Failed to load wallet from environment")?;
    let message =
        program_deployment_transaction::Message::new(embedded_program_elf().to_vec());
    let tx = ProgramDeploymentTransaction::new(message);
    let tx_hash = wallet
        .sequencer_client
        .send_transaction(NSSATransaction::ProgramDeployment(tx))
        .await
        .context("Failed to submit program deployment transaction")?;

    println!("Program ID: {}", format_program_id(embedded_program_id()));
    println!("Deployment transaction hash: {tx_hash}");
    Ok(())
}

async fn address_new() -> Result<()> {
    let mut wallet = WalletCore::from_env().context("Failed to load wallet from environment")?;
    let (account_id, chain_index) = wallet.create_new_account_private(None);
    let (key_chain, _) = wallet
        .storage()
        .user_data
        .get_private_account(account_id)
        .cloned()
        .context("Newly created private account missing from wallet storage")?;

    wallet
        .store_persistent_data()
        .await
        .context("Failed to persist wallet storage after creating address")?;

    let address = ReceivingAddressV1::new(
        key_chain.nullifier_public_key.0,
        key_chain.viewing_public_key.0.clone(),
    )?;

    println!(
        "Generated receiving address for account_id Private/{account_id} at path {chain_index}"
    );
    println!("Address {}", address.encode()?);
    println!("npk {}", hex::encode(address.npk));
    println!("vpk {}", hex::encode(address.vpk));

    Ok(())
}

async fn account_init(account_id_input: &str) -> Result<()> {
    let account_id = parse_private_account_id(account_id_input)?;
    let mut wallet = WalletCore::from_env().context("Failed to load wallet from environment")?;
    let program = embedded_program()?;
    let program_id = program.id();
    let instruction_data = Program::serialize_instruction(InstructionV1::Init)
        .context("Failed to serialize init instruction")?;

    let (tx_hash, shared_secrets) = wallet
        .send_privacy_preserving_tx_with_pre_check(
            vec![PrivacyPreservingAccount::PrivateOwned(account_id)],
            instruction_data,
            &program.into(),
            |accounts| {
                let [account] = accounts else {
                    return Err(ExecutionFailureKind::AccountDataError(account_id));
                };
                if **account != Account::default() {
                    return Err(ExecutionFailureKind::AccountDataError(account_id));
                }
                Ok(())
            },
        )
        .await
        .context("Failed to initialize private account under Tribute to Talk program")?;

    let tx = poll_privacy_transaction(&wallet, tx_hash).await?;
    let shared_secret = shared_secrets
        .first()
        .copied()
        .context("Init transaction did not return a sender shared secret")?;

    wallet
        .decode_insert_privacy_preserving_transaction_results(
            &tx,
            &[AccDecodeData::Decode(shared_secret, account_id)],
        )
        .context("Failed to decode local account update for init transaction")?;
    wallet
        .store_persistent_data()
        .await
        .context("Failed to persist wallet storage after init")?;

    println!("Initialized Private/{account_id} under program {}", format_program_id(program_id));
    println!("Transaction hash: {tx_hash}");

    Ok(())
}

async fn send_payment(
    from_input: &str,
    to_input: &str,
    amount: u128,
    message: &str,
    receipt_out: &PathBuf,
) -> Result<()> {
    let from = parse_private_account_id(from_input)?;
    let receiving_address = ReceivingAddressV1::decode(to_input)
        .with_context(|| format!("Invalid receiving address: {to_input}"))?;
    let message_bytes = message.as_bytes().to_vec();
    ensure!(
        message_bytes.len() <= MAX_MESSAGE_BYTES,
        "Message exceeds maximum allowed length of {MAX_MESSAGE_BYTES} bytes"
    );

    let mut wallet = WalletCore::from_env().context("Failed to load wallet from environment")?;
    let program = embedded_program()?;
    let program_id = program.id();
    let to_npk = NullifierPublicKey(receiving_address.npk);
    let to_vpk = Secp256k1Point(receiving_address.vpk.clone());
    let instruction_data = Program::serialize_instruction(InstructionV1::Send {
        amount,
        message: message_bytes,
    })
    .context("Failed to serialize send instruction")?;

    let (tx_hash, shared_secrets) = wallet
        .send_privacy_preserving_tx_with_pre_check(
            vec![
                PrivacyPreservingAccount::PrivateOwned(from),
                PrivacyPreservingAccount::PrivateForeign {
                    npk: to_npk,
                    vpk: to_vpk,
                },
            ],
            instruction_data,
            &program.into(),
            |accounts| {
                let [sender, recipient] = accounts else {
                    return Err(ExecutionFailureKind::AccountDataError(from));
                };
                if sender.program_owner != program_id {
                    return Err(ExecutionFailureKind::AccountDataError(from));
                }
                if sender.balance < amount {
                    return Err(ExecutionFailureKind::InsufficientFundsError);
                }
                if **recipient != Account::default() {
                    return Err(ExecutionFailureKind::AccountDataError(AccountId::from(
                        &NullifierPublicKey(receiving_address.npk),
                    )));
                }
                Ok(())
            },
        )
        .await
        .context("Failed to submit anonymous payment note transaction")?;

    let tx = poll_privacy_transaction(&wallet, tx_hash).await?;
    let sender_secret = shared_secrets
        .first()
        .copied()
        .context("Send transaction did not return a sender shared secret")?;

    wallet
        .decode_insert_privacy_preserving_transaction_results(
            &tx,
            &[AccDecodeData::Decode(sender_secret, from), AccDecodeData::Skip],
        )
        .context("Failed to decode sender update from payment transaction")?;
    wallet
        .store_persistent_data()
        .await
        .context("Failed to persist wallet storage after send")?;

    let tx_bytes = borsh::to_vec(&tx).context("Failed to serialize privacy transaction")?;
    let receipt = ReceiptV1::new(program_id, tx_hash.0, tx_bytes, current_timestamp_ms()?);
    let receipt_json =
        serde_json::to_vec_pretty(&receipt).context("Failed to encode payment receipt")?;
    tokio::fs::write(receipt_out, receipt_json)
        .await
        .with_context(|| format!("Failed to write receipt to {}", receipt_out.display()))?;

    println!("Transaction hash: {tx_hash}");
    println!("Receipt written to {}", receipt_out.display());

    Ok(())
}

async fn verify_receipt(
    receipt_path: &PathBuf,
    account_id_input: &str,
    check_chain: bool,
) -> Result<()> {
    let account_id = parse_private_account_id(account_id_input)?;
    let wallet = WalletCore::from_env().context("Failed to load wallet from environment")?;
    let receipt_json = tokio::fs::read(receipt_path)
        .await
        .with_context(|| format!("Failed to read receipt from {}", receipt_path.display()))?;
    let receipt: ReceiptV1 =
        serde_json::from_slice(&receipt_json).context("Failed to parse receipt JSON")?;
    ensure!(
        receipt.version == RECEIPT_VERSION,
        "Unsupported receipt version: {}",
        receipt.version
    );
    ensure!(
        receipt.program_id == embedded_program_id(),
        "Receipt program id {} does not match embedded Tribute to Talk program {}",
        format_program_id(receipt.program_id),
        format_program_id(embedded_program_id())
    );

    let tx_bytes = receipt.tx_bytes().context("Failed to decode transaction payload from receipt")?;
    let tx: PrivacyPreservingTransaction =
        PrivacyPreservingTransaction::deserialize(&mut tx_bytes.as_slice())
            .context("Failed to decode privacy transaction from receipt")?;
    ensure!(tx.hash() == receipt.tx_hash, "Receipt transaction hash mismatch");

    let (key_chain, chain_index) =
        private_key_material(&wallet, account_id).context("Failed to load receiver key material")?;
    let verified = verify_private_note(&tx, &receipt, &key_chain, chain_index)?;

    println!("Receipt verified for Private/{account_id}");
    println!("Amount: {}", verified.balance);
    println!("Message: {}", String::from_utf8_lossy(&verified.message));
    println!("Transaction hash: {}", HashType(receipt.tx_hash));

    if check_chain {
        let remote = wallet
            .sequencer_client
            .get_transaction(HashType(receipt.tx_hash))
            .await
            .context("Failed to query sequencer for transaction hash")?;
        ensure!(remote.is_some(), "Transaction hash not found on sequencer");
        println!("Chain inclusion: confirmed by transaction lookup");
    }

    Ok(())
}

fn verify_private_note(
    tx: &PrivacyPreservingTransaction,
    receipt: &ReceiptV1,
    key_chain: &KeyChain,
    chain_index: Option<u32>,
) -> Result<VerifiedNote> {
    ensure!(
        tx.message.public_account_ids.is_empty(),
        "Receipt is not a private-to-private payment transaction"
    );
    ensure!(
        tx.message.nonces.is_empty(),
        "Receipt unexpectedly contains public nonces"
    );
    ensure!(
        tx.message.public_post_states.is_empty(),
        "Receipt unexpectedly contains public post states"
    );
    ensure!(
        tx.witness_set().signatures_and_public_keys().is_empty(),
        "Receipt unexpectedly contains public signatures"
    );

    verify_transaction_proof(tx)?;

    let expected_view_tag = EncryptedAccountData::compute_view_tag(
        &key_chain.nullifier_public_key,
        &key_chain.viewing_public_key,
    );

    let mut matches = tx
        .message
        .encrypted_private_post_states
        .iter()
        .zip(&tx.message.new_commitments)
        .enumerate()
        .filter(|(_, (encrypted, _))| encrypted.view_tag == expected_view_tag)
        .filter_map(|(index, (encrypted, commitment))| {
            let shared_secret =
                key_chain.calculate_shared_secret_receiver(&encrypted.epk, chain_index);
            EncryptionScheme::decrypt(
                &encrypted.ciphertext,
                &shared_secret,
                commitment,
                index as u32,
            )
        })
        .filter(|account| account.program_owner == receipt.program_id)
        .filter_map(|account| {
            PaymentNoteDataV1::from_account_data(&account.data)
                .ok()
                .map(|note| VerifiedNote {
                    balance: account.balance,
                    message: note.message,
                })
        })
        .collect::<Vec<_>>();

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => bail!("Receipt could not be decrypted with the provided receiver account"),
        _ => bail!("Receipt decrypted into multiple candidate notes for the provided account"),
    }
}

fn verify_transaction_proof(tx: &PrivacyPreservingTransaction) -> Result<()> {
    let proof_bytes = tx.witness_set().proof().clone().into_inner();
    let inner: InnerReceipt =
        borsh::from_slice(&proof_bytes).context("Failed to decode inner Risc0 receipt")?;
    let circuit_output = PrivacyPreservingCircuitOutput {
        public_pre_states: Vec::new(),
        public_post_states: tx.message.public_post_states.clone(),
        ciphertexts: tx
            .message
            .encrypted_private_post_states
            .iter()
            .cloned()
            .map(|item| item.ciphertext)
            .collect(),
        new_commitments: tx.message.new_commitments.clone(),
        new_nullifiers: tx.message.new_nullifiers.clone(),
    };
    let receipt = Receipt::new(inner, circuit_output.to_bytes());
    receipt
        .verify(PRIVACY_PRESERVING_CIRCUIT_ID)
        .context("Privacy-preserving proof verification failed")
}

async fn poll_privacy_transaction(
    wallet: &WalletCore,
    tx_hash: HashType,
) -> Result<PrivacyPreservingTransaction> {
    let tx = wallet
        .poll_native_token_transfer(tx_hash)
        .await
        .with_context(|| format!("Failed to poll transaction {tx_hash}"))?;

    match tx {
        NSSATransaction::PrivacyPreserving(tx) => Ok(tx),
        NSSATransaction::Public(_) => bail!("Expected privacy-preserving transaction, got public"),
        NSSATransaction::ProgramDeployment(_) => {
            bail!("Expected privacy-preserving transaction, got deployment")
        }
    }
}

fn private_key_material(wallet: &WalletCore, account_id: AccountId) -> Result<(KeyChain, Option<u32>)> {
    let (key_chain, _) = wallet
        .storage()
        .user_data
        .get_private_account(account_id)
        .cloned()
        .context("Private account not found in wallet storage")?;
    let chain_index = wallet
        .storage()
        .user_data
        .private_key_tree
        .account_id_map
        .get(&account_id)
        .and_then(|index| index.index());

    Ok((key_chain, chain_index))
}

fn embedded_program() -> Result<Program> {
    Program::new(embedded_program_elf().to_vec())
        .context("Embedded Tribute to Talk ELF is not a valid LEZ program")
}

fn embedded_program_elf() -> &'static [u8] {
    tribute_to_talk_methods::TRIBUTE_TO_TALK_ELF
}

fn embedded_program_id() -> ProgramId {
    tribute_to_talk_methods::TRIBUTE_TO_TALK_ID
}

fn parse_private_account_id(value: &str) -> Result<AccountId> {
    let stripped = value
        .strip_prefix("Private/")
        .ok_or_else(|| anyhow!("Account ID must use the Private/ prefix"))?;
    AccountId::from_str(stripped).context("Failed to parse private account id")
}

fn current_timestamp_ms() -> Result<u64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("System clock is before UNIX_EPOCH")?;
    u64::try_from(duration.as_millis()).context("Current timestamp does not fit into u64")
}

fn format_program_id(program_id: ProgramId) -> String {
    let bytes = program_id
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use nssa_core::Commitment;

    #[test]
    fn parse_private_account_id_requires_prefix() {
        assert!(parse_private_account_id("Public/not-private").is_err());
    }

    #[test]
    fn receipt_hash_mismatch_is_detected() {
        let tx = PrivacyPreservingTransaction::deserialize(&mut borsh::to_vec(
            &PrivacyPreservingTransaction::new(
                nssa::privacy_preserving_transaction::message::Message {
                    public_account_ids: vec![],
                    nonces: vec![],
                    public_post_states: vec![],
                    encrypted_private_post_states: vec![],
                    new_commitments: vec![Commitment::new(
                        &NullifierPublicKey([1_u8; 32]),
                        &Account::default(),
                    )],
                    new_nullifiers: vec![],
                },
                nssa::privacy_preserving_transaction::witness_set::WitnessSet::from_raw_parts(
                    vec![],
                    nssa::privacy_preserving_transaction::circuit::Proof::from_inner(vec![]),
                ),
            ),
        )
        .unwrap()
        .as_slice())
        .unwrap();
        let receipt = ReceiptV1 {
            version: RECEIPT_VERSION,
            program_id: [0_u32; 8],
            tx_hash: [9_u8; 32],
            privacy_tx_borsh_base64: base64::engine::general_purpose::STANDARD
                .encode(borsh::to_vec(&tx).unwrap()),
            created_at: 0,
        };

        assert_ne!(tx.hash(), receipt.tx_hash);
    }
}

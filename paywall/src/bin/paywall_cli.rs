use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use nssa::{
    privacy_preserving_transaction::circuit::ProgramWithDependencies,
    program::Program,
    public_transaction::{Message, WitnessSet},
    AccountId, PrivateKey,
};
use paywall::{
    build_bundle, compute_bundle_commitment, compute_bundle_nullifier, decode_hex_array,
    decode_state, derive_endpoint_address, derive_pool_account_id, encode_instruction,
    endpoint_fee, parse_and_verify_bundle, EndpointDescriptor, EndpointSecretMaterial, Instruction,
    ParsedProofBundle, PaywallState, ProofBundle, POOL_PDA_SEED_BYTES,
};
use rand::RngCore;
use serde::{de::DeserializeOwned, Serialize};
use wallet::{PrivacyPreservingAccount, WalletCore};

#[derive(Parser, Debug)]
#[command(name = "paywall_cli")]
#[command(about = "Anonymous fee-gated messenger workflow for LSSA")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to compiled paywall guest binary
    #[arg(
        long,
        default_value = "paywall/target/riscv32im-risc0-zkvm-elf/docker/paywall.bin"
    )]
    program_path: PathBuf,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize paywall state account and bind a global pool account
    ArtistInit {
        /// Existing Public/<ACCOUNT_ID> state account (created if omitted)
        #[arg(long)]
        state_account: Option<String>,
        /// Existing Public/<ACCOUNT_ID> pool account override (defaults to paywall PDA)
        #[arg(long)]
        pool_account: Option<String>,
    },
    /// Register a fee endpoint and export endpoint files
    ArtistCreateEndpoint {
        #[arg(long)]
        state_account: String,
        #[arg(long)]
        fee: u128,
        #[arg(long)]
        descriptor_out: PathBuf,
        #[arg(long)]
        endpoint_secret_out: PathBuf,
    },
    /// Privately pay endpoint fee and export message proof bundle
    PayerPayAndSign {
        #[arg(long)]
        descriptor: PathBuf,
        #[arg(long)]
        payer_account: String,
        #[arg(long)]
        message: String,
        #[arg(long)]
        bundle_out: PathBuf,
    },
    /// Verify a proof bundle against on-chain state without redeeming
    ArtistVerifyBundle {
        #[arg(long)]
        state_account: String,
        #[arg(long)]
        endpoint_secret: PathBuf,
        #[arg(long)]
        bundle: PathBuf,
    },
    /// Privately redeem a valid bundle and withdraw fee to a private recipient account
    ArtistRedeemBundle {
        #[arg(long)]
        state_account: String,
        #[arg(long)]
        endpoint_secret: PathBuf,
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        recipient_private_account: String,
    },
    /// Sync private wallet accounts to latest sequencer block
    WalletSyncPrivate,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut wallet = WalletCore::from_env()?;

    match cli.command {
        Commands::ArtistInit {
            state_account,
            pool_account,
        } => {
            let paywall_program = load_paywall_program(&cli.program_path)?;
            let default_pool_account = derive_pool_account_id(paywall_program.id());
            let pool_account = match pool_account {
                Some(account) => parse_public_account(&account)?,
                None => default_pool_account,
            };

            let state_account = if let Some(account) = state_account {
                parse_public_account(&account)?
            } else {
                let (new_state_account, _) = wallet.create_new_account_public(None);
                wallet.store_persistent_data().await?;
                new_state_account
            };

            submit_public_instruction(
                &wallet,
                &paywall_program,
                vec![state_account],
                Instruction::Initialize {
                    pool_account_id: pool_account,
                },
            )
            .await?;

            println!("Initialized paywall state account: Public/{state_account}");
            println!("Configured pool account: Public/{pool_account}");
            println!(
                "Expected pool PDA seed (hex): {}",
                hex::encode(POOL_PDA_SEED_BYTES)
            );
        }
        Commands::ArtistCreateEndpoint {
            state_account,
            fee,
            descriptor_out,
            endpoint_secret_out,
        } => {
            let state_account = parse_public_account(&state_account)?;
            let state = fetch_paywall_state(&wallet, state_account).await?;

            let mut endpoint_secret = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut endpoint_secret);
            let endpoint_address = derive_endpoint_address(endpoint_secret);

            let paywall_program = load_paywall_program(&cli.program_path)?;
            submit_public_instruction(
                &wallet,
                &paywall_program,
                vec![state_account],
                Instruction::RegisterEndpoint {
                    endpoint_address,
                    fee,
                },
            )
            .await?;

            let descriptor = EndpointDescriptor {
                state_account_id: state_account,
                pool_account_id: state.pool_account_id,
                endpoint_address_hex: hex::encode(endpoint_address),
                fee,
            };
            write_json(&descriptor_out, &descriptor)?;

            let endpoint_secret_material = EndpointSecretMaterial {
                endpoint_secret_hex: hex::encode(endpoint_secret),
                endpoint_address_hex: hex::encode(endpoint_address),
            };
            write_json(&endpoint_secret_out, &endpoint_secret_material)?;

            println!("Registered endpoint with fee {fee}");
            println!("Descriptor: {}", descriptor_out.display());
            println!("Endpoint secret: {}", endpoint_secret_out.display());
        }
        Commands::PayerPayAndSign {
            descriptor,
            payer_account,
            message,
            bundle_out,
        } => {
            let payer_account = parse_private_account(&payer_account)?;
            let descriptor: EndpointDescriptor = read_json(&descriptor)?;
            let endpoint_address = decode_hex_array::<32>(&descriptor.endpoint_address_hex)
                .context("invalid endpoint address in endpoint descriptor")?;

            let mut payment_secret = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut payment_secret);
            let pseudonym_private_key = PrivateKey::new_os_random();
            let bundle = build_bundle(
                endpoint_address,
                message,
                payment_secret,
                &pseudonym_private_key,
            );
            let parsed_bundle = parse_and_verify_bundle(&bundle)?;

            let paywall_program = load_paywall_program(&cli.program_path)?;
            let program_with_dependencies =
                with_authenticated_transfer_dependency(paywall_program.clone());

            submit_private_instruction(
                &wallet,
                &program_with_dependencies,
                vec![
                    PrivacyPreservingAccount::Public(descriptor.state_account_id),
                    PrivacyPreservingAccount::PrivateOwned(payer_account),
                    PrivacyPreservingAccount::Public(descriptor.pool_account_id),
                ],
                Instruction::PayForMessage {
                    endpoint_address: parsed_bundle.endpoint_address,
                    payment_secret: parsed_bundle.payment_secret,
                    message_hash: parsed_bundle.message_hash,
                    payer_pseudonym_pubkey: parsed_bundle.payer_pseudonym_pubkey,
                },
            )
            .await?;

            write_json(&bundle_out, &bundle)?;
            println!("Submitted private payment and produced proof bundle");
            println!("Bundle: {}", bundle_out.display());
            println!(
                "Run `wallet-sync-private` after block inclusion for local private-state refresh."
            );
        }
        Commands::ArtistVerifyBundle {
            state_account,
            endpoint_secret,
            bundle,
        } => {
            let state_account = parse_public_account(&state_account)?;
            let endpoint_secret_material: EndpointSecretMaterial = read_json(&endpoint_secret)?;
            let bundle: ProofBundle = read_json(&bundle)?;
            let state = fetch_paywall_state(&wallet, state_account).await?;

            let endpoint_secret =
                decode_hex_array::<32>(&endpoint_secret_material.endpoint_secret_hex)
                    .context("invalid endpoint secret hex")?;
            let verified = verify_bundle_against_state(&bundle, endpoint_secret, &state)?;

            println!("Bundle verification succeeded");
            println!("Matched endpoint fee: {}", verified.fee);
            println!("Commitment: {}", hex::encode(verified.commitment));
            println!("Nullifier (unused): {}", hex::encode(verified.nullifier));
        }
        Commands::ArtistRedeemBundle {
            state_account,
            endpoint_secret,
            bundle,
            recipient_private_account,
        } => {
            let state_account = parse_public_account(&state_account)?;
            let recipient_private_account = parse_private_account(&recipient_private_account)?;
            let endpoint_secret_material: EndpointSecretMaterial = read_json(&endpoint_secret)?;
            let bundle: ProofBundle = read_json(&bundle)?;

            let state = fetch_paywall_state(&wallet, state_account).await?;
            let endpoint_secret =
                decode_hex_array::<32>(&endpoint_secret_material.endpoint_secret_hex)
                    .context("invalid endpoint secret hex")?;
            let verified = verify_bundle_against_state(&bundle, endpoint_secret, &state)?;

            let paywall_program = load_paywall_program(&cli.program_path)?;
            let program_with_dependencies =
                with_authenticated_transfer_dependency(paywall_program.clone());

            submit_private_instruction(
                &wallet,
                &program_with_dependencies,
                vec![
                    PrivacyPreservingAccount::Public(state_account),
                    PrivacyPreservingAccount::Public(state.pool_account_id),
                    PrivacyPreservingAccount::PrivateOwned(recipient_private_account),
                ],
                Instruction::ConsumeAndWithdraw {
                    endpoint_secret,
                    payment_secret: verified.parsed_bundle.payment_secret,
                    message_hash: verified.parsed_bundle.message_hash,
                    payer_pseudonym_pubkey: verified.parsed_bundle.payer_pseudonym_pubkey,
                },
            )
            .await?;

            println!("Redeem transaction submitted");
            println!(
                "Run `wallet-sync-private` after block inclusion to refresh recipient private balance."
            );
        }
        Commands::WalletSyncPrivate => {
            let latest_block = wallet.sequencer_client.get_last_block().await?.last_block;
            wallet.sync_to_block(latest_block).await?;
            wallet.store_persistent_data().await?;
            println!("Synced private accounts to block {latest_block}");
        }
    }

    Ok(())
}

#[derive(Debug)]
struct VerifiedBundle {
    fee: u128,
    commitment: [u8; 32],
    nullifier: [u8; 32],
    parsed_bundle: ParsedProofBundle,
}

fn verify_bundle_against_state(
    bundle: &ProofBundle,
    endpoint_secret: [u8; 32],
    state: &PaywallState,
) -> Result<VerifiedBundle> {
    let parsed_bundle = parse_and_verify_bundle(bundle)?;

    let endpoint_address = derive_endpoint_address(endpoint_secret);
    if endpoint_address != parsed_bundle.endpoint_address {
        bail!("bundle endpoint does not match endpoint secret")
    }

    let fee = endpoint_fee(state, endpoint_address)
        .ok_or_else(|| anyhow::anyhow!("endpoint not registered in paywall state"))?;

    let commitment = compute_bundle_commitment(&parsed_bundle, fee);
    if !state.commitments.contains(&commitment) {
        bail!("bundle commitment not present in on-chain paywall state")
    }

    let nullifier = compute_bundle_nullifier(&parsed_bundle, endpoint_secret);
    if state.nullifiers.contains(&nullifier) {
        bail!("bundle nullifier already consumed")
    }

    Ok(VerifiedBundle {
        fee,
        commitment,
        nullifier,
        parsed_bundle,
    })
}

fn parse_public_account(account: &str) -> Result<AccountId> {
    let Some(base58) = account.strip_prefix("Public/") else {
        bail!("expected Public/<ACCOUNT_ID>");
    };

    base58
        .parse()
        .with_context(|| format!("invalid public account id: {base58}"))
}

fn parse_private_account(account: &str) -> Result<AccountId> {
    let Some(base58) = account.strip_prefix("Private/") else {
        bail!("expected Private/<ACCOUNT_ID>");
    };

    base58
        .parse()
        .with_context(|| format!("invalid private account id: {base58}"))
}

fn load_paywall_program(program_path: &Path) -> Result<Program> {
    let bytecode = fs::read(program_path).with_context(|| {
        format!(
            "failed to read paywall program at {}",
            program_path.display()
        )
    })?;
    Program::new(bytecode).context("failed to load paywall program")
}

fn with_authenticated_transfer_dependency(program: Program) -> ProgramWithDependencies {
    let authenticated_transfer_program = Program::authenticated_transfer_program();
    let dependencies: HashMap<_, _> = [(
        authenticated_transfer_program.id(),
        authenticated_transfer_program,
    )]
    .into_iter()
    .collect();
    ProgramWithDependencies::new(program, dependencies)
}

async fn fetch_paywall_state(
    wallet: &WalletCore,
    state_account: AccountId,
) -> Result<PaywallState> {
    let account = wallet.get_account_public(state_account).await?;
    decode_state(&account.data)
}

async fn submit_public_instruction(
    wallet: &WalletCore,
    program: &Program,
    accounts: Vec<AccountId>,
    instruction: Instruction,
) -> Result<()> {
    let nonces = wallet.get_accounts_nonces(accounts.clone()).await?;
    let serialized_instruction = encode_instruction(&instruction)?;
    let instruction_data = Program::serialize_instruction(serialized_instruction)?;

    let message =
        Message::try_new(program.id(), accounts.clone(), nonces, instruction_data).unwrap();

    let signing_keys = accounts
        .iter()
        .filter_map(|account_id| wallet.get_account_public_signing_key(*account_id))
        .collect::<Vec<_>>();

    let witness_set = WitnessSet::for_message(&message, &signing_keys);
    let tx = nssa::PublicTransaction::new(message, witness_set);
    let response = wallet.sequencer_client.send_tx_public(tx).await?;

    println!("Public transaction submitted: {:?}", response.tx_hash);
    Ok(())
}

async fn submit_private_instruction(
    wallet: &WalletCore,
    program_with_dependencies: &ProgramWithDependencies,
    accounts: Vec<PrivacyPreservingAccount>,
    instruction: Instruction,
) -> Result<()> {
    let serialized_instruction = encode_instruction(&instruction)?;
    let instruction_data = Program::serialize_instruction(serialized_instruction)?;
    let (response, _) = wallet
        .send_privacy_preserving_tx(accounts, instruction_data, program_with_dependencies)
        .await?;

    println!("Private transaction submitted: {:?}", response.tx_hash);
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read json file {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse json file {}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}

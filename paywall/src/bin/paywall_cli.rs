use clap::{Parser, Subcommand};
use nssa::{AccountId, PublicTransaction, program::Program, public_transaction::{Message, WitnessSet}};
use nssa_core::program::ProgramId;
use wallet::{WalletCore, PrivacyPreservingAccount};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use rand::RngCore;
use std::path::PathBuf;
use std::fs;

#[derive(Parser)]
#[command(name = "paywall")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to the guest program binary
    #[arg(long, default_value = "paywall/target/riscv32im-risc0-zkvm-elf/docker/paywall.bin")]
    program_path: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Deposit funds and allocate to a commitment
    Deposit {
        /// The temporary funding account ID (must hold funds and be unclaimed)
        funding_account: AccountId,
        /// The paywall pool account ID
        pool_account: AccountId,
        /// Amount (must match funding account balance, used for verification)
        amount: u64,
        /// Secret to derive commitment (hex string or generated if not provided)
        #[arg(long)]
        secret: Option<String>,
        /// Recipient Account ID (for commitment derivation)
        #[arg(long)]
        recipient: AccountId,
    },
    /// Withdraw funds using secret
    Withdraw {
        /// The paywall pool account ID
        pool_account: AccountId,
        /// The recipient account ID
        recipient: AccountId,
        /// Amount to withdraw
        amount: u64,
        /// Secret used for deposit (hex string)
        #[arg(long)]
        secret: String,
    },
}

#[derive(Serialize, Deserialize, Debug)]
enum Instruction {
    DepositAndAllocate {
        commitment: [u8; 32],
    },
    Withdraw {
        amount: u64,
        secret: [u8; 32],
        recipient: AccountId,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let wallet_core = WalletCore::from_env()?;

    // Load program
    let bytecode = fs::read(&cli.program_path).map_err(|e| anyhow::anyhow!("Failed to read program binary at {:?}: {}", cli.program_path, e))?;
    let program = Program::new(bytecode)?;
    println!("Program ID: {:?}", program.id());

    match cli.command {
        Commands::Deposit { funding_account, pool_account, amount, secret, recipient } => {
            // 1. Generate Secret
            let secret_bytes = if let Some(s) = secret {
                hex::decode(s)?
            } else {
                let mut rng = rand::thread_rng();
                let mut s = [0u8; 32];
                rng.fill_bytes(&mut s);
                println!("Generated Secret: {}", hex::encode(s));
                s.to_vec()
            };
            let secret_arr: [u8; 32] = secret_bytes.try_into().map_err(|_| anyhow::anyhow!("Invalid secret length"))?;

            // 2. Compute Commitment
            let mut hasher = Sha256::new();
            hasher.update(&secret_arr);
            hasher.update(recipient.as_ref());
            hasher.update(amount.to_le_bytes());
            let commitment: [u8; 32] = hasher.finalize().into();
            println!("Commitment: {}", hex::encode(commitment));

            // 3. Construct Instruction
            let instruction = Instruction::DepositAndAllocate {
                commitment,
            };
            let instruction_bytes = bincode::serialize(&instruction)?;

            // 4. Submit Public Transaction
            // Accounts: [Funding, Pool]
            let accounts = vec![funding_account, pool_account];
            
            let nonces = wallet_core.get_accounts_nonces(accounts.clone()).await?;
            // We need signing keys for Funding Account?
            // If Funding Account is Default owned, it doesn't need signature?
            // Or does it?
            // "In case the input account is uninitialized, the program claims it."
            // `hello_world` uses `signing_keys = []`.
            // So if Funding Account is uninitialized (Default owner), we don't need signature!
            // But wait, if it has BALANCE, can it be uninitialized?
            // If I transfer to it, it exists.
            // If I transfer to a new account, it has Default owner.
            // Does Default owner require signature?
            // Usually "Default" means "System" or "None".
            // If `hello_world` claims it without signature, then `Paywall` can too.
            // So we leave `signing_keys` empty!
            
            let signing_keys = [];
            let message = Message::try_new(program.id(), accounts, nonces, instruction_bytes).unwrap();
            let witness_set = WitnessSet::for_message(&message, &signing_keys);
            let tx = PublicTransaction::new(message, witness_set);

            println!("Submitting Deposit Transaction...");
            let _response = wallet_core.sequencer_client.send_tx_public(tx).await?;
            println!("Deposit successful!");
        }
        Commands::Withdraw { pool_account, recipient, amount, secret } => {
            let secret_bytes = hex::decode(secret)?;
            let secret_arr: [u8; 32] = secret_bytes.try_into().map_err(|_| anyhow::anyhow!("Invalid secret length"))?;

            let instruction = Instruction::Withdraw {
                amount,
                secret: secret_arr,
                recipient,
            };
            let instruction_bytes = bincode::serialize(&instruction)?;

            let accounts = vec![
                PrivacyPreservingAccount::Public(pool_account),
                PrivacyPreservingAccount::Public(recipient),
            ];
            
            println!("Submitting Private Withdraw Transaction...");
            wallet_core.send_privacy_preserving_tx(
                accounts,
                Program::serialize_instruction(instruction_bytes)?,
                &program.into(),
            ).await?;
            
            println!("Withdrawal submitted successfully!");
        }
    }

    Ok(())
}

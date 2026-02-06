use clap::Parser;
use hex::FromHex;
use nssa::{program::Program, AccountId};
use serde::{Deserialize, Serialize};
use wallet::{PrivacyPreservingAccount, WalletCore};

#[derive(Serialize, Deserialize, Debug)]
struct TicketInitInstruction {
    secret: [u8; 32],
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Initialize a private account with a ticket secret")]
struct Args {
    /// Path to ticket init program binary (.bin)
    program_path: String,
    /// Ticket secret (hex-encoded 32 bytes)
    secret_hex: String,
    /// Optional existing private account id to reuse
    #[arg(long = "account-id")]
    account_id: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let mut wallet_core = WalletCore::from_env().unwrap();

    let secret: [u8; 32] =
        <[u8; 32]>::from_hex(args.secret_hex.trim_start_matches("0x"))
            .expect("Secret must be 32 bytes hex");

    let account_id = if let Some(id) = args.account_id.as_deref() {
        let id_str = id.trim_start_matches("Private/");
        id_str.parse::<AccountId>().expect("Invalid account id")
    } else {
        let (id, _chain_index) = wallet_core.create_new_account_private(None);
        println!("Created new private account: {}", id);
        // Persist freshly generated private keys so a later process can use the same account.
        wallet_core
            .store_persistent_data()
            .await
            .expect("Failed to persist wallet data");
        id
    };

    let bytecode: Vec<u8> = std::fs::read(&args.program_path).unwrap();
    let program = Program::new(bytecode).unwrap();

    let instruction = TicketInitInstruction { secret };

    let accounts = vec![PrivacyPreservingAccount::PrivateOwned(account_id)];
    wallet_core
        .send_privacy_preserving_tx(
            accounts,
            Program::serialize_instruction(instruction).unwrap(),
            &program.into(),
        )
        .await
        .unwrap();

    // Persist any local updates after transaction submission as well.
    wallet_core
        .store_persistent_data()
        .await
        .expect("Failed to persist wallet data");

    println!("Ticket init transaction submitted.");
    println!("Run:");
    println!("wallet account sync-private");
    println!("wallet account get --account-id Private/{}", account_id);
}

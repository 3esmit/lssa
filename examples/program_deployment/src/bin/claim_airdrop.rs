use clap::Parser;
use hex::FromHex;
use nssa::{program::Program, AccountId};
use serde::{Deserialize, Serialize};
use wallet::{PrivacyPreservingAccount, WalletCore};

#[derive(Serialize, Deserialize, Debug)]
enum AirdropInstruction {
    Init { merkle_root: [u8; 32] },
    Claim { path: Vec<[u8; 32]>, index: u64 },
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to airdrop program binary (.bin)
    program_path: String,
    /// Registry public account id
    registry_id: String,
    /// Recipient private account id
    recipient_id: String,
    /// Merkle path as comma-separated hex 32-byte nodes
    merkle_path: String,
    /// Leaf index in the Merkle tree (uint)
    index: u64,
}

#[tokio::main]
async fn main() {
    // Initialize wallet
    let wallet_core = WalletCore::from_env().unwrap();

    // Parse arguments
    let args = Args::parse();

    // Load the program
    let bytecode = std::fs::read(&args.program_path).expect("Failed to read program file");
    let program = Program::new(bytecode).unwrap();

    // Parse Account IDs
    // Remove "Public/" or "Private/" prefix if present for parsing into AccountId
    // Actually AccountId::from_str might handle it if it's in the standard format?
    // Let's assume standard format or strip it.
    // Standard format for AccountId is usually just the Base58 string.
    // The wallet CLI adds the prefix for display/parsing convenience in CLI.
    // Let's try parsing directly.
    
    let registry_id_str = args.registry_id.trim_start_matches("Public/");
    let registry_id: AccountId = registry_id_str.parse().expect("Invalid registry ID");

    let recipient_id_str = args.recipient_id.trim_start_matches("Private/");
    let recipient_id: AccountId = recipient_id_str.parse().expect("Invalid recipient ID");

    let path: Vec<[u8; 32]> = if args.merkle_path.trim().is_empty() {
        Vec::new()
    } else {
        args.merkle_path
            .split(',')
            .map(|s| {
                <[u8; 32]>::from_hex(s.trim_start_matches("0x"))
                    .expect("Each merkle path node must be 32 bytes hex")
            })
            .collect()
    };

    let instruction = AirdropInstruction::Claim {
        path,
        index: args.index,
    };

    println!("Submitting claim transaction...");
    println!("Registry: {}", registry_id);
    println!("Recipient: {}", recipient_id);

    // Construct and submit the privacy-preserving transaction
    // Note: The order of accounts in the vector must match the order expected by the program (Registry, Recipient)
    let accounts = vec![
        PrivacyPreservingAccount::Public(registry_id),
        PrivacyPreservingAccount::PrivateOwned(recipient_id),
    ];

    wallet_core
        .send_privacy_preserving_tx(
            accounts,
            Program::serialize_instruction(instruction).unwrap(),
            &program.into(),
        )
        .await
        .unwrap();

    println!("Claim transaction submitted successfully!");
    println!("To verify, sync your private account and check the data:");
    println!("wallet account sync-private");
    println!("wallet account get --account-id Private/{}", recipient_id);
    println!("Note: recipient private account data must contain the 32-byte ticket secret.");
}

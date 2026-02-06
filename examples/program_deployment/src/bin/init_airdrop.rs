use clap::Parser;
use hex::FromHex;
use nssa::{
    program::Program,
    public_transaction::{Message, WitnessSet},
    PublicTransaction,
};
use serde::Serialize;
use wallet::WalletCore;

#[derive(Serialize)]
enum AirdropInstruction {
    Init { merkle_root: [u8; 32] },
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to airdrop program binary (.bin)
    program_path: String,
    /// Merkle root (hex-encoded 32 bytes)
    merkle_root: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize wallet
    let mut wallet_core = WalletCore::from_env().unwrap();
    
    // Create new registry account
    let (registry_id, _chain_index) = wallet_core.create_new_account_public(None);
    println!("Created new Registry ID: {}", registry_id);

    // Load program from .bin file (same pattern as run_hello_world)
    let bytecode: Vec<u8> = std::fs::read(&args.program_path).unwrap();
    let program = Program::new(bytecode).unwrap();

    // Parse merkle root
    let root_bytes =
        <[u8; 32]>::from_hex(args.merkle_root.trim_start_matches("0x"))
            .expect("Merkle root must be 32 bytes hex");

    // Build Init instruction
    let instruction = AirdropInstruction::Init {
        merkle_root: root_bytes,
    };
    // Public execution following hello_world pattern: no nonces, no signing keys
    let nonces = vec![];
    let signing_keys = [];
    let message = Message::try_new(program.id(), vec![registry_id], nonces, instruction).unwrap();
    let witness_set = WitnessSet::for_message(&message, &signing_keys);
    let tx = PublicTransaction::new(message, witness_set);

    // Submit transaction
    wallet_core
        .sequencer_client
        .send_tx_public(tx)
        .await
        .unwrap();

    println!("Init transaction sent. Polling for registry initialization...");

    // Poll registry account until data is non-empty
    let mut attempts = 0;
    loop {
        if attempts > 60 {
            panic!("Timeout waiting for registry initialization");
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        match wallet_core.get_account_public(registry_id).await {
            Ok(account) => {
                if !account.data.is_empty() {
                    println!("Registry initialized! Data len: {}", account.data.len());
                    break;
                }
            }
            Err(_) => {}
        }
        attempts += 1;
        println!("Waiting for registry initialization... (attempt {})", attempts);
    }
}

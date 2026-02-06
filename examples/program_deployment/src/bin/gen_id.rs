use wallet::WalletCore;

#[tokio::main]
async fn main() {
    let mut wallet_core = WalletCore::from_env().unwrap();
    let (account_id, private_key) = wallet_core.create_new_account_public(None);
    println!("New Account ID: {}", account_id);
    println!("Private Key: {:?}", private_key);
}

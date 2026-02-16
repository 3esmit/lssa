use std::{collections::HashMap, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use integration_tests::{TestContext, TIME_TO_WAIT_FOR_BLOCK_SECONDS};
use nssa::{
    privacy_preserving_transaction::circuit::ProgramWithDependencies,
    program::Program,
    program_deployment_transaction::Message as DeploymentMessage,
    public_transaction::{Message, WitnessSet},
    AccountId, PrivateKey, ProgramDeploymentTransaction, PublicTransaction,
};
use paywall::{
    build_bundle, compute_bundle_commitment, compute_bundle_nullifier, decode_state,
    derive_endpoint_address, derive_payment_commitment, derive_pool_account_id, encode_instruction,
    endpoint_fee, hash_message, parse_and_verify_bundle, Instruction,
};
use wallet::{PrivacyPreservingAccount, WalletCore};

#[tokio::test]
async fn anonymous_fee_gated_messenger_e2e() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let payer_private_account = ctx.existing_private_accounts()[0];
    let artist_private_account = ctx.existing_private_accounts()[1];

    let state_account = {
        let wallet = ctx.wallet_mut();
        let (state_account, _) = wallet.create_new_account_public(None);
        wallet.store_persistent_data().await?;
        state_account
    };

    let (paywall_program, program_with_dependencies) = deploy_program(ctx.wallet()).await?;
    let pool_account = derive_pool_account_id(paywall_program.id());

    submit_public_instruction(
        ctx.wallet(),
        &paywall_program,
        vec![state_account],
        Instruction::Initialize {
            pool_account_id: pool_account,
        },
    )
    .await?;
    wait_for_block().await;

    let endpoint_secret = [7u8; 32];
    let endpoint_address = derive_endpoint_address(endpoint_secret);
    let fee = 321u128;

    submit_public_instruction(
        ctx.wallet(),
        &paywall_program,
        vec![state_account],
        Instruction::RegisterEndpoint {
            endpoint_address,
            fee,
        },
    )
    .await?;
    wait_for_block().await;

    let state_after_register = fetch_state(ctx.wallet(), state_account).await?;
    assert_eq!(state_after_register.pool_account_id, pool_account);
    assert_eq!(
        endpoint_fee(&state_after_register, endpoint_address),
        Some(fee)
    );

    let payment_secret = [9u8; 32];
    let bundle = build_bundle(
        endpoint_address,
        "paying for this private message".to_string(),
        payment_secret,
        &PrivateKey::new_os_random(),
    );
    let parsed_bundle = parse_and_verify_bundle(&bundle)?;

    submit_private_instruction(
        ctx.wallet(),
        &program_with_dependencies,
        vec![
            PrivacyPreservingAccount::Public(state_account),
            PrivacyPreservingAccount::PrivateOwned(payer_private_account),
            PrivacyPreservingAccount::Public(pool_account),
        ],
        Instruction::PayForMessage {
            endpoint_address: parsed_bundle.endpoint_address,
            payment_secret: parsed_bundle.payment_secret,
            message_hash: parsed_bundle.message_hash,
            payer_pseudonym_pubkey: parsed_bundle.payer_pseudonym_pubkey,
        },
    )
    .await?;
    wait_for_block().await;

    let pool_after_pay = ctx.wallet().get_account_public(pool_account).await?;
    assert_eq!(pool_after_pay.balance, fee);

    let state_after_pay = fetch_state(ctx.wallet(), state_account).await?;
    let expected_commitment = compute_bundle_commitment(&parsed_bundle, fee);
    assert!(state_after_pay.commitments.contains(&expected_commitment));

    // Negative: wrong endpoint fee descriptor would fail verification (commitment mismatch).
    let wrong_fee_commitment = derive_payment_commitment(
        parsed_bundle.payment_secret,
        parsed_bundle.endpoint_address,
        fee + 1,
        parsed_bundle.message_hash,
        parsed_bundle.payer_pseudonym_pubkey,
    );
    assert!(!state_after_pay.commitments.contains(&wrong_fee_commitment));

    // Offline verify path
    assert_eq!(
        hash_message(&bundle.message),
        parsed_bundle.message_hash,
        "message hash must match proof bundle"
    );
    assert_eq!(
        derive_endpoint_address(endpoint_secret),
        parsed_bundle.endpoint_address,
        "endpoint secret must map to bundle endpoint"
    );
    let expected_nullifier = compute_bundle_nullifier(&parsed_bundle, endpoint_secret);
    assert!(!state_after_pay.nullifiers.contains(&expected_nullifier));

    // Negative: wrong endpoint secret fails redeem
    let wrong_secret_attempt = submit_private_instruction(
        ctx.wallet(),
        &program_with_dependencies,
        vec![
            PrivacyPreservingAccount::Public(state_account),
            PrivacyPreservingAccount::Public(pool_account),
            PrivacyPreservingAccount::PrivateOwned(artist_private_account),
        ],
        Instruction::ConsumeAndWithdraw {
            endpoint_secret: [88; 32],
            payment_secret: parsed_bundle.payment_secret,
            message_hash: parsed_bundle.message_hash,
            payer_pseudonym_pubkey: parsed_bundle.payer_pseudonym_pubkey,
        },
    )
    .await;
    assert!(wrong_secret_attempt.is_err());

    // Negative: wrong message hash fails redeem
    let mut wrong_message_hash = parsed_bundle.message_hash;
    wrong_message_hash[0] ^= 0xFF;
    let wrong_message_attempt = submit_private_instruction(
        ctx.wallet(),
        &program_with_dependencies,
        vec![
            PrivacyPreservingAccount::Public(state_account),
            PrivacyPreservingAccount::Public(pool_account),
            PrivacyPreservingAccount::PrivateOwned(artist_private_account),
        ],
        Instruction::ConsumeAndWithdraw {
            endpoint_secret,
            payment_secret: parsed_bundle.payment_secret,
            message_hash: wrong_message_hash,
            payer_pseudonym_pubkey: parsed_bundle.payer_pseudonym_pubkey,
        },
    )
    .await;
    assert!(wrong_message_attempt.is_err());

    let artist_balance_before = ctx
        .wallet()
        .get_account_private(artist_private_account)
        .context("artist private account not found in local wallet")?
        .balance;

    submit_private_instruction(
        ctx.wallet(),
        &program_with_dependencies,
        vec![
            PrivacyPreservingAccount::Public(state_account),
            PrivacyPreservingAccount::Public(pool_account),
            PrivacyPreservingAccount::PrivateOwned(artist_private_account),
        ],
        Instruction::ConsumeAndWithdraw {
            endpoint_secret,
            payment_secret: parsed_bundle.payment_secret,
            message_hash: parsed_bundle.message_hash,
            payer_pseudonym_pubkey: parsed_bundle.payer_pseudonym_pubkey,
        },
    )
    .await?;
    wait_for_block().await;

    let state_after_redeem = fetch_state(ctx.wallet(), state_account).await?;
    assert!(state_after_redeem.nullifiers.contains(&expected_nullifier));

    let pool_after_redeem = ctx.wallet().get_account_public(pool_account).await?;
    assert_eq!(pool_after_redeem.balance, 0);

    sync_private_accounts(ctx.wallet_mut()).await?;

    let artist_balance_after = ctx
        .wallet()
        .get_account_private(artist_private_account)
        .context("artist private account not found after sync")?
        .balance;
    assert_eq!(artist_balance_after, artist_balance_before + fee);

    // Replay should fail because nullifier is already used.
    let replay_attempt = submit_private_instruction(
        ctx.wallet(),
        &program_with_dependencies,
        vec![
            PrivacyPreservingAccount::Public(state_account),
            PrivacyPreservingAccount::Public(pool_account),
            PrivacyPreservingAccount::PrivateOwned(artist_private_account),
        ],
        Instruction::ConsumeAndWithdraw {
            endpoint_secret,
            payment_secret: parsed_bundle.payment_secret,
            message_hash: parsed_bundle.message_hash,
            payer_pseudonym_pubkey: parsed_bundle.payer_pseudonym_pubkey,
        },
    )
    .await;
    assert!(replay_attempt.is_err());

    // Negative verify: tampered bundle message hash should fail local verification.
    let mut tampered_bundle = bundle.clone();
    tampered_bundle.message_hash_hex = hex::encode([0u8; 32]);
    assert!(parse_and_verify_bundle(&tampered_bundle).is_err());

    Ok(())
}

async fn deploy_program(wallet: &WalletCore) -> Result<(Program, ProgramWithDependencies)> {
    let program_path = PathBuf::from("target/riscv32im-risc0-zkvm-elf/docker/paywall.bin");
    let bytecode = std::fs::read(&program_path).with_context(|| {
        format!(
            "failed to read paywall binary at {}",
            program_path.display()
        )
    })?;

    let program = Program::new(bytecode.clone())?;
    let deploy_message = DeploymentMessage::new(bytecode);
    let deploy_tx = ProgramDeploymentTransaction::new(deploy_message);
    wallet.sequencer_client.send_tx_program(deploy_tx).await?;

    wait_for_block().await;

    let authenticated_transfer_program = Program::authenticated_transfer_program();
    let dependencies: HashMap<_, _> = [(
        authenticated_transfer_program.id(),
        authenticated_transfer_program,
    )]
    .into_iter()
    .collect();

    Ok((
        program.clone(),
        ProgramWithDependencies::new(program, dependencies),
    ))
}

async fn fetch_state(
    wallet: &WalletCore,
    state_account: AccountId,
) -> Result<paywall::PaywallState> {
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
    let tx = PublicTransaction::new(message, witness_set);
    wallet.sequencer_client.send_tx_public(tx).await?;

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

    let _ = wallet
        .send_privacy_preserving_tx(accounts, instruction_data, program_with_dependencies)
        .await?;

    Ok(())
}

async fn sync_private_accounts(wallet: &mut WalletCore) -> Result<()> {
    let latest = wallet.sequencer_client.get_last_block().await?.last_block;
    wallet.sync_to_block(latest).await
}

async fn wait_for_block() {
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;
}

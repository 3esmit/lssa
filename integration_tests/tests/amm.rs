use std::time::Duration;

use amm_core::{PoolDefinition, compute_liquidity_token_pda, compute_pool_pda, compute_vault_pda};
use anyhow::Result;
use integration_tests::{TIME_TO_WAIT_FOR_BLOCK_SECONDS, TestContext, format_public_account_id};
use log::info;
use nssa::{Account, AccountId, program::Program};
use token_core::{TokenDefinition, TokenHolding};
use tokio::test;
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::{amm::AmmProgramAgnosticSubcommand, token::TokenProgramAgnosticSubcommand},
};

struct AmmStateIds {
    definition_a: AccountId,
    definition_b: AccountId,
    pool: AccountId,
    vault_a: AccountId,
    vault_b: AccountId,
    lp_definition: AccountId,
}

async fn fetch_account(ctx: &TestContext, account_id: AccountId) -> Result<Account> {
    Ok(ctx
        .sequencer_client()
        .get_account(account_id)
        .await?
        .account)
}

fn read_pool_definition(account: &Account) -> PoolDefinition {
    PoolDefinition::try_from(&account.data).expect("Pool account should decode")
}

fn read_fungible_balance(account: &Account) -> u128 {
    match TokenHolding::try_from(&account.data).expect("Token holding should decode") {
        TokenHolding::Fungible { balance, .. } => balance,
        _ => panic!("Expected fungible token holding"),
    }
}

fn read_fungible_supply(account: &Account) -> u128 {
    match TokenDefinition::try_from(&account.data).expect("Token definition should decode") {
        TokenDefinition::Fungible { total_supply, .. } => total_supply,
        _ => panic!("Expected fungible token definition"),
    }
}

async fn assert_user_balances(
    ctx: &TestContext,
    user_holding_a: AccountId,
    user_holding_b: AccountId,
    user_holding_lp: AccountId,
    expected_a: u128,
    expected_b: u128,
    expected_lp: u128,
) -> Result<()> {
    let user_holding_a_acc = fetch_account(ctx, user_holding_a).await?;
    let user_holding_b_acc = fetch_account(ctx, user_holding_b).await?;
    let user_holding_lp_acc = fetch_account(ctx, user_holding_lp).await?;

    assert_eq!(read_fungible_balance(&user_holding_a_acc), expected_a);
    assert_eq!(read_fungible_balance(&user_holding_b_acc), expected_b);
    assert_eq!(read_fungible_balance(&user_holding_lp_acc), expected_lp);

    Ok(())
}

async fn assert_amm_state(
    ctx: &TestContext,
    amm_ids: &AmmStateIds,
    expected_reserve_a: u128,
    expected_reserve_b: u128,
    expected_lp_supply: u128,
    expected_vault_a_balance: u128,
    expected_vault_b_balance: u128,
    expected_active: bool,
) -> Result<()> {
    let pool_account = fetch_account(ctx, amm_ids.pool).await?;
    let vault_a_account = fetch_account(ctx, amm_ids.vault_a).await?;
    let vault_b_account = fetch_account(ctx, amm_ids.vault_b).await?;
    let lp_definition_account = fetch_account(ctx, amm_ids.lp_definition).await?;

    let pool_definition = read_pool_definition(&pool_account);

    assert_eq!(pool_definition.definition_token_a_id, amm_ids.definition_a);
    assert_eq!(pool_definition.definition_token_b_id, amm_ids.definition_b);
    assert_eq!(pool_definition.vault_a_id, amm_ids.vault_a);
    assert_eq!(pool_definition.vault_b_id, amm_ids.vault_b);
    assert_eq!(pool_definition.liquidity_pool_id, amm_ids.lp_definition);
    assert_eq!(pool_definition.reserve_a, expected_reserve_a);
    assert_eq!(pool_definition.reserve_b, expected_reserve_b);
    assert_eq!(pool_definition.liquidity_pool_supply, expected_lp_supply);
    assert_eq!(pool_definition.active, expected_active);

    assert_eq!(
        read_fungible_balance(&vault_a_account),
        expected_vault_a_balance
    );
    assert_eq!(
        read_fungible_balance(&vault_b_account),
        expected_vault_b_balance
    );
    assert_eq!(
        read_fungible_supply(&lp_definition_account),
        expected_lp_supply
    );

    Ok(())
}

#[test]
async fn amm_public() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    // Create new account for the token definition
    let SubcommandReturnValue::RegisterAccount {
        account_id: definition_account_id_1,
    } = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: None,
            label: None,
        })),
    )
    .await?
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new account for the token supply holder
    let SubcommandReturnValue::RegisterAccount {
        account_id: supply_account_id_1,
    } = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: None,
            label: None,
        })),
    )
    .await?
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new account for receiving a token transaction
    let SubcommandReturnValue::RegisterAccount {
        account_id: recipient_account_id_1,
    } = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: None,
            label: None,
        })),
    )
    .await?
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new account for the token definition
    let SubcommandReturnValue::RegisterAccount {
        account_id: definition_account_id_2,
    } = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: None,
            label: None,
        })),
    )
    .await?
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new account for the token supply holder
    let SubcommandReturnValue::RegisterAccount {
        account_id: supply_account_id_2,
    } = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: None,
            label: None,
        })),
    )
    .await?
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new account for receiving a token transaction
    let SubcommandReturnValue::RegisterAccount {
        account_id: recipient_account_id_2,
    } = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: None,
            label: None,
        })),
    )
    .await?
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new token
    let subcommand = TokenProgramAgnosticSubcommand::New {
        definition_account_id: format_public_account_id(definition_account_id_1),
        supply_account_id: format_public_account_id(supply_account_id_1),
        name: "A NAM1".to_string(),
        total_supply: 37,
    };
    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Transfer 7 tokens from `supply_acc` to the account at account_id `recipient_account_id_1`
    let subcommand = TokenProgramAgnosticSubcommand::Send {
        from: format_public_account_id(supply_account_id_1),
        to: Some(format_public_account_id(recipient_account_id_1)),
        to_npk: None,
        to_vpk: None,
        amount: 7,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Create new token
    let subcommand = TokenProgramAgnosticSubcommand::New {
        definition_account_id: format_public_account_id(definition_account_id_2),
        supply_account_id: format_public_account_id(supply_account_id_2),
        name: "A NAM2".to_string(),
        total_supply: 37,
    };
    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Transfer 7 tokens from `supply_acc` to the account at account_id `recipient_account_id_2`
    let subcommand = TokenProgramAgnosticSubcommand::Send {
        from: format_public_account_id(supply_account_id_2),
        to: Some(format_public_account_id(recipient_account_id_2)),
        to_npk: None,
        to_vpk: None,
        amount: 7,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("=================== SETUP FINISHED ===============");

    // Create new AMM

    // Setup accounts
    // Create new account for the user holding lp
    let SubcommandReturnValue::RegisterAccount {
        account_id: user_holding_lp,
    } = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: None,
            label: None,
        })),
    )
    .await?
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    let amm_program_id = Program::amm().id();
    let pool_account_id = compute_pool_pda(
        amm_program_id,
        definition_account_id_1,
        definition_account_id_2,
    );
    let amm_ids = AmmStateIds {
        definition_a: definition_account_id_1,
        definition_b: definition_account_id_2,
        pool: pool_account_id,
        vault_a: compute_vault_pda(amm_program_id, pool_account_id, definition_account_id_1),
        vault_b: compute_vault_pda(amm_program_id, pool_account_id, definition_account_id_2),
        lp_definition: compute_liquidity_token_pda(amm_program_id, pool_account_id),
    };

    let subcommand = AmmProgramAgnosticSubcommand::New {
        user_holding_a: format_public_account_id(recipient_account_id_1),
        user_holding_b: format_public_account_id(recipient_account_id_2),
        user_holding_lp: format_public_account_id(user_holding_lp),
        balance_a: 3,
        balance_b: 3,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::AMM(subcommand)).await?;
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    assert_user_balances(
        &ctx,
        recipient_account_id_1,
        recipient_account_id_2,
        user_holding_lp,
        4,
        4,
        3,
    )
    .await?;
    assert_amm_state(&ctx, &amm_ids, 3, 3, 3, 3, 3, true).await?;

    info!("=================== AMM DEFINITION FINISHED ===============");

    // Make swap

    let subcommand = AmmProgramAgnosticSubcommand::Swap {
        user_holding_a: format_public_account_id(recipient_account_id_1),
        user_holding_b: format_public_account_id(recipient_account_id_2),
        amount_in: 2,
        min_amount_out: 1,
        token_definition: definition_account_id_1.to_string(),
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::AMM(subcommand)).await?;
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    assert_user_balances(
        &ctx,
        recipient_account_id_1,
        recipient_account_id_2,
        user_holding_lp,
        2,
        5,
        3,
    )
    .await?;
    assert_amm_state(&ctx, &amm_ids, 5, 2, 3, 5, 2, true).await?;

    info!("=================== FIRST SWAP FINISHED ===============");

    let subcommand = AmmProgramAgnosticSubcommand::Swap {
        user_holding_a: format_public_account_id(recipient_account_id_1),
        user_holding_b: format_public_account_id(recipient_account_id_2),
        amount_in: 2,
        min_amount_out: 1,
        token_definition: definition_account_id_2.to_string(),
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::AMM(subcommand)).await?;
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    assert_user_balances(
        &ctx,
        recipient_account_id_1,
        recipient_account_id_2,
        user_holding_lp,
        4,
        3,
        3,
    )
    .await?;
    assert_amm_state(&ctx, &amm_ids, 3, 4, 3, 3, 4, true).await?;

    info!("=================== SECOND SWAP FINISHED ===============");

    let subcommand = AmmProgramAgnosticSubcommand::AddLiquidity {
        user_holding_a: format_public_account_id(recipient_account_id_1),
        user_holding_b: format_public_account_id(recipient_account_id_2),
        user_holding_lp: format_public_account_id(user_holding_lp),
        min_amount_lp: 1,
        max_amount_a: 2,
        max_amount_b: 2,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::AMM(subcommand)).await?;
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    assert_user_balances(
        &ctx,
        recipient_account_id_1,
        recipient_account_id_2,
        user_holding_lp,
        3,
        1,
        4,
    )
    .await?;
    assert_amm_state(&ctx, &amm_ids, 4, 6, 4, 4, 6, true).await?;

    info!("=================== ADD LIQ FINISHED ===============");

    let subcommand = AmmProgramAgnosticSubcommand::RemoveLiquidity {
        user_holding_a: format_public_account_id(recipient_account_id_1),
        user_holding_b: format_public_account_id(recipient_account_id_2),
        user_holding_lp: format_public_account_id(user_holding_lp),
        balance_lp: 2,
        min_amount_a: 1,
        min_amount_b: 1,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::AMM(subcommand)).await?;
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    assert_user_balances(
        &ctx,
        recipient_account_id_1,
        recipient_account_id_2,
        user_holding_lp,
        5,
        4,
        2,
    )
    .await?;
    assert_amm_state(&ctx, &amm_ids, 2, 3, 2, 2, 3, true).await?;

    info!("Success!");

    Ok(())
}

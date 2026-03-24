use std::collections::HashSet;

#[cfg(any(feature = "host", test))]
use borsh::{BorshDeserialize, BorshSerialize};
use risc0_zkvm::{DeserializeOwned, guest::env, serde::Deserializer};
use serde::{Deserialize, Serialize};

use crate::account::{Account, AccountId, AccountWithMetadata};

pub const DEFAULT_PROGRAM_ID: ProgramId = [0; 8];
pub const MAX_NUMBER_CHAINED_CALLS: usize = 10;

pub type ProgramId = [u32; 8];
pub type InstructionData = Vec<u32>;
pub struct ProgramInput<T> {
    pub pre_states: Vec<AccountWithMetadata>,
    pub instruction: T,
}

/// A 32-byte seed used to compute a *Program-Derived `AccountId`* (PDA).
///
/// Each program can derive up to `2^256` unique account IDs by choosing different
/// seeds. PDAs allow programs to control namespaced account identifiers without
/// collisions between programs.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct PdaSeed([u8; 32]);

impl PdaSeed {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl From<(&ProgramId, &PdaSeed)> for AccountId {
    fn from(value: (&ProgramId, &PdaSeed)) -> Self {
        use risc0_zkvm::sha::{Impl, Sha256 as _};
        const PROGRAM_DERIVED_ACCOUNT_ID_PREFIX: &[u8; 32] =
            b"/NSSA/v0.2/AccountId/PDA/\x00\x00\x00\x00\x00\x00\x00";

        let mut bytes = [0; 96];
        bytes[0..32].copy_from_slice(PROGRAM_DERIVED_ACCOUNT_ID_PREFIX);
        let program_id_bytes: &[u8] =
            bytemuck::try_cast_slice(value.0).expect("ProgramId should be castable to &[u8]");
        bytes[32..64].copy_from_slice(program_id_bytes);
        bytes[64..].copy_from_slice(&value.1.0);
        Self::new(
            Impl::hash_bytes(&bytes)
                .as_bytes()
                .try_into()
                .expect("Hash output must be exactly 32 bytes long"),
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ChainedCall {
    /// The program ID of the program to execute.
    pub program_id: ProgramId,
    pub pre_states: Vec<AccountWithMetadata>,
    /// The instruction data to pass.
    pub instruction_data: InstructionData,
    pub pda_seeds: Vec<PdaSeed>,
}

impl ChainedCall {
    /// Creates a new chained call serializing the given instruction.
    pub fn new<I: Serialize>(
        program_id: ProgramId,
        pre_states: Vec<AccountWithMetadata>,
        instruction: &I,
    ) -> Self {
        Self {
            program_id,
            pre_states,
            instruction_data: risc0_zkvm::serde::to_vec(instruction)
                .expect("Serialization to Vec<u32> should not fail"),
            pda_seeds: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_pda_seeds(mut self, pda_seeds: Vec<PdaSeed>) -> Self {
        self.pda_seeds = pda_seeds;
        self
    }
}

/// Represents the final state of an `Account` after a program execution.
///
/// A post state may optionally request that the executing program
/// becomes the owner of the account (a “claim”). This is used to signal
/// that the program intends to take ownership of the account.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(any(feature = "host", test), derive(PartialEq, Eq))]
pub struct AccountPostState {
    account: Account,
    claim: bool,
}

impl AccountPostState {
    /// Creates a post state without a claim request.
    /// The executing program is not requesting ownership of the account.
    #[must_use]
    pub const fn new(account: Account) -> Self {
        Self {
            account,
            claim: false,
        }
    }

    /// Creates a post state that requests ownership of the account.
    /// This indicates that the executing program intends to claim the
    /// account as its own and is allowed to mutate it.
    #[must_use]
    pub const fn new_claimed(account: Account) -> Self {
        Self {
            account,
            claim: true,
        }
    }

    /// Creates a post state that requests ownership of the account
    /// if the account's program owner is the default program ID.
    #[must_use]
    pub fn new_claimed_if_default(account: Account) -> Self {
        let claim = account.program_owner == DEFAULT_PROGRAM_ID;
        Self { account, claim }
    }

    /// Returns `true` if this post state requests that the account
    /// be claimed (owned) by the executing program.
    #[must_use]
    pub const fn requires_claim(&self) -> bool {
        self.claim
    }

    /// Returns the underlying account.
    #[must_use]
    pub const fn account(&self) -> &Account {
        &self.account
    }

    /// Returns the underlying account.
    pub const fn account_mut(&mut self) -> &mut Account {
        &mut self.account
    }

    /// Consumes the post state and returns the underlying account.
    #[must_use]
    pub fn into_account(self) -> Account {
        self.account
    }
}

pub type BlockId = u64;
/// Unix timestamp in milliseconds.
pub type Timestamp = u64;

#[derive(Serialize, Deserialize, Clone, Copy)]
#[cfg_attr(
    any(feature = "host", test),
    derive(Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)
)]
pub struct ValidityWindow {
    from: Option<BlockId>,
    to: Option<BlockId>,
    from_timestamp: Option<Timestamp>,
    to_timestamp: Option<Timestamp>,
}

impl ValidityWindow {
    #[must_use]
    pub const fn new_unbounded() -> Self {
        Self {
            from: None,
            to: None,
            from_timestamp: None,
            to_timestamp: None,
        }
    }

    /// Valid for block IDs in the range [from, to) and timestamps in [from_timestamp,
    /// to_timestamp).
    #[must_use]
    pub fn is_valid_for(&self, block_id: BlockId, timestamp_ms: Timestamp) -> bool {
        self.from.is_none_or(|start| block_id >= start)
            && self.to.is_none_or(|end| block_id < end)
            && self.from_timestamp.is_none_or(|t| timestamp_ms >= t)
            && self.to_timestamp.is_none_or(|t| timestamp_ms < t)
    }

    /// Valid for block IDs in the range [from, to), where `from` is included and `to` is excluded.
    /// Ignores timestamp bounds.
    #[must_use]
    pub fn is_valid_for_block_id(&self, id: BlockId) -> bool {
        self.from.is_none_or(|start| id >= start) && self.to.is_none_or(|end| id < end)
    }

    const fn check_window(&self) -> Result<(), InvalidWindow> {
        if let (Some(from_id), Some(until_id)) = (self.from, self.to)
            && from_id >= until_id
        {
            return Err(InvalidWindow);
        }
        if let (Some(from_ts), Some(until_ts)) = (self.from_timestamp, self.to_timestamp)
            && from_ts >= until_ts
        {
            return Err(InvalidWindow);
        }
        Ok(())
    }

    #[must_use]
    pub const fn from(&self) -> Option<BlockId> {
        self.from
    }

    #[must_use]
    pub const fn to(&self) -> Option<BlockId> {
        self.to
    }

    #[must_use]
    pub const fn from_timestamp(&self) -> Option<Timestamp> {
        self.from_timestamp
    }

    #[must_use]
    pub const fn to_timestamp(&self) -> Option<Timestamp> {
        self.to_timestamp
    }
}

impl TryFrom<(Option<BlockId>, Option<BlockId>)> for ValidityWindow {
    type Error = InvalidWindow;

    fn try_from(value: (Option<BlockId>, Option<BlockId>)) -> Result<Self, Self::Error> {
        let this = Self {
            from: value.0,
            to: value.1,
            from_timestamp: None,
            to_timestamp: None,
        };
        this.check_window()?;
        Ok(this)
    }
}

impl
    TryFrom<(
        Option<BlockId>,
        Option<BlockId>,
        Option<Timestamp>,
        Option<Timestamp>,
    )> for ValidityWindow
{
    type Error = InvalidWindow;

    fn try_from(
        value: (
            Option<BlockId>,
            Option<BlockId>,
            Option<Timestamp>,
            Option<Timestamp>,
        ),
    ) -> Result<Self, Self::Error> {
        let this = Self {
            from: value.0,
            to: value.1,
            from_timestamp: value.2,
            to_timestamp: value.3,
        };
        this.check_window()?;
        Ok(this)
    }
}

#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
#[error("Invalid window")]
pub struct InvalidWindow;

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct ProgramOutput {
    /// The instruction data the program received to produce this output.
    pub instruction_data: InstructionData,
    /// The account pre states the program received to produce this output.
    pub pre_states: Vec<AccountWithMetadata>,
    /// The account post states the program execution produced.
    pub post_states: Vec<AccountPostState>,
    /// The list of chained calls to other programs.
    pub chained_calls: Vec<ChainedCall>,
    /// The window where the program output is valid.
    /// Valid for block IDs in the range [from, to), where `from` is included and `to` is excluded.
    /// `None` means unbounded on that side.
    pub validity_window: ValidityWindow,
}

impl ProgramOutput {
    #[must_use]
    pub const fn new(
        instruction_data: InstructionData,
        pre_states: Vec<AccountWithMetadata>,
        post_states: Vec<AccountPostState>,
    ) -> Self {
        Self {
            instruction_data,
            pre_states,
            post_states,
            chained_calls: Vec::new(),
            validity_window: ValidityWindow::new_unbounded(),
        }
    }

    pub fn write(self) {
        env::commit(&self);
    }

    #[must_use]
    pub fn with_chained_calls(mut self, chained_calls: Vec<ChainedCall>) -> Self {
        self.chained_calls = chained_calls;
        self
    }

    pub fn valid_from_id(mut self, id: Option<BlockId>) -> Result<Self, InvalidWindow> {
        self.validity_window.from = id;
        self.validity_window.check_window()?;
        Ok(self)
    }

    pub fn valid_until_id(mut self, id: Option<BlockId>) -> Result<Self, InvalidWindow> {
        self.validity_window.to = id;
        self.validity_window.check_window()?;
        Ok(self)
    }

    pub fn valid_from_timestamp(mut self, ts: Option<Timestamp>) -> Result<Self, InvalidWindow> {
        self.validity_window.from_timestamp = ts;
        self.validity_window.check_window()?;
        Ok(self)
    }

    pub fn valid_until_timestamp(mut self, ts: Option<Timestamp>) -> Result<Self, InvalidWindow> {
        self.validity_window.to_timestamp = ts;
        self.validity_window.check_window()?;
        Ok(self)
    }
}

/// Representation of a number as `lo + hi * 2^128`.
#[derive(PartialEq, Eq)]
struct WrappedBalanceSum {
    lo: u128,
    hi: u128,
}

impl WrappedBalanceSum {
    /// Constructs a [`WrappedBalanceSum`] from an iterator of balances.
    ///
    /// Returns [`None`] if balance sum overflows `lo + hi * 2^128` representation, which is not
    /// expected in practical scenarios.
    fn from_balances(balances: impl Iterator<Item = u128>) -> Option<Self> {
        let mut wrapped = Self { lo: 0, hi: 0 };

        for balance in balances {
            let (new_sum, did_overflow) = wrapped.lo.overflowing_add(balance);
            if did_overflow {
                wrapped.hi = wrapped.hi.checked_add(1)?;
            }
            wrapped.lo = new_sum;
        }

        Some(wrapped)
    }
}

#[must_use]
pub fn compute_authorized_pdas(
    caller_program_id: Option<ProgramId>,
    pda_seeds: &[PdaSeed],
) -> HashSet<AccountId> {
    caller_program_id
        .map(|caller_program_id| {
            pda_seeds
                .iter()
                .map(|pda_seed| AccountId::from((&caller_program_id, pda_seed)))
                .collect()
        })
        .unwrap_or_default()
}

/// Reads the NSSA inputs from the guest environment.
#[must_use]
pub fn read_nssa_inputs<T: DeserializeOwned>() -> (ProgramInput<T>, InstructionData) {
    let pre_states: Vec<AccountWithMetadata> = env::read();
    let instruction_words: InstructionData = env::read();
    let instruction = T::deserialize(&mut Deserializer::new(instruction_words.as_ref())).unwrap();
    (
        ProgramInput {
            pre_states,
            instruction,
        },
        instruction_words,
    )
}

pub fn write_nssa_outputs(
    instruction_data: InstructionData,
    pre_states: Vec<AccountWithMetadata>,
    post_states: Vec<AccountPostState>,
) {
    ProgramOutput::new(instruction_data, pre_states, post_states).write();
}

pub fn write_nssa_outputs_with_chained_call(
    instruction_data: InstructionData,
    pre_states: Vec<AccountWithMetadata>,
    post_states: Vec<AccountPostState>,
    chained_calls: Vec<ChainedCall>,
) {
    ProgramOutput::new(instruction_data, pre_states, post_states)
        .with_chained_calls(chained_calls)
        .write();
}

/// Validates well-behaved program execution.
///
/// # Parameters
/// - `pre_states`: The list of input accounts, each annotated with authorization metadata.
/// - `post_states`: The list of resulting accounts after executing the program logic.
/// - `executing_program_id`: The identifier of the program that was executed.
#[must_use]
pub fn validate_execution(
    pre_states: &[AccountWithMetadata],
    post_states: &[AccountPostState],
    executing_program_id: ProgramId,
) -> bool {
    // 1. Check account ids are all different
    if !validate_uniqueness_of_account_ids(pre_states) {
        return false;
    }

    // 2. Lengths must match
    if pre_states.len() != post_states.len() {
        return false;
    }

    for (pre, post) in pre_states.iter().zip(post_states) {
        // 3. Nonce must remain unchanged
        if pre.account.nonce != post.account.nonce {
            return false;
        }

        // 4. Program ownership changes are not allowed
        if pre.account.program_owner != post.account.program_owner {
            return false;
        }

        let account_program_owner = pre.account.program_owner;

        // 5. Decreasing balance only allowed if owned by executing program
        if post.account.balance < pre.account.balance
            && account_program_owner != executing_program_id
        {
            return false;
        }

        // 6. Data changes only allowed if owned by executing program or if account pre state has
        //    default values
        if pre.account.data != post.account.data
            && pre.account != Account::default()
            && account_program_owner != executing_program_id
        {
            return false;
        }

        // 7. If a post state has default program owner, the pre state must have been a default
        //    account
        if post.account.program_owner == DEFAULT_PROGRAM_ID && pre.account != Account::default() {
            return false;
        }
    }

    // 8. Total balance is preserved

    let Some(total_balance_pre_states) =
        WrappedBalanceSum::from_balances(pre_states.iter().map(|pre| pre.account.balance))
    else {
        return false;
    };

    let Some(total_balance_post_states) =
        WrappedBalanceSum::from_balances(post_states.iter().map(|post| post.account.balance))
    else {
        return false;
    };

    if total_balance_pre_states != total_balance_post_states {
        return false;
    }

    true
}

fn validate_uniqueness_of_account_ids(pre_states: &[AccountWithMetadata]) -> bool {
    let number_of_accounts = pre_states.len();
    let number_of_account_ids = pre_states
        .iter()
        .map(|account| &account.account_id)
        .collect::<HashSet<_>>()
        .len();

    number_of_accounts == number_of_account_ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_state_new_with_claim_constructor() {
        let account = Account {
            program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
            balance: 1337,
            data: vec![0xde, 0xad, 0xbe, 0xef].try_into().unwrap(),
            nonce: 10_u128.into(),
        };

        let account_post_state = AccountPostState::new_claimed(account.clone());

        assert_eq!(account, account_post_state.account);
        assert!(account_post_state.requires_claim());
    }

    #[test]
    fn post_state_new_without_claim_constructor() {
        let account = Account {
            program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
            balance: 1337,
            data: vec![0xde, 0xad, 0xbe, 0xef].try_into().unwrap(),
            nonce: 10_u128.into(),
        };

        let account_post_state = AccountPostState::new(account.clone());

        assert_eq!(account, account_post_state.account);
        assert!(!account_post_state.requires_claim());
    }

    #[test]
    fn post_state_account_getter() {
        let mut account = Account {
            program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
            balance: 1337,
            data: vec![0xde, 0xad, 0xbe, 0xef].try_into().unwrap(),
            nonce: 10_u128.into(),
        };

        let mut account_post_state = AccountPostState::new(account.clone());

        assert_eq!(account_post_state.account(), &account);
        assert_eq!(account_post_state.account_mut(), &mut account);
    }
}

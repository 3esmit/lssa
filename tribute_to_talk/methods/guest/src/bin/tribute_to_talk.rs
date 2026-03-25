use tribute_to_talk_core::{InstructionV1, PaymentNoteDataV1};
use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::{
        AccountPostState, ProgramInput, read_nssa_inputs, write_nssa_outputs,
    },
};

fn initialize_account(pre_state: AccountWithMetadata) -> AccountPostState {
    assert!(pre_state.is_authorized, "Account must be authorized");
    assert!(
        pre_state.account == Account::default(),
        "Account must be uninitialized"
    );

    let mut account = pre_state.account;
    account.data = PaymentNoteDataV1::empty()
        .into_account_data()
        .expect("empty payment note data should be valid");

    AccountPostState::new_claimed(account)
}

fn send_payment(
    sender: AccountWithMetadata,
    recipient: AccountWithMetadata,
    amount: u128,
    message: Vec<u8>,
) -> Vec<AccountPostState> {
    assert!(sender.is_authorized, "Sender must be authorized");
    assert!(
        recipient.account == Account::default(),
        "Recipient address must be uninitialized"
    );

    let empty_note = PaymentNoteDataV1::empty()
        .into_account_data()
        .expect("empty payment note data should be valid");
    let recipient_note = PaymentNoteDataV1::new(message)
        .expect("message must satisfy length bounds")
        .into_account_data()
        .expect("recipient payment note data should be valid");

    let sender_post = {
        let mut account = sender.account;
        account.balance = account
            .balance
            .checked_sub(amount)
            .expect("Sender has insufficient balance");
        account.data = empty_note;
        AccountPostState::new(account)
    };

    let recipient_post = {
        let mut account = recipient.account;
        account.balance = amount;
        account.data = recipient_note;
        AccountPostState::new_claimed(account)
    };

    vec![sender_post, recipient_post]
}

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction,
        },
        instruction_words,
    ) = read_nssa_inputs::<InstructionV1>();

    let post_states = match (pre_states.as_slice(), instruction) {
        ([account], InstructionV1::Init) => vec![initialize_account(account.clone())],
        ([sender, recipient], InstructionV1::Send { amount, message }) => {
            send_payment(sender.clone(), recipient.clone(), amount, message)
        }
        _ => panic!("invalid input shape for Tribute to Talk"),
    };

    write_nssa_outputs(instruction_words, pre_states, post_states);
}

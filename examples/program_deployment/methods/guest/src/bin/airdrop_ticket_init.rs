use nssa_core::program::{
    read_nssa_inputs, write_nssa_outputs, AccountPostState, DEFAULT_PROGRAM_ID, ProgramInput,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct TicketInitInstruction {
    secret: [u8; 32],
}

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction,
        },
        instruction_data,
    ) = read_nssa_inputs::<TicketInitInstruction>();

    if pre_states.len() != 1 {
        panic!("Ticket init requires exactly 1 account");
    }

    let pre_state = &pre_states[0];
    if pre_state.account.program_owner != DEFAULT_PROGRAM_ID {
        panic!("Account already initialized");
    }

    let mut post_account = pre_state.account.clone();
    post_account.data = instruction.secret.to_vec().try_into().expect("Data too large");

    write_nssa_outputs(
        instruction_data,
        pre_states,
        vec![AccountPostState::new_claimed(post_account)],
    );
}

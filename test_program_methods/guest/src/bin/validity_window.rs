use nssa_core::program::{
    AccountPostState, BlockId, ProgramInput, ProgramOutput, read_nssa_inputs,
};

type Instruction = (Option<BlockId>, Option<BlockId>);

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction: (from_id, until_id),
        },
        instruction_words,
    ) = read_nssa_inputs::<Instruction>();

    let Ok([pre]) = <[_; 1]>::try_from(pre_states) else {
        return;
    };

    let post = pre.account.clone();

    ProgramOutput::new(
        instruction_words,
        vec![pre],
        vec![AccountPostState::new(post)],
    )
    .try_with_validity_window((from_id, until_id))
    .unwrap()
    .write();
}

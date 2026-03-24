use nssa_core::program::{
    AccountPostState, BlockId, ProgramInput, ProgramOutput, Timestamp, read_nssa_inputs,
};

type Instruction = (
    Option<BlockId>,
    Option<BlockId>,
    Option<Timestamp>,
    Option<Timestamp>,
);

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction: (from_id, until_id, from_ts, until_ts),
        },
        instruction_words,
    ) = read_nssa_inputs::<Instruction>();

    let Ok([pre]) = <[_; 1]>::try_from(pre_states) else {
        return;
    };

    let post = pre.account.clone();

    let output = ProgramOutput::new(
        instruction_words,
        vec![pre],
        vec![AccountPostState::new(post)],
    )
    .valid_from_id(from_id)
    .unwrap()
    .valid_until_id(until_id)
    .unwrap()
    .valid_from_timestamp(from_ts)
    .unwrap()
    .valid_until_timestamp(until_ts)
    .unwrap();

    output.write();
}

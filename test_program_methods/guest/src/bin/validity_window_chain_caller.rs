use nssa_core::program::{
    AccountPostState, BlockId, ChainedCall, ProgramId, ProgramInput, ProgramOutput,
    read_nssa_inputs,
};
use risc0_zkvm::serde::to_vec;

/// A program that sets a validity window on its output and chains to another program with a
/// potentially different validity window.
///
/// Instruction: (`from_id`, `until_id`, `chained_program_id`, `chained_from`, `chained_until`)
/// The initial output uses [`from_id`, `until_id`) and chains to `chained_program_id` with
/// [`chained_from`, `chained_until`).
type Instruction = (
    Option<BlockId>,
    Option<BlockId>,
    ProgramId,
    Option<BlockId>,
    Option<BlockId>,
);

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction: (from_id, until_id, chained_program_id, chained_from, chained_until),
        },
        instruction_words,
    ) = read_nssa_inputs::<Instruction>();

    let [pre] = <[_; 1]>::try_from(pre_states.clone())
        .unwrap_or_else(|_| panic!("Expected exactly one pre state"));
    let post = pre.account.clone();

    let chained_instruction = to_vec(&(chained_from, chained_until)).unwrap();
    let chained_call = ChainedCall {
        program_id: chained_program_id,
        instruction_data: chained_instruction,
        pre_states,
        pda_seeds: vec![],
    };

    ProgramOutput::new(
        instruction_words,
        vec![pre],
        vec![AccountPostState::new(post)],
    )
    .try_with_validity_window((from_id, until_id))
    .unwrap()
    .with_chained_calls(vec![chained_call])
    .write();
}

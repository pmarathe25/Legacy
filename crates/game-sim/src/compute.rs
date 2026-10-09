//! Pure CPU batch contract. No world references, allocation, or ECS handles cross it.
use game_api::Error;
use rayon::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ProgressInput {
    pub units: u64,
    pub remainder: u64,
    pub units_per_minute: u64,
    pub elapsed_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgressOutput {
    pub units: u64,
    pub remainder: u64,
}

pub fn progress(input: ProgressInput) -> Result<ProgressOutput, Error> {
    let amount = u128::from(input.units_per_minute) * u128::from(input.elapsed_ms)
        + u128::from(input.remainder);
    Ok(ProgressOutput {
        units: input
            .units
            .checked_add(u64::try_from(amount / 60_000).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?,
        remainder: (amount % 60_000) as u64,
    })
}

pub(crate) fn evaluate(
    pool: &rayon::ThreadPool,
    input: &[ProgressInput],
    reverse: bool,
) -> Result<Vec<ProgressOutput>, Error> {
    let evaluate = || {
        if pool.current_num_threads() == 1 || input.len() < 256 {
            input.iter().copied().map(progress).collect()
        } else if reverse {
            // Perturb task submission and merge order without changing logical row keys.
            let mut output: Vec<_> = input
                .par_iter()
                .with_min_len(256)
                .enumerate()
                .rev()
                .map(|(i, v)| progress(*v).map(|out| (i, out)))
                .collect::<Result<_, _>>()?;
            output.sort_unstable_by_key(|(i, _)| *i);
            Ok(output.into_iter().map(|(_, v)| v).collect())
        } else {
            input
                .par_iter()
                .with_min_len(256)
                .copied()
                .map(progress)
                .collect()
        }
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pool.install(evaluate)))
        .map_err(|_| Error::WorkerFailure)?
}

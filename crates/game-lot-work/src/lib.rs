//! Physical production plans over actual dated custody, independent of social motives.
mod state;
mod validation;
use game_lot_content::Catalog;
use game_lot_model::{Container, Movement, Place, SimpleTransfer, Transaction};
use game_production_model::{Amount, Job, JobPhase, Recipe, Start};
use rayon::prelude::*;
pub use state::{Phase, Snapshot, Work, WorkInterval, WorkProgress};
use std::collections::HashMap;
pub use validation::validate;

pub fn validate_actions(catalog: &Catalog, actions: &[Transaction]) -> Result<(), String> {
    if actions.par_iter().any(|action| {
        !action.creations.is_empty()
            || action.movements.iter().any(|movement| {
                !catalog.contains(movement.good)
                    || movement.to.is_some_and(|place| {
                        matches!(place, Place::JobInput(_) | Place::JobOutput(_))
                    })
            })
    }) {
        return Err("invalid external physical action".into());
    }
    Ok(())
}
pub fn release_actions(actions: &mut Vec<Transaction>, combined: Option<Vec<Transaction>>) {
    std::mem::take(actions)
        .into_par_iter()
        .chain(combined.unwrap_or_default().into_par_iter())
        .for_each(drop);
}
pub fn validate_starts(work: &[Work], requests: &[Start]) -> Result<(), String> {
    let mut workers: Vec<_> = work
        .par_iter()
        .filter(|row| row.phase == Phase::Working)
        .map(|row| row.job.worker)
        .collect();
    workers.par_sort_unstable();
    if requests.par_iter().any(|request| {
        work.binary_search_by_key(&request.id, |row| row.job.id)
            .is_ok()
            || workers.binary_search(&request.worker).is_ok()
    }) {
        return Err("job identity or worker is already active".into());
    }
    Ok(())
}

pub struct Starts {
    pub containers: Vec<Container>,
    pub jobs: Vec<Job>,
    pub transactions: Vec<Transaction>,
}
fn offsets(counts: &[u64]) -> Result<Vec<u64>, String> {
    let offsets = game_lot_planning::prefixes(counts);
    offsets
        .into_par_iter()
        .map(|value| u64::try_from(value).map_err(|_| "operation identity overflow".into()))
        .collect()
}
fn capacity(items: &[Amount]) -> Result<u64, String> {
    items
        .iter()
        .try_fold(0u64, |sum, item| sum.checked_add(item.quantity))
        .filter(|sum| *sum > 0)
        .ok_or("invalid bundle capacity".into())
}
fn start_jobs(requests: &[Start], recipes: &[Recipe]) -> Result<Vec<Job>, String> {
    #[cfg(feature = "profile")]
    let mut phase = game_diagnostics::span("work.start.validate");
    let mut ordered = requests.to_vec();
    ordered.par_sort_unstable_by_key(|row| row.id);
    let mut workers: Vec<_> = ordered.par_iter().map(|row| row.worker).collect();
    workers.par_sort_unstable();
    if ordered.par_windows(2).any(|rows| rows[0].id == rows[1].id)
        || workers.par_windows(2).any(|rows| rows[0] == rows[1])
    {
        return Err("duplicate job or worker".into());
    }
    let mut index = HashMap::new();
    for recipe in recipes {
        if recipe.id == 0 || index.insert(recipe.id, recipe).is_some() {
            return Err("invalid recipe identity".into());
        }
    }
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("work.start.jobs");
    }
    let jobs: Vec<Job> = ordered
        .par_iter()
        .map(|request| {
            if request.id == 0 || request.worker == 0 || request.region == 0 || request.units == 0 {
                return Err("invalid work start".to_string());
            }
            let recipe = index.get(&request.recipe).ok_or("unknown recipe")?;
            if recipe.ticks == 0 || recipe.inputs.is_empty() || recipe.outputs.is_empty() {
                return Err("invalid recipe".to_string());
            }
            let scale = |items: &[Amount]| {
                items
                    .iter()
                    .map(|item| {
                        if item.good == 0 || item.quantity == 0 {
                            return Err("invalid recipe amount".to_string());
                        }
                        Ok(Amount {
                            good: item.good,
                            quantity: item
                                .quantity
                                .checked_mul(u64::from(request.units))
                                .ok_or("recipe amount overflow")?,
                        })
                    })
                    .collect::<Result<_, String>>()
            };
            let reserved = scale(&recipe.inputs)?;
            let outputs = scale(&recipe.outputs)?;
            Ok(Job {
                id: request.id,
                worker: request.worker,
                region: request.region,
                recipe: request.recipe,
                remaining: u64::from(recipe.ticks)
                    .checked_mul(u64::from(request.units))
                    .ok_or("work duration overflow")?,
                phase: JobPhase::Working,
                reserved,
                outputs,
            })
        })
        .collect::<Result<_, String>>()?;
    #[cfg(feature = "profile")]
    drop(phase);
    Ok(jobs)
}

/// Construct custody and input reservations; the caller decides who attempts work.
pub fn start(requests: &[Start], recipes: &[Recipe]) -> Result<Starts, String> {
    let jobs = start_jobs(requests, recipes)?;
    #[cfg(feature = "profile")]
    let phase = game_diagnostics::span("work.start.transactions");
    let counts: Vec<_> = jobs
        .par_iter()
        .map(|job| job.reserved.len() as u64)
        .collect();
    let offsets = offsets(&counts)?;
    let results: Vec<_> = jobs
        .par_iter()
        .enumerate()
        .map(|(index, job)| {
            let input = Container::new(Place::JobInput(job.id), capacity(&job.reserved)?, vec![])?;
            let output = Container::new(Place::JobOutput(job.id), capacity(&job.outputs)?, vec![])?;
            let movements = job
                .reserved
                .iter()
                .enumerate()
                .map(|(n, amount)| {
                    Ok(Movement {
                        batch: None,
                        item: None,
                        id: offsets[index]
                            .checked_add(n as u64)
                            .and_then(|id| id.checked_add(1))
                            .ok_or("operation identity overflow")?,
                        from: Place::Stock {
                            region: job.region,
                            good: amount.good,
                        },
                        to: Some(input.place),
                        good: amount.good,
                        quantity: amount.quantity,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok((
                input,
                output,
                Transaction {
                    id: job.id,
                    movements,
                    creations: vec![],
                },
            ))
        })
        .collect::<Result<_, String>>()?;
    let containers = results
        .par_iter()
        .flat_map_iter(|(input, output, _)| [input.clone(), output.clone()])
        .collect();
    let transactions = results
        .into_par_iter()
        .map(|(_, _, transaction)| transaction)
        .collect();
    #[cfg(feature = "profile")]
    drop(phase);
    Ok(Starts {
        containers,
        jobs,
        transactions,
    })
}

/// Packed single-input starts for one shared stock. Unsupported shapes retain
/// the ordinary transaction planner and its validation behavior.
pub struct FlatStarts {
    pub containers: Vec<Container>,
    pub jobs: Vec<Job>,
    pub transfers: Vec<SimpleTransfer>,
}

pub fn start_flat(requests: &[Start], recipes: &[Recipe]) -> Result<Option<FlatStarts>, String> {
    if requests.len() < 128 {
        return Ok(None);
    }
    let first = requests[0];
    if requests
        .par_iter()
        .any(|row| row.region != first.region || row.recipe != first.recipe)
        || recipes
            .iter()
            .find(|recipe| recipe.id == first.recipe)
            .is_none_or(|recipe| recipe.inputs.len() != 1)
    {
        return Ok(None);
    }
    let jobs = start_jobs(requests, recipes)?;
    #[cfg(feature = "profile")]
    let _phase = game_diagnostics::span("work.start.flat_transactions");
    let chunk_size = jobs.len().div_ceil(64).clamp(16, 2048);
    let chunks: Vec<_> = jobs
        .par_chunks(chunk_size)
        .enumerate()
        .map(|(chunk_index, chunk)| {
            let mut containers = Vec::with_capacity(chunk.len() * 2);
            let mut transfers = Vec::with_capacity(chunk.len());
            for (offset, job) in chunk.iter().enumerate() {
                let index = chunk_index * chunk_size + offset;
                let input =
                    Container::new(Place::JobInput(job.id), capacity(&job.reserved)?, vec![])?;
                let output =
                    Container::new(Place::JobOutput(job.id), capacity(&job.outputs)?, vec![])?;
                let amount = &job.reserved[0];
                let movement = Movement {
                    batch: None,
                    item: None,
                    id: u64::try_from(index)
                        .map_err(|_| "operation identity overflow")?
                        .checked_add(1)
                        .ok_or("operation identity overflow")?,
                    from: Place::Stock {
                        region: job.region,
                        good: amount.good,
                    },
                    to: Some(input.place),
                    good: amount.good,
                    quantity: amount.quantity,
                };
                containers.extend([input, output]);
                transfers.push(SimpleTransfer {
                    action: job.id,
                    movement,
                });
            }
            Ok((containers, transfers))
        })
        .collect::<Result<_, String>>()?;
    let (container_chunks, transfer_chunks): (Vec<_>, Vec<_>) = chunks.into_iter().unzip();
    let (containers, transfers) = rayon::join(
        || container_chunks.into_par_iter().flatten().collect(),
        || transfer_chunks.into_par_iter().flatten().collect(),
    );
    Ok(Some(FlatStarts {
        containers,
        jobs,
        transfers,
    }))
}

/// A completed worker may discover that reserved inputs spoiled or disappeared.
pub struct Finishes {
    pub transactions: Vec<Transaction>,
    pub interrupted: Vec<u64>,
}
/// Plan completions from actual input custody. Missing inputs interrupt work;
/// the caller can return any surviving lots with `cancel`.
pub fn finish(
    rows: &[(&Job, &Container)],
    catalog: &Catalog,
    boundary: u64,
) -> Result<Finishes, String> {
    let mut ids: Vec<_> = rows.par_iter().map(|(job, _)| job.id).collect();
    ids.par_sort_unstable();
    if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("duplicate finishing job".into());
    }
    let ready: Vec<_> = rows
        .par_iter()
        .map(|(job, input)| {
            if job.id == 0
                || job.phase != JobPhase::Working
                || job.remaining != 0
                || job.reserved.is_empty()
                || job.outputs.is_empty()
                || input.place != Place::JobInput(job.id)
                || input.validate().is_err()
            {
                return Err("invalid finishing job custody".to_string());
            }
            let has_inputs = job.reserved.iter().all(|item| {
                let required: u128 = job
                    .reserved
                    .iter()
                    .filter(|row| row.good == item.good)
                    .map(|row| u128::from(row.quantity))
                    .sum();
                let available: u128 = input
                    .lots
                    .iter()
                    .filter(|lot| {
                        lot.good == item.good && lot.expires_at.is_none_or(|at| at > boundary)
                    })
                    .map(|lot| u128::from(lot.quantity))
                    .sum();
                available >= required
            });
            Ok((*job, has_inputs))
        })
        .collect::<Result<_, String>>()?;
    let interrupted = ready
        .par_iter()
        .filter_map(|(job, ready)| (!ready).then_some(job.id))
        .collect();
    let jobs: Vec<_> = ready
        .par_iter()
        .filter_map(|(job, ready)| ready.then_some(*job))
        .collect();
    Ok(Finishes {
        transactions: finish_ready(&jobs, catalog, boundary)?,
        interrupted,
    })
}

fn finish_ready(
    jobs: &[&Job],
    catalog: &Catalog,
    boundary: u64,
) -> Result<Vec<Transaction>, String> {
    let counts: Vec<_> = jobs
        .par_iter()
        .map(|job| (job.reserved.len() + job.outputs.len()) as u64)
        .collect();
    let offsets = offsets(&counts)?;
    jobs.par_iter()
        .enumerate()
        .map(|(index, job)| {
            if job.id == 0
                || job.phase != JobPhase::Working
                || job.remaining != 0
                || job.reserved.is_empty()
                || job.outputs.is_empty()
            {
                return Err("invalid finishing job".into());
            }
            let id = |n: usize| {
                offsets[index]
                    .checked_add(n as u64)
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(|| "operation identity overflow".to_string())
            };
            let movements = job
                .reserved
                .iter()
                .enumerate()
                .map(|(n, item)| {
                    Ok(Movement {
                        batch: None,
                        item: None,
                        id: id(n)?,
                        from: Place::JobInput(job.id),
                        to: None,
                        good: item.good,
                        quantity: item.quantity,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let creations = job
                .outputs
                .iter()
                .enumerate()
                .map(|(n, item)| {
                    catalog.creation(
                        id(job.reserved.len() + n)?,
                        Place::JobOutput(job.id),
                        item.good,
                        item.quantity,
                        boundary,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Transaction {
                id: job.id,
                movements,
                creations,
            })
        })
        .collect()
}

/// Deliver only surviving output without creating or refreshing any goods.
pub fn deliver(rows: &[(&Job, &Container)], boundary: u64) -> Result<Cancellations, String> {
    return_custody(rows, boundary, true)
}

/// An interruption can leave no usable materials even before return is attempted.
pub struct Cancellations {
    pub returns: Vec<Transaction>,
    pub empty: Vec<u64>,
}
/// Return only physically surviving reserved cohorts; expiry is still resolved by custody.
pub fn cancel(rows: &[(&Job, &Container)], boundary: u64) -> Result<Cancellations, String> {
    return_custody(rows, boundary, false)
}
fn return_custody(
    rows: &[(&Job, &Container)],
    boundary: u64,
    output: bool,
) -> Result<Cancellations, String> {
    let mut ids: Vec<_> = rows.par_iter().map(|(job, _)| job.id).collect();
    ids.par_sort_unstable();
    if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("duplicate cancelled job".into());
    }
    let amounts: Vec<Vec<Amount>> = rows
        .par_iter()
        .map(|(job, input)| {
            if job.id == 0
                || job.region == 0
                || job.phase
                    != if output {
                        JobPhase::Waiting
                    } else {
                        JobPhase::Working
                    }
                || input.place
                    != if output {
                        Place::JobOutput(job.id)
                    } else {
                        Place::JobInput(job.id)
                    }
                || input.validate().is_err()
            {
                return Err("invalid cancelled job custody".to_string());
            }
            let mut amounts: Vec<Amount> = Vec::new();
            for lot in input
                .lots
                .iter()
                .filter(|lot| lot.expires_at.is_none_or(|at| at > boundary))
            {
                if let Some(last) = amounts.last_mut().filter(|last| last.good == lot.good) {
                    last.quantity = last
                        .quantity
                        .checked_add(lot.quantity)
                        .ok_or("cancelled quantity overflow")?;
                } else {
                    amounts.push(Amount {
                        good: lot.good,
                        quantity: lot.quantity,
                    });
                }
            }
            Ok(amounts)
        })
        .collect::<Result<_, String>>()?;
    let counts: Vec<_> = amounts.par_iter().map(|items| items.len() as u64).collect();
    let offsets = offsets(&counts)?;
    let prepared: Vec<_> = rows
        .par_iter()
        .zip(amounts.par_iter())
        .enumerate()
        .map(|(index, ((job, _), items))| {
            if items.is_empty() {
                return Ok((job.id, None));
            }
            let movements = items
                .iter()
                .enumerate()
                .map(|(n, item)| {
                    Ok(Movement {
                        batch: None,
                        item: None,
                        id: offsets[index]
                            .checked_add(n as u64)
                            .and_then(|id| id.checked_add(1))
                            .ok_or("operation identity overflow")?,
                        from: if output {
                            Place::JobOutput(job.id)
                        } else {
                            Place::JobInput(job.id)
                        },
                        to: Some(Place::Stock {
                            region: job.region,
                            good: item.good,
                        }),
                        good: item.good,
                        quantity: item.quantity,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok((
                job.id,
                Some(Transaction {
                    id: job.id,
                    movements,
                    creations: vec![],
                }),
            ))
        })
        .collect::<Result<_, String>>()?;
    let empty = prepared
        .par_iter()
        .filter_map(|(id, transaction)| transaction.is_none().then_some(*id))
        .collect();
    let returns = prepared
        .into_par_iter()
        .filter_map(|(_, transaction)| transaction)
        .collect();
    Ok(Cancellations { returns, empty })
}

#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;

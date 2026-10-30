//! One concentrated election. Arguments: workers, eligible voters.
use game_parallel_institutions::{Action, Body, Cast, Institutions};
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(100_000), |v| v.parse())?;
    if workers == 0 || count == 0 || count > u32::MAX as usize {
        return Err("invalid arguments".into());
    }
    let people: Vec<_> = (0..count)
        .map(|id| (id as u64 + 100, Some(1), 100, 0))
        .collect();
    let mut state = Institutions {
        next_id: 1,
        bodies: vec![Body {
            id: 1,
            region: 1,
            members: people.iter().map(|p| p.0).collect(),
            quorum: (count / 2 + 1) as u32,
            chair: Some(100),
            trade_open: true,
        }],
        ballots: vec![],
        events: vec![],
        requests: vec![],
        admissions: vec![],
    };
    state.open(&people, 1, 100, Action::Trade { open: false }, 1, 0)?;
    let votes: Vec<_> = people
        .iter()
        .map(|person| Cast {
            ballot: 1,
            voter: person.0,
            yes: true,
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let start = Instant::now();
    pool.install(|| -> Result<(), String> {
        state.vote(&people, &votes)?;
        state.prepare(&people, 1).commit(&mut state);
        Ok(())
    })?;
    let elapsed = start.elapsed();
    if state.bodies[0].trade_open || !state.events[0].adopted {
        return Err("ballot outcome differs".into());
    }
    println!(
        "workers={workers} voters={count} elapsed_seconds={:.6}",
        elapsed.as_secs_f64()
    );
    Ok(())
}

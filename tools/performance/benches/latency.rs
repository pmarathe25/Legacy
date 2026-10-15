use game_core::*;
use game_performance::{fixture, metrics, setting};
use game_session::{Input, Output, Session};
use std::time::{Duration, Instant};
fn main() {
    metrics::metadata();
    let population = setting("LEGACY_POPULATION", 10_000);
    let workers = setting("LEGACY_WORKERS", 1);
    let (mut sim, _) = fixture(population, workers, false);
    // Enter the first work/rest transition before sampling queued input.
    while sim.advance(SimTime(10 * MINUTE.0), 32).unwrap().stop == game_sim::Stop::Budget {}
    let session = Session::spawn(sim).unwrap();
    let mut pauses = vec![];
    let mut commands = vec![];
    let begin = Instant::now();
    for sequence in 1..=100 {
        session
            .input
            .send(Input::Advance(SimTime(120 * MINUTE.0)))
            .unwrap();
        std::thread::sleep(Duration::from_millis(1));
        let start = Instant::now();
        session.input.send(Input::Pause).unwrap();
        loop {
            if matches!(session.output.recv().unwrap(), Output::Paused(_)) {
                break;
            }
        }
        pauses.push(start.elapsed().as_secs_f64() * 1000.);
        let start = Instant::now();
        session
            .input
            .send(Input::Command {
                sequence,
                command: game_api::Command::ObserveSurroundings,
            })
            .unwrap();
        loop {
            if let Output::Command(result) = session.output.recv().unwrap() {
                result.unwrap();
                break;
            }
        }
        commands.push(start.elapsed().as_secs_f64() * 1000.);
    }
    for (name, mut times) in [("pause", pauses), ("command", commands)] {
        times.sort_by(f64::total_cmp);
        println!(
            "population={population} workers={workers} operation={name} samples={} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} max_ms={:.3} elapsed_ms={:.3}",
            times.len(),
            metrics::percentile(&times, 50),
            metrics::percentile(&times, 95),
            metrics::percentile(&times, 99),
            times.last().unwrap(),
            begin.elapsed().as_secs_f64() * 1000.
        );
    }
    session.input.send(Input::Save).unwrap();
    loop {
        if let Output::Save(result) = session.output.recv().unwrap() {
            let save = result.unwrap();
            game_assembly::Simulation::load(&save, workers).unwrap();
            break;
        }
    }
    metrics::rss();
}

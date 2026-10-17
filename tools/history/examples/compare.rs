//! Compare full validated worlds after canonical loading, including older evidence formats.
use std::{path::Path, time::Instant};
fn canonical(path: &Path) -> String {
    let started = Instant::now();
    let source = std::fs::read_to_string(path).unwrap();
    let simulation = game_assembly::Simulation::load(&source, 1).unwrap();
    let input_bytes = source.len();
    drop(source);
    let saved = simulation.save().unwrap();
    eprintln!(
        "{}: {} → {} bytes, {:.2}s",
        path.display(),
        input_bytes,
        saved.len(),
        started.elapsed().as_secs_f64()
    );
    saved
}
fn main() {
    let paths: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(paths.len(), 2, "supply two save paths");
    assert!(
        canonical(Path::new(&paths[0])) == canonical(Path::new(&paths[1])),
        "worlds differ"
    );
    println!("Full validated worlds match.");
}

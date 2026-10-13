use game_assembly::benchmark::interval_staging_sample;

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    println!(
        "global interval wrapper benchmark; generic kernel fixture, 60,000 ms step; five samples; setup excluded"
    );
    println!("history is synthetic old TransferFailed receipt stress, not a played scenario");
    println!(
        "actors,workers,history,arriving,wrapper_median_ns,evaluation_median_ns,prepare_interval_median_ns,publish_interval_median_ns"
    );
    for actors in [1_000, 2_000, 4_000, 10_000] {
        for workers in [1, 4] {
            for history in [0, 100_000] {
                for arriving in [false, true] {
                    let samples: Vec<_> = (0..5)
                        .map(|_| {
                            interval_staging_sample(actors, workers, history, arriving).unwrap()
                        })
                        .collect();
                    println!(
                        "{actors},{workers},{history},{arriving},{},{},{},{}",
                        median(samples.iter().map(|s| s.wrapper_ns).collect()),
                        median(samples.iter().map(|s| s.evaluation_ns).collect()),
                        median(samples.iter().map(|s| s.prepare_interval_ns).collect()),
                        median(samples.iter().map(|s| s.publish_interval_ns).collect()),
                    );
                }
            }
        }
    }
}

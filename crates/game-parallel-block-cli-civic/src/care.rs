//! Text commands for treatment.
use game_parallel::CareStart;
use game_parallel_fused_physical::Kernel;
fn number<T: std::str::FromStr>(
    words: &mut std::str::SplitWhitespace<'_>,
    name: &str,
) -> Result<T, String> {
    words
        .next()
        .ok_or_else(|| format!("missing {name}"))?
        .parse()
        .map_err(|_| format!("invalid {name}"))
}
pub fn handle(engine: &mut Kernel, line: &str) -> Result<bool, String> {
    let mut words = line.split_whitespace();
    match words.next() {
        Some("care") => {
            if words.next().is_some() {
                return Err("unexpected argument".into());
            }
            let world = engine.export();
            let care = world.care.as_ref().ok_or("care is disabled")?;
            for job in &care.jobs {
                println!(
                    "care={} caregiver={} patient={} region={} remaining={}",
                    job.id, job.caregiver, job.patient, job.region, job.remaining
                );
            }
            for event in &care.events {
                println!(
                    "tick={} care={} caregiver={} patient={} healed={} interrupted={}",
                    event.tick,
                    event.id,
                    event.caregiver,
                    event.patient,
                    event.healed,
                    event.interrupted
                );
            }
        }
        Some("treat") => {
            let id = number(&mut words, "care id")?;
            let caregiver = number(&mut words, "caregiver")?;
            let patient = number(&mut words, "patient")?;
            if words.next().is_some() {
                return Err("unexpected argument".into());
            }
            engine.start_care(&[CareStart {
                id,
                caregiver,
                patient,
            }])?;
            println!("care={id} started");
        }
        _ => return Ok(false),
    }
    Ok(true)
}

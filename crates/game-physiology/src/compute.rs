use super::*;
use rayon::prelude::*;
/// Independent numeric work can gain another compute backend without gaining world ownership.
pub trait Batch {
    fn evaluate(
        &self,
        input: &[(PersonId, Body)],
        rules: &Rules,
        at: SimTime,
        pool: &rayon::ThreadPool,
    ) -> Vec<(PersonId, Body)>;
}
pub struct Cpu;
impl Batch for Cpu {
    fn evaluate(
        &self,
        input: &[(PersonId, Body)],
        rules: &Rules,
        at: SimTime,
        pool: &rayon::ThreadPool,
    ) -> Vec<(PersonId, Body)> {
        let update = |(id, p): &(PersonId, Body)| (*id, advance_body(p, rules, at));
        if input.len() < 512 {
            input.iter().map(update).collect()
        } else {
            pool.install(|| input.par_iter().map(update).collect())
        }
    }
}
pub fn advance_body(body: &Body, rules: &Rules, at: SimTime) -> Body {
    let mut p = body.clone();
    let ticks = (at.0.saturating_sub(p.last.0)) / rules.tick.0;
    for _ in 0..ticks {
        if p.alive {
            if p.kind == Kind::Mortal {
                p.hunger = p.hunger.saturating_add(rules.hunger_per_tick);
            }
            if p.resting {
                p.fatigue = p.fatigue.saturating_sub(rules.rest_per_tick);
            } else {
                p.fatigue = p.fatigue.saturating_add(rules.fatigue_per_tick);
            }
            if p.kind == Kind::Guardian {
                p.health = p
                    .health
                    .saturating_add(rules.guardian_recovery)
                    .min(rules.max_health);
            } else if p.hunger >= rules.deprivation {
                p.health = p.health.saturating_sub(rules.harm_per_tick);
            }
        }
        p.last.0 += rules.tick.0;
        if p.kind == Kind::Mortal && p.life_limit.is_some_and(|limit| limit <= p.last) {
            p.health = 0;
        }
    }
    p
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guardian_does_not_starve_and_numeric_batch_matches_workers() {
        let r = Rules::default();
        let body = Body {
            kind: Kind::Guardian,
            born: SimTime(0),
            alive: true,
            hunger: 0,
            fatigue: 0,
            health: 0,
            resting: false,
            last: SimTime(0),
            life_limit: None,
        };
        let input = vec![(PersonId(1), body); 1000];
        let one = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        let four = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();
        let a = Cpu.evaluate(&input, &r, SimTime(120_000), &one);
        assert_eq!(a, Cpu.evaluate(&input, &r, SimTime(120_000), &four));
        assert_eq!(a[0].1.hunger, 0);
        assert_eq!(a[0].1.health, 4);
    }
}

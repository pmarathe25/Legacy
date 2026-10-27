//! Owner-controlled personal mood updates with deterministic publication.
use super::*;
use rayon::prelude::*;

#[derive(Clone, Serialize, Deserialize)]
pub struct Experiences {
    pub effects: Vec<Experience>,
    pub minimum_parallel: usize,
}
impl Request for Experiences {
    const KEY: &'static str = "vitality.experiences";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Experiences>(|tx, batch| {
        require(batch.effects.len() <= 100_000, "too many experiences")?;
        require(
            !tx.has_before::<Experience>(),
            "experience hooks require ordered execution",
        )?;
        let unique: std::collections::BTreeSet<_> =
            batch.effects.iter().map(|r| r.person).collect();
        require(
            unique.len() == batch.effects.len(),
            "overlapping experiences",
        )?;
        let mut rows: Vec<_> = batch
            .effects
            .into_iter()
            .filter_map(|effect| {
                let life = tx
                    .get::<Vitality>()
                    .ok()?
                    .people
                    .get(&effect.person)
                    .copied()?;
                let resilience = ability(tx, effect.person, "resilience");
                Some((effect, life, resilience))
            })
            .collect();
        let now = tx.now();
        let apply = |(effect, life, resilience): &mut (Experience, Life, u64)| {
            life.grow(now);
            let (stress, satisfaction) = life.condition(now);
            let change = if effect.stress > 0 {
                scaled(effect.stress as u64, 1000, factor(*resilience)).min(1000) as i32
            } else {
                effect.stress
            };
            life.stress = (i64::from(stress) + i64::from(change)).clamp(0, 1000) as u32;
            life.satisfaction = satisfaction
                .saturating_add(effect.satisfaction)
                .clamp(-1000, 1000);
            life.updated = now;
        };
        if rows.len() >= batch.minimum_parallel && tx.compute().current_num_threads() > 1 {
            game_diagnostics::interaction_profile!(batch "mood", rows.len(), true);
            tx.compute().install(|| rows.par_iter_mut().for_each(apply));
        } else {
            game_diagnostics::interaction_profile!(batch "mood", rows.len(), false);
            rows.iter_mut().for_each(apply);
        }
        let state = tx.edit::<Vitality>()?;
        for (effect, life, _) in rows {
            state.people.insert(effect.person, life);
        }
        Ok(())
    })
}

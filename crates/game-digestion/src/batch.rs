//! Independent stomach rows prepared and updated by their owner.
use super::*;
use rayon::prelude::*;

#[derive(Clone, Serialize, Deserialize)]
pub struct IngestMany {
    pub effects: Vec<Ingest>,
    pub minimum_parallel: usize,
}
impl Request for IngestMany {
    const KEY: &'static str = "digestion.ingest-many";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<IngestMany>(|tx, batch| {
        require(batch.effects.len() <= 100_000, "too many ingestions")?;
        require(
            !tx.has_before::<Ingest>(),
            "ingestion hooks require ordered execution",
        )?;
        let unique: std::collections::BTreeSet<_> =
            batch.effects.iter().map(|r| r.person).collect();
        require(
            unique.len() == batch.effects.len(),
            "overlapping ingestions",
        )?;
        let now = tx.now();
        let apply = |effect: &Ingest| {
            (
                effect.person,
                Stomach {
                    amount: remaining(tx, effect.person).saturating_add(effect.amount),
                    at: now,
                },
            )
        };
        let rows: Vec<_> = if batch.effects.len() >= batch.minimum_parallel
            && tx.compute().current_num_threads() > 1
        {
            game_diagnostics::interaction_profile!(batch "digestion", batch.effects.len(), true);
            tx.compute()
                .install(|| batch.effects.par_iter().map(apply).collect())
        } else {
            game_diagnostics::interaction_profile!(batch "digestion", batch.effects.len(), false);
            batch.effects.iter().map(apply).collect()
        };
        let state = tx.edit::<Digestion>()?;
        for (person, stomach) in rows {
            state.0.insert(person, stomach);
        }
        Ok(())
    })
}

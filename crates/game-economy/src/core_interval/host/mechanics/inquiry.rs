//! A person's chosen reflection can compare the experiments they remember.
use super::*;
use game_mechanics::inquiry::{Comparison, Conclusion};

impl CoreHost {
    pub fn mechanical_comparisons_controlled(
        &self,
        person: u64,
    ) -> Result<Vec<Comparison>, String> {
        let actor = self.controlled_person(person)?;
        if actor.health == 0 {
            return Err("person cannot recall their mechanical comparisons".into());
        }
        let study = self
            .mechanics
            .as_ref()
            .and_then(|owner| indexed(&owner.memories, person, |row| row.person))
            .and_then(|memory| memory.inquiry.as_ref());
        Ok(study.map_or_else(Vec::new, |study| study.comparisons.clone()))
    }

    pub(in crate::core_interval::host) fn complete_mechanical_reflection(
        &mut self,
        events: &[game_pastimes::Event],
    ) -> Vec<Conclusion> {
        let Some(owner) = &mut self.mechanics else {
            return Vec::new();
        };
        if events.is_empty()
            || !owner
                .memories
                .par_iter()
                .any(|memory| memory.inquiry.is_some())
        {
            return Vec::new();
        }
        let mut completed: Vec<_> = events
            .par_iter()
            .filter(|row| row.outcome == game_pastimes::Outcome::Completed)
            .collect();
        completed.par_sort_unstable_by_key(|row| row.person);
        if completed.is_empty() {
            return Vec::new();
        }
        let mut conclusions: Vec<_> = owner
            .memories
            .par_iter_mut()
            .filter_map(|memory| {
                let study = memory.inquiry.as_mut()?;
                let choice = indexed(&completed, memory.person, |row| row.person)?;
                (study.activity == choice.activity)
                    .then(|| study.reflect(memory.person, &memory.observations, choice.at_millis))
            })
            .flatten()
            .collect();
        conclusions
            .par_sort_unstable_by_key(|row| (row.at_millis, row.person, row.evidence.conditions));
        conclusions
    }
}

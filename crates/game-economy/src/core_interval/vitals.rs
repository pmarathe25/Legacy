use super::{CoreInterval, Person};
use rayon::prelude::*;

/// Delayed fractional credit while a stepper owns the world exclusively.
/// Stored whole hunger and health stay current throughout this window.
#[derive(Clone, Copy)]
pub(super) struct DeferredVitals {
    millis: u64,
    whole_change_after: Option<u128>,
}

impl CoreInterval {
    pub(super) fn defer_quiet_vitals(&self, millis: u64) -> Result<Option<DeferredVitals>, String> {
        let opening = match self.deferred_vitals {
            Some(opening) => opening,
            None => DeferredVitals {
                millis: 0,
                whole_change_after: game_economy_person::next_whole_vital_change(
                    &self.people,
                    &self.rules,
                    self.physical.tick_millis(),
                )
                .map_err(str::to_owned)?,
            },
        };
        let millis = opening
            .millis
            .checked_add(millis)
            .ok_or("vitals time overflow")?;
        Ok(opening
            .whole_change_after
            .is_none_or(|after| u128::from(millis) < after)
            .then_some(DeferredVitals { millis, ..opening }))
    }

    pub(super) fn stage_vital_rows(&self, millis: u64) -> Result<Vec<Person>, String> {
        #[cfg(feature = "profile")]
        let _phase = game_diagnostics::span("elapsed.progress.vitals");
        #[cfg(feature = "profile")]
        {
            game_diagnostics::count("elapsed.rows.vitals", self.people.len() as u64);
            game_diagnostics::count(
                "elapsed.inline_bytes.vitals",
                std::mem::size_of_val(self.people.as_slice()) as u64,
            );
        }
        game_economy_person::stage_vitals(
            &self.people,
            &self.rules,
            millis,
            self.physical.tick_millis(),
        )
        .map_err(str::to_owned)
    }

    pub(super) fn projected_people(&self) -> Vec<Person> {
        match self.deferred_vitals {
            Some(vitals) => self
                .stage_vital_rows(vitals.millis)
                .expect("validated fractional vitals"),
            None => self.people.par_iter().cloned().collect(),
        }
    }

    pub(super) fn projected_person(&self, id: u64) -> Option<Person> {
        let index = self
            .people
            .binary_search_by_key(&id, |person| person.id)
            .ok()?;
        let person = &self.people[index];
        Some(match self.deferred_vitals {
            None => person.clone(),
            Some(vitals) => game_economy_person::project_vitals(
                person,
                &self.rules,
                vitals.millis,
                self.physical.tick_millis(),
            )
            .expect("validated fractional vitals"),
        })
    }

    pub(super) fn materialize_vitals(&mut self) {
        if let Some(vitals) = self.deferred_vitals.take() {
            self.people = self
                .stage_vital_rows(vitals.millis)
                .expect("validated fractional vitals");
        }
    }
}

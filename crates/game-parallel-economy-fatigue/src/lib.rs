//! Stage individual fatigue from actual work and personal rest choices.
use game_parallel_economy_person::Person;
use game_parallel_fatigue::Activity;
pub use game_parallel_fatigue::Draft;
pub use game_parallel_fatigue::{Fatigue, Rules};
use game_parallel_lot_runtime::{Events, Work};
use game_parallel_production_model::Start;
use rayon::prelude::*;

pub fn resting(owner: Option<&Fatigue>, people: &[Person]) -> Vec<u64> {
    let Some(owner) = owner else {
        return vec![];
    };
    let mut ids: Vec<_> = owner
        .people
        .par_iter()
        .filter(|p| {
            p.resting
                && people
                    .binary_search_by_key(&p.id, |p| p.id)
                    .ok()
                    .is_some_and(|i| !people[i].controlled)
        })
        .map(|p| p.id)
        .collect();
    ids.par_sort_unstable();
    ids
}
pub fn pause_rates(resting: &[u64], work: &[Work], rates: &mut Vec<(u64, u64)>) {
    if resting.is_empty() {
        return;
    }
    *rates = rates
        .par_iter()
        .filter(|(job, _)| {
            work.binary_search_by_key(job, |w| w.job.id)
                .ok()
                .is_none_or(|i| resting.binary_search(&work[i].job.worker).is_err())
        })
        .copied()
        .collect();
}
pub struct Context<'a> {
    pub owner: Option<&'a Fatigue>,
    pub people: &'a [Person],
    pub work: &'a [Work],
    pub rates: &'a [(u64, u64)],
    pub starts: &'a [Start],
    pub events: &'a Events,
    pub farming: &'a [u64],
    pub gathering: &'a [u64],
    pub medical: &'a [u64],
    pub hauling: &'a [u64],
    pub care: &'a [u64],
    pub fighters: &'a [u64],
    pub resting: &'a [u64],
    pub busy: &'a [u64],
    pub meals: &'a [(u64, u32)],
    pub manual: Option<u64>,
}
impl Context<'_> {
    pub fn prepare(&self) -> Result<Option<Draft>, String> {
        let Some(owner) = self.owner else {
            return if self.manual.is_some() {
                Err("fatigue is disabled".into())
            } else {
                Ok(None)
            };
        };
        if let Some(id) = self.manual {
            let person = self
                .people
                .binary_search_by_key(&id, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown person")?;
            if !person.controlled
                || person.health == 0
                || person.region.is_none()
                || self.fighters.binary_search(&id).is_ok()
            {
                return Err("person cannot rest here".into());
            }
        }
        let mut exertion: Vec<_> = self
            .rates
            .par_iter()
            .filter_map(|(job, _)| {
                self.work
                    .binary_search_by_key(job, |w| w.job.id)
                    .ok()
                    .map(|i| self.work[i].job.worker)
            })
            .chain(
                self.starts
                    .par_iter()
                    .filter(|s| self.events.started.binary_search(&s.id).is_ok())
                    .map(|s| s.worker),
            )
            .chain(self.farming.par_iter().copied())
            .chain(self.gathering.par_iter().copied())
            .chain(self.medical.par_iter().copied())
            .chain(self.hauling.par_iter().copied())
            .chain(self.care.par_iter().copied())
            .chain(self.fighters.par_iter().copied())
            .collect();
        exertion.par_sort_unstable();
        exertion.dedup();
        let activity: Vec<_> = self
            .people
            .par_iter()
            .map(|person| {
                let working = exertion.binary_search(&person.id).is_ok();
                let eating = self
                    .meals
                    .binary_search_by_key(&person.id, |row| row.0)
                    .is_ok();
                let available = person.controlled
                    || self.resting.binary_search(&person.id).is_ok()
                    || self.busy.binary_search(&person.id).is_err();
                Activity {
                    id: person.id,
                    alive: person.health > 0,
                    working,
                    traveling: person.region.is_none(),
                    can_rest: person.region.is_some() && !working && !eating && available,
                }
            })
            .collect();
        owner
            .prepare_with(&activity, |id| {
                self.people
                    .binary_search_by_key(&id, |p| p.id)
                    .ok()
                    .filter(|&i| self.people[i].controlled)
                    .map(|_| self.manual == Some(id))
            })
            .map(Some)
            .map_err(str::to_owned)
    }
}
pub fn apply(draft: Option<&Draft>, people: &mut [Person]) {
    let Some(draft) = draft else {
        return;
    };
    people.par_iter_mut().for_each(|person| {
        if let Ok(i) = draft
            .consequences
            .binary_search_by_key(&person.id, |c| c.id)
        {
            person.health = person.health.saturating_sub(draft.consequences[i].damage);
        }
    });
}
#[macro_export]
macro_rules! prepare_world {
    ($world:expr, ($rates:expr, $starts:expr, $physical:expr), ($farming:expr, $gathering:expr, $medical:expr, $hauling:expr, $care:expr, $fighters:expr), ($resting:expr, $busy:expr, $meals:expr, $choice:expr)) => {{
        let manual = match $choice {
            Some(game_parallel_economy_interaction::PhysicalChoice::Rest { person }) => {
                Some(person)
            }
            _ => None,
        };
        $crate::Context {
            owner: $world.fatigue.as_ref(),
            people: &$world.people,
            work: $world.physical.work(),
            rates: &$rates,
            starts: &$starts,
            events: $physical.events(),
            farming: $farming.as_ref().map_or(&[][..], |p| p.actors.as_slice()),
            gathering: $gathering.as_ref().map_or(&[][..], |p| p.actors.as_slice()),
            medical: &$medical,
            hauling: $hauling.as_ref().map_or(&[][..], |p| p.actors.as_slice()),
            care: $care.as_ref().map_or(&[][..], |p| p.actors.as_slice()),
            fighters: $fighters,
            resting: &$resting,
            busy: &$busy,
            meals: &$meals,
            manual,
        }
        .prepare()?
    }};
}
#[macro_export]
macro_rules! validate_world {
    ($state:expr, $ids:expr) => {
        if let Some(fatigue) = &$state.fatigue {
            fatigue.validate(&$ids)?;
        }
    };
}
#[macro_export]
macro_rules! rest_method {
    () => {
        /// Spend one world boundary resting by personal choice.
        pub fn rest(&mut self, person: u64) -> Result<(), String> {
            game_parallel_economy_campaign::preflight!(self);
            let draft = self.prepare_interaction(
                None,
                None,
                None,
                Some(game_parallel_economy_interaction::PhysicalChoice::Rest { person }),
            )?;
            self.commit(draft)?;
            Ok(())
        }
    };
}

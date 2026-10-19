use game_core::*;
use game_storage::{Agenda, Table};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub epoch: SimTime,
    pub cycle: SimTime,
    pub cursor: Option<PersonId>,
    pub agenda: Agenda<PersonId, SimTime>,
    pub ready: Table<PersonId, ()>,
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub waking: Table<PersonId, SimTime>,
}
impl Schedule {
    pub fn valid_at(&self, now: SimTime, period: SimDuration) -> bool {
        period.0 > 0
            && self.epoch <= now
            && self.cycle <= now
            && self.agenda.iter().all(|(_, at)| {
                *at >= now && *at > self.epoch && (at.0 - self.epoch.0).is_multiple_of(period.0)
            })
    }
    pub fn due<'a>(&'a self, now: &'a SimTime) -> impl Iterator<Item = PersonId> {
        let lower = if self.cycle == *now {
            self.cursor.as_ref()
        } else {
            None
        };
        game_storage::merge_unique(
            self.ready.scan_after(lower).map(|(id, _)| *id),
            self.agenda
                .due(now)
                .copied()
                .filter(move |id| lower.is_none_or(|last| id > last)),
        )
    }
    pub fn set(&mut self, person: PersonId, next: Next) {
        match next {
            Next::Active => {
                if !self.ready.contains_key(&person) {
                    self.agenda.set(person, None);
                    self.ready.insert(person, ());
                }
            }
            Next::Waiting(at) => {
                self.ready.remove(&person);
                self.agenda.set(person, at);
            }
        }
    }
    pub fn waiting(
        &self,
        at: Option<SimTime>,
        earliest: SimTime,
        period: SimDuration,
    ) -> Result<Next> {
        Ok(Next::Waiting(
            at.map(|at| self.grid(at.max(earliest), period))
                .transpose()?,
        ))
    }
    pub fn grid(&self, at: SimTime, period: SimDuration) -> Result<SimTime> {
        let steps = at.0.saturating_sub(self.epoch.0).div_ceil(period.0).max(1);
        self.epoch.after(SimDuration(
            steps.checked_mul(period.0).ok_or(Error::Overflow)?,
        ))
    }
}
pub enum Next {
    Active,
    Waiting(Option<SimTime>),
}

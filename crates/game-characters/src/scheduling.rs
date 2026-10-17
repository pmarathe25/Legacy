use super::*;
use game_storage::{Agenda, Table};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Schedule {
    pub epoch: SimTime,
    pub cycle: SimTime,
    pub cursor: Option<PersonId>,
    pub agenda: Agenda<PersonId, SimTime>,
    pub ready: Table<PersonId, ()>,
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
    fn grid(&self, at: SimTime, period: SimDuration) -> Result<SimTime> {
        let steps = at.0.saturating_sub(self.epoch.0).div_ceil(period.0).max(1);
        self.epoch.after(SimDuration(
            steps.checked_mul(period.0).ok_or(Error::Overflow)?,
        ))
    }
}
pub(super) enum Next {
    Active,
    Waiting(Option<SimTime>),
}

#[derive(Clone, Serialize, Deserialize)]
struct Resume {
    person: PersonId,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    hunger_only: bool,
}
impl Request for Resume {
    const KEY: &'static str = "characters.resume";
    type Response = ();
}
fn prompted(tx: &Transaction<'_>, state: &Minds, person: PersonId, hunger_only: bool) -> bool {
    state.people.get(&person).is_some_and(|m| {
        let body = &tx.get::<game_physiology::Bodies>().unwrap().people[&person];
        body.hunger >= state.rules.meal_hunger
            || (!hunger_only && (m.plan.is_some() || body.fatigue > state.rules.rest_fatigue))
    })
}
pub(super) fn food_action(mind: &Mind, kind: &str) -> bool {
    mind.plan.is_some() && matches!(kind, "gather" | "travel")
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_physiology::Fed>("characters.fed", |tx, e| wake(tx, e.patient, true))?;
    b.request::<Resume>(|tx, r| {
        let state = tx.get::<Minds>()?;
        if !state.running
            || !prompted(tx, state, r.person, r.hunger_only)
            || tx
                .get::<Actions>()?
                .0
                .get(&r.person)
                .is_some_and(|a| food_action(&state.people[&r.person], &a.kind))
        {
            return Ok(());
        }
        review(tx, r.person)?;
        if tx.get::<Minds>()?.people.contains_key(&r.person) {
            let next = next(tx, r.person)?;
            tx.edit::<Minds>()?.schedule.set(r.person, next);
        }
        Ok(())
    })
}
pub(super) fn wake(tx: &mut Transaction<'_>, person: PersonId, hunger_only: bool) -> Result<()> {
    let state = tx.get::<Minds>()?;
    if state.running && state.people.contains_key(&person) {
        let resume = prompted(tx, state, person, hunger_only);
        if !state.schedule.ready.contains_key(&person) {
            tx.edit::<Minds>()?.schedule.set(person, Next::Active);
        }
        if resume {
            let request = Resume {
                person,
                hunger_only,
            };
            tx.schedule(tx.now(), 50, request)?;
        }
    }
    Ok(())
}
pub(super) fn next(tx: &Transaction<'_>, person: PersonId) -> Result<Next> {
    let state = tx.get::<Minds>()?;
    let next = tx.now().after(state.rules.review)?;
    let Some(action) = tx.get::<Actions>()?.0.get(&person) else {
        return Ok(Next::Active);
    };
    let body = &tx.get::<game_physiology::Bodies>()?.people[&person];
    let interruptible = state.rules.hunger_interrupts.contains(&action.kind)
        && !food_action(&state.people[&person], &action.kind);
    if interruptible && body.hunger >= state.rules.urgent_hunger {
        return Ok(Next::Active);
    }
    let mut at = action.until;
    if interruptible {
        let threshold = game_physiology::hunger_threshold(
            body,
            &tx.get::<game_physiology::Bodies>()?.rules,
            state.rules.urgent_hunger,
        );
        at = match (at, threshold) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }
    Ok(Next::Waiting(
        at.map(|at| state.schedule.grid(at.max(next), state.rules.review))
            .transpose()?,
    ))
}

//! Resolve leisure choices through physical custody and the shared elapsed clock.
use super::*;
use game_pastimes::{Activity, Event, Outcome, Session};

#[derive(Default)]
pub(super) struct SettledPastimes {
    pub activities: Vec<Event>,
    pub conclusions: Vec<MechanicalConclusion>,
    pub hearings: Vec<HeardPerformance>,
}

impl CoreHost {
    pub fn pastime_practice_controlled(&self, person: u64) -> Result<&[PastimePractice], String> {
        self.controlled_person(person)?;
        Ok(self
            .pastimes
            .as_ref()
            .map_or(&[], |owner| owner.training.for_person(person)))
    }

    pub fn heard_performances_controlled(
        &self,
        person: u64,
    ) -> Result<Vec<HeardPerformance>, String> {
        self.controlled_person(person)?;
        Ok(self
            .pastimes
            .as_ref()
            .and_then(|owner| owner.culture.as_ref())
            .map_or_else(Vec::new, |culture| culture.remembered(person).collect()))
    }

    pub fn listen_for_controlled(
        &mut self,
        person: u64,
        activity: u32,
        value: i64,
    ) -> Result<(), String> {
        self.controlled_person(person)?;
        let owner = self
            .pastimes
            .as_mut()
            .ok_or("no leisure activities are available")?;
        owner.activity(activity).ok_or("unknown activity")?;
        let culture = owner.culture.as_mut().ok_or("performances are disabled")?;
        culture
            .set_interest(person, activity, value)
            .then_some(())
            .ok_or_else(|| "this person already has sixteen listening interests".into())
    }

    pub fn present_performance_controlled(
        &mut self,
        person: u64,
        activity: u32,
        enabled: bool,
    ) -> Result<(), String> {
        self.controlled_person(person)?;
        let owner = self
            .pastimes
            .as_mut()
            .ok_or("no leisure activities are available")?;
        owner.activity(activity).ok_or("unknown activity")?;
        let culture = owner.culture.as_mut().ok_or("performances are disabled")?;
        culture.set_presentation(game_pastimes::Presentation { person, activity }, enabled);
        Ok(())
    }

    fn performance_venue(&self, person: u64) -> Option<game_pastimes::Venue> {
        let actor = self.core.person(person)?;
        let region = actor.region.filter(|_| actor.health > 0)?;
        let endpoint = self.local_partition(person);
        (!matches!(endpoint.0, 2 | 4)).then_some((region, endpoint))
    }

    pub(super) fn next_pastime_finish(&self) -> Option<u64> {
        self.pastimes
            .as_ref()?
            .next_finish(self.core.simulated_millis())
    }

    pub(super) fn pastime_people(&self) -> impl Iterator<Item = u64> + '_ {
        self.pastimes
            .iter()
            .flat_map(|owner| owner.active.iter().map(|row| row.person))
    }

    pub(super) fn pastime_ready(&self, person: u64, activity: &Activity, opening: bool) -> bool {
        if activity.tools.is_empty() && (!opening || activity.materials.is_empty()) {
            return true;
        }
        self.core
            .physical
            .custody(game_lots::Place::Person(person))
            .is_some_and(|cargo| {
                let quantity = |good| {
                    cargo
                        .lots
                        .iter()
                        .filter(|lot| lot.good == good)
                        .map(|lot| lot.quantity)
                        .sum::<u64>()
                };
                activity
                    .tools
                    .iter()
                    .all(|row| quantity(row.good) >= row.quantity)
                    && (!opening
                        || activity
                            .materials
                            .iter()
                            .all(|row| quantity(row.good) >= row.quantity))
            })
    }

    pub(super) fn start_pastimes(&mut self, blocked: &[u64]) -> Result<Vec<Event>, String> {
        let Some(owner) = &self.pastimes else {
            return Ok(Vec::new());
        };
        if owner.minds.is_empty() {
            return Ok(Vec::new());
        }
        let busy = self.choice_busy(blocked);
        let now = self.core.simulated_millis();
        let selected = owner.choose(
            now,
            |id| {
                let person = self.core.person(id)?;
                (person.health > 0 && !person.controlled && busy.binary_search(&id).is_err())
                    .then_some(person.region)
                    .flatten()
            },
            |id, activity| self.pastime_ready(id, activity, true),
        );
        self.publish_pastime_choices(selected)
    }

    pub fn start_pastime_controlled(
        &mut self,
        person: u64,
        activity: u32,
    ) -> Result<Option<Event>, String> {
        let actor = self.controlled_person(person)?;
        let region = actor.region.ok_or("the person is travelling")?;
        if self.choice_busy(&[]).binary_search(&person).is_ok()
            || self.assisting_people().binary_search(&person).is_ok()
        {
            return Err("the controlled person is occupied".into());
        }
        if self.core.next_deadline(None)?.after_millis == 0 {
            return Err("settle the current event before starting an activity".into());
        }
        let owner = self
            .pastimes
            .as_ref()
            .ok_or("no leisure activities are available")?;
        let activity = owner.activity(activity).ok_or("unknown activity")?;
        let Some(selected) = owner
            .contemplate(person, region, activity.id, self.core.simulated_millis())
            .filter(|_| self.pastime_ready(person, activity, true))
        else {
            return Ok(None);
        };
        Ok(self
            .publish_pastime_choices(vec![selected])?
            .into_iter()
            .next())
    }

    pub fn stop_pastime_controlled(&mut self, person: u64) -> Result<Option<Event>, String> {
        self.controlled_person(person)?;
        let now = self.core.simulated_millis();
        Ok(self
            .pastimes
            .as_mut()
            .and_then(|owner| owner.stop(person, now)))
    }

    pub fn available_pastimes(&self, person: u64) -> Result<Vec<(u32, String)>, String> {
        self.controlled_person(person)?;
        Ok(self
            .pastimes
            .iter()
            .flat_map(|owner| owner.activities.iter())
            .filter(|activity| self.pastime_ready(person, activity, true))
            .map(|activity| (activity.id, activity.name.clone()))
            .collect())
    }

    fn publish_pastime_choices(&mut self, selected: Vec<Session>) -> Result<Vec<Event>, String> {
        if selected.is_empty() {
            return Ok(Vec::new());
        }
        let owner = self.pastimes.as_ref().expect("prepared pastime choices");
        let stride = owner
            .activities
            .par_iter()
            .map(|row| row.materials.len())
            .max()
            .unwrap_or(0);
        let stride = u64::try_from(stride).map_err(|_| "too many activity materials")?;
        let actors = u64::try_from(selected.len()).map_err(|_| "too many activity choices")?;
        actors
            .checked_mul(stride)
            .ok_or("activity movement identity overflow")?;
        let actions: Vec<_> = selected
            .par_iter()
            .enumerate()
            .filter_map(|(slot, session)| {
                let activity = owner.activity(session.activity).expect("known pastime");
                (!activity.materials.is_empty()).then(|| game_lots::Transaction {
                    id: slot as u64 + 1,
                    movements: activity
                        .materials
                        .iter()
                        .enumerate()
                        .map(|(index, row)| game_lots::Movement {
                            id: slot as u64 * stride + index as u64 + 1,
                            good: row.good,
                            quantity: row.quantity,
                            from: game_lots::Place::Person(session.person),
                            to: None,
                            item: None,
                            batch: None,
                        })
                        .collect(),
                    creations: Vec::new(),
                })
            })
            .collect();
        let mut accepted = selected;
        if !actions.is_empty() {
            let draft = self.core.physical.prepare_interval_consumption(
                &actions,
                self.core.seed ^ self.core.decision_time_key(),
            )?;
            accepted = accepted
                .into_par_iter()
                .enumerate()
                .filter_map(|(slot, session)| {
                    let id = slot as u64 + 1;
                    draft
                        .transactions()
                        .binary_search_by_key(&id, |row| row.id)
                        .ok()
                        .is_none_or(|index| draft.transactions()[index].accepted)
                        .then_some(session)
                })
                .collect();
            self.core.physical.commit_interval_consumption(draft)?;
        }
        Ok(self
            .pastimes
            .as_mut()
            .expect("prepared pastime owner")
            .start(accepted))
    }

    pub(super) fn settle_pastimes(&mut self, blocked: &[u64]) -> SettledPastimes {
        if self.pastime_people().next().is_none() {
            return SettledPastimes::default();
        }
        let Some(mut owner) = self.pastimes.take() else {
            return SettledPastimes::default();
        };
        let events = owner.settle(self.core.simulated_millis(), |session, activity| {
            self.core.person(session.person).is_some_and(|person| {
                person.health > 0
                    && person.region == Some(session.region)
                    && self.pastime_ready(person.id, activity, false)
            })
        });
        let hearings = owner
            .culture
            .as_ref()
            .filter(|culture| !culture.presentations.is_empty() && !culture.listeners.is_empty())
            .filter(|_| {
                events
                    .iter()
                    .any(|event| event.outcome == Outcome::Completed)
            })
            .map_or_else(Vec::new, |culture| {
                let busy = self.choice_busy(blocked);
                let assisting = self.assisting_people();
                let now = self.core.simulated_millis();
                culture.hear(
                    &events,
                    |person| self.performance_venue(person),
                    |person| {
                        busy.binary_search(&person).is_err()
                            && assisting.binary_search(&person).is_err()
                            && owner.session(person).is_none()
                    },
                    |person| {
                        owner.preferred_value(person, now, |activity| {
                            self.pastime_ready(person, activity, true)
                        })
                    },
                )
            });
        if let Some(culture) = &mut owner.culture {
            culture.remember(&hearings);
        }
        self.pastimes = Some(owner);
        self.remember_pastimes(&events);
        if let Some(purpose) = &mut self.purpose {
            let tick = self.core.physical.tick();
            let experiences: Vec<_> = events
                .par_iter()
                .filter(|event| event.outcome == Outcome::Completed)
                .map(|event| game_purpose::Experience {
                    person: event.person,
                    domain: personal_choice::PASTIME,
                    outcome: game_purpose::ActionOutcome::Completed,
                    tick,
                })
                .collect();
            if !experiences.is_empty() {
                purpose.owner.appraise(&experiences);
            }
        }
        let conclusions = self.complete_mechanical_reflection(&events);
        SettledPastimes {
            activities: events,
            conclusions,
            hearings,
        }
    }
}

//! Guardian judgment and intent over ordinary authoritative people and journeys.
use game_parallel_economy_person::{
    Person,
    travel::{Departure, Travel},
};
use game_parallel_guardians::ReleaseDecision;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guardians {
    pub guardians: Vec<Guardian>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guardian {
    pub person: u64,
    pub mentorship_required: u64,
    #[serde(default)]
    pub student: Option<u64>,
    #[serde(default)]
    pub mentorship: u64,
    #[serde(default)]
    pub released: bool,
}

impl Guardians {
    pub fn validate(&self, people: &[Person], current: Option<u64>) -> Result<(), String> {
        let mut ids = HashSet::new();
        if self.guardians.iter().any(|guardian| {
            !ids.insert(guardian.person)
                || guardian.mentorship_required == 0
                || Some(guardian.person) == current
                || people
                    .binary_search_by_key(&guardian.person, |p| p.id)
                    .ok()
                    .is_none_or(|i| people[i].immortal.is_none() || people[i].controlled)
                || guardian
                    .student
                    .is_some_and(|id| people.binary_search_by_key(&id, |p| p.id).is_err())
        }) {
            return Err("invalid embodied guardian".into());
        }
        Ok(())
    }

    /// Read committed bodies once and return ordinary departures, never mutate bodies.
    pub fn prepare(
        &mut self,
        current: Option<u64>,
        people: &[Person],
        travel: Option<&Travel>,
        busy: &[u64],
    ) -> (Vec<Departure>, Vec<u64>) {
        let target = current.and_then(|id| {
            people
                .binary_search_by_key(&id, |p| p.id)
                .ok()
                .and_then(|i| people[i].region)
        });
        let routing = target
            .filter(|target| {
                self.guardians.iter().any(|guardian| {
                    people
                        .binary_search_by_key(&guardian.person, |p| p.id)
                        .ok()
                        .is_some_and(|i| {
                            let person = &people[i];
                            person.health > 0
                                && !person.controlled
                                && busy.binary_search(&person.id).is_err()
                                && person.region.is_some_and(|region| region != *target)
                        })
                })
            })
            .and_then(|region| {
                travel.map(|travel| game_parallel_economy_person::travel::routes(travel, &[region]))
            });
        let choices: Vec<_> = self
            .guardians
            .par_iter_mut()
            .filter_map(|guardian| {
                if guardian.student != current {
                    guardian.student = current;
                    guardian.mentorship = 0;
                    guardian.released = false;
                }
                let person = &people[people
                    .binary_search_by_key(&guardian.person, |p| p.id)
                    .ok()?];
                if person.health == 0 || person.controlled || busy.binary_search(&person.id).is_ok()
                {
                    return None;
                }
                let (region, target) = person.region.zip(target)?;
                if region == target {
                    if guardian.released {
                        return None;
                    }
                    guardian.mentorship = guardian.mentorship.saturating_add(1);
                    Some((person.id, None))
                } else {
                    let destination = routing.as_ref()?.get(&(region, target))?.1;
                    Some((
                        person.id,
                        Some(Departure {
                            person: person.id,
                            destination,
                        }),
                    ))
                }
            })
            .collect();
        let departures = choices.iter().filter_map(|choice| choice.1).collect();
        let actors = choices.into_iter().map(|choice| choice.0).collect();
        (departures, actors)
    }

    pub fn request_release(
        &mut self,
        id: u64,
        current: Option<u64>,
        people: &[Person],
    ) -> Result<ReleaseDecision, String> {
        let student = current.ok_or("there is no current ruler")?;
        let guardian = self
            .guardians
            .iter_mut()
            .find(|g| g.person == id)
            .ok_or("unknown guardian")?;
        let person = &people[people
            .binary_search_by_key(&id, |p| p.id)
            .map_err(|_| "unknown guardian body")?];
        let pupil = &people[people
            .binary_search_by_key(&student, |p| p.id)
            .map_err(|_| "unknown ruler")?];
        if guardian.student != Some(student)
            || person.health == 0
            || pupil.health == 0
            || person.region.is_none()
            || person.region != pupil.region
        {
            return Ok(ReleaseDecision::NotPresent);
        }
        if guardian.mentorship < guardian.mentorship_required {
            return Ok(ReleaseDecision::NotReady);
        }
        guardian.released = true;
        Ok(ReleaseDecision::Accepted)
    }
}

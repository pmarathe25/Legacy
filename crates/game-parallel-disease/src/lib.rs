//! Data-defined infection, immunity and exposure through actual encounters.
use game_parallel_social::Encounter;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Disease {
    pub strains: Vec<Strain>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub treatments: Vec<Treatment>,
    #[serde(default)]
    pub cases: Vec<Case>,
    #[serde(default)]
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strain {
    pub id: u32,
    pub name: String,
    pub incubation: u64,
    pub infectious_ticks: u64,
    pub immunity_ticks: u64,
    pub damage_per_tick: u32,
    pub transmission_per_million: u32,
}
/// A completed course of this medicine clears the specified infection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Treatment {
    pub strain: u32,
    pub medicine: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub person: u64,
    pub strain: u32,
    pub infected_at: u64,
    pub source: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Infected,
    Recovered,
    Treated,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub strain: u32,
    pub source: Option<u64>,
    pub kind: EventKind,
}
#[derive(Clone, Copy, Debug)]
pub struct Person {
    pub id: u64,
    pub health: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Damage {
    pub person: u64,
    pub amount: u32,
}
#[derive(Clone, Debug)]
pub struct Draft {
    pub state: Disease,
    pub damage: Vec<Damage>,
}
impl Strain {
    fn end(&self) -> Option<u64> {
        self.incubation
            .checked_add(self.infectious_ticks)?
            .checked_add(self.immunity_ticks)
    }
    pub fn contagious(&self, age: u64) -> bool {
        age >= self.incubation && age < self.incubation + self.infectious_ticks
    }
}
impl Disease {
    pub fn validate(&self, people: &HashSet<u64>, now: u64) -> Result<(), String> {
        let mut strains = HashSet::new();
        if self.strains.iter().any(|strain| {
            !strains.insert(strain.id)
                || strain.name.is_empty()
                || strain.infectious_ticks == 0
                || strain.end().is_none()
                || strain.transmission_per_million > 1_000_000
        }) {
            return Err("invalid disease strain".into());
        }
        let mut treatments = HashSet::new();
        if self.treatments.iter().any(|treatment| {
            !strains.contains(&treatment.strain)
                || !treatments.insert((treatment.strain, treatment.medicine))
        }) {
            return Err("invalid disease treatment".into());
        }
        let mut cases = HashSet::new();
        if self.cases.iter().any(|case| {
            !people.contains(&case.person)
                || !strains.contains(&case.strain)
                || !cases.insert(case.person)
                || case.infected_at > now
                || case
                    .source
                    .is_some_and(|source| source == case.person || !people.contains(&source))
        }) {
            return Err("invalid disease case".into());
        }
        if self.events.iter().any(|event| {
            event.tick > now
                || !people.contains(&event.person)
                || !strains.contains(&event.strain)
                || event
                    .source
                    .is_some_and(|source| source == event.person || !people.contains(&source))
        }) {
            return Err("invalid disease event".into());
        }
        Ok(())
    }
    /// Snapshot transmission through disjoint encounter pairs. No new infection
    /// can itself transmit within this generation.
    pub fn prepare(
        &self,
        people: &[Person],
        encounters: &[Encounter],
        seed: u64,
        now: u64,
    ) -> Result<Draft, &'static str> {
        let identities: HashSet<_> = people.par_iter().map(|person| person.id).collect();
        self.validate(&identities, now)
            .map_err(|_| "invalid disease state")?;
        self.prepare_validated(people, encounters, seed, now)
    }
    /// The host validated the persistent owner on import and maintained its invariants.
    pub fn prepare_validated(
        &self,
        people: &[Person],
        encounters: &[Encounter],
        seed: u64,
        now: u64,
    ) -> Result<Draft, &'static str> {
        let next = now.checked_add(1).ok_or("tick overflow")?;
        let mut people = people.to_vec();
        if people.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            people.par_sort_unstable_by_key(|person| person.id);
        }
        if people.par_windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err("duplicate disease person");
        }
        let mut cases: Vec<_> = self.cases.par_iter().collect();
        if cases
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            cases.par_sort_unstable_by_key(|case| case.person);
        }
        let find_case = |id| {
            cases
                .binary_search_by_key(&id, |case| case.person)
                .ok()
                .map(|i| cases[i])
        };
        let find_person = |id| {
            people
                .binary_search_by_key(&id, |person| person.id)
                .ok()
                .map(|i| people[i])
        };
        let find_strain = |id| self.strains.iter().find(|strain| strain.id == id);
        if cases
            .par_windows(2)
            .any(|pair| pair[0].person == pair[1].person)
            || cases.par_iter().any(|case| {
                case.infected_at > now
                    || find_person(case.person).is_none()
                    || find_strain(case.strain).is_none()
            })
        {
            return Err("invalid disease state");
        }
        let mut actors: Vec<_> = encounters
            .par_iter()
            .flat_map_iter(|event| [event.a, event.b])
            .collect();
        actors.par_sort_unstable();
        if actors.par_windows(2).any(|pair| pair[0] == pair[1])
            || encounters.par_iter().any(|event| {
                event.tick != next
                    || event.a >= event.b
                    || find_person(event.a).is_none()
                    || find_person(event.b).is_none()
            })
        {
            return Err("invalid disease encounter");
        }
        let active = |id: u64| -> Option<(&Case, &Strain)> {
            let case = find_case(id)?;
            let strain = find_strain(case.strain)?;
            (now - case.infected_at < strain.end()?).then_some((case, strain))
        };
        let infection: Vec<_> = encounters
            .par_iter()
            .filter_map(|encounter| {
                let try_infection = |source: u64, target: u64| {
                    let (case, strain) = active(source)?;
                    if !strain.contagious(now - case.infected_at)
                        || active(target).is_some()
                        || find_person(source)?.health == 0
                        || find_person(target)?.health == 0
                        || game_parallel_genetics::random(seed ^ source, target, next) % 1_000_000
                            >= u64::from(strain.transmission_per_million)
                    {
                        return None;
                    }
                    Some(Case {
                        person: target,
                        strain: strain.id,
                        infected_at: next,
                        source: Some(source),
                    })
                };
                try_infection(encounter.a, encounter.b)
                    .or_else(|| try_infection(encounter.b, encounter.a))
            })
            .collect();
        let damage = cases
            .par_iter()
            .filter_map(|case| {
                let strain = find_strain(case.strain)?;
                (find_person(case.person)?.health > 0
                    && strain.contagious(now - case.infected_at)
                    && strain.damage_per_tick > 0)
                    .then_some(Damage {
                        person: case.person,
                        amount: strain.damage_per_tick,
                    })
            })
            .collect();
        let mut events: Vec<_> = cases
            .par_iter()
            .filter_map(|case| {
                let strain = find_strain(case.strain)?;
                (now - case.infected_at == strain.incubation + strain.infectious_ticks
                    && find_person(case.person)?.health > 0)
                    .then_some(Event {
                        tick: next,
                        person: case.person,
                        strain: case.strain,
                        source: case.source,
                        kind: EventKind::Recovered,
                    })
            })
            .chain(infection.par_iter().map(|case| Event {
                tick: next,
                person: case.person,
                strain: case.strain,
                source: case.source,
                kind: EventKind::Infected,
            }))
            .collect();
        events.par_sort_unstable_by_key(|event| {
            (
                event.person,
                event.strain,
                match event.kind {
                    EventKind::Recovered => 0,
                    EventKind::Infected => 1,
                    EventKind::Treated => 2,
                },
                event.source,
            )
        });
        let mut retained: Vec<_> = cases
            .par_iter()
            .filter(|case| {
                let strain = find_strain(case.strain).unwrap();
                now - case.infected_at < strain.end().unwrap()
                    && find_person(case.person).is_some_and(|person| person.health > 0)
            })
            .map(|case| (*case).clone())
            .collect();
        // An expired case may be replaced by an infection from this snapshot.
        retained.par_extend(infection);
        retained.par_sort_unstable_by_key(|case| case.person);
        Ok(Draft {
            state: Self {
                strains: self.strains.clone(),
                treatments: self.treatments.clone(),
                cases: retained,
                events,
            },
            damage,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> Disease {
        Disease {
            strains: vec![Strain {
                id: 1,
                name: "fever".into(),
                incubation: 1,
                infectious_ticks: 2,
                immunity_ticks: 2,
                damage_per_tick: 3,
                transmission_per_million: 1_000_000,
            }],
            treatments: vec![],
            cases: vec![Case {
                person: 1,
                strain: 1,
                infected_at: 0,
                source: None,
            }],
            events: vec![],
        }
    }
    fn pair(tick: u64, a: u64, b: u64) -> Encounter {
        Encounter {
            tick,
            region: 1,
            a,
            b,
            pressured: None,
        }
    }
    #[test]
    fn infection_uses_prior_generation_and_respects_incubation_and_immunity() {
        let people: Vec<_> = (1..=3).map(|id| Person { id, health: 100 }).collect();
        let mut state = state();
        let first = state.prepare(&people, &[pair(1, 1, 2)], 7, 0).unwrap();
        assert!(first.damage.is_empty());
        assert_eq!(first.state.cases.len(), 1);
        state = first.state;
        let second = state.prepare(&people, &[pair(2, 1, 2)], 7, 1).unwrap();
        assert_eq!(
            second.damage,
            vec![Damage {
                person: 1,
                amount: 3
            }]
        );
        assert_eq!(
            second
                .state
                .cases
                .iter()
                .find(|case| case.person == 2)
                .unwrap()
                .infected_at,
            2
        );
        state = second.state;
        state = state
            .prepare(&people, &[pair(3, 2, 3)], 7, 2)
            .unwrap()
            .state;
        assert!(!state.cases.iter().any(|case| case.person == 3));
        state = state
            .prepare(&people, &[pair(4, 2, 3)], 7, 3)
            .unwrap()
            .state;
        assert!(state.cases.iter().any(|case| case.person == 3));
        let restored: Disease =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(restored, state);
    }
    #[test]
    fn invalid_encounter_and_tick_overflow_do_not_mutate_state() {
        let state = state();
        let before = state.clone();
        let people = [Person { id: 1, health: 100 }, Person { id: 2, health: 100 }];
        assert!(
            state
                .prepare(&people, &[pair(1, 1, 2), pair(1, 1, 2)], 1, 0)
                .is_err()
        );
        assert!(state.prepare(&people, &[], 1, u64::MAX).is_err());
        assert_eq!(state, before);
    }
    #[test]
    fn independent_exposures_match_across_workers() {
        let count = 6000u64;
        let people: Vec<_> = (1..=count).map(|id| Person { id, health: 100 }).collect();
        let mut state = state();
        state.strains[0].incubation = 0;
        state.cases = (1..=count / 2)
            .map(|id| Case {
                person: id,
                strain: 1,
                infected_at: 0,
                source: None,
            })
            .collect();
        let contacts: Vec<_> = (1..=count / 2)
            .map(|id| pair(1, id, id + count / 2))
            .collect();
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| state.prepare(&people, &contacts, 5, 0).unwrap())
        };
        let a = run(1);
        let b = run(4);
        assert_eq!(a.state, b.state);
        assert_eq!(a.damage, b.damage);
        assert_eq!(a.state.cases.len(), count as usize);
    }
}

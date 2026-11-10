//! Data-defined infection, immunity and exposure through actual encounters.
use game_social::Encounter;
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
    /// Probability for an actual combat strike; zero disables this route.
    #[serde(default)]
    pub combat_transmission_per_million: u32,
    /// Probability for a newborn to acquire this infection from its mother.
    #[serde(default)]
    pub birth_transmission_per_million: u32,
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
                || strain.combat_transmission_per_million > 1_000_000
                || strain.birth_transmission_per_million > 1_000_000
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
                || !cases.insert((case.person, case.strain))
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
    /// Admit infections from actual staged births using the prior generation.
    /// `births` contains unique (child, mother) IDs already validated by the host.
    /// Newborns cannot transmit or take disease damage in this boundary.
    pub fn admit_births_validated(
        &mut self,
        previous: &Disease,
        births: &[(u64, u64)],
        seed: u64,
        tick: u64,
    ) {
        if births.is_empty()
            || previous.cases.is_empty()
            || tick == 0
            || !previous
                .strains
                .iter()
                .any(|s| s.birth_transmission_per_million > 0)
        {
            return;
        }
        let now = tick - 1;
        let mut cases: Vec<_> = previous.cases.par_iter().collect();
        if cases
            .par_windows(2)
            .any(|pair| (pair[0].person, pair[0].strain) > (pair[1].person, pair[1].strain))
        {
            cases.par_sort_unstable_by_key(|case| (case.person, case.strain));
        }
        let infections: Vec<_> = births
            .par_iter()
            .flat_map_iter(|&(child, mother)| {
                let begin = cases.partition_point(|case| case.person < mother);
                let end = cases.partition_point(|case| case.person <= mother);
                cases[begin..end]
                    .iter()
                    .filter_map(move |case| {
                        let strain = previous
                            .strains
                            .iter()
                            .find(|strain| strain.id == case.strain)?;
                        let age = now.checked_sub(case.infected_at)?;
                        if !strain.contagious(age)
                            || game_genetics::random(
                                seed ^ 0x6269727468 ^ mother ^ u64::from(strain.id),
                                child,
                                tick,
                            ) % 1_000_000
                                >= u64::from(strain.birth_transmission_per_million)
                        {
                            return None;
                        }
                        Some(Case {
                            person: child,
                            strain: strain.id,
                            infected_at: tick,
                            source: Some(mother),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        if infections.is_empty() {
            return;
        }
        self.events
            .par_extend(infections.par_iter().map(|case| Event {
                tick,
                person: case.person,
                strain: case.strain,
                source: case.source,
                kind: EventKind::Infected,
            }));
        self.cases.par_extend(infections);
        if self
            .cases
            .par_windows(2)
            .any(|pair| (pair[0].person, pair[0].strain) > (pair[1].person, pair[1].strain))
        {
            self.cases
                .par_sort_unstable_by_key(|case| (case.person, case.strain));
        }
        self.events.par_sort_unstable_by_key(|event| {
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
    }
    /// Stage infections from actual physical contacts without advancing disease
    /// damage or allowing those new cases to transmit in this generation.
    pub fn admit_contacts_validated(
        &mut self,
        previous: &Disease,
        people: &[Person],
        contacts: &[Encounter],
        seed: u64,
        now: u64,
    ) -> Result<(), &'static str> {
        if contacts.is_empty()
            || previous.cases.is_empty()
            || !previous
                .strains
                .iter()
                .any(|strain| strain.transmission_per_million > 0)
        {
            return Ok(());
        }
        let tick = now.checked_add(1).ok_or("tick overflow")?;
        let preview = previous.prepare_validated(people, contacts, seed, now)?;
        let mut existing: Vec<_> = self
            .cases
            .par_iter()
            .map(|case| ((case.person, case.strain), case.infected_at))
            .collect();
        existing.par_sort_unstable();
        let cases: Vec<_> = preview
            .state
            .cases
            .into_par_iter()
            .filter(|case| {
                case.infected_at == tick
                    && existing
                        .binary_search_by_key(&(case.person, case.strain), |row| row.0)
                        .ok()
                        .is_none_or(|index| existing[index].1 < tick)
            })
            .collect();
        if cases.is_empty() {
            return Ok(());
        }
        let mut keys: Vec<_> = cases
            .par_iter()
            .map(|case| (case.person, case.strain))
            .collect();
        keys.par_sort_unstable();
        self.cases = std::mem::take(&mut self.cases)
            .into_par_iter()
            .filter(|case| keys.binary_search(&(case.person, case.strain)).is_err())
            .collect();
        self.events
            .par_extend(preview.state.events.into_par_iter().filter(|event| {
                keys.binary_search(&(event.person, event.strain)).is_ok()
                    && matches!(event.kind, EventKind::Infected | EventKind::Recovered)
            }));
        self.cases.par_extend(cases);
        self.cases
            .par_sort_unstable_by_key(|case| (case.person, case.strain));
        self.events.par_sort_unstable_by_key(|event| {
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
        Ok(())
    }
    /// Evaluate all distinct contacts against the prior generation; newly infected
    /// people cannot transmit until a later generation.
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
        self.prepare_validated_with_combat(people, encounters, &[], seed, now)
    }
    /// Combat pairs are actual strikes supplied by the combat owner, independently configurable per strain.
    pub fn prepare_validated_with_combat(
        &self,
        people: &[Person],
        encounters: &[Encounter],
        combat: &[(u64, u64)],
        seed: u64,
        now: u64,
    ) -> Result<Draft, &'static str> {
        let next = now.checked_add(1).ok_or("tick overflow")?;
        let mut people: Vec<_> = people.par_iter().copied().collect();
        if people.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            people.par_sort_unstable_by_key(|person| person.id);
        }
        if people.par_windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err("duplicate disease person");
        }
        let mut cases: Vec<_> = self.cases.par_iter().collect();
        if cases
            .par_windows(2)
            .any(|pair| (pair[0].person, pair[0].strain) > (pair[1].person, pair[1].strain))
        {
            cases.par_sort_unstable_by_key(|case| (case.person, case.strain));
        }
        let person_cases = |id| {
            let start = cases.partition_point(|case| case.person < id);
            let end = cases.partition_point(|case| case.person <= id);
            &cases[start..end]
        };
        let dense = people.first().zip(people.last()).and_then(|(first, last)| {
            (last.id.checked_sub(first.id) == Some((people.len() - 1) as u64)).then_some(first.id)
        });
        let find_person = |id: u64| {
            if let Some(first) = dense {
                id.checked_sub(first)
                    .and_then(|offset| usize::try_from(offset).ok())
                    .and_then(|index| people.get(index))
                    .copied()
            } else {
                people
                    .binary_search_by_key(&id, |person| person.id)
                    .ok()
                    .map(|i| people[i])
            }
        };
        let find_strain = |id| self.strains.iter().find(|strain| strain.id == id);
        if cases
            .par_windows(2)
            .any(|pair| (pair[0].person, pair[0].strain) == (pair[1].person, pair[1].strain))
            || cases.par_iter().any(|case| {
                case.infected_at > next
                    || find_person(case.person).is_none()
                    || find_strain(case.strain).is_none()
            })
        {
            return Err("invalid disease state");
        }
        let mut pairs: Vec<_> = encounters
            .par_iter()
            .map(|event| (event.a, event.b))
            .collect();
        if pairs.par_windows(2).any(|pair| pair[0] > pair[1]) {
            pairs.par_sort_unstable();
        }
        if pairs.par_windows(2).any(|pair| pair[0] == pair[1])
            || encounters.par_iter().any(|event| {
                event.tick != next
                    || event.a >= event.b
                    || find_person(event.a).is_none()
                    || find_person(event.b).is_none()
            })
        {
            return Err("invalid disease encounter");
        }
        let mut combat_pairs: Vec<_> = combat.par_iter().copied().collect();
        if combat_pairs.par_windows(2).any(|pair| pair[0] > pair[1]) {
            combat_pairs.par_sort_unstable();
        }
        if combat_pairs.par_windows(2).any(|pair| pair[0] == pair[1])
            || combat_pairs
                .par_iter()
                .any(|&(a, b)| a >= b || find_person(a).is_none() || find_person(b).is_none())
        {
            return Err("invalid combat exposure");
        }
        let active = |id: u64, strain: u32| {
            person_cases(id)
                .binary_search_by_key(&strain, |case| case.strain)
                .ok()
                .and_then(|index| {
                    let case = person_cases(id)[index];
                    (case.infected_at > now
                        || now - case.infected_at < find_strain(strain)?.end()?)
                    .then_some(case)
                })
        };
        let ordinary_can_transmit = !cases.is_empty()
            && self
                .strains
                .iter()
                .any(|strain| strain.transmission_per_million > 0);
        let combat_can_transmit = !cases.is_empty()
            && self
                .strains
                .iter()
                .any(|strain| strain.combat_transmission_per_million > 0);
        let transmissible_encounters = if ordinary_can_transmit {
            encounters
        } else {
            &[]
        };
        let transmissible_combat = if combat_can_transmit {
            combat_pairs.as_slice()
        } else {
            &[]
        };
        let mut candidates: Vec<_> = transmissible_encounters
            .par_iter()
            .map(|encounter| (encounter.a, encounter.b, false))
            .chain(transmissible_combat.par_iter().map(|&(a, b)| (a, b, true)))
            .flat_map_iter(|(a, b, combat)| {
                [(a, b), (b, a)]
                    .into_iter()
                    .flat_map(move |(source, target)| {
                        person_cases(source).iter().filter_map(move |case| {
                            let strain = find_strain(case.strain)?;
                            let probability = if combat {
                                strain.combat_transmission_per_million
                            } else {
                                strain.transmission_per_million
                            };
                            let route_seed = if combat { seed ^ 0x636f6d626174 } else { seed };
                            if case.infected_at > now
                                || !strain.contagious(now - case.infected_at)
                                || active(target, strain.id).is_some()
                                || find_person(source)?.health == 0
                                || find_person(target)?.health == 0
                                || game_genetics::random(
                                    route_seed ^ source ^ u64::from(strain.id),
                                    target,
                                    next,
                                ) % 1_000_000
                                    >= u64::from(probability)
                            {
                                return None;
                            }
                            let case = Case {
                                person: target,
                                strain: strain.id,
                                infected_at: next,
                                source: Some(source),
                            };
                            let priority = game_genetics::random(
                                seed ^ case.person,
                                source,
                                next ^ u64::from(case.strain),
                            );
                            Some((case.person, case.strain, priority, source, case))
                        })
                    })
            })
            .collect();
        let key = |row: &(u64, u32, u64, u64, Case)| (row.0, row.1, row.2, row.3);
        if candidates
            .par_windows(2)
            .any(|pair| key(&pair[0]) > key(&pair[1]))
        {
            candidates.par_sort_unstable_by_key(key);
        }
        let infection: Vec<_> = candidates
            .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
            .map(|group| group[0].4.clone())
            .collect();
        let damage = cases
            .par_chunk_by(|a, b| a.person == b.person)
            .filter_map(|group| {
                let person = group[0].person;
                if find_person(person)?.health == 0 {
                    return None;
                }
                let amount = group.iter().fold(0u32, |total, case| {
                    let strain = find_strain(case.strain).expect("validated strain");
                    if case.infected_at <= now && strain.contagious(now - case.infected_at) {
                        total.saturating_add(strain.damage_per_tick)
                    } else {
                        total
                    }
                });
                (amount > 0).then_some(Damage { person, amount })
            })
            .collect();
        let mut events: Vec<_> = cases
            .par_iter()
            .filter_map(|case| {
                let strain = find_strain(case.strain)?;
                (case.infected_at <= now
                    && now - case.infected_at == strain.incubation + strain.infectious_ticks
                    && find_person(case.person)?.health > 0)
                    .then_some(Event {
                        tick: next,
                        person: case.person,
                        strain: case.strain,
                        source: case.source,
                        kind: EventKind::Recovered,
                    })
            })
            .chain(
                cases
                    .par_iter()
                    .filter(|case| case.infected_at == next)
                    .map(|case| Event {
                        tick: next,
                        person: case.person,
                        strain: case.strain,
                        source: case.source,
                        kind: EventKind::Infected,
                    }),
            )
            .chain(
                self.events
                    .par_iter()
                    .filter(|event| event.tick == next && event.kind == EventKind::Recovered)
                    .cloned(),
            )
            .chain(infection.par_iter().map(|case| Event {
                tick: next,
                person: case.person,
                strain: case.strain,
                source: case.source,
                kind: EventKind::Infected,
            }))
            .collect();
        let event_key = |event: &Event| {
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
        };
        if events
            .par_windows(2)
            .any(|pair| event_key(&pair[0]) > event_key(&pair[1]))
        {
            events.par_sort_unstable_by_key(event_key);
        }
        events.dedup_by(|a, b| event_key(a) == event_key(b));
        let mut retained: Vec<_> = cases
            .par_iter()
            .filter(|case| {
                let strain = find_strain(case.strain).unwrap();
                (case.infected_at > now || now - case.infected_at < strain.end().unwrap())
                    && find_person(case.person).is_some_and(|person| person.health > 0)
            })
            .map(|case| (*case).clone())
            .collect();
        // An expired case may be replaced by an infection from this snapshot.
        retained.par_extend(infection);
        if retained
            .par_windows(2)
            .any(|pair| (pair[0].person, pair[0].strain) > (pair[1].person, pair[1].strain))
        {
            retained.par_sort_unstable_by_key(|case| (case.person, case.strain));
        }
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
                combat_transmission_per_million: 0,
                birth_transmission_per_million: 0,
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
            also_pressured: None,
        }
    }
    #[test]
    fn staged_birth_infections_do_not_transmit_or_take_damage_in_their_birth_generation() {
        let mut previous = state();
        previous.strains[0].incubation = 0;
        previous.strains[0].birth_transmission_per_million = 1_000_000;
        let mut staged = previous.clone();
        staged.admit_births_validated(&previous, &[(2, 1)], 7, 1);
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    staged
                        .prepare_validated(
                            &[
                                Person { id: 1, health: 100 },
                                Person { id: 2, health: 100 },
                                Person { id: 3, health: 100 },
                            ],
                            &[pair(1, 1, 2), pair(1, 2, 3)],
                            7,
                            0,
                        )
                        .unwrap()
                })
        };
        let one = run(1);
        let many = run(6);
        assert_eq!(one.state, many.state);
        assert_eq!(one.damage, many.damage);
        assert_eq!(
            one.damage,
            vec![Damage {
                person: 1,
                amount: 3
            }]
        );
        assert_eq!(one.state.cases.len(), 2);
        assert_eq!(one.state.cases[1].source, Some(1));
        assert_eq!(
            one.state
                .events
                .iter()
                .filter(|event| event.person == 2)
                .count(),
            1
        );
        assert!(one.state.events.iter().all(|event| event.person != 3));
    }
    #[test]
    fn staged_physical_contact_infections_are_unique_and_wait_until_next_generation() {
        let mut previous = state();
        previous.strains[0].incubation = 0;
        let people = [
            Person { id: 1, health: 100 },
            Person { id: 2, health: 100 },
            Person { id: 3, health: 100 },
        ];
        let mut staged = previous.clone();
        staged
            .admit_contacts_validated(&previous, &people, &[pair(1, 1, 2)], 7, 0)
            .unwrap();
        staged
            .admit_contacts_validated(&previous, &people, &[pair(1, 1, 2)], 7, 0)
            .unwrap();
        assert_eq!(staged.cases.len(), 2);
        assert_eq!(staged.events.len(), 1);
        let next = staged
            .prepare_validated(&people, &[pair(1, 2, 3)], 7, 0)
            .unwrap();
        assert_eq!(next.state.cases.len(), 2);
        assert_eq!(next.state.events.len(), 1);
        assert_eq!(
            next.damage,
            vec![Damage {
                person: 1,
                amount: 3
            }]
        );
    }
    #[test]
    fn staged_contact_reinfection_keeps_the_recovery_receipt() {
        let mut previous = state();
        previous.strains[0].incubation = 0;
        previous.strains[0].infectious_ticks = 1;
        previous.strains[0].immunity_ticks = 0;
        previous.cases = vec![
            Case {
                person: 1,
                strain: 1,
                infected_at: 1,
                source: None,
            },
            Case {
                person: 2,
                strain: 1,
                infected_at: 0,
                source: None,
            },
        ];
        let people = [Person { id: 1, health: 100 }, Person { id: 2, health: 100 }];
        let mut staged = previous.clone();
        staged
            .admit_contacts_validated(&previous, &people, &[pair(2, 1, 2)], 7, 1)
            .unwrap();
        assert_eq!(staged.cases[1].infected_at, 2);
        let next = staged.prepare_validated(&people, &[], 7, 1).unwrap();
        assert_eq!(
            next.state
                .events
                .iter()
                .filter(|event| event.person == 2)
                .map(|event| event.kind)
                .collect::<Vec<_>>(),
            vec![EventKind::Recovered, EventKind::Infected]
        );
    }
    #[test]
    fn birth_exposure_uses_only_contagious_prior_cases() {
        let mut previous = state();
        previous.strains[0].birth_transmission_per_million = 1_000_000;
        let run = |workers, tick| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut next = previous.clone();
                    next.admit_births_validated(&previous, &[(2, 1), (3, 2), (4, 9)], 7, tick);
                    next
                })
        };
        let exposed = run(1, 2);
        assert_eq!(exposed, run(4, 2));
        assert_eq!(
            exposed
                .cases
                .iter()
                .map(|c| (c.person, c.source))
                .collect::<Vec<_>>(),
            [(1, None), (2, Some(1))]
        );
        assert_eq!(exposed.cases[1].infected_at, 2);
        assert_eq!(exposed.events[0].kind, EventKind::Infected);
        assert_eq!(run(1, 1).cases.len(), 1); // Mother is still incubating.
        let mut disabled = previous.clone();
        disabled.strains[0].birth_transmission_per_million = 0;
        let mut next = disabled.clone();
        next.admit_births_validated(&disabled, &[(2, 1)], 7, 2);
        assert_eq!(next, disabled);
        let restored: Disease =
            serde_json::from_str(&serde_json::to_string(&exposed).unwrap()).unwrap();
        assert_eq!(restored, exposed);
        let mut old_save = serde_json::to_value(&disabled).unwrap();
        old_save["strains"][0]
            .as_object_mut()
            .unwrap()
            .remove("birth_transmission_per_million");
        assert_eq!(
            serde_json::from_value::<Disease>(old_save).unwrap(),
            disabled
        );
        let mut shuffled = previous.clone();
        shuffled.cases.insert(
            0,
            Case {
                person: 9,
                strain: 1,
                infected_at: 0,
                source: None,
            },
        );
        let mut sorted = shuffled.clone();
        sorted.cases.sort_unstable_by_key(|case| case.person);
        let mut from_shuffled = shuffled.clone();
        let mut from_sorted = sorted.clone();
        from_shuffled.admit_births_validated(&shuffled, &[(2, 1), (10, 9)], 7, 2);
        from_sorted.admit_births_validated(&sorted, &[(2, 1), (10, 9)], 7, 2);
        assert_eq!(from_shuffled, from_sorted);
        let mut mixed = previous.clone();
        mixed.strains[0].birth_transmission_per_million = 500_000;
        let births: Vec<_> = (2..10_002).map(|child| (child, 1)).collect();
        let run_many = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut next = mixed.clone();
                    next.admit_births_validated(&mixed, &births, 7, 2);
                    next
                })
        };
        let many = run_many(1);
        assert_eq!(many, run_many(4));
        assert!((4_000..6_000).contains(&many.events.len()));
        let mut invalid = previous;
        invalid.strains[0].birth_transmission_per_million = 1_000_001;
        assert!(invalid.validate(&HashSet::from([1]), 1).is_err());
    }

    #[test]
    fn disabled_transmission_still_validates_contacts_and_progresses_illness() {
        let people = [Person { id: 1, health: 100 }, Person { id: 2, health: 100 }];
        let mut disease = state();
        disease.strains[0].incubation = 0;
        disease.strains[0].transmission_per_million = 0;
        let draft = disease
            .prepare_validated_with_combat(&people, &[pair(1, 1, 2)], &[(1, 2)], 7, 0)
            .unwrap();
        assert_eq!(draft.state.cases.len(), 1);
        assert_eq!(
            draft.damage,
            [Damage {
                person: 1,
                amount: 3
            }]
        );
        assert!(
            disease
                .prepare_validated_with_combat(&people, &[pair(1, 1, 2), pair(1, 1, 2)], &[], 7, 0)
                .is_err()
        );
        assert!(
            disease
                .prepare_validated_with_combat(&people, &[], &[(1, 2), (1, 2)], 7, 0)
                .is_err()
        );
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
    #[test]
    fn one_infectious_person_can_expose_many_without_same_tick_cascades() {
        let people: Vec<_> = (1..=4).map(|id| Person { id, health: 100 }).collect();
        let mut state = state();
        state.strains[0].incubation = 0;
        let contacts = [pair(1, 1, 2), pair(1, 1, 3), pair(1, 2, 4)];
        let next = state.prepare(&people, &contacts, 7, 0).unwrap().state;
        assert!(
            next.cases
                .iter()
                .any(|case| case.person == 2 && case.source == Some(1))
        );
        assert!(
            next.cases
                .iter()
                .any(|case| case.person == 3 && case.source == Some(1))
        );
        assert!(!next.cases.iter().any(|case| case.person == 4));
    }
    #[test]
    fn combat_transmission_is_a_separate_per_strain_choice() {
        let people: Vec<_> = (1..=2).map(|id| Person { id, health: 100 }).collect();
        let mut state = state();
        state.strains[0].incubation = 0;
        assert_eq!(
            state
                .prepare_validated_with_combat(&people, &[], &[(1, 2)], 7, 0)
                .unwrap()
                .state
                .cases
                .len(),
            1
        );
        state.strains[0].transmission_per_million = 0;
        state.strains[0].combat_transmission_per_million = 1_000_000;
        assert_eq!(
            state
                .prepare_validated(&people, &[pair(1, 1, 2)], 7, 0)
                .unwrap()
                .state
                .cases
                .len(),
            1
        );
        let draft = state
            .prepare_validated_with_combat(&people, &[], &[(1, 2)], 7, 0)
            .unwrap();
        assert!(
            draft
                .state
                .cases
                .iter()
                .any(|case| case.person == 2 && case.source == Some(1))
        );
        assert!(
            state
                .prepare_validated_with_combat(&people, &[], &[(1, 2), (1, 2)], 7, 0)
                .is_err()
        );
    }
    #[test]
    fn older_saved_strains_default_to_no_combat_transmission() {
        let mut saved = serde_json::to_value(state()).unwrap();
        saved["strains"][0]
            .as_object_mut()
            .unwrap()
            .remove("combat_transmission_per_million");
        let loaded: Disease = serde_json::from_value(saved).unwrap();
        assert_eq!(loaded.strains[0].combat_transmission_per_million, 0);
    }
    #[test]
    fn simultaneous_different_strains_can_infect_the_same_person() {
        let people: Vec<_> = (1..=3).map(|id| Person { id, health: 100 }).collect();
        let mut state = state();
        state.strains[0].incubation = 0;
        let mut other = state.strains[0].clone();
        other.id = 2;
        state.strains.push(other);
        state.cases.push(Case {
            person: 2,
            strain: 2,
            infected_at: 0,
            source: None,
        });
        let run = |workers, contacts: Vec<_>| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| state.prepare(&people, &contacts, 42, 0).unwrap().state)
        };
        let first = run(1, vec![pair(1, 1, 3), pair(1, 2, 3)]);
        let second = run(4, vec![pair(1, 2, 3), pair(1, 1, 3)]);
        assert_eq!(first, second);
        assert_eq!(
            first.cases.iter().filter(|case| case.person == 3).count(),
            2
        );
        assert!(
            first
                .cases
                .iter()
                .any(|case| case.person == 3 && case.strain == 1 && case.source == Some(1))
        );
        assert!(
            first
                .cases
                .iter()
                .any(|case| case.person == 3 && case.strain == 2 && case.source == Some(2))
        );
    }

    #[test]
    fn reciprocal_strains_spread_independently_and_damage_adds() {
        let mut disease = state();
        disease.strains[0].incubation = 0;
        disease.strains[0].damage_per_tick = u32::MAX - 2;
        let mut second = disease.strains[0].clone();
        second.id = 2;
        second.damage_per_tick = 9;
        disease.strains.push(second);
        disease.cases.push(Case {
            person: 2,
            strain: 2,
            infected_at: 0,
            source: None,
        });
        let people = [Person { id: 1, health: 100 }, Person { id: 2, health: 100 }];
        let first = disease.prepare(&people, &[pair(1, 1, 2)], 7, 0).unwrap();
        assert_eq!(
            first
                .state
                .cases
                .iter()
                .map(|case| (case.person, case.strain))
                .collect::<Vec<_>>(),
            [(1, 1), (1, 2), (2, 1), (2, 2)]
        );
        assert_eq!(
            first.damage,
            [
                Damage {
                    person: 1,
                    amount: u32::MAX - 2
                },
                Damage {
                    person: 2,
                    amount: 9
                }
            ]
        );
        let next = first.state.prepare(&people, &[], 7, 1).unwrap();
        assert_eq!(
            next.damage,
            [
                Damage {
                    person: 1,
                    amount: u32::MAX
                },
                Damage {
                    person: 2,
                    amount: u32::MAX
                }
            ]
        );
    }

    #[test]
    fn births_can_acquire_multiple_contagious_maternal_strains() {
        let mut previous = state();
        previous.strains[0].incubation = 0;
        previous.strains[0].birth_transmission_per_million = 1_000_000;
        let mut other = previous.strains[0].clone();
        other.id = 2;
        previous.strains.push(other);
        previous.cases.push(Case {
            person: 1,
            strain: 2,
            infected_at: 0,
            source: None,
        });
        let mut next = previous.clone();
        next.admit_births_validated(&previous, &[(2, 1)], 7, 1);
        assert_eq!(
            next.cases
                .iter()
                .filter(|case| case.person == 2)
                .map(|case| case.strain)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(
            next.cases
                .iter()
                .filter(|case| case.person == 2)
                .all(|case| case.infected_at == 1 && case.source == Some(1))
        );
    }

    #[test]
    fn competing_sources_for_one_strain_are_deterministic() {
        let mut disease = state();
        disease.strains[0].incubation = 0;
        disease.cases.push(Case {
            person: 2,
            strain: 1,
            infected_at: 0,
            source: None,
        });
        let people: Vec<_> = (1..=3).map(|id| Person { id, health: 100 }).collect();
        let run = |workers, events: Vec<_>| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| disease.prepare(&people, &events, 42, 0).unwrap().state)
        };
        let a = run(1, vec![pair(1, 1, 3), pair(1, 2, 3)]);
        let b = run(4, vec![pair(1, 2, 3), pair(1, 1, 3)]);
        assert_eq!(a, b);
        assert_eq!(
            a.cases
                .iter()
                .filter(|case| case.person == 3 && case.strain == 1)
                .count(),
            1
        );
    }

    #[test]
    fn immunity_blocks_only_the_matching_strain() {
        let mut disease = state();
        disease.strains[0].incubation = 0;
        disease.strains[0].infectious_ticks = 1;
        disease.strains[0].immunity_ticks = 2;
        let mut second = disease.strains[0].clone();
        second.id = 2;
        disease.strains.push(second);
        disease.cases = vec![
            Case {
                person: 1,
                strain: 1,
                infected_at: 2,
                source: None,
            },
            Case {
                person: 1,
                strain: 2,
                infected_at: 2,
                source: None,
            },
            Case {
                person: 2,
                strain: 1,
                infected_at: 0,
                source: None,
            },
        ];
        let people = [Person { id: 1, health: 100 }, Person { id: 2, health: 100 }];
        let next = disease
            .prepare(&people, &[pair(3, 1, 2)], 7, 2)
            .unwrap()
            .state;
        assert_eq!(
            next.cases
                .iter()
                .filter(|case| case.person == 2 && case.strain == 1)
                .count(),
            1
        );
        assert!(
            next.cases
                .iter()
                .any(|case| case.person == 2 && case.strain == 2 && case.infected_at == 3)
        );
    }

    #[test]
    #[ignore = "performance measurement"]
    fn coinfection_exposure_benchmark() {
        let count = std::env::var("LEGACY_PEOPLE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100_000u64);
        let workers = std::env::var("LEGACY_WORKERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let concentrated = std::env::var_os("LEGACY_CONCENTRATED").is_some();
        let people: Vec<_> = (1..=count).map(|id| Person { id, health: 100 }).collect();
        let sources = if concentrated { count - 1 } else { count / 2 };
        let contacts: Vec<_> = (1..=sources)
            .map(|id| pair(1, id, if concentrated { count } else { id + sources }))
            .collect();
        let mut disease = state();
        disease.strains[0].incubation = 0;
        let mut other = disease.strains[0].clone();
        other.id = 2;
        disease.strains.push(other);
        disease.cases = (1..=sources)
            .flat_map(|person| {
                [1, 2].map(|strain| Case {
                    person,
                    strain,
                    infected_at: 0,
                    source: None,
                })
            })
            .collect();
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut samples = Vec::new();
                for round in 0..4 {
                    let start = std::time::Instant::now();
                    let draft = disease.prepare_validated(&people, &contacts, 7, 0).unwrap();
                    let elapsed = start.elapsed().as_secs_f64();
                    let infected = draft
                        .state
                        .events
                        .iter()
                        .filter(|event| event.kind == EventKind::Infected)
                        .count();
                    assert_eq!(
                        infected,
                        if concentrated {
                            2
                        } else {
                            sources as usize * 2
                        }
                    );
                    if round > 0 {
                        samples.push(elapsed);
                    }
                }
                samples.sort_by(f64::total_cmp);
                println!(
                    "people={count} workers={workers} concentrated={concentrated} seconds={:.6}",
                    samples[1]
                );
            });
    }
    #[test]
    #[ignore = "performance measurement"]
    fn concentrated_infectious_contacts_benchmark() {
        let workers = std::env::var("LEGACY_WORKERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let count = std::env::var("LEGACY_PEOPLE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100_000u64);
        let people: Vec<_> = (1..=count).map(|id| Person { id, health: 100 }).collect();
        let contacts: Vec<_> = (2..=count).map(|id| pair(1, 1, id)).collect();
        let mut state = state();
        state.strains[0].incubation = 0;
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut samples = Vec::new();
                for round in 0..4 {
                    let start = std::time::Instant::now();
                    let draft = state.prepare_validated(&people, &contacts, 7, 0).unwrap();
                    let elapsed = start.elapsed().as_secs_f64();
                    assert_eq!(draft.state.cases.len(), count as usize);
                    if round > 0 {
                        samples.push(elapsed);
                    }
                }
                samples.sort_by(f64::total_cmp);
                println!("workers={workers} people={count} seconds={:.6}", samples[1]);
            });
    }
}

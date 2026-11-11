//! A person's own choice to consume real medicine, which may be ineffective.
use game_disease::{Disease, Event, EventKind};
use game_economy_person::Person;
use game_lot_runtime::Events;
use game_lots::{Movement, Place, Transaction, View};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub medicine: u32,
    pub quantity: u64,
    pub willing: bool,
    /// Act when observed health is at most this value; absent uses below maximum health.
    #[serde(default)]
    pub health_at_most: Option<u32>,
}

/// A possible personal medicine use before the person compares activities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opportunity {
    pub person: u64,
    pub medicine: u32,
    pub quantity: u64,
    pub health: u32,
}

/// Respect explicit personal thresholds without imposing one on people who
/// have not specified it. The physical owner decides actual consumption later.
pub fn opportunities_now(
    policies: &[Policy],
    people: &[Person],
    physical: &game_lot_runtime::Runtime,
    busy: &[u64],
) -> Result<Vec<Opportunity>, String> {
    if people.par_windows(2).any(|rows| rows[0].id >= rows[1].id)
        || busy.par_windows(2).any(|rows| rows[0] >= rows[1])
    {
        return Err("treatment people and exclusions must be ordered and unique".into());
    }
    Ok(policies
        .par_iter()
        .filter_map(|policy| {
            let person = people
                .binary_search_by_key(&policy.person, |person| person.id)
                .ok()
                .map(|index| &people[index])?;
            if !policy.willing
                || person.controlled
                || person.health == 0
                || busy.binary_search(&person.id).is_ok()
                || policy
                    .health_at_most
                    .is_some_and(|threshold| person.health > threshold)
            {
                return None;
            }
            let holding = physical.custody(Place::Person(person.id))?;
            let available: u128 = holding
                .lots
                .iter()
                .filter(|lot| {
                    lot.good == policy.medicine
                        && lot.expires_at.is_none_or(|at| at > physical.tick())
                })
                .map(|lot| u128::from(lot.quantity))
                .sum();
            (policy.quantity > 0 && available >= u128::from(policy.quantity)).then_some(
                Opportunity {
                    person: person.id,
                    medicine: policy.medicine,
                    quantity: policy.quantity,
                    health: person.health,
                },
            )
        })
        .collect())
}
/// Names are resolved through the authored goods catalog when creating a world.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remedy {
    pub strain: u32,
    pub medicine: String,
    pub quantity: u64,
    pub people: Vec<u64>,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub health_at_most: Option<u32>,
}
pub fn configure(
    disease: Option<&mut Disease>,
    policies: &mut Vec<Policy>,
    remedies: &[Remedy],
    lookup: impl Fn(&str) -> Option<u32>,
) -> Result<(), String> {
    if remedies.is_empty() {
        return Ok(());
    }
    let disease = disease.ok_or("remedies require disease")?;
    let mut known: HashSet<_> = disease
        .treatments
        .iter()
        .map(|t| (t.strain, t.medicine))
        .collect();
    let mut recipients: HashSet<_> = policies.iter().map(|p| p.person).collect();
    let strains: HashSet<_> = disease.strains.iter().map(|s| s.id).collect();
    let mut additions = Vec::new();
    let mut people = Vec::new();
    for remedy in remedies {
        let medicine = lookup(&remedy.medicine)
            .ok_or_else(|| format!("unknown medicine {}", remedy.medicine))?;
        if remedy.quantity == 0
            || !strains.contains(&remedy.strain)
            || !known.insert((remedy.strain, medicine))
            || remedy
                .people
                .iter()
                .any(|&person| person == 0 || !recipients.insert(person))
        {
            return Err("invalid remedy definition".into());
        }
        additions.push(game_disease::Treatment {
            strain: remedy.strain,
            medicine,
        });
        people.extend(remedy.people.iter().map(|&person| Policy {
            person,
            medicine,
            quantity: remedy.quantity,
            willing: remedy.willing,
            health_at_most: remedy.health_at_most,
        }));
    }
    disease.treatments.extend(additions);
    policies.extend(people);
    Ok(())
}
#[macro_export]
macro_rules! configure_remedies {
    ($scenario:expr, $goods:expr) => {{
        let mut dynamics = $scenario.dynamics;
        if !$scenario.remedies.is_empty() {
            let owner = dynamics.as_mut().ok_or("remedies require dynamics")?;
            $crate::configure(
                owner.disease.as_mut(),
                &mut owner.treatment,
                &$scenario.remedies,
                |name| $goods.get(name).copied(),
            )?;
        }
        dynamics
    }};
}
pub fn validate(
    policies: &[Policy],
    disease: Option<&Disease>,
    people: &HashSet<u64>,
    goods: &HashSet<u32>,
) -> Result<(), String> {
    let mut ids = HashSet::new();
    if policies.iter().any(|p| {
        !people.contains(&p.person)
            || !goods.contains(&p.medicine)
            || p.quantity == 0
            || !ids.insert(p.person)
            || disease.is_none_or(|d| !d.treatments.iter().any(|t| t.medicine == p.medicine))
    }) {
        return Err("invalid personal treatment preference".into());
    }
    Ok(())
}
pub struct Plan {
    pub actions: Vec<Transaction>,
    pub actors: Vec<u64>,
    targets: Vec<(u64, u32, u32, u64)>,
}

/// Capture the exact opening episodes a selected medicine can affect.
pub fn targets(disease: &Disease, choices: &[(u64, u32)], now: u64) -> Vec<(u64, u32, u32, u64)> {
    if choices.is_empty() {
        return Vec::new();
    }
    let mut choices: Vec<_> = choices.par_iter().copied().collect();
    choices.par_sort_unstable();
    let choices = &choices;
    disease
        .cases
        .par_iter()
        .flat_map_iter(|case| {
            let active = disease
                .strains
                .iter()
                .find(|strain| strain.id == case.strain)
                .and_then(|strain| {
                    now.checked_sub(case.infected_at).map(|age| {
                        age < strain.incubation + strain.infectious_ticks + strain.immunity_ticks
                    })
                })
                .unwrap_or(false);
            disease
                .treatments
                .iter()
                .filter(move |remedy| {
                    active
                        && remedy.strain == case.strain
                        && choices
                            .binary_search(&(case.person, remedy.medicine))
                            .is_ok()
                })
                .map(move |remedy| (case.person, remedy.medicine, case.strain, case.infected_at))
                .collect::<Vec<_>>()
        })
        .collect()
}
#[derive(Clone, Copy)]
pub struct Choice {
    pub manual: Option<u64>,
    pub max_health: u32,
}
pub fn prepare(
    disease: Option<&Disease>,
    policies: &[Policy],
    people: &[Person],
    busy: &[u64],
    view: &View,
    base: u64,
    choice: Choice,
) -> Result<Plan, String> {
    prepare_at(
        disease,
        policies,
        people,
        busy,
        view,
        base,
        (choice, view.boundary.saturating_sub(1)),
    )
}

/// Prepare against the actual current time when acting within a quantum.
pub fn prepare_at(
    disease: Option<&Disease>,
    policies: &[Policy],
    people: &[Person],
    busy: &[u64],
    view: &View,
    base: u64,
    timing: (Choice, u64),
) -> Result<Plan, String> {
    let (choice, now) = timing;
    let manual = choice.manual;
    let Some(disease) = disease else {
        if manual.is_some() {
            return Err("treatment is disabled".into());
        }
        return Ok(Plan {
            actions: vec![],
            actors: vec![],
            targets: vec![],
        });
    };
    if let Some(id) = manual {
        let person = people
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .map(|i| &people[i])
            .ok_or("unknown person")?;
        policies
            .iter()
            .find(|p| p.person == id)
            .ok_or("person has no treatment preference")?;
        if !person.controlled || person.health == 0 {
            return Err("person cannot use this treatment".into());
        }
    }
    let chosen: Vec<_> = policies
        .par_iter()
        .filter_map(|policy| {
            let person = people
                .binary_search_by_key(&policy.person, |p| p.id)
                .ok()
                .map(|i| &people[i])?;
            let explicit = manual == Some(person.id);
            if (!policy.willing && !explicit)
                || (person.controlled && !explicit)
                || person.health == 0
                || (!explicit && busy.binary_search(&person.id).is_ok())
                || (!explicit
                    && policy
                        .health_at_most
                        .map_or(person.health >= choice.max_health, |threshold| {
                            person.health > threshold
                        }))
                || (!explicit
                    && view
                        .container(Place::Person(person.id))
                        .is_none_or(|c| c.quantity(policy.medicine) < policy.quantity))
            {
                return None;
            }
            Some(policy)
        })
        .collect();
    base.checked_add(chosen.len() as u64)
        .ok_or("action count overflow")?;
    let actions = chosen
        .par_iter()
        .enumerate()
        .map(|(i, &p)| {
            let id = base + i as u64;
            Transaction {
                id,
                movements: vec![Movement {
                    item: None,
                    id,
                    from: Place::Person(p.person),
                    to: None,
                    good: p.medicine,
                    quantity: p.quantity,
                }],
                creations: vec![],
            }
        })
        .collect();
    let selected: Vec<_> = chosen.par_iter().map(|p| (p.person, p.medicine)).collect();
    Ok(Plan {
        actions,
        actors: chosen.par_iter().map(|p| p.person).collect(),
        targets: targets(disease, &selected, now),
    })
}
#[macro_export]
macro_rules! prepare_treatment {
    ($world:expr, $busy:expr, $view:expr, $base:expr, $choice:expr) => {{
        let manual = match $choice {
            Some(game_economy_interaction::PhysicalChoice::Treat { person }) => Some(person),
            _ => None,
        };
        $crate::prepare(
            $world.dynamics.as_ref().and_then(|d| d.disease.as_ref()),
            $world
                .dynamics
                .as_ref()
                .map_or(&[][..], |d| d.treatment.as_slice()),
            &$world.people,
            &$busy,
            &$view,
            $base,
            $crate::Choice {
                manual,
                max_health: $world.rules.max_health,
            },
        )?
    }};
}
pub fn apply(disease: Option<&mut Disease>, plan: &Plan, events: &Events) {
    apply_outcomes(disease, plan, &events.physical_actions, events.tick);
}
/// Apply only to episodes selected before the physical consumption was committed.
pub fn apply_outcomes(
    disease: Option<&mut Disease>,
    plan: &Plan,
    outcomes: &[game_lots::TransactionOutcome],
    tick: u64,
) {
    if plan.targets.is_empty() {
        return;
    }
    let mut accepted: Vec<_> = plan
        .actions
        .par_iter()
        .zip(plan.actors.par_iter())
        .filter_map(|(action, &person)| {
            outcomes
                .binary_search_by_key(&action.id, |row| row.id)
                .ok()
                .filter(|&index| outcomes[index].accepted)
                .map(|_| (person, action.movements[0].good))
        })
        .collect();
    accepted.par_sort_unstable();
    let mut treated: Vec<_> = plan
        .targets
        .par_iter()
        .filter(|row| accepted.binary_search(&(row.0, row.1)).is_ok())
        .map(|row| (row.0, row.2, row.3))
        .collect();
    treated.par_sort_unstable();
    apply_episodes(disease, &treated, tick);
}
/// Record only the opening episodes affected by actual consumed medicine.
pub fn apply_episodes(disease: Option<&mut Disease>, treated: &[(u64, u32, u64)], tick: u64) {
    if treated.is_empty() {
        return;
    }
    let Some(disease) = disease else {
        return;
    };
    let mut emitted: Vec<_> = disease
        .cases
        .par_iter()
        .filter(|case| {
            treated
                .binary_search(&(case.person, case.strain, case.infected_at))
                .is_ok()
        })
        .map(|case| Event {
            tick,
            person: case.person,
            strain: case.strain,
            source: case.source,
            kind: EventKind::Treated,
        })
        .collect();
    disease.cases = std::mem::take(&mut disease.cases)
        .into_par_iter()
        .filter(|case| {
            treated
                .binary_search(&(case.person, case.strain, case.infected_at))
                .is_err()
        })
        .collect();
    disease.events.append(&mut emitted);
    disease.events.par_sort_unstable_by_key(|event| {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "performance measurement"]
    fn independent_multistrain_treatment_benchmark() {
        let count = std::env::var("LEGACY_PEOPLE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100_000u64);
        let workers = std::env::var("LEGACY_WORKERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let mut disease = Disease {
            strains: (1..=2)
                .map(|id| game_disease::Strain {
                    id,
                    name: format!("strain-{id}"),
                    incubation: 0,
                    infectious_ticks: 10,
                    immunity_ticks: 0,
                    damage_per_tick: 0,
                    transmission_per_million: 0,
                    combat_transmission_per_million: 0,
                    birth_transmission_per_million: 0,
                })
                .collect(),
            treatments: vec![game_disease::Treatment {
                strain: 1,
                medicine: 1,
            }],
            cases: (1..=count)
                .flat_map(|person| {
                    [1, 2].map(|strain| game_disease::Case {
                        person,
                        strain,
                        infected_at: 0,
                        source: None,
                    })
                })
                .collect(),
            events: vec![],
        };
        let choices: Vec<_> = (1..=count).map(|person| (person, 1)).collect();
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let start = std::time::Instant::now();
                let mut episodes: Vec<_> = targets(&disease, &choices, 0)
                    .into_par_iter()
                    .map(|row| (row.0, row.2, row.3))
                    .collect();
                episodes.par_sort_unstable();
                apply_episodes(Some(&mut disease), &episodes, 1);
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(disease.cases.len(), count as usize);
                assert!(disease.cases.iter().all(|case| case.strain == 2));
                println!("patients={count} workers={workers} seconds={elapsed:.6}");
            });
    }
    #[test]
    fn treatment_removes_only_the_selected_opening_episode() {
        let mut disease = Disease {
            strains: vec![],
            treatments: vec![],
            events: vec![],
            cases: vec![
                game_disease::Case {
                    person: 1,
                    strain: 1,
                    infected_at: 2,
                    source: None,
                },
                game_disease::Case {
                    person: 1,
                    strain: 2,
                    infected_at: 3,
                    source: None,
                },
                game_disease::Case {
                    person: 2,
                    strain: 1,
                    infected_at: 5,
                    source: None,
                },
            ],
        };
        apply_episodes(Some(&mut disease), &[(1, 1, 2), (2, 1, 0)], 5);
        assert_eq!(
            disease
                .cases
                .iter()
                .map(|case| (case.person, case.strain, case.infected_at))
                .collect::<Vec<_>>(),
            [(1, 2, 3), (2, 1, 5)]
        );
        assert_eq!(disease.events.len(), 1);
        assert_eq!((disease.events[0].person, disease.events[0].strain), (1, 1));
    }
    #[test]
    fn ineffective_course_cannot_clear_an_infection_acquired_at_the_same_boundary() {
        let mut disease = Disease {
            strains: vec![],
            treatments: vec![],
            cases: vec![game_disease::Case {
                person: 1,
                strain: 1,
                infected_at: 1,
                source: Some(2),
            }],
            events: vec![],
        };
        let plan = Plan {
            actions: vec![Transaction {
                id: 1,
                movements: vec![],
                creations: vec![],
            }],
            actors: vec![1],
            targets: vec![],
        };
        let events = Events {
            tick: 1,
            started: vec![],
            start_denied: vec![],
            abandoned: vec![],
            suspended: vec![],
            adopted: vec![],
            finished: vec![],
            delivered: vec![],
            output_lost: vec![],
            interrupted: vec![],
            returned: vec![],
            empty_returns: vec![],
            spoiled: vec![],
            physical_actions: vec![game_lots::TransactionOutcome {
                id: 1,
                accepted: true,
            }],
            physical_movements: vec![].into(),
        };
        apply(Some(&mut disease), &plan, &events);
        assert_eq!(disease.cases.len(), 1);
        assert!(disease.events.is_empty());
    }
    #[test]
    fn invalid_remedy_batch_does_not_partially_mutate_state() {
        let mut disease = Disease {
            strains: vec![game_disease::Strain {
                id: 1,
                name: "fever".into(),
                incubation: 0,
                infectious_ticks: 2,
                immunity_ticks: 0,
                damage_per_tick: 0,
                transmission_per_million: 0,
                combat_transmission_per_million: 0,
                birth_transmission_per_million: 0,
            }],
            treatments: vec![],
            cases: vec![],
            events: vec![],
        };
        let before = disease.clone();
        let mut policies = vec![];
        let remedies = [
            Remedy {
                strain: 1,
                medicine: "herb".into(),
                quantity: 1,
                people: vec![1],
                willing: true,
                health_at_most: None,
            },
            Remedy {
                strain: 9,
                medicine: "herb".into(),
                quantity: 1,
                people: vec![2],
                willing: true,
                health_at_most: None,
            },
        ];
        assert!(configure(Some(&mut disease), &mut policies, &remedies, |_| Some(1)).is_err());
        assert_eq!(disease, before);
        assert!(policies.is_empty());
    }
}

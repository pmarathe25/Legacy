//! A person's own choice to consume real medicine for an existing infection.
use game_parallel_disease::{Disease, Event, EventKind};
use game_parallel_economy_person::Person;
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Movement, Place, Transaction, View};
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
        additions.push(game_parallel_disease::Treatment {
            strain: remedy.strain,
            medicine,
        });
        people.extend(remedy.people.iter().map(|&person| Policy {
            person,
            medicine,
            quantity: remedy.quantity,
            willing: remedy.willing,
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
}
pub fn prepare(
    disease: Option<&Disease>,
    policies: &[Policy],
    people: &[Person],
    busy: &[u64],
    view: &View,
    base: u64,
    manual: Option<u64>,
) -> Result<Plan, String> {
    let Some(disease) = disease else {
        if manual.is_some() {
            return Err("treatment is disabled".into());
        }
        return Ok(Plan {
            actions: vec![],
            actors: vec![],
        });
    };
    if let Some(id) = manual {
        let person = people
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .map(|i| &people[i])
            .ok_or("unknown person")?;
        let policy = policies
            .iter()
            .find(|p| p.person == id)
            .ok_or("person has no treatment preference")?;
        let case = disease
            .cases
            .iter()
            .find(|c| c.person == id)
            .ok_or("person has no infection")?;
        let strain = disease
            .strains
            .iter()
            .find(|s| s.id == case.strain)
            .ok_or("unknown infection")?;
        let age = view
            .boundary
            .checked_sub(1)
            .and_then(|now| now.checked_sub(case.infected_at))
            .ok_or("invalid infection time")?;
        if !person.controlled
            || person.health == 0
            || age >= strain.incubation + strain.infectious_ticks + strain.immunity_ticks
            || !disease
                .treatments
                .iter()
                .any(|t| t.strain == case.strain && t.medicine == policy.medicine)
        {
            return Err("person cannot use this treatment".into());
        }
    }
    let mut cases: Vec<_> = disease.cases.par_iter().collect();
    cases.par_sort_unstable_by_key(|case| case.person);
    let chosen: Vec<_> = policies
        .par_iter()
        .filter_map(|policy| {
            let person = people
                .binary_search_by_key(&policy.person, |p| p.id)
                .ok()
                .map(|i| &people[i])?;
            let case = cases
                .binary_search_by_key(&policy.person, |c| c.person)
                .ok()
                .map(|i| cases[i])?;
            let strain = disease
                .strains
                .iter()
                .find(|strain| strain.id == case.strain)?;
            let age = view
                .boundary
                .checked_sub(1)?
                .checked_sub(case.infected_at)?;
            let explicit = manual == Some(person.id);
            if (!policy.willing && !explicit)
                || (person.controlled && !explicit)
                || person.health == 0
                || (!explicit && busy.binary_search(&person.id).is_ok())
                || age >= strain.incubation + strain.infectious_ticks + strain.immunity_ticks
                || !disease
                    .treatments
                    .iter()
                    .any(|t| t.strain == case.strain && t.medicine == policy.medicine)
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
        .map(|(i, p)| {
            let id = base + i as u64;
            Transaction {
                id,
                movements: vec![Movement {
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
    Ok(Plan {
        actions,
        actors: chosen.par_iter().map(|p| p.person).collect(),
    })
}
pub fn treated(disease: Option<&Disease>, person: u64) -> bool {
    disease.is_some_and(|d| {
        d.events
            .iter()
            .any(|event| event.person == person && event.kind == EventKind::Treated)
    })
}
#[macro_export]
macro_rules! prepare_treatment {
    ($world:expr, $busy:expr, $view:expr, $base:expr, $choice:expr) => {{
        let manual = match $choice {
            Some(game_parallel_economy_interaction::PhysicalChoice::Treat { person }) => {
                Some(person)
            }
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
            manual,
        )?
    }};
}
pub fn apply(disease: Option<&mut Disease>, plan: &Plan, events: &Events) {
    let mut treated: Vec<_> = plan
        .actions
        .par_iter()
        .zip(plan.actors.par_iter())
        .filter_map(|(action, &person)| {
            events
                .physical_actions
                .binary_search_by_key(&action.id, |row| row.id)
                .ok()
                .filter(|&index| events.physical_actions[index].accepted)
                .map(|_| person)
        })
        .collect();
    treated.par_sort_unstable();
    apply_people(disease, &treated, events.tick);
}
/// Record courses that consumed medicine through the physical owner.
pub fn apply_people(disease: Option<&mut Disease>, treated: &[u64], tick: u64) {
    if treated.is_empty() {
        return;
    }
    let Some(disease) = disease else {
        return;
    };
    let mut emitted: Vec<_> = disease
        .cases
        .par_iter()
        .filter(|case| treated.binary_search(&case.person).is_ok())
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
        .filter(|case| treated.binary_search(&case.person).is_err())
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
    fn invalid_remedy_batch_does_not_partially_mutate_state() {
        let mut disease = Disease {
            strains: vec![game_parallel_disease::Strain {
                id: 1,
                name: "fever".into(),
                incubation: 0,
                infectious_ticks: 2,
                immunity_ticks: 0,
                damage_per_tick: 0,
                transmission_per_million: 0,
                combat_transmission_per_million: 0,
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
            },
            Remedy {
                strain: 9,
                medicine: "herb".into(),
                quantity: 1,
                people: vec![2],
                willing: true,
            },
        ];
        assert!(configure(Some(&mut disease), &mut policies, &remedies, |_| Some(1)).is_err());
        assert_eq!(disease, before);
        assert!(policies.is_empty());
    }
}

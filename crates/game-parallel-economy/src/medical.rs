//! Personal medical assistance consumes the caregiver's physical supplies.
use game_parallel_disease::{Disease, EventKind};
use game_parallel_economy_person::{Person, travel::Colocation};
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Movement, Place, Transaction, View};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::Good;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub patient: u64,
    pub medicine: u32,
    pub quantity: u64,
    pub willing: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Medical {
    pub policies: Vec<Policy>,
}
impl Clone for Medical {
    fn clone(&self) -> Self {
        Self {
            policies: self.policies.par_iter().cloned().collect(),
        }
    }
}
impl Medical {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        disease: Option<&Disease>,
    ) -> Result<(), String> {
        let mut actors: Vec<_> = self.policies.par_iter().map(|p| p.person).collect();
        actors.par_sort_unstable();
        if actors.par_windows(2).any(|p| p[0] == p[1])
            || self.policies.par_iter().any(|p| {
                !people.contains(&p.person)
                    || !people.contains(&p.patient)
                    || p.person == p.patient
                    || !goods.contains(&p.medicine)
                    || p.quantity == 0
                    || p.pressured_by
                        .is_some_and(|id| id == p.person || !people.contains(&id))
                    || p.pressure_threshold == Some(0)
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
                    || disease
                        .is_none_or(|d| !d.treatments.iter().any(|t| t.medicine == p.medicine))
            })
        {
            return Err("invalid personal medical preference".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
struct Choice {
    actor: u64,
    patient: u64,
    medicine: u32,
    quantity: u64,
    effective: bool,
}
pub struct Plan {
    pub actors: Vec<u64>,
    pub actions: Vec<Transaction>,
    records: Vec<(u64, Choice)>,
}
pub struct Context<'a> {
    pub owner: Option<&'a Medical>,
    pub disease: Option<&'a Disease>,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub goods: &'a [Good],
    pub view: &'a View,
    pub busy: &'a [u64],
    pub blocked: &'a [u64],
    pub resting: &'a [u64],
    pub seed: u64,
    pub tick: u64,
    pub manual: Option<(u64, u64, u32, u64)>,
    pub locations: &'a Colocation,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        if self.manual.is_none() && self.owner.is_none_or(|o| o.policies.is_empty()) {
            return Ok(Plan {
                actors: Vec::new(),
                actions: Vec::new(),
                records: Vec::new(),
            });
        }
        let cases: HashMap<_, _> = self
            .disease
            .map_or(&[][..], |d| d.cases.as_slice())
            .par_iter()
            .map(|c| (c.person, c.strain))
            .collect();
        let remedies: HashSet<_> = self
            .disease
            .into_iter()
            .flat_map(|d| &d.treatments)
            .map(|t| (t.strain, t.medicine))
            .collect();
        let effective = |patient, medicine| {
            cases
                .get(&patient)
                .is_some_and(|strain| remedies.contains(&(*strain, medicine)))
        };
        let find = |id| {
            self.people
                .binary_search_by_key(&id, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
        };
        let available = |patient: &Person| {
            self.blocked.binary_search(&patient.id).is_err()
                && (patient.controlled
                    || self.resting.binary_search(&patient.id).is_ok()
                    || self.busy.binary_search(&patient.id).is_err())
        };
        let opinions = self.opinions.map(Opinions::index);
        let policies = self.owner.map_or(&[][..], |o| o.policies.as_slice());
        let mut choices: Vec<_> = policies
            .par_iter()
            .filter_map(|p| {
                let actor = find(p.person)?;
                let patient = find(p.patient)?;
                self.locations
                    .region((actor.id, actor.region), (patient.id, patient.region))?;
                let pressured =
                    p.pressured_by
                        .zip(p.pressure_threshold)
                        .is_some_and(|(other, threshold)| {
                            opinions.as_ref().is_some_and(|index| {
                                let (fear, obligation) = index.feelings(actor.id, other);
                                fear.saturating_add(obligation) >= threshold
                            })
                        });
                (!actor.controlled
                    && actor.health > 0
                    && patient.health > 0
                    && self.busy.binary_search(&actor.id).is_err()
                    && available(patient)
                    && (p.willing || pressured)
                    && effective(patient.id, p.medicine)
                    && self
                        .view
                        .container(Place::Person(actor.id))
                        .is_some_and(|c| c.quantity(p.medicine) >= p.quantity))
                .then_some(Choice {
                    actor: actor.id,
                    patient: patient.id,
                    medicine: p.medicine,
                    quantity: p.quantity,
                    effective: true,
                })
            })
            .collect();
        if let Some((actor, patient, medicine, quantity)) = self.manual {
            let a = find(actor).ok_or("unknown caregiver")?;
            let p = find(patient).ok_or("unknown patient")?;
            if self.disease.is_none()
                || !a.controlled
                || a.health == 0
                || p.health == 0
                || actor == patient
                || self
                    .locations
                    .region((a.id, a.region), (p.id, p.region))
                    .is_none()
                || !available(p)
                || quantity == 0
                || !self.goods.iter().any(|g| g.id == medicine)
            {
                return Err(
                    "care requires a living controlled caregiver and an available local patient"
                        .into(),
                );
            }
            choices.push(Choice {
                actor,
                patient,
                medicine,
                quantity,
                effective: effective(patient, medicine),
            });
        }
        choices.par_sort_unstable_by_key(|c| (c.actor, c.patient));
        let votes: Vec<_> = (0..choices.len())
            .into_par_iter()
            .map(|_| AtomicU8::new(0))
            .collect();
        let mut proposals: Vec<_> = choices
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, c)| {
                let rank =
                    game_parallel_genetics::random(self.seed ^ c.patient, c.actor, self.tick);
                [
                    (c.actor, rank, c.actor, c.patient, i),
                    (c.patient, rank, c.actor, c.patient, i),
                ]
            })
            .collect();
        proposals.par_sort_unstable();
        proposals.par_chunk_by(|a, b| a.0 == b.0).for_each(|group| {
            votes[group[0].4].fetch_add(1, Ordering::Relaxed);
        });
        let chosen: Vec<_> = choices
            .into_par_iter()
            .enumerate()
            .filter_map(|(i, c)| (votes[i].load(Ordering::Relaxed) == 2).then_some(c))
            .collect();
        base.checked_add(chosen.len() as u64)
            .ok_or("medical action overflow")?;
        let records: Vec<_> = chosen
            .par_iter()
            .enumerate()
            .map(|(i, c)| (base + i as u64, *c))
            .collect();
        let actions = records
            .par_iter()
            .map(|&(id, c)| Transaction {
                id,
                movements: vec![Movement {
                    id,
                    from: Place::Person(c.actor),
                    to: None,
                    good: c.medicine,
                    quantity: c.quantity,
                }],
                creations: vec![],
            })
            .collect();
        let mut actors: Vec<_> = chosen
            .par_iter()
            .flat_map_iter(|c| [c.actor, c.patient])
            .collect();
        actors.par_sort_unstable();
        actors.dedup();
        Ok(Plan {
            actors,
            actions,
            records,
        })
    }
}
impl Plan {
    pub fn caregivers(&self) -> Vec<u64> {
        self.records.par_iter().map(|(_, c)| c.actor).collect()
    }
    fn accepted(&self, events: &Events) -> Vec<Choice> {
        if self.records.is_empty() {
            return Vec::new();
        }
        let mut accepted: Vec<_> = events
            .physical_actions
            .par_iter()
            .filter(|d| d.accepted)
            .map(|d| d.id)
            .collect();
        accepted.par_sort_unstable();
        self.records
            .par_iter()
            .filter(|r| accepted.binary_search(&r.0).is_ok())
            .map(|r| r.1)
            .collect()
    }
    pub fn contacts(&self, events: &Events) -> Vec<(u64, u64)> {
        self.accepted(events)
            .into_par_iter()
            .map(|c| (c.actor, c.patient))
            .collect()
    }
    pub fn apply(&self, disease: Option<&mut Disease>, events: &Events) {
        let mut patients: Vec<_> = self
            .accepted(events)
            .into_par_iter()
            .filter(|c| c.effective)
            .map(|c| c.patient)
            .collect();
        patients.par_sort_unstable();
        game_parallel_economy_dynamics::treatment::apply_people(disease, &patients, events.tick);
    }
    pub fn treated(&self, actor: u64, patient: u64, disease: Option<&Disease>) -> bool {
        self.records
            .iter()
            .any(|(_, c)| c.actor == actor && c.patient == patient && c.effective)
            && disease.is_some_and(|d| {
                d.events
                    .iter()
                    .any(|e| e.person == patient && e.kind == EventKind::Treated)
            })
    }
    /// The patient interprets an actual treatment attempt, regardless of its outcome.
    pub fn help(&self, events: &Events) -> Vec<(u64, u64)> {
        self.accepted(events)
            .into_par_iter()
            .map(|choice| (choice.patient, choice.actor))
            .collect()
    }
}

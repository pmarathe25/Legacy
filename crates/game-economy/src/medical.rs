//! Personal medical assistance consumes the caregiver's physical supplies.
use game_disease::Disease;
use game_economy_person::{Person, travel::Colocation};
use game_lot_runtime::Events;
use game_lots::{Movement, Place, Transaction, View};
use game_opinions::Opinions;
use game_production_model::Good;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub patient: u64,
    pub medicine: u32,
    pub quantity: u64,
    pub willing: bool,
    /// If set, act at or below this observed health; absent uses below maximum health.
    #[serde(default)]
    pub patient_health_at_most: Option<u32>,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}
/// A person's choice to help someone they have met, using observable health.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquaintancePolicy {
    pub person: u64,
    pub medicine: u32,
    pub quantity: u64,
    pub patient_health_at_most: u32,
    pub willing: bool,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
    #[serde(default)]
    pub urgency_weight: i32,
    #[serde(default)]
    pub affinity_weight: i32,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Medical {
    pub policies: Vec<Policy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acquaintances: Vec<AcquaintancePolicy>,
}
impl Clone for Medical {
    fn clone(&self) -> Self {
        Self {
            policies: self.policies.par_iter().cloned().collect(),
            acquaintances: self.acquaintances.par_iter().cloned().collect(),
        }
    }
}
impl Medical {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        disease: Option<&Disease>,
        social: bool,
    ) -> Result<(), String> {
        let mut actors: Vec<_> = self
            .policies
            .par_iter()
            .map(|p| p.person)
            .chain(self.acquaintances.par_iter().map(|p| p.person))
            .collect();
        actors.par_sort_unstable();
        if actors.par_windows(2).any(|p| p[0] == p[1])
            || (!social && !self.acquaintances.is_empty())
            || self.acquaintances.par_iter().any(|p| {
                !people.contains(&p.person)
                    || !goods.contains(&p.medicine)
                    || p.quantity == 0
                    || p.pressure_threshold == Some(0)
                    || disease
                        .is_none_or(|d| !d.treatments.iter().any(|t| t.medicine == p.medicine))
            })
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
}
pub struct Plan {
    pub actors: Vec<u64>,
    pub actions: Vec<Transaction>,
    records: Vec<(u64, Choice)>,
    targets: Vec<(u64, u32, u32, u64)>,
}
pub struct Context<'a> {
    pub owner: Option<&'a Medical>,
    pub disease: Option<&'a Disease>,
    pub social: Option<&'a game_social::Social>,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub goods: &'a [Good],
    pub view: &'a View,
    pub busy: &'a [u64],
    pub blocked: &'a [u64],
    pub available_patients: &'a [u64],
    pub communication: &'a game_economy_confinement::Communication<'a>,
    pub resting: &'a [u64],
    pub seed: u64,
    pub tick: u64,
    pub max_health: u32,
    pub manual: Option<(u64, u64, u32, u64)>,
    pub locations: &'a Colocation,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        if self.manual.is_none()
            && self
                .owner
                .is_none_or(|o| o.policies.is_empty() && o.acquaintances.is_empty())
        {
            return Ok(Plan {
                actors: Vec::new(),
                actions: Vec::new(),
                records: Vec::new(),
                targets: Vec::new(),
            });
        }
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
                    || self.busy.binary_search(&patient.id).is_err()
                    || self.available_patients.binary_search(&patient.id).is_ok())
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
                    && self.communication.can_touch(actor.id, patient.id)
                    && (p.willing || pressured)
                    && p.patient_health_at_most.map_or_else(
                        || patient.health < self.max_health,
                        |threshold| patient.health <= threshold,
                    )
                    && self
                        .view
                        .container(Place::Person(actor.id))
                        .is_some_and(|c| c.quantity(p.medicine) >= p.quantity))
                .then_some(Choice {
                    actor: actor.id,
                    patient: patient.id,
                    medicine: p.medicine,
                    quantity: p.quantity,
                })
            })
            .collect();
        let mut preferences: Vec<_> = self
            .owner
            .map_or(&[][..], |owner| owner.acquaintances.as_slice())
            .par_iter()
            .collect();
        if preferences
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            preferences.par_sort_unstable_by_key(|p| p.person);
        }
        let dense = preferences
            .first()
            .zip(preferences.last())
            .and_then(|(first, last)| {
                (last.person.checked_sub(first.person) == Some((preferences.len() - 1) as u64))
                    .then_some(first.person)
            });
        if !preferences.is_empty() {
            let mut candidates: Vec<_> = self
                .social
                .map_or(&[][..], |s| s.relationships.as_slice())
                .par_iter()
                .flat_map_iter(|relation| [(relation.a, relation.b), (relation.b, relation.a)])
                .filter_map(|(actor_id, patient_id)| {
                    let preference = if let Some(first) = dense {
                        preferences
                            .get(usize::try_from(actor_id.checked_sub(first)?).ok()?)
                            .copied()?
                    } else {
                        let index = preferences
                            .binary_search_by_key(&actor_id, |p| p.person)
                            .ok()?;
                        preferences[index]
                    };
                    let actor = find(actor_id)?;
                    let patient = find(patient_id)?;
                    self.locations
                        .region((actor.id, actor.region), (patient.id, patient.region))?;
                    let pressured = preference.pressure_threshold.is_some_and(|threshold| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(actor_id, patient_id);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                    if actor.controlled
                        || actor.health == 0
                        || patient.health == 0
                        || self.busy.binary_search(&actor_id).is_ok()
                        || !available(patient)
                        || !self.communication.can_touch(actor_id, patient_id)
                        || (!preference.willing && !pressured)
                        || patient.health > preference.patient_health_at_most
                        || self
                            .view
                            .container(Place::Person(actor_id))
                            .is_none_or(|c| c.quantity(preference.medicine) < preference.quantity)
                    {
                        return None;
                    }
                    let affinity = opinions
                        .as_ref()
                        .map_or(0, |index| index.affinity(actor_id, patient_id));
                    let score = i128::from(preference.patient_health_at_most - patient.health)
                        * i128::from(preference.urgency_weight)
                        + i128::from(affinity) * i128::from(preference.affinity_weight);
                    Some((
                        actor_id,
                        std::cmp::Reverse(score),
                        patient_id,
                        Choice {
                            actor: actor_id,
                            patient: patient_id,
                            medicine: preference.medicine,
                            quantity: preference.quantity,
                        },
                    ))
                })
                .collect();
            if candidates
                .par_windows(2)
                .any(|pair| (pair[0].0, pair[0].1, pair[0].2) > (pair[1].0, pair[1].1, pair[1].2))
            {
                candidates.par_sort_unstable_by_key(|row| (row.0, row.1, row.2));
            }
            choices.par_extend(
                candidates
                    .par_chunk_by(|a, b| a.0 == b.0)
                    .map(|group| group[0].3),
            );
        }
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
                || !self.communication.can_touch(actor, patient)
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
            });
        }
        if choices
            .par_windows(2)
            .any(|pair| (pair[0].actor, pair[0].patient) > (pair[1].actor, pair[1].patient))
        {
            choices.par_sort_unstable_by_key(|c| (c.actor, c.patient));
        }
        let votes: Vec<_> = (0..choices.len())
            .into_par_iter()
            .map(|_| AtomicU8::new(0))
            .collect();
        let mut proposals: Vec<_> = choices
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, c)| {
                let rank = game_genetics::random(self.seed ^ c.patient, c.actor, self.tick);
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
                    item: None,
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
        let selected: Vec<_> = chosen
            .par_iter()
            .map(|choice| (choice.patient, choice.medicine))
            .collect();
        let targets = self.disease.map_or_else(Vec::new, |disease| {
            game_economy_dynamics::treatment::targets(
                disease,
                &selected,
                self.tick.saturating_sub(1),
            )
        });
        Ok(Plan {
            actors,
            actions,
            records,
            targets,
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
            .map(|c| (c.patient, c.medicine))
            .collect();
        patients.par_sort_unstable();
        let mut episodes: Vec<_> = self
            .targets
            .par_iter()
            .filter(|row| patients.binary_search(&(row.0, row.1)).is_ok())
            .map(|row| (row.0, row.2, row.3))
            .collect();
        episodes.par_sort_unstable();
        game_economy_dynamics::treatment::apply_episodes(disease, &episodes, events.tick);
    }
    pub fn attempted(&self, actor: u64, patient: u64, events: &Events) -> bool {
        self.records.iter().any(|(id, choice)| {
            choice.actor == actor
                && choice.patient == patient
                && events
                    .physical_actions
                    .binary_search_by_key(id, |event| event.id)
                    .ok()
                    .is_some_and(|index| events.physical_actions[index].accepted)
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

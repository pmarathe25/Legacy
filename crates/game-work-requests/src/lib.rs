//! Addressed requests for physical work; no agreement or organization is required.
mod experience;
pub use experience::{Memory, SuccessReceipt, WorkLink};
use game_agreements::{Agreements, Status};
use game_opinions::Opinions;
use game_production::Production;
use game_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requests {
    pub next_id: u64,
    /// Enroll newborns with general preferences, never another person's experiences.
    #[serde(default)]
    pub inherit_births: bool,
    /// Maximum personal work memories per observer; zero disables learning.
    #[serde(default)]
    pub memory_limit: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub work_links: Vec<WorkLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memories: Vec<Memory>,
    #[serde(default)]
    pub preferences: Vec<Preference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub askers: Vec<Asker>,
    #[serde(default)]
    pub pending: Vec<Request>,
    #[serde(default)]
    pub events: Vec<Event>,
}
impl Clone for Requests {
    fn clone(&self) -> Self {
        Self {
            next_id: self.next_id,
            inherit_births: self.inherit_births,
            memory_limit: self.memory_limit,
            work_links: self.work_links.par_iter().cloned().collect(),
            memories: self.memories.par_iter().cloned().collect(),
            preferences: self.preferences.par_iter().cloned().collect(),
            askers: self.askers.par_iter().cloned().collect(),
            pending: self.pending.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub effort: u64,
    #[serde(default)]
    pub help_value: u64,
    /// This person's signed motivation per unit of their own directed affinity.
    #[serde(default)]
    pub affinity_weight: i64,
    /// Each individual decides how much their own trust affects willingness.
    #[serde(default)]
    pub trust_weight: i64,
    /// This person's valuation of an outstanding offer, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offer_value: Option<OfferValue>,
    #[serde(default)]
    pub disregard_claims: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfferValue {
    pub good: u32,
    pub value_per_unit: i64,
    pub max_quantity: u64,
}
/// Snapshot of outstanding personal promises, indexed by recipient, maker, and good.
pub struct OfferIndex {
    rows: Vec<(u64, u64, u32, u64)>,
    practices: Vec<PracticeOffer>,
}
/// Terms received by the worker, scoped to one recipe and not enforceable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PracticeOffer {
    pub worker: u64,
    pub requester: u64,
    pub recipe: u32,
    pub good: u32,
    pub quantity: u64,
}
impl OfferIndex {
    pub fn quantity(&self, worker: u64, requester: u64, good: u32) -> u64 {
        self.rows
            .binary_search_by_key(&(worker, requester, good), |row| (row.0, row.1, row.2))
            .ok()
            .map_or(0, |i| self.rows[i].3)
    }
    /// Whether the requester has an outstanding promise addressed to this worker.
    pub fn has_offer(&self, worker: u64, requester: u64) -> bool {
        let index = self
            .rows
            .partition_point(|row| (row.0, row.1) < (worker, requester));
        self.rows
            .get(index)
            .is_some_and(|row| (row.0, row.1) == (worker, requester))
    }
    /// Include personally received practice terms without creating an agreement.
    pub fn with_practices(
        existing: Option<Self>,
        mut practices: Vec<PracticeOffer>,
    ) -> Option<Self> {
        if practices.is_empty() {
            return existing;
        }
        practices.par_sort_unstable_by_key(|row| (row.worker, row.requester, row.recipe, row.good));
        let practices = practices
            .par_chunk_by(|a, b| {
                (a.worker, a.requester, a.recipe, a.good)
                    == (b.worker, b.requester, b.recipe, b.good)
            })
            .map(|group| PracticeOffer {
                // Revisions of one person's terms are alternatives, not separate gifts.
                quantity: group.par_iter().map(|row| row.quantity).max().unwrap_or(0),
                ..group[0]
            })
            .collect();
        let mut index = existing.unwrap_or(Self {
            rows: vec![],
            practices: vec![],
        });
        index.practices = practices;
        Some(index)
    }
    pub fn quantity_for(&self, worker: u64, requester: u64, recipe: u32, good: u32) -> u64 {
        let practice = self
            .practices
            .binary_search_by_key(&(worker, requester, recipe, good), |row| {
                (row.worker, row.requester, row.recipe, row.good)
            })
            .ok()
            .map_or(0, |i| self.practices[i].quantity);
        self.quantity(worker, requester, good)
            .saturating_add(practice)
    }
}
impl Preference {
    /// No agreement or feeling forces compliance; this is the person's present evaluation.
    pub fn motivated(
        &self,
        fear: u64,
        obligation: u64,
        affinity: i32,
        trust: i32,
        offered: u64,
    ) -> bool {
        let value = i128::from(self.help_value)
            + i128::from(fear)
            + i128::from(obligation)
            + i128::from(affinity) * i128::from(self.affinity_weight)
            + i128::from(trust) * i128::from(self.trust_weight);
        let offer = self.offer_value.as_ref().map_or(0, |value| {
            i128::from(value.value_per_unit)
                .saturating_mul(i128::from(offered.min(value.max_quantity)))
        });
        value.saturating_add(offer) >= i128::from(self.effort)
    }
}
/// Periodically ask a local person to help with an active personal goal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asker {
    pub person: u64,
    pub interval: u64,
    pub lifetime: u64,
    /// This person may reconsider asking someone to whom they made an offer.
    #[serde(default)]
    pub retry_after_offer: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: u64,
    pub requester: u64,
    pub worker: u64,
    pub region: u64,
    pub recipe: u32,
    pub created_at: u64,
    pub expires: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub request: u64,
    pub worker: u64,
    pub outcome: Outcome,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Started { job: u64 },
    Refused,
    Expired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub requester: u64,
    pub worker: u64,
    pub region: u64,
    pub recipe: u32,
    pub expires: u64,
}
pub struct Decision {
    pub request: u64,
    pub worker: u64,
    pub region: u64,
    pub recipe: u32,
    pub help: bool,
    pub disregard_claims: bool,
}
pub struct Draft {
    pub decisions: Vec<Decision>,
    pub expired: Vec<Event>,
}

impl Requests {
    /// Only addressed, still-pending promises can inform a recipient's own choice.
    pub fn offer_index(&self, agreements: Option<&Agreements>, tick: u64) -> Option<OfferIndex> {
        let agreements = agreements.filter(|owner| !owner.contracts.is_empty())?;
        if !self.askers.par_iter().any(|a| a.retry_after_offer)
            && !self.preferences.par_iter().any(|p| {
                p.offer_value
                    .as_ref()
                    .is_some_and(|v| v.value_per_unit != 0)
            })
        {
            return None;
        }
        let mut rows: Vec<_> = agreements
            .contracts
            .par_iter()
            .filter_map(|contract| {
                let live = match contract.status {
                    Status::Proposed => contract.due > tick,
                    Status::Active => contract.due >= tick,
                    _ => false,
                };
                let quantity = contract.quantity.saturating_sub(contract.delivered);
                (live && quantity > 0).then_some((
                    contract.creditor,
                    contract.debtor,
                    contract.good,
                    quantity,
                ))
            })
            .collect();
        rows.par_sort_unstable_by_key(|row| (row.0, row.1, row.2));
        let rows = rows
            .par_chunk_by(|a, b| (a.0, a.1, a.2) == (b.0, b.1, b.2))
            .map(|group| {
                (
                    group[0].0,
                    group[0].1,
                    group[0].2,
                    group
                        .par_iter()
                        .map(|row| row.3)
                        .reduce(|| 0, u64::saturating_add),
                )
            })
            .collect();
        Some(OfferIndex {
            rows,
            practices: vec![],
        })
    }
    /// Copy only personal inclinations for actual newborns; no pending requests or memories.
    pub fn enroll(&mut self, family: &game_families::Families, tick: u64) {
        if !self.inherit_births || !family.rules.enroll_births {
            return;
        }
        let newborns: Vec<_> = family
            .members
            .par_iter()
            .filter(|member| member.born == Some(tick))
            .collect();
        if newborns.is_empty() {
            return;
        }
        let mut preferences: Vec<_> = self.preferences.par_iter().collect();
        preferences.par_sort_unstable_by_key(|row| row.person);
        let mut askers: Vec<_> = self.askers.par_iter().collect();
        askers.par_sort_unstable_by_key(|row| row.person);
        let inherited_preferences: Vec<_> = newborns
            .par_iter()
            .filter_map(|member| {
                if preferences
                    .binary_search_by_key(&member.id, |row| row.person)
                    .is_ok()
                {
                    return None;
                }
                let parent = member.parents.iter().find_map(|id| {
                    preferences
                        .binary_search_by_key(id, |row| row.person)
                        .ok()
                        .map(|i| preferences[i])
                })?;
                let mut preference = parent.clone();
                preference.person = member.id;
                Some(preference)
            })
            .collect();
        let inherited_askers: Vec<_> = newborns
            .into_par_iter()
            .filter_map(|member| {
                if askers
                    .binary_search_by_key(&member.id, |row| row.person)
                    .is_ok()
                {
                    return None;
                }
                let parent = member.parents.iter().find_map(|id| {
                    askers
                        .binary_search_by_key(id, |row| row.person)
                        .ok()
                        .map(|i| askers[i])
                })?;
                let mut asker = parent.clone();
                asker.person = member.id;
                Some(asker)
            })
            .collect();
        self.preferences.par_extend(inherited_preferences);
        self.askers.par_extend(inherited_askers);
    }

    pub fn validate(
        &self,
        people: &[PersonSnapshot],
        production: &Production,
    ) -> Result<(), String> {
        let mut preferences: Vec<_> = self.preferences.par_iter().map(|row| row.person).collect();
        preferences.par_sort_unstable();
        let mut askers: Vec<_> = self.askers.par_iter().map(|row| row.person).collect();
        askers.par_sort_unstable();
        let mut ids: Vec<_> = self.pending.par_iter().map(|row| row.id).collect();
        ids.par_sort_unstable();
        let recipes: HashSet<_> = production.recipes.iter().map(|row| row.id).collect();
        let goods: HashSet<_> = production.goods.iter().map(|row| row.id).collect();
        let regions: HashSet<_> = production
            .inventories
            .iter()
            .map(|row| row.region)
            .collect();
        if !self.valid_experience(people, &recipes, &regions)
            || self.next_id == 0
            || askers.par_windows(2).any(|row| row[0] == row[1])
            || self.askers.par_iter().any(|row| {
                row.interval == 0 || row.lifetime == 0 || person_index(people, row.person).is_none()
            })
            || preferences.par_windows(2).any(|w| w[0] == w[1])
            || preferences
                .par_iter()
                .any(|&person| person_index(people, person).is_none())
            || self.preferences.par_iter().any(|p| {
                p.offer_value
                    .as_ref()
                    .is_some_and(|value| value.max_quantity == 0 || !goods.contains(&value.good))
            })
            || ids.par_windows(2).any(|w| w[0] == w[1])
            || self.pending.par_iter().any(|row| {
                row.id == 0
                    || row.id >= self.next_id
                    || row.requester == row.worker
                    || person_index(people, row.requester).is_none()
                    || person_index(people, row.worker).is_none()
                    || !recipes.contains(&row.recipe)
                    || !regions.contains(&row.region)
                    || row.expires < row.created_at
            })
            || self.events.par_iter().any(|row| {
                row.request == 0
                    || row.request >= self.next_id
                    || person_index(people, row.worker).is_none()
            })
        {
            return Err("invalid work requests".into());
        }
        Ok(())
    }
    /// Send an addressed request; receiving a demand does not imply approval.
    pub fn open(
        &mut self,
        proposal: Proposal,
        people: &[PersonSnapshot],
        production: &Production,
        now: u64,
    ) -> Result<u64, String> {
        Ok(self.open_batch(&[proposal], people, production, now)?[0])
    }

    /// Validate the entire batch before allocating IDs or changing any inbox.
    pub fn open_batch(
        &mut self,
        proposals: &[Proposal],
        people: &[PersonSnapshot],
        production: &Production,
        now: u64,
    ) -> Result<Vec<u64>, String> {
        if proposals.is_empty() {
            return Ok(Vec::new());
        }
        let count = u64::try_from(proposals.len()).map_err(|_| "request id overflow")?;
        let next = self
            .next_id
            .checked_add(count)
            .ok_or("request id overflow")?;
        let recipes: HashSet<_> = production.recipes.iter().map(|row| row.id).collect();
        let regions: HashSet<_> = production
            .inventories
            .iter()
            .map(|row| row.region)
            .collect();
        if proposals.par_iter().any(|proposal| {
            let a = person_index(people, proposal.requester).map(|i| people[i]);
            let b = person_index(people, proposal.worker).map(|i| people[i]);
            proposal.requester == proposal.worker
                || proposal.expires <= now
                || !matches!((a,b), (Some((_,Some(ar),ah,_)),Some((_,Some(br),bh,_))) if ar == proposal.region && br == ar && ah > 0 && bh > 0)
                || !recipes.contains(&proposal.recipe)
                || !regions.contains(&proposal.region)
        }) {
            return Err("invalid work request".into());
        }
        let mut existing: Vec<_> = self.pending.par_iter().map(|row| row.worker).collect();
        if existing.par_windows(2).any(|pair| pair[0] > pair[1]) {
            existing.par_sort_unstable();
        }
        let mut recipients: Vec<_> = proposals.par_iter().map(|row| row.worker).collect();
        if recipients.par_windows(2).any(|pair| pair[0] > pair[1]) {
            recipients.par_sort_unstable();
        }
        if recipients.par_chunk_by(|a, b| a == b).any(|rows| {
            let start = existing.partition_point(|id| *id < rows[0]);
            let end = existing.partition_point(|id| *id <= rows[0]);
            rows.len() + end - start > 4
        }) {
            return Err("recipient request inbox is full".into());
        }
        let first = self.next_id;
        let rows: Vec<_> = proposals
            .par_iter()
            .enumerate()
            .map(|(i, proposal)| Request {
                id: first + i as u64,
                requester: proposal.requester,
                worker: proposal.worker,
                region: proposal.region,
                recipe: proposal.recipe,
                created_at: now,
                expires: proposal.expires,
            })
            .collect();
        self.pending.par_extend(rows);
        self.next_id = next;
        Ok((first..next).collect())
    }
    pub fn prepare(
        &self,
        people: &[PersonSnapshot],
        opinions: Option<&Opinions>,
        busy: &[u64],
        tick: u64,
    ) -> Draft {
        self.prepare_with_offers(people, opinions, busy, None, tick)
    }
    pub fn prepare_with_offers(
        &self,
        people: &[PersonSnapshot],
        opinions: Option<&Opinions>,
        busy: &[u64],
        offers: Option<&OfferIndex>,
        tick: u64,
    ) -> Draft {
        if self.pending.is_empty() {
            return Draft {
                decisions: Vec::new(),
                expired: Vec::new(),
            };
        }
        let mut busy = busy.to_vec();
        busy.par_sort_unstable();
        let mut prefs: Vec<_> = self.preferences.par_iter().collect();
        if prefs
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            prefs.par_sort_unstable_by_key(|row| row.person);
        }
        let dense = prefs.first().and_then(|first| {
            prefs.last().and_then(|last| {
                (last.person.checked_sub(first.person) == Some((prefs.len() - 1) as u64))
                    .then_some(first.person)
            })
        });
        let preference = |person: u64| {
            dense
                .and_then(|first| person.checked_sub(first))
                .and_then(|index| usize::try_from(index).ok())
                .filter(|&index| index < prefs.len())
                .or_else(|| {
                    if dense.is_none() {
                        prefs.binary_search_by_key(&person, |p| p.person).ok()
                    } else {
                        None
                    }
                })
                .map(|index| prefs[index])
        };
        let index = opinions
            .filter(|opinions| !opinions.records.is_empty())
            .map(Opinions::index);
        let initial_trust = opinions.map_or(0, |opinions| opinions.rules.initial);
        let mut eligible: Vec<_> = self
            .pending
            .par_iter()
            .filter(|row| {
                row.expires >= tick
                    && busy.binary_search(&row.worker).is_err()
                    && person_index(people, row.worker)
                        .is_some_and(|i| people[i].1 == Some(row.region) && people[i].2 > 0)
                    && preference(row.worker).is_some()
            })
            .collect();
        if eligible
            .par_windows(2)
            .any(|pair| (pair[0].worker, pair[0].id) > (pair[1].worker, pair[1].id))
        {
            eligible.par_sort_unstable_by_key(|row| (row.worker, row.id));
        }
        let decide = |row: &Request| {
            let preference = preference(row.worker).expect("eligible preference");
            let (fear, obligation) = index
                .as_ref()
                .map_or((0, 0), |index| index.feelings(row.worker, row.requester));
            let affinity = if preference.affinity_weight == 0 {
                0
            } else {
                index
                    .as_ref()
                    .map_or(0, |index| index.affinity(row.worker, row.requester))
            };
            let trust = if preference.trust_weight == 0 {
                0
            } else {
                index.as_ref().map_or(initial_trust, |index| {
                    index.trust(row.worker, row.requester)
                })
            };
            Decision {
                request: row.id,
                worker: row.worker,
                region: row.region,
                recipe: row.recipe,
                help: preference.motivated(
                    fear,
                    obligation,
                    affinity,
                    trust,
                    preference
                        .offer_value
                        .as_ref()
                        .and_then(|value| {
                            offers.map(|index| {
                                index.quantity_for(
                                    row.worker,
                                    row.requester,
                                    row.recipe,
                                    value.good,
                                )
                            })
                        })
                        .unwrap_or(0),
                ),
                disregard_claims: preference.disregard_claims,
            }
        };
        let decisions = if eligible
            .par_windows(2)
            .any(|pair| pair[0].worker == pair[1].worker)
        {
            eligible
                .par_chunk_by(|a, b| a.worker == b.worker)
                .map(|rows| decide(rows[0]))
                .collect()
        } else {
            eligible.par_iter().map(|row| decide(row)).collect()
        };
        let expired = self
            .pending
            .par_iter()
            .filter(|row| row.expires < tick)
            .map(|row| Event {
                tick,
                request: row.id,
                worker: row.worker,
                outcome: Outcome::Expired,
            })
            .collect();
        Draft { decisions, expired }
    }
}
#[cfg(test)]
extern crate self as game_work_requests;
#[cfg(test)]
mod preference_tests {
    use super::{OfferIndex, OfferValue, PracticeOffer, Preference, Requests};
    use game_agreements::{Agreements, Contract, Status};

    #[test]
    fn signed_personal_affinity_can_raise_or_lower_the_value_of_help() {
        let mut preference = Preference {
            person: 1,
            effort: 5,
            help_value: 0,
            affinity_weight: 3,
            trust_weight: 0,
            offer_value: None,
            disregard_claims: false,
        };
        assert!(preference.motivated(0, 0, 2, 0, 0));
        assert!(!preference.motivated(0, 0, -2, 0, 0));
        preference.affinity_weight = -3;
        assert!(preference.motivated(0, 0, -2, 0, 0));
        assert!(!preference.motivated(0, 0, 2, 0, 0));
        assert!(preference.motivated(u64::MAX, u64::MAX, i32::MIN, 0, 0));
        preference.affinity_weight = 0;
        assert!(!preference.motivated(0, 0, i32::MAX, 0, 0));
    }

    #[test]
    fn outstanding_offers_are_addressed_and_expire_without_imposing_value() {
        let preference = Preference {
            person: 2,
            effort: 5,
            help_value: 0,
            affinity_weight: 0,
            trust_weight: 0,
            offer_value: Some(OfferValue {
                good: 1,
                value_per_unit: 3,
                max_quantity: 2,
            }),
            disregard_claims: false,
        };
        let requests = Requests {
            next_id: 1,
            inherit_births: false,
            memory_limit: 0,
            work_links: vec![],
            memories: vec![],
            preferences: vec![preference.clone()],
            askers: vec![],
            pending: vec![],
            events: vec![],
        };
        let contract = |id, creditor, good, quantity, delivered, due, status| Contract {
            id,
            debtor: 1,
            creditor,
            good,
            quantity,
            delivered,
            due,
            status,
        };
        let agreements = Agreements {
            next_id: 10,
            policies: vec![],
            offering: vec![],
            work_offering: vec![],
            bulk_debtors: vec![],
            events: vec![],
            contracts: vec![
                contract(1, 2, 1, 3, 1, 6, Status::Proposed),
                contract(2, 2, 1, 4, 3, 5, Status::Active),
                contract(3, 2, 1, 99, 0, 5, Status::Proposed),
                contract(4, 2, 1, 99, 0, 4, Status::Active),
                contract(5, 2, 1, 99, 0, 6, Status::Withdrawn),
                contract(6, 2, 1, 99, 0, 6, Status::Declined),
                contract(7, 2, 1, 99, 0, 6, Status::Defaulted),
                contract(8, 2, 1, 99, 99, 6, Status::Fulfilled),
                contract(9, 3, 1, 7, 0, 6, Status::Proposed),
                contract(10, 2, 2, 8, 0, 6, Status::Proposed),
            ],
        };
        let index = requests.offer_index(Some(&agreements), 5).unwrap();
        assert_eq!(index.quantity(2, 1, 1), 3);
        assert_eq!(index.quantity(3, 1, 1), 7);
        assert_eq!(index.quantity(2, 1, 2), 8);
        assert_eq!(index.quantity(1, 2, 1), 0);
        assert!(preference.motivated(0, 0, 0, 0, index.quantity(2, 1, 1)));
        let index = OfferIndex::with_practices(
            Some(index),
            vec![
                PracticeOffer {
                    worker: 2,
                    requester: 1,
                    recipe: 9,
                    good: 1,
                    quantity: 4,
                },
                PracticeOffer {
                    worker: 2,
                    requester: 1,
                    recipe: 9,
                    good: 1,
                    quantity: 2,
                },
            ],
        )
        .unwrap();
        assert_eq!(index.quantity_for(2, 1, 9, 1), 7);
        assert_eq!(index.quantity_for(2, 1, 8, 1), 3);
        assert_eq!(index.quantity_for(3, 1, 9, 1), 7);
        assert!(!preference.motivated(0, 0, 0, 0, 1));
        let mut dislike = preference;
        dislike.help_value = 5;
        dislike.offer_value.as_mut().unwrap().value_per_unit = -1;
        assert!(!dislike.motivated(0, 0, 0, 0, 1));
    }
}
#[cfg(test)]
#[path = "../tests/support/experience.rs"]
mod experience_tests;

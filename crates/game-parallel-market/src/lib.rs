//! Explicit proposals and recipient decisions for physical exchange.
mod learning;
use game_parallel_production_model::{Amount, PersonSnapshot, person_index};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Market {
    pub next_id: u64,
    #[serde(default)]
    pub policies: Vec<Policy>,
    #[serde(default)]
    pub offers: Vec<Offer>,
    #[serde(default)]
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub id: u64,
    pub proposer: u64,
    pub recipient: u64,
    pub give: SmallVec<[Amount; 1]>,
    pub receive: SmallVec<[Amount; 1]>,
    pub proposed_at: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub offer: u64,
    pub actor: u64,
    pub outcome: Outcome,
    /// Private incentives considered by an automatic recipient, when recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub influence: Option<Influence>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Accepted,
    Declined,
    Withdrawn,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub enabled: bool,
    #[serde(default)]
    pub propose: bool,
    /// The person may ask a local holder for a gift, without promising a return.
    #[serde(default)]
    pub request_help: bool,
    #[serde(default)]
    pub min_gain: u64,
    /// Personal value placed on helping with an uncompensated request.
    #[serde(default)]
    pub help_value: u64,
    pub values: Vec<Value>,
    /// Subjective incentives toward specific people; these do not imply approval.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub influences: Vec<Influence>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Influence {
    pub person: u64,
    #[serde(default)]
    pub attachment: u64,
    #[serde(default)]
    pub obligation: u64,
    #[serde(default)]
    pub fear: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Value {
    pub good: u32,
    pub value: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub proposer: u64,
    pub recipient: u64,
    pub give: Vec<Amount>,
    pub receive: Vec<Amount>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub id: u64,
    pub actor: u64,
    pub outcome: Outcome,
}
fn items_valid(items: &[Amount], goods: &HashSet<u32>) -> bool {
    items.iter().enumerate().all(|(i, item)| {
        item.quantity > 0
            && goods.contains(&item.good)
            && !items[..i].iter().any(|other| other.good == item.good)
    })
}
impl Market {
    pub fn gift_pairs(offers: &[Offer], decisions: &[Decision]) -> Vec<(u64, u64)> {
        learning::gifts(offers, decisions)
    }
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        tick: u64,
    ) -> Result<(), String> {
        self.validate_with(|person| people.contains(&person), goods, tick)
    }
    pub fn validate_index(
        &self,
        people: &[PersonSnapshot],
        goods: &HashSet<u32>,
        tick: u64,
    ) -> Result<(), String> {
        self.validate_with(|person| person_index(people, person).is_some(), goods, tick)
    }
    fn validate_with(
        &self,
        contains: impl Fn(u64) -> bool + Sync,
        goods: &HashSet<u32>,
        tick: u64,
    ) -> Result<(), String> {
        let mut policies: Vec<_> = self
            .policies
            .par_iter()
            .map(|policy| policy.person)
            .collect();
        if policies.par_windows(2).any(|pair| pair[0] > pair[1]) {
            policies.par_sort_unstable();
        }
        if policies.par_windows(2).any(|pair| pair[0] == pair[1])
            || self.policies.par_iter().any(|policy| {
                !contains(policy.person)
                    || {
                        let mut seen = HashSet::new();
                        policy.influences.iter().any(|influence| {
                            influence.person == policy.person
                                || !contains(influence.person)
                                || !seen.insert(influence.person)
                        })
                    }
                    || policy.values.iter().enumerate().any(|(i, value)| {
                        !goods.contains(&value.good)
                            || policy.values[..i]
                                .iter()
                                .any(|other| other.good == value.good)
                    })
            })
        {
            return Err("invalid market policy".into());
        }
        let mut ids: Vec<_> = self.offers.par_iter().map(|offer| offer.id).collect();
        if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
            ids.par_sort_unstable();
        }
        if ids.par_windows(2).any(|pair| pair[0] == pair[1])
            || self.offers.par_iter().any(|offer| {
                offer.id >= self.next_id
                    || offer.proposer == offer.recipient
                    || !contains(offer.proposer)
                    || !contains(offer.recipient)
                    || offer.proposed_at > tick
                    || (offer.give.is_empty() && offer.receive.is_empty())
                    || !items_valid(&offer.give, goods)
                    || !items_valid(&offer.receive, goods)
            })
            || self.events.par_iter().any(|event| {
                event.tick > tick
                    || event.offer >= self.next_id
                    || !contains(event.actor)
                    || event.influence.as_ref().is_some_and(|influence| {
                        influence.person == event.actor || !contains(influence.person)
                    })
            })
        {
            return Err("invalid market state".into());
        }
        Ok(())
    }
    /// Assign stable IDs in request order; all offers are prepared together.
    pub fn propose_batch(
        &self,
        proposals: &[Proposal],
        tick: u64,
    ) -> Result<(Self, Vec<u64>), String> {
        let count = u64::try_from(proposals.len()).map_err(|_| "offer count overflow")?;
        let next_id = self.next_id.checked_add(count).ok_or("offer id overflow")?;
        let ids: Vec<_> = (self.next_id..next_id).collect();
        let new: Vec<_> = proposals
            .par_iter()
            .zip(ids.par_iter())
            .map(|(proposal, &id)| Offer {
                id,
                proposer: proposal.proposer,
                recipient: proposal.recipient,
                give: proposal.give.iter().cloned().collect(),
                receive: proposal.receive.iter().cloned().collect(),
                proposed_at: tick,
            })
            .collect();
        let mut offers: Vec<_> = self.offers.par_iter().cloned().collect();
        offers.extend(new);
        Ok((
            Self {
                next_id,
                policies: self.policies.par_iter().cloned().collect(),
                offers,
                events: self.events.par_iter().cloned().collect(),
            },
            ids,
        ))
    }
    /// Validate every decision before removing any offer.
    pub fn decide_batch(
        &self,
        decisions: &[Decision],
        tick: u64,
    ) -> Result<(Self, Vec<Offer>), String> {
        let mut ids: Vec<_> = decisions.par_iter().map(|decision| decision.id).collect();
        if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
            ids.par_sort_unstable();
        }
        if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("duplicate offer decision".into());
        }
        let mut offers: Vec<_> = self.offers.par_iter().collect();
        offers.par_sort_unstable_by_key(|offer| offer.id);
        let dense_start = offers.first().and_then(|first| {
            offers.last().and_then(|last| {
                (last.id.checked_sub(first.id) == Some((offers.len() - 1) as u64))
                    .then_some(first.id)
            })
        });
        let chosen: Vec<_> = decisions
            .par_iter()
            .map(|decision| -> Result<Offer, String> {
                let index = dense_start
                    .and_then(|first| decision.id.checked_sub(first))
                    .and_then(|offset| usize::try_from(offset).ok())
                    .filter(|&index| index < offers.len())
                    .or_else(|| {
                        if dense_start.is_none() {
                            offers
                                .binary_search_by_key(&decision.id, |offer| offer.id)
                                .ok()
                        } else {
                            None
                        }
                    })
                    .ok_or("unknown offer")?;
                let offer = offers[index];
                if match decision.outcome {
                    Outcome::Accepted | Outcome::Declined => decision.actor != offer.recipient,
                    Outcome::Withdrawn => decision.actor != offer.proposer,
                } {
                    return Err("actor cannot decide this offer".into());
                }
                Ok(offer.clone())
            })
            .collect::<Result<_, _>>()?;
        let remaining: Vec<_> = self
            .offers
            .par_iter()
            .filter(|offer| ids.binary_search(&offer.id).is_err())
            .cloned()
            .collect();
        let mut events: Vec<_> = self.events.par_iter().cloned().collect();
        events.extend(
            decisions
                .par_iter()
                .map(|d| Event {
                    tick,
                    offer: d.id,
                    actor: d.actor,
                    outcome: d.outcome,
                    influence: None,
                })
                .collect::<Vec<_>>(),
        );
        Ok((
            Self {
                next_id: self.next_id,
                policies: self.policies.par_iter().cloned().collect(),
                offers: remaining,
                events,
            },
            chosen,
        ))
    }
}
/// A compact decision delta prepared against an unchanged market snapshot.
pub struct DecisionDraft {
    ids: Vec<u64>,
    events: Vec<Event>,
    gifts: Vec<(u64, u64)>,
}
impl DecisionDraft {
    pub fn events(&self) -> &[Event] {
        &self.events
    }
    pub fn gifts(&self) -> &[(u64, u64)] {
        &self.gifts
    }
    pub fn record_influences(&mut self, influences: &[(u64, Influence)]) {
        self.events.par_iter_mut().for_each(|event| {
            if let Ok(index) = influences.binary_search_by_key(&event.offer, |row| row.0) {
                event.influence = Some(influences[index].1.clone());
            }
        });
    }
    pub fn commit(self, market: &mut Market) {
        market.offers = std::mem::take(&mut market.offers)
            .into_par_iter()
            .filter(|offer| self.ids.binary_search(&offer.id).is_err())
            .collect();
        market.events.extend(self.events);
    }
}
impl Market {
    pub fn stage_decisions(
        &self,
        decisions: &[Decision],
        tick: u64,
        track_help: bool,
    ) -> Result<DecisionDraft, String> {
        let mut ids: Vec<_> = decisions.par_iter().map(|decision| decision.id).collect();
        if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
            ids.par_sort_unstable();
        }
        if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("duplicate offer decision".into());
        }
        let mut offers: Vec<_> = self.offers.par_iter().collect();
        if offers.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            offers.par_sort_unstable_by_key(|offer| offer.id);
        }
        let dense = offers.first().and_then(|first| {
            offers.last().and_then(|last| {
                (last.id.checked_sub(first.id) == Some((offers.len() - 1) as u64))
                    .then_some(first.id)
            })
        });
        if decisions.par_iter().any(|decision| {
            let index = dense
                .and_then(|first| decision.id.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < offers.len())
                .or_else(|| {
                    if dense.is_none() {
                        offers
                            .binary_search_by_key(&decision.id, |offer| offer.id)
                            .ok()
                    } else {
                        None
                    }
                });
            let Some(index) = index else {
                return true;
            };
            match decision.outcome {
                Outcome::Accepted | Outcome::Declined => decision.actor != offers[index].recipient,
                Outcome::Withdrawn => decision.actor != offers[index].proposer,
            }
        }) {
            return Err("invalid offer decision".into());
        }
        let gifts = if track_help {
            let selected: Vec<_> = decisions
                .par_iter()
                .map(|decision| {
                    let index = offers
                        .binary_search_by_key(&decision.id, |offer| offer.id)
                        .expect("validated offer");
                    offers[index].clone()
                })
                .collect();
            learning::gifts(&selected, decisions)
        } else {
            Vec::new()
        };
        let events = decisions
            .par_iter()
            .map(|decision| Event {
                tick,
                offer: decision.id,
                actor: decision.actor,
                outcome: decision.outcome,
                influence: None,
            })
            .collect();
        Ok(DecisionDraft { ids, events, gifts })
    }
}

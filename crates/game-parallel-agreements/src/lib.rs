//! Agreements for future delivery; custody changes are owned by the host.
pub mod runtime;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agreements {
    pub next_id: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policies: Vec<Policy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub offering: Vec<OfferPolicy>,
    /// People who can consider multiple deliveries from their own custody in one tick.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bulk_debtors: Vec<u64>,
    #[serde(default)]
    pub contracts: Vec<Contract>,
    #[serde(default)]
    pub events: Vec<Event>,
}
impl Clone for Agreements {
    fn clone(&self) -> Self {
        Self {
            next_id: self.next_id,
            policies: self.policies.par_iter().cloned().collect(),
            offering: self.offering.par_iter().cloned().collect(),
            bulk_debtors: self.bulk_debtors.par_iter().copied().collect(),
            contracts: self.contracts.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
        }
    }
}
/// One person's inclination to offer a revocable future delivery to another.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfferPolicy {
    pub person: u64,
    pub recipient: u64,
    pub good: u32,
    pub quantity: u64,
    pub due_after: u64,
    pub retry_ticks: u64,
    pub willing: bool,
    #[serde(default)]
    pub recipient_hunger_at_least: u32,
    #[serde(default)]
    pub minimum_trust: Option<i32>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
    #[serde(default)]
    pub next_review: u64,
    /// Require personally observed, newly completed requested work before proposing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_work_recipe: Option<u32>,
    /// Last observed completion already considered by this person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_work_offer_at: Option<u64>,
}
/// Personal consent and fulfillment choices for automatic agreements.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    #[serde(default)]
    pub honor: bool,
    #[serde(default)]
    pub accept: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_trust: Option<i32>,
    /// A debtor may condition honoring a promise on their trust in its creditor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_honor_trust: Option<i32>,
    /// Enough directed fear can motivate delivery even without ordinary willingness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fear_honor_threshold: Option<u64>,
    /// A personal felt obligation can independently motivate delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation_honor_threshold: Option<u64>,
    /// Fear or felt obligation can motivate outward acceptance without private approval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accept_pressure_threshold: Option<u64>,
}
impl Policy {
    /// The person chooses whether to deliver; the agreement itself does not compel it.
    pub fn honors(&self, trust: Option<i32>, fear: Option<u64>) -> bool {
        self.honors_with_feelings(trust, fear, None)
    }
    pub fn honors_with_feelings(
        &self,
        trust: Option<i32>,
        fear: Option<u64>,
        obligation: Option<u64>,
    ) -> bool {
        (self.honor
            && self
                .minimum_honor_trust
                .is_none_or(|limit| trust.is_some_and(|value| value >= limit)))
            || self
                .fear_honor_threshold
                .is_some_and(|limit| fear.is_some_and(|value| value >= limit))
            || self
                .obligation_honor_threshold
                .is_some_and(|limit| obligation.is_some_and(|value| value >= limit))
    }
    pub fn accepts(&self, trust: Option<i32>, fear: Option<u64>, obligation: Option<u64>) -> bool {
        (self.accept
            && self
                .minimum_trust
                .is_none_or(|limit| trust.is_some_and(|value| value >= limit)))
            || self.accept_pressure_threshold.is_some_and(|limit| {
                fear.unwrap_or(0).saturating_add(obligation.unwrap_or(0)) >= limit
            })
    }
    pub fn automatic(&self) -> bool {
        self.honor
            || self.accept
            || self.fear_honor_threshold.is_some()
            || self.obligation_honor_threshold.is_some()
            || self.accept_pressure_threshold.is_some()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub id: u64,
    pub debtor: u64,
    pub creditor: u64,
    pub good: u32,
    pub quantity: u64,
    pub delivered: u64,
    pub due: u64,
    pub status: Status,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Proposed,
    Active,
    Fulfilled,
    Declined,
    Withdrawn,
    Defaulted,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub contract: u64,
    pub actor: u64,
    pub kind: EventKind,
    pub quantity: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Proposed,
    Accepted,
    Declined,
    Withdrawn,
    Delivered,
    Fulfilled,
    Defaulted,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub debtor: u64,
    pub creditor: u64,
    pub good: u32,
    pub quantity: u64,
    pub due: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub contract: u64,
    pub actor: u64,
    pub accept: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub contract: u64,
    pub actor: u64,
    pub quantity: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transfer {
    pub debtor: u64,
    pub creditor: u64,
    pub good: u32,
    pub quantity: u64,
}
/// The host must apply every transfer successfully before publishing `state`.
pub struct DeliveryDraft {
    pub state: Agreements,
    pub transfers: Vec<Transfer>,
}

impl Agreements {
    /// Remove authored relationships when creating a new population.
    pub fn reset_for_generation(&mut self) {
        self.next_id = 1;
        self.policies.clear();
        self.offering.clear();
        self.bulk_debtors.clear();
        self.contracts.clear();
        self.events.clear();
    }

    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        now: u64,
    ) -> Result<(), &'static str> {
        let mut policies: Vec<_> = self
            .policies
            .par_iter()
            .map(|policy| policy.person)
            .collect();
        policies.par_sort_unstable();
        let mut offers: Vec<_> = self.offering.par_iter().map(|offer| offer.person).collect();
        offers.par_sort_unstable();
        if offers.par_windows(2).any(|pair| pair[0] == pair[1])
            || self.offering.par_iter().any(|p| {
                !people.contains(&p.person)
                    || !people.contains(&p.recipient)
                    || p.person == p.recipient
                    || !goods.contains(&p.good)
                    || p.quantity == 0
                    || p.due_after == 0
                    || p.retry_ticks == 0
                    || p.pressure_threshold == Some(0)
                    || p.last_work_offer_at
                        .is_some_and(|t| t > now || p.after_work_recipe.is_none())
            })
            || policies.par_windows(2).any(|pair| pair[0] == pair[1])
            || policies.par_iter().any(|person| !people.contains(person))
            || self.policies.par_iter().any(|policy| {
                policy.fear_honor_threshold == Some(0)
                    || policy.obligation_honor_threshold == Some(0)
                    || policy.accept_pressure_threshold == Some(0)
            })
        {
            return Err("invalid agreement policy");
        }
        let mut bulk = self.bulk_debtors.clone();
        bulk.par_sort_unstable();
        let mut honoring: Vec<_> = self
            .policies
            .par_iter()
            .filter(|policy| {
                policy.honor
                    || policy.fear_honor_threshold.is_some()
                    || policy.obligation_honor_threshold.is_some()
            })
            .map(|policy| policy.person)
            .collect();
        honoring.par_sort_unstable();
        if bulk.par_windows(2).any(|pair| pair[0] == pair[1])
            || bulk
                .par_iter()
                .any(|person| honoring.binary_search(person).is_err())
        {
            return Err("invalid bulk agreement authorization");
        }
        let mut ids: Vec<_> = self.contracts.par_iter().map(|row| row.id).collect();
        ids.par_sort_unstable();
        if self.next_id == 0
            || ids.par_windows(2).any(|w| w[0] == w[1])
            || self.contracts.par_iter().any(|row| {
                row.id == 0
                    || row.id >= self.next_id
                    || row.debtor == row.creditor
                    || !people.contains(&row.debtor)
                    || !people.contains(&row.creditor)
                    || !goods.contains(&row.good)
                    || row.quantity == 0
                    || row.delivered > row.quantity
                    || (row.status == Status::Fulfilled && row.delivered != row.quantity)
                    || (matches!(
                        row.status,
                        Status::Proposed | Status::Active | Status::Declined | Status::Withdrawn
                    ) && row.delivered == row.quantity)
            })
            || self.events.par_iter().any(|event| {
                event.tick > now
                    || ids.binary_search(&event.contract).is_err()
                    || !people.contains(&event.actor)
            })
        {
            return Err("invalid agreement state");
        }
        Ok(())
    }
    /// Reserve IDs and create proposals as a single all-or-nothing command.
    pub fn propose(
        &self,
        proposals: &[Proposal],
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        now: u64,
    ) -> Result<(Self, Vec<u64>), &'static str> {
        if proposals.is_empty() {
            return Ok((self.clone(), Vec::new()));
        }
        if proposals.par_iter().any(|p| {
            p.debtor == p.creditor
                || !people.contains(&p.debtor)
                || !people.contains(&p.creditor)
                || !goods.contains(&p.good)
                || p.quantity == 0
                || p.due <= now
        }) {
            return Err("invalid agreement proposal");
        }
        let mut state = self.clone();
        state.events.clear();
        let ids = state.append_validated(proposals, now)?;
        Ok((state, ids))
    }
    /// Append host-validated personal proposals to a private draft without replacing its events.
    pub fn append_validated(
        &mut self,
        proposals: &[Proposal],
        now: u64,
    ) -> Result<Vec<u64>, &'static str> {
        if proposals.is_empty() {
            return Ok(Vec::new());
        }
        let end = self
            .next_id
            .checked_add(u64::try_from(proposals.len()).map_err(|_| "agreement count overflow")?)
            .ok_or("agreement id overflow")?;
        let ids: Vec<_> = (self.next_id..end).into_par_iter().collect();
        let added: Vec<_> = proposals
            .par_iter()
            .zip(ids.par_iter())
            .map(|(p, &id)| Contract {
                id,
                debtor: p.debtor,
                creditor: p.creditor,
                good: p.good,
                quantity: p.quantity,
                delivered: 0,
                due: p.due,
                status: Status::Proposed,
            })
            .collect();
        let events: Vec<_> = proposals
            .par_iter()
            .zip(ids.par_iter())
            .map(|(p, &id)| Event {
                tick: now,
                contract: id,
                actor: p.debtor,
                kind: EventKind::Proposed,
                quantity: 0,
            })
            .collect();
        self.contracts.par_extend(added);
        self.events.par_extend(events);
        self.next_id = end;
        Ok(ids)
    }
    /// Only the creditor can accept or decline; the debtor can withdraw.
    pub fn decide(&self, decisions: &[Decision], now: u64) -> Result<Self, &'static str> {
        let mut next = self.clone();
        next.decide_in_place(decisions, now)?;
        Ok(next)
    }
    /// Validate every choice before modifying the host's private draft.
    pub fn decide_in_place(
        &mut self,
        decisions: &[Decision],
        now: u64,
    ) -> Result<(), &'static str> {
        if decisions.is_empty() {
            return Ok(());
        }
        let mut requests = decisions.to_vec();
        requests.par_sort_unstable_by_key(|d| d.contract);
        if requests
            .par_windows(2)
            .any(|w| w[0].contract == w[1].contract)
        {
            return Err("duplicate agreement decision");
        }
        let ordered = !self.contracts.par_windows(2).any(|w| w[0].id > w[1].id);
        let dense = ordered
            .then(|| {
                self.contracts
                    .first()
                    .zip(self.contracts.last())
                    .and_then(|(first, last)| {
                        (last.id.checked_sub(first.id) == Some((self.contracts.len() - 1) as u64))
                            .then_some(first.id)
                    })
            })
            .flatten();
        let index = (!ordered).then(|| {
            let mut rows: Vec<_> = self
                .contracts
                .par_iter()
                .enumerate()
                .map(|(i, c)| (c.id, i))
                .collect();
            rows.par_sort_unstable();
            rows
        });
        let rows: Vec<_> = requests
            .par_iter()
            .map(|d| -> Result<_, &'static str> {
                let slot = dense
                    .and_then(|first| d.contract.checked_sub(first))
                    .and_then(|offset| usize::try_from(offset).ok())
                    .filter(|&i| i < self.contracts.len())
                    .or_else(|| {
                        if ordered && dense.is_none() {
                            self.contracts
                                .binary_search_by_key(&d.contract, |c| c.id)
                                .ok()
                        } else if !ordered {
                            index.as_ref().and_then(|rows| {
                                rows.binary_search_by_key(&d.contract, |r| r.0)
                                    .ok()
                                    .map(|i| rows[i].1)
                            })
                        } else {
                            None
                        }
                    })
                    .ok_or("unknown agreement")?;
                let contract = &self.contracts[slot];
                if contract.status != Status::Proposed || contract.due <= now {
                    return Err("agreement is unavailable");
                }
                let (status, kind) = if d.actor == contract.creditor {
                    if d.accept {
                        (Status::Active, EventKind::Accepted)
                    } else {
                        (Status::Declined, EventKind::Declined)
                    }
                } else if d.actor == contract.debtor && !d.accept {
                    (Status::Withdrawn, EventKind::Withdrawn)
                } else {
                    return Err("actor cannot decide agreement");
                };
                Ok((
                    slot,
                    status,
                    Event {
                        tick: now,
                        contract: contract.id,
                        actor: d.actor,
                        kind,
                        quantity: 0,
                    },
                ))
            })
            .collect::<Result<_, _>>()?;
        let events = rows.par_iter().map(|row| row.2.clone()).collect();
        let mut patches: Vec<_> = rows.into_par_iter().map(|row| (row.0, row.1)).collect();
        patches.par_sort_unstable_by_key(|row| row.0);
        self.contracts
            .par_chunks_mut(1024)
            .enumerate()
            .for_each(|(block, chunk)| {
                let base = block * 1024;
                let start = patches.partition_point(|row| row.0 < base);
                let end = patches.partition_point(|row| row.0 < base + chunk.len());
                for &(slot, status) in &patches[start..end] {
                    chunk[slot - base].status = status;
                }
            });
        self.events = events;
        Ok(())
    }
    /// Stage deliveries; the caller must perform their physical transfers atomically.
    pub fn deliver(&self, requests: &[Delivery], now: u64) -> Result<DeliveryDraft, &'static str> {
        if requests.is_empty() {
            return Ok(DeliveryDraft {
                state: self.clone(),
                transfers: Vec::new(),
            });
        }
        let mut requests = requests.to_vec();
        requests.par_sort_unstable_by_key(|d| d.contract);
        if requests
            .par_windows(2)
            .any(|w| w[0].contract == w[1].contract)
        {
            return Err("duplicate agreement delivery");
        }
        let mut index: Vec<_> = self.contracts.par_iter().map(|c| (c.id, c)).collect();
        index.par_sort_unstable_by_key(|row| row.0);
        let rows: Vec<_> = requests
            .par_iter()
            .map(|request| -> Result<_, &'static str> {
                let i = index
                    .binary_search_by_key(&request.contract, |row| row.0)
                    .map_err(|_| "unknown agreement")?;
                let c = index[i].1;
                let remaining = c
                    .quantity
                    .checked_sub(c.delivered)
                    .ok_or("invalid agreement state")?;
                if c.status != Status::Active
                    || request.actor != c.debtor
                    || now > c.due
                    || request.quantity == 0
                    || request.quantity > remaining
                {
                    return Err("invalid agreement delivery");
                }
                Ok((
                    c.id,
                    c.delivered + request.quantity,
                    Transfer {
                        debtor: c.debtor,
                        creditor: c.creditor,
                        good: c.good,
                        quantity: request.quantity,
                    },
                    Event {
                        tick: now,
                        contract: c.id,
                        actor: c.debtor,
                        kind: if c.delivered + request.quantity == c.quantity {
                            EventKind::Fulfilled
                        } else {
                            EventKind::Delivered
                        },
                        quantity: request.quantity,
                    },
                ))
            })
            .collect::<Result<_, _>>()?;
        let mut state = self.clone();
        state.contracts.par_iter_mut().for_each(|contract| {
            if let Ok(i) = rows.binary_search_by_key(&contract.id, |row| row.0) {
                contract.delivered = rows[i].1;
                if contract.delivered == contract.quantity {
                    contract.status = Status::Fulfilled;
                }
            }
        });
        state.events = rows.par_iter().map(|row| row.3.clone()).collect();
        let transfers = rows.into_par_iter().map(|row| row.2).collect();
        Ok(DeliveryDraft { state, transfers })
    }
    /// Unsettled commitments default after their deadline; proposals lapse.
    pub fn advance(&self, tick: u64) -> Self {
        let mut next = self.clone();
        next.advance_in_place(tick);
        next
    }
    /// Publish deadline outcomes after all fallible tick stages succeed.
    pub fn advance_in_place(&mut self, tick: u64) {
        let events: Vec<_> = self
            .contracts
            .par_iter_mut()
            .filter_map(|contract| {
                if contract.due < tick
                    && matches!(contract.status, Status::Active | Status::Proposed)
                {
                    let active = contract.status == Status::Active;
                    contract.status = if active {
                        Status::Defaulted
                    } else {
                        Status::Withdrawn
                    };
                    Some(Event {
                        tick,
                        contract: contract.id,
                        actor: contract.debtor,
                        kind: if active {
                            EventKind::Defaulted
                        } else {
                            EventKind::Withdrawn
                        },
                        quantity: contract.quantity.saturating_sub(contract.delivered),
                    })
                } else {
                    None
                }
            })
            .collect();
        self.events = events;
        self.events.par_sort_unstable_by_key(|event| event.contract);
    }
}

#[cfg(test)]
#[path = "../tests/support/unit.rs"]
mod tests;

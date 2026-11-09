//! Personal, non-binding requests for a particular cultivation action.
use crate::{Context, Phase};
use game_lots::TransactionOutcome;
use game_opinions::Index;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asker {
    pub person: u64,
    pub worker: u64,
    pub field: u64,
    pub interval: u64,
    pub lifetime: u64,
}

/// A personal inclination to look for a local field and a potential helper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discoverer {
    pub person: u64,
    pub interval: u64,
    pub lifetime: u64,
}

/// A person's own rule for reconsidering requests and waiting before retrying.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WithdrawalPreference {
    pub person: u64,
    pub affinity_below: i32,
    pub retry_after: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryBackoff {
    pub person: u64,
    pub until: u64,
}

/// A recipient's personal valuation of an outstanding promise for one good.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfferValue {
    pub person: u64,
    pub good: u32,
    pub value_per_unit: i64,
    pub max_quantity: u64,
}

/// Latest personally witnessed requested cultivation for one outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkMemory {
    pub observer: u64,
    pub worker: u64,
    pub success: bool,
    pub tick: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub help_value: i64,
    pub effort: u64,
    #[serde(default)]
    pub affinity_weight: i64,
    #[serde(default)]
    pub trust_weight: i64,
    #[serde(default)]
    pub fear_weight: i64,
    #[serde(default)]
    pub obligation_weight: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: u64,
    pub requester: u64,
    pub worker: u64,
    pub field: u64,
    pub phase: Phase,
    pub created_at: u64,
    pub expires: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Refused,
    Expired,
    Stale,
    Fulfilled,
    Failed,
    Withdrawn,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub request: u64,
    pub outcome: Outcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requests {
    pub next_id: u64,
    #[serde(default)]
    pub askers: Vec<Asker>,
    #[serde(default)]
    pub discoverers: Vec<Discoverer>,
    #[serde(default)]
    pub withdrawal_preferences: Vec<WithdrawalPreference>,
    #[serde(default)]
    pub retry_backoffs: Vec<RetryBackoff>,
    #[serde(default)]
    pub offer_values: Vec<OfferValue>,
    #[serde(default)]
    pub work_memories: Vec<WorkMemory>,
    #[serde(default)]
    pub preferences: Vec<Preference>,
    #[serde(default)]
    pub pending: Vec<Request>,
    /// Outcomes from the latest boundary only.
    #[serde(default)]
    pub events: Vec<Event>,
}

impl Requests {
    pub fn validate(&self, people: &HashSet<u64>, fields: &[u64]) -> Result<(), String> {
        let mut askers: Vec<_> = self
            .askers
            .par_iter()
            .map(|a| a.person)
            .chain(self.discoverers.par_iter().map(|a| a.person))
            .collect();
        let mut preferences: Vec<_> = self.preferences.par_iter().map(|a| a.person).collect();
        let mut withdrawal_preferences: Vec<_> = self
            .withdrawal_preferences
            .par_iter()
            .map(|row| row.person)
            .collect();
        let mut retry_backoffs: Vec<_> = self
            .retry_backoffs
            .par_iter()
            .map(|row| row.person)
            .collect();
        let mut offer_values: Vec<_> = self.offer_values.par_iter().map(|row| row.person).collect();
        let mut work_memories: Vec<_> = self
            .work_memories
            .par_iter()
            .map(|row| (row.observer, row.success))
            .collect();
        let mut ids: Vec<_> = self.pending.par_iter().map(|r| r.id).collect();
        askers.par_sort_unstable();
        preferences.par_sort_unstable();
        withdrawal_preferences.par_sort_unstable();
        retry_backoffs.par_sort_unstable();
        offer_values.par_sort_unstable();
        work_memories.par_sort_unstable();
        ids.par_sort_unstable();
        if self.next_id == 0
            || ids.last().is_some_and(|id| *id >= self.next_id)
            || ids.first() == Some(&0)
            || ids.par_windows(2).any(|p| p[0] == p[1])
            || askers.par_windows(2).any(|p| p[0] == p[1])
            || preferences.par_windows(2).any(|p| p[0] == p[1])
            || withdrawal_preferences.par_windows(2).any(|p| p[0] == p[1])
            || retry_backoffs.par_windows(2).any(|p| p[0] == p[1])
            || offer_values.par_windows(2).any(|p| p[0] == p[1])
            || work_memories.par_windows(2).any(|p| p[0] == p[1])
            || self.work_memories.par_iter().any(|row| {
                row.observer == row.worker
                    || !people.contains(&row.observer)
                    || !people.contains(&row.worker)
                    || row.tick == 0
            })
            || self
                .offer_values
                .par_iter()
                .any(|row| !people.contains(&row.person))
            || self
                .withdrawal_preferences
                .par_iter()
                .any(|row| !people.contains(&row.person) || row.retry_after == 0)
            || self
                .retry_backoffs
                .par_iter()
                .any(|row| !people.contains(&row.person) || row.until == 0)
            || self
                .preferences
                .par_iter()
                .any(|p| !people.contains(&p.person))
            || self.askers.par_iter().any(|a| {
                !people.contains(&a.person)
                    || !people.contains(&a.worker)
                    || a.person == a.worker
                    || fields.binary_search(&a.field).is_err()
                    || a.interval == 0
                    || a.lifetime == 0
            })
            || self
                .discoverers
                .par_iter()
                .any(|a| !people.contains(&a.person) || a.interval == 0 || a.lifetime == 0)
            || self.pending.par_iter().any(|r| {
                !people.contains(&r.requester)
                    || !people.contains(&r.worker)
                    || r.worker == r.requester
                    || fields.binary_search(&r.field).is_err()
                    || r.phase == Phase::Growing
                    || r.expires <= r.created_at
            })
        {
            return Err("invalid cultivation requests".into());
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Attempt {
    pub request: u64,
    pub worker: u64,
    pub field: u64,
}
pub struct Prepared {
    owner: Requests,
    pub attempts: Vec<Attempt>,
    pub reserved: Vec<u64>,
    pub fields: Vec<u64>,
    pub requesters: Vec<u64>,
}

pub fn prepare(
    owner: &Requests,
    context: &Context<'_>,
    opinions: Option<&Index<'_>>,
) -> Result<Prepared, String> {
    let mut fields: Vec<_> = context.agriculture.fields.par_iter().collect();
    fields.par_sort_unstable_by_key(|f| f.id);
    let mut preferences: Vec<_> = owner.preferences.par_iter().collect();
    preferences.par_sort_unstable_by_key(|p| p.person);
    let mut values: Vec<_> = owner.offer_values.par_iter().collect();
    values.par_sort_unstable_by_key(|row| row.person);
    let mut offers: Vec<_> = if values.par_iter().any(|row| row.value_per_unit != 0) {
        context
            .agreements
            .map_or(&[][..], |owner| owner.contracts.as_slice())
            .par_iter()
            .filter_map(|contract| {
                use game_agreements::Status;
                let live = match contract.status {
                    Status::Proposed => contract.due > context.tick,
                    Status::Active => contract.due >= context.tick,
                    _ => false,
                };
                let remaining = contract.quantity.saturating_sub(contract.delivered);
                (live && remaining > 0).then_some((
                    contract.creditor,
                    contract.debtor,
                    contract.good,
                    remaining,
                ))
            })
            .collect()
    } else {
        Vec::new()
    };
    offers.par_sort_unstable_by_key(|row| (row.0, row.1, row.2));
    let offers: Vec<_> = offers
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
    let field = |id| {
        fields
            .binary_search_by_key(&id, |f| f.id)
            .ok()
            .map(|i| fields[i])
    };
    let person = |id| {
        context
            .people
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .map(|i| &context.people[i])
    };
    let unavailable = |id| {
        context.busy.binary_search(&id).is_ok()
            || context.manual.is_some_and(|(actor, _)| actor == id)
    };
    if let Some((actor, id, accept)) = context.manual_response {
        let request = owner
            .pending
            .iter()
            .find(|request| request.id == id && request.worker == actor)
            .ok_or("unknown cultivation request")?;
        let worker = person(actor).ok_or("unknown person")?;
        let f = field(request.field).ok_or("unknown requested field")?;
        if !worker.controlled
            || worker.health == 0
            || request.created_at >= context.tick
            || context.tick > request.expires
        {
            return Err("person cannot respond to this cultivation request".into());
        }
        if accept
            && (worker.hunger > context.agriculture.max_hunger
                || worker.region != Some(f.region)
                || context.season.is_some_and(|s| match f.phase {
                    Phase::Fallow => !s.planting_allowed,
                    Phase::Ripe => !s.harvest_allowed,
                    Phase::Growing => false,
                })
                || (f.phase == Phase::Fallow
                    && context.claims.is_some_and(|claims| {
                        let crop = context
                            .agriculture
                            .crops
                            .iter()
                            .find(|crop| crop.id == f.crop)
                            .expect("validated crop");
                        !claims.allows(worker.id, f.region, crop.seed)
                    })))
        {
            return Err("person cannot cultivate this field now".into());
        }
    }
    if let Some((actor, id)) = context.manual_withdrawal {
        let request = owner
            .pending
            .iter()
            .find(|request| request.id == id && request.requester == actor)
            .ok_or("unknown cultivation request")?;
        let requester = person(actor).ok_or("unknown person")?;
        if !requester.controlled
            || requester.health == 0
            || request.created_at >= context.tick
            || context.tick > request.expires
        {
            return Err("person cannot withdraw this cultivation request".into());
        }
    }
    // Withdrawal is a personal communication, selected from the opening state.
    let mut automatic = if owner.withdrawal_preferences.is_empty() {
        Vec::new()
    } else {
        let mut rules: Vec<_> = owner.withdrawal_preferences.par_iter().collect();
        rules.par_sort_unstable_by_key(|rule| rule.person);
        owner
            .pending
            .par_iter()
            .filter_map(|request| {
                let requester = person(request.requester)?;
                let worker = person(request.worker)?;
                let rule = rules.get(
                    rules
                        .binary_search_by_key(&requester.id, |rule| rule.person)
                        .ok()?,
                )?;
                if requester.controlled
                    || requester.health == 0
                    || worker.health == 0
                    || requester.region.is_none()
                    || requester.region != worker.region
                    || unavailable(requester.id)
                    || context.contact_blocked.binary_search(&requester.id).is_ok()
                    || context.contact_blocked.binary_search(&worker.id).is_ok()
                    || request.created_at >= context.tick
                    || context.tick > request.expires
                    || field(request.field).is_none_or(|field| field.phase != request.phase)
                    || context
                        .manual_response
                        .is_some_and(|(_, id, _)| id == request.id)
                    || opinions.map_or(0, |index| index.affinity(requester.id, worker.id))
                        >= rule.affinity_below
                {
                    return None;
                }
                Some((request.id, requester.id, rule.retry_after))
            })
            .collect::<Vec<_>>()
    };
    automatic.par_sort_unstable_by_key(|row| (row.1, row.0));
    let automatic: Vec<_> = automatic
        .par_chunk_by(|a, b| a.1 == b.1)
        .map(|group| group[0])
        .collect();
    let mut withdrawn: Vec<_> = automatic.par_iter().map(|row| row.0).collect();
    withdrawn.par_sort_unstable();
    let mut withdrawal_actors: Vec<_> = automatic.par_iter().map(|row| row.1).collect();
    withdrawal_actors.par_sort_unstable();
    let mut retry_backoffs: Vec<_> = owner
        .retry_backoffs
        .par_iter()
        .filter(|row| row.until > context.tick)
        .cloned()
        .collect();
    let new_backoffs: Vec<_> = automatic
        .par_iter()
        .map(|&(_, person, delay)| {
            Ok(RetryBackoff {
                person,
                until: context
                    .tick
                    .checked_add(delay)
                    .ok_or("cultivation retry overflow")?,
            })
        })
        .collect::<Result<_, String>>()?;
    retry_backoffs.par_extend(new_backoffs);
    retry_backoffs.par_sort_unstable_by_key(|row| (row.person, row.until));
    retry_backoffs = retry_backoffs
        .par_chunk_by(|a, b| a.person == b.person)
        .map(|group| group[group.len() - 1].clone())
        .collect();
    let waiting = |person| {
        retry_backoffs
            .binary_search_by_key(&person, |row| row.person)
            .ok()
            .is_some_and(|i| retry_backoffs[i].until > context.tick)
    };
    enum Evaluation {
        Event(Event),
        Candidate(Request),
        Retained(Request),
    }
    let evaluations: Vec<_> = owner
        .pending
        .par_iter()
        .map(|request| -> Result<_, String> {
            let f = field(request.field).ok_or("unknown requested field")?;
            let worker = person(request.worker).ok_or("unknown requested worker")?;
            let response = context
                .manual_response
                .filter(|(actor, id, _)| *actor == worker.id && *id == request.id);
            Ok(
                if withdrawn.binary_search(&request.id).is_ok()
                    || context
                        .manual_withdrawal
                        .is_some_and(|(actor, id)| actor == request.requester && id == request.id)
                {
                    Evaluation::Event(Event {
                        request: request.id,
                        outcome: Outcome::Withdrawn,
                    })
                } else if context.tick > request.expires {
                    Evaluation::Event(Event {
                        request: request.id,
                        outcome: Outcome::Expired,
                    })
                } else if response.is_some_and(|(_, _, accept)| !accept) {
                    Evaluation::Event(Event {
                        request: request.id,
                        outcome: Outcome::Refused,
                    })
                } else if f.phase != request.phase {
                    Evaluation::Event(Event {
                        request: request.id,
                        outcome: Outcome::Stale,
                    })
                } else if context.tick <= request.created_at
                    || (worker.controlled && response.is_none())
                    || worker.health == 0
                    || worker.hunger > context.agriculture.max_hunger
                    || worker.region != Some(f.region)
                    || (unavailable(worker.id) && response.is_none())
                    || (withdrawal_actors.binary_search(&worker.id).is_ok() && response.is_none())
                    || context.manual.is_some_and(|(_, id)| id == f.id)
                    || context.reserved_field == Some(f.id)
                {
                    Evaluation::Retained(request.clone())
                } else if response.is_some() {
                    Evaluation::Candidate(request.clone())
                } else if let Ok(i) = preferences.binary_search_by_key(&worker.id, |p| p.person) {
                    let p = preferences[i];
                    let (fear, obligation) = opinions
                        .map_or((0, 0), |index| index.feelings(worker.id, request.requester));
                    let affinity =
                        opinions.map_or(0, |index| index.affinity(worker.id, request.requester));
                    let trust =
                        opinions.map_or(0, |index| index.trust(worker.id, request.requester));
                    let offer_score = values
                        .binary_search_by_key(&worker.id, |row| row.person)
                        .ok()
                        .map_or(0, |i| {
                            let value = values[i];
                            let quantity = offers
                                .binary_search_by_key(
                                    &(worker.id, request.requester, value.good),
                                    |row| (row.0, row.1, row.2),
                                )
                                .ok()
                                .map_or(0, |j| offers[j].3)
                                .min(value.max_quantity);
                            i128::from(quantity) * i128::from(value.value_per_unit)
                        });
                    let score = i128::from(p.help_value)
                        .saturating_add(i128::from(fear) * i128::from(p.fear_weight))
                        .saturating_add(i128::from(obligation) * i128::from(p.obligation_weight))
                        .saturating_add(i128::from(affinity) * i128::from(p.affinity_weight))
                        .saturating_add(i128::from(trust) * i128::from(p.trust_weight))
                        .saturating_add(offer_score);
                    if score < i128::from(p.effort) {
                        Evaluation::Event(Event {
                            request: request.id,
                            outcome: Outcome::Refused,
                        })
                    } else if context.claims.is_some_and(|claims| {
                        !claims.allows_field(worker.id, f.id, Some(request.requester))
                    }) || context.season.is_some_and(|s| match f.phase {
                        Phase::Fallow => !s.planting_allowed,
                        Phase::Ripe => !s.harvest_allowed,
                        Phase::Growing => true,
                    }) || (f.phase == Phase::Fallow
                        && context.claims.is_some_and(|claims| {
                            let crop = context
                                .agriculture
                                .crops
                                .iter()
                                .find(|crop| crop.id == f.crop)
                                .expect("validated crop");
                            !claims.allows(worker.id, f.region, crop.seed)
                        }))
                    {
                        Evaluation::Retained(request.clone())
                    } else {
                        Evaluation::Candidate(request.clone())
                    }
                } else {
                    Evaluation::Retained(request.clone())
                },
            )
        })
        .collect::<Result<_, _>>()?;
    let events: Vec<_> = evaluations
        .par_iter()
        .filter_map(|evaluation| match evaluation {
            Evaluation::Event(event) => Some(event.clone()),
            _ => None,
        })
        .collect();
    let mut candidates: Vec<_> = evaluations
        .par_iter()
        .filter_map(|evaluation| match evaluation {
            Evaluation::Candidate(request) => Some(request.clone()),
            _ => None,
        })
        .collect();
    let mut retained: Vec<_> = evaluations
        .into_par_iter()
        .filter_map(|evaluation| match evaluation {
            Evaluation::Retained(request) => Some(request),
            _ => None,
        })
        .collect();
    let manual_id = context.manual_response.map(|(_, id, _)| id);
    candidates.par_sort_unstable_by_key(|r| (r.worker, Some(r.id) != manual_id, r.id));
    let mut chosen: Vec<_> = candidates
        .par_chunk_by(|a, b| a.worker == b.worker)
        .map(|group| group[0].clone())
        .collect();
    chosen.par_sort_unstable_by_key(|r| (r.field, Some(r.id) != manual_id, r.id));
    let chosen: Vec<_> = chosen
        .par_chunk_by(|a, b| a.field == b.field)
        .map(|group| group[0].clone())
        .collect();
    let mut selected_ids: Vec<_> = chosen.par_iter().map(|r| r.id).collect();
    selected_ids.par_sort_unstable();
    retained.par_extend(
        candidates
            .into_par_iter()
            .filter(|r| selected_ids.binary_search(&r.id).is_err()),
    );
    let attempts: Vec<_> = chosen
        .par_iter()
        .map(|r| Attempt {
            request: r.id,
            worker: r.worker,
            field: r.field,
        })
        .collect();
    let mut reserved: Vec<_> = attempts.par_iter().map(|a| a.worker).collect();
    reserved.par_extend(withdrawal_actors.par_iter().copied());
    reserved.par_sort_unstable();
    let mut used_fields: Vec<_> = attempts.par_iter().map(|a| a.field).collect();
    used_fields.par_sort_unstable();
    let mut already: Vec<_> = owner.pending.par_iter().map(|r| r.requester).collect();
    already.par_sort_unstable();
    let mut proposals: Vec<_> = owner
        .askers
        .par_iter()
        .filter_map(|a| {
            let asker = person(a.person)?;
            let worker = person(a.worker)?;
            let f = field(a.field)?;
            if !context.tick.is_multiple_of(a.interval)
                || already.binary_search(&a.person).is_ok()
                || waiting(a.person)
                || asker.controlled
                || asker.health == 0
                || asker.hunger > context.agriculture.max_hunger
                || worker.health == 0
                || asker.region != Some(f.region)
                || worker.region != Some(f.region)
                || unavailable(a.person)
                || context.contact_blocked.binary_search(&a.worker).is_ok()
                || (unavailable(a.worker) && !worker.controlled)
                || reserved.binary_search(&a.person).is_ok()
                || f.phase == Phase::Growing
            {
                return None;
            }
            Some((a.person, a.worker, a.field, f.phase, a.lifetime))
        })
        .collect();
    if !owner.discoverers.is_empty() {
        let mut workers: Vec<_> = context
            .people
            .par_iter()
            .filter_map(|worker| {
                let region = worker.region?;
                (worker.health > 0
                    && context.contact_blocked.binary_search(&worker.id).is_err()
                    && (!unavailable(worker.id) || worker.controlled))
                    .then_some((region, worker.id))
            })
            .collect();
        workers.par_sort_unstable();
        let mut opportunities: Vec<_> = context
            .agriculture
            .fields
            .par_iter()
            .filter(|field| match field.phase {
                Phase::Growing => false,
                Phase::Fallow => context.season.is_none_or(|season| season.planting_allowed),
                Phase::Ripe => context.season.is_none_or(|season| season.harvest_allowed),
            })
            .map(|field| (field.region, field.id, field.phase))
            .collect();
        opportunities.par_sort_unstable_by_key(|row| (row.0, row.1));
        proposals.par_extend(owner.discoverers.par_iter().filter_map(|a| {
            let asker = person(a.person)?;
            let region = asker.region?;
            if !context.tick.is_multiple_of(a.interval)
                || already.binary_search(&a.person).is_ok()
                || waiting(a.person)
                || asker.controlled
                || asker.health == 0
                || asker.hunger > context.agriculture.max_hunger
                || unavailable(a.person)
                || reserved.binary_search(&a.person).is_ok()
            {
                return None;
            }
            let first = workers.partition_point(|row| row.0 < region);
            let last = workers.partition_point(|row| row.0 <= region);
            let nearby = &workers[first..last];
            if nearby.len() < 2 {
                return None;
            }
            let hash = a
                .person
                .wrapping_mul(0x9e3779b97f4a7c15)
                .wrapping_add(context.tick);
            let mut index = (hash % nearby.len() as u64) as usize;
            if nearby[index].1 == a.person {
                index = (index + 1) % nearby.len();
            }
            let first = opportunities.partition_point(|row| row.0 < region);
            let last = opportunities.partition_point(|row| row.0 <= region);
            let fields = &opportunities[first..last];
            if fields.is_empty() {
                return None;
            }
            let chosen = fields[(hash.rotate_left(23) % fields.len() as u64) as usize];
            Some((a.person, nearby[index].1, chosen.1, chosen.2, a.lifetime))
        }));
    }
    proposals.par_sort_unstable_by_key(|row| row.0);
    let count = u64::try_from(proposals.len()).map_err(|_| "cultivation request overflow")?;
    let next_id = owner
        .next_id
        .checked_add(count)
        .ok_or("cultivation request overflow")?;
    let opened: Vec<_> = proposals
        .into_par_iter()
        .enumerate()
        .map(|(i, (requester, worker, field, phase, lifetime))| {
            Ok(Request {
                id: owner.next_id + i as u64,
                requester,
                worker,
                field,
                phase,
                created_at: context.tick,
                expires: context
                    .tick
                    .checked_add(lifetime)
                    .ok_or("cultivation request expiry overflow")?,
            })
        })
        .collect::<Result<_, String>>()?;
    let mut requesters: Vec<_> = opened.par_iter().map(|request| request.requester).collect();
    requesters.par_extend(withdrawal_actors);
    requesters.par_sort_unstable();
    requesters.dedup();
    retained.par_extend(opened);
    reserved.par_extend(requesters.par_iter().copied());
    reserved.par_sort_unstable();
    reserved.dedup();
    retained.par_sort_unstable_by_key(|r| r.id);
    Ok(Prepared {
        owner: Requests {
            next_id,
            askers: owner.askers.par_iter().cloned().collect(),
            discoverers: owner.discoverers.par_iter().cloned().collect(),
            withdrawal_preferences: owner.withdrawal_preferences.par_iter().cloned().collect(),
            retry_backoffs,
            offer_values: owner.offer_values.par_iter().cloned().collect(),
            work_memories: owner.work_memories.par_iter().cloned().collect(),
            preferences: owner.preferences.par_iter().cloned().collect(),
            pending: retained,
            events,
        },
        attempts,
        reserved,
        fields: used_fields,
        requesters,
    })
}

impl Prepared {
    pub fn publish(
        mut self,
        changes: &[(u64, u64, Phase, u32)],
        decisions: &[&TransactionOutcome],
    ) -> Result<Requests, String> {
        let events: Vec<_> = self
            .attempts
            .par_iter()
            .map(|attempt| -> Result<_, String> {
                let action = changes
                    .binary_search_by_key(&attempt.field, |row| row.0)
                    .ok()
                    .map(|i| changes[i].1)
                    .ok_or("missing requested cultivation action")?;
                let decision = decisions
                    .binary_search_by_key(&action, |r| r.id)
                    .ok()
                    .map(|i| decisions[i])
                    .ok_or("missing requested cultivation outcome")?;
                Ok(Event {
                    request: attempt.request,
                    outcome: if decision.accepted {
                        Outcome::Fulfilled
                    } else {
                        Outcome::Failed
                    },
                })
            })
            .collect::<Result<_, _>>()?;
        self.owner.events.par_extend(events);
        self.owner
            .events
            .par_sort_unstable_by_key(|event| event.request);
        Ok(self.owner)
    }
}

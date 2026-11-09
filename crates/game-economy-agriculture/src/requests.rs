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
    pub preferences: Vec<Preference>,
    #[serde(default)]
    pub pending: Vec<Request>,
    /// Outcomes from the latest boundary only.
    #[serde(default)]
    pub events: Vec<Event>,
}

impl Requests {
    pub fn validate(&self, people: &HashSet<u64>, fields: &[u64]) -> Result<(), String> {
        let mut askers: Vec<_> = self.askers.par_iter().map(|a| a.person).collect();
        let mut preferences: Vec<_> = self.preferences.par_iter().map(|a| a.person).collect();
        let mut ids: Vec<_> = self.pending.par_iter().map(|r| r.id).collect();
        askers.par_sort_unstable();
        preferences.par_sort_unstable();
        ids.par_sort_unstable();
        if self.next_id == 0
            || ids.last().is_some_and(|id| *id >= self.next_id)
            || ids.first() == Some(&0)
            || ids.par_windows(2).any(|p| p[0] == p[1])
            || askers.par_windows(2).any(|p| p[0] == p[1])
            || preferences.par_windows(2).any(|p| p[0] == p[1])
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
            Ok(if context.tick > request.expires {
                Evaluation::Event(Event {
                    request: request.id,
                    outcome: Outcome::Expired,
                })
            } else if f.phase != request.phase {
                Evaluation::Event(Event {
                    request: request.id,
                    outcome: Outcome::Stale,
                })
            } else if context.tick <= request.created_at
                || worker.controlled
                || worker.health == 0
                || worker.hunger > context.agriculture.max_hunger
                || worker.region != Some(f.region)
                || unavailable(worker.id)
                || context.manual.is_some_and(|(_, id)| id == f.id)
            {
                Evaluation::Retained(request.clone())
            } else if let Ok(i) = preferences.binary_search_by_key(&worker.id, |p| p.person) {
                let p = preferences[i];
                let (fear, obligation) =
                    opinions.map_or((0, 0), |index| index.feelings(worker.id, request.requester));
                let affinity =
                    opinions.map_or(0, |index| index.affinity(worker.id, request.requester));
                let trust = opinions.map_or(0, |index| index.trust(worker.id, request.requester));
                let score = i128::from(p.help_value)
                    .saturating_add(i128::from(fear) * i128::from(p.fear_weight))
                    .saturating_add(i128::from(obligation) * i128::from(p.obligation_weight))
                    .saturating_add(i128::from(affinity) * i128::from(p.affinity_weight))
                    .saturating_add(i128::from(trust) * i128::from(p.trust_weight));
                if score < i128::from(p.effort) {
                    Evaluation::Event(Event {
                        request: request.id,
                        outcome: Outcome::Refused,
                    })
                } else if context.season.is_some_and(|s| match f.phase {
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
            })
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
    candidates.par_sort_unstable_by_key(|r| (r.worker, r.id));
    let mut chosen: Vec<_> = candidates
        .par_chunk_by(|a, b| a.worker == b.worker)
        .map(|group| group[0].clone())
        .collect();
    chosen.par_sort_unstable_by_key(|r| (r.field, r.id));
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
                || asker.controlled
                || asker.health == 0
                || asker.hunger > context.agriculture.max_hunger
                || worker.health == 0
                || asker.region != Some(f.region)
                || worker.region != Some(f.region)
                || unavailable(a.person)
                || unavailable(a.worker)
                || reserved.binary_search(&a.person).is_ok()
                || f.phase == Phase::Growing
            {
                return None;
            }
            Some((a, f.phase))
        })
        .collect();
    proposals.par_sort_unstable_by_key(|(a, _)| a.person);
    let count = u64::try_from(proposals.len()).map_err(|_| "cultivation request overflow")?;
    let next_id = owner
        .next_id
        .checked_add(count)
        .ok_or("cultivation request overflow")?;
    let opened: Vec<_> = proposals
        .into_par_iter()
        .enumerate()
        .map(|(i, (a, phase))| {
            Ok(Request {
                id: owner.next_id + i as u64,
                requester: a.person,
                worker: a.worker,
                field: a.field,
                phase,
                created_at: context.tick,
                expires: context
                    .tick
                    .checked_add(a.lifetime)
                    .ok_or("cultivation request expiry overflow")?,
            })
        })
        .collect::<Result<_, String>>()?;
    let requesters: Vec<_> = opened.par_iter().map(|request| request.requester).collect();
    retained.par_extend(opened);
    reserved.par_extend(requesters.par_iter().copied());
    reserved.par_sort_unstable();
    reserved.dedup();
    retained.par_sort_unstable_by_key(|r| r.id);
    Ok(Prepared {
        owner: Requests {
            next_id,
            askers: owner.askers.clone(),
            preferences: owner.preferences.clone(),
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

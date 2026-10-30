//! Bounded, observer-owned recollections of requested work.
use super::{Event, Outcome, Request, Requests};
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkLink {
    pub request: u64,
    pub requester: u64,
    pub worker: u64,
    pub job: u64,
    pub region: u64,
    pub recipe: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub observer: u64,
    pub worker: u64,
    pub recipe: u32,
    pub successes: u64,
    pub failures: u64,
    pub last_tick: u64,
    /// Latest personally observed successful completion, distinct from later refusals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at: Option<u64>,
}
impl Requests {
    pub(super) fn valid_experience(
        &self,
        people: &[PersonSnapshot],
        recipes: &HashSet<u32>,
        regions: &HashSet<u64>,
    ) -> bool {
        if self.memory_limit > 32 || (self.memory_limit == 0 && !self.memories.is_empty()) {
            return false;
        }
        let mut links: Vec<_> = self.work_links.par_iter().map(|r| r.job).collect();
        links.par_sort_unstable();
        let mut memories: Vec<_> = self
            .memories
            .par_iter()
            .map(|r| (r.observer, r.worker, r.recipe))
            .collect();
        memories.par_sort_unstable();
        !links.par_windows(2).any(|w| w[0] == w[1])
            && !memories.par_windows(2).any(|w| w[0] == w[1])
            && !memories
                .par_chunk_by(|a, b| a.0 == b.0)
                .any(|g| g.len() > self.memory_limit)
            && self.work_links.par_iter().all(|r| {
                r.request > 0
                    && r.request < self.next_id
                    && r.requester != r.worker
                    && person_index(people, r.requester).is_some()
                    && person_index(people, r.worker).is_some()
                    && recipes.contains(&r.recipe)
                    && regions.contains(&r.region)
            })
            && self.memories.par_iter().all(|r| {
                r.observer != r.worker
                    && person_index(people, r.observer).is_some()
                    && person_index(people, r.worker).is_some()
                    && recipes.contains(&r.recipe)
                    && (r.successes > 0 || r.failures > 0)
                    && r.last_success_at
                        .is_none_or(|tick| r.successes > 0 && tick <= r.last_tick)
            })
    }
    pub fn record_start(&mut self, request: &Request, job: u64) {
        self.work_links.push(WorkLink {
            request: request.id,
            requester: request.requester,
            worker: request.worker,
            job,
            region: request.region,
            recipe: request.recipe,
        });
    }
    pub fn record_starts(&mut self, events: &[Event]) {
        let mut starts: Vec<_> = events
            .par_iter()
            .filter_map(|e| match e.outcome {
                Outcome::Started { job } => Some((e.request, job)),
                _ => None,
            })
            .collect();
        starts.par_sort_unstable_by_key(|r| r.0);
        let links: Vec<_> = self
            .pending
            .par_iter()
            .filter_map(|r| {
                let i = starts.binary_search_by_key(&r.id, |s| s.0).ok()?;
                Some(WorkLink {
                    request: r.id,
                    requester: r.requester,
                    worker: r.worker,
                    job: starts[i].1,
                    region: r.region,
                    recipe: r.recipe,
                })
            })
            .collect();
        self.work_links.par_extend(links);
    }
    /// Resolve links even when nobody witnessed them. Each observer attends to at most
    /// one outcome per tick, selected deterministically, and retains bounded history.
    pub fn resolve<F>(&mut self, completed: &[(u64, bool)], tick: u64, visible: F)
    where
        F: Fn(&WorkLink) -> bool + Sync,
    {
        if completed.is_empty() || self.work_links.is_empty() {
            return;
        }
        let mut completed = completed.to_vec();
        completed.par_sort_unstable_by_key(|r| r.0);
        let observations: Vec<_> = self
            .work_links
            .par_iter()
            .filter_map(|link| {
                let i = completed.binary_search_by_key(&link.job, |r| r.0).ok()?;
                visible(link).then_some((
                    link.requester,
                    link.worker,
                    link.recipe,
                    link.request,
                    completed[i].1,
                ))
            })
            .collect();
        self.work_links = std::mem::take(&mut self.work_links)
            .into_par_iter()
            .filter(|link| completed.binary_search_by_key(&link.job, |r| r.0).is_err())
            .collect();
        self.learn(observations, tick);
    }
    /// A refusal is an observed choice, not a breach of an obligation.
    pub fn observe_refusals(&mut self, events: &[Event], visible: &[u64], tick: u64) {
        if self.memory_limit == 0 || visible.is_empty() {
            return;
        }
        let mut ids: Vec<_> = events
            .par_iter()
            .filter(|e| {
                matches!(e.outcome, Outcome::Refused) && visible.binary_search(&e.request).is_ok()
            })
            .map(|e| e.request)
            .collect();
        ids.par_sort_unstable();
        let observations = self
            .pending
            .par_iter()
            .filter(|r| ids.binary_search(&r.id).is_ok())
            .map(|r| (r.requester, r.worker, r.recipe, r.id, false))
            .collect();
        self.learn(observations, tick);
    }
    pub fn observe_refusal(&mut self, request: &Request, tick: u64) {
        if self.memory_limit > 0 {
            self.learn(
                vec![(
                    request.requester,
                    request.worker,
                    request.recipe,
                    request.id,
                    false,
                )],
                tick,
            );
        }
    }
    fn learn(&mut self, mut observations: Vec<(u64, u64, u32, u64, bool)>, tick: u64) {
        if self.memory_limit == 0 || observations.is_empty() {
            return;
        }
        observations.par_sort_unstable();
        let observations: Vec<_> = observations
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|rows| rows[0])
            .collect();
        let mut old = std::mem::take(&mut self.memories);
        old.par_sort_unstable_by_key(|r| (r.observer, r.worker, r.recipe));
        let changed: Vec<Vec<Memory>> = observations
            .par_iter()
            .map(|&(observer, worker, recipe, _, success)| {
                let start = old.partition_point(|r| r.observer < observer);
                let end = old.partition_point(|r| r.observer <= observer);
                let mut rows = old[start..end].to_vec();
                if rows.iter().any(|row| row.last_tick == tick) {
                    return rows;
                }
                if let Some(row) = rows
                    .iter_mut()
                    .find(|r| r.worker == worker && r.recipe == recipe)
                {
                    if success {
                        row.successes = row.successes.saturating_add(1);
                        row.last_success_at = Some(tick);
                    } else {
                        row.failures = row.failures.saturating_add(1);
                    }
                    row.last_tick = tick;
                } else {
                    if rows.len() == self.memory_limit {
                        let i = rows
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, r)| (r.last_tick, r.worker, r.recipe))
                            .unwrap()
                            .0;
                        rows.remove(i);
                    }
                    rows.push(Memory {
                        observer,
                        worker,
                        recipe,
                        successes: u64::from(success),
                        failures: u64::from(!success),
                        last_tick: tick,
                        last_success_at: success.then_some(tick),
                    });
                }
                rows
            })
            .collect();
        let mut result: Vec<_> = old
            .into_par_iter()
            .filter(|r| {
                observations
                    .binary_search_by_key(&r.observer, |o| o.0)
                    .is_err()
            })
            .collect();
        result.par_extend(changed.into_par_iter().flatten());
        result.par_sort_unstable_by_key(|r| (r.observer, r.worker, r.recipe));
        self.memories = result;
    }
}

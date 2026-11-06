//! Disjoint individual capture attempts against real people and site capacity.
use super::{Confinement, Event, EventKind, Hold, ordered_index};
use game_parallel_economy_person::Person;
use game_parallel_opinions::Opinions;
use game_parallel_work_sites::Facilities;
use rayon::prelude::*;
use std::collections::HashMap;

pub struct Context<'a> {
    pub owner: &'a mut Confinement,
    pub people: &'a [Person],
    pub sites: &'a Facilities,
    pub busy: &'a [u64],
    /// Controlled people with a submitted action or other physical commitment.
    pub controlled_unavailable: &'a [u64],
    pub opinions: Option<&'a Opinions>,
    pub seed: u64,
    pub tick: u64,
}

#[derive(Clone, Copy)]
struct Choice {
    actor: u64,
    target: u64,
    site: u64,
    priority: u64,
    succeeds: bool,
}

pub struct Outcome {
    pub participants: Vec<u64>,
    pub captured: Vec<u64>,
}

impl Context<'_> {
    pub fn prepare(self) -> Outcome {
        if self.owner.capture_policies.is_empty() {
            return Outcome {
                participants: vec![],
                captured: vec![],
            };
        }
        let need_opinions = self
            .owner
            .capture_policies
            .par_iter()
            .any(|p| p.pressure_threshold.is_some())
            || self
                .owner
                .capture_responses
                .par_iter()
                .any(|p| p.pressure_threshold.is_some());
        let opinions = need_opinions
            .then(|| self.opinions.map(Opinions::index))
            .flatten();
        let pressured = |observer: u64, subject: u64, threshold: Option<u64>| {
            threshold.is_some_and(|threshold| {
                opinions.as_ref().is_some_and(|index| {
                    let (fear, obligation) = index.feelings(observer, subject);
                    fear.saturating_add(obligation) >= threshold
                })
            })
        };
        let mut occupancy: Vec<_> = self.owner.holds.par_iter().map(|hold| hold.site).collect();
        occupancy.par_sort_unstable();
        let slots: HashMap<_, _> = self
            .owner
            .cells
            .par_iter()
            .filter_map(|cell| {
                let site = &self.sites.sites[self
                    .sites
                    .sites
                    .binary_search_by_key(&cell.site, |s| s.id)
                    .ok()?];
                let start = occupancy.partition_point(|id| *id < cell.site);
                let end = occupancy.partition_point(|id| *id <= cell.site);
                let free = site.capacity.saturating_sub((end - start) as u64);
                (cell.resistance > 0 && free > 0).then_some((cell.site, (site.region, free)))
            })
            .collect();
        let find = |id| ordered_index(self.people, id, |row| row.id).map(|i| &self.people[i]);
        let effort = |id| {
            ordered_index(&self.owner.escape, id, |row| row.person)
                .map_or(self.owner.default_effort, |i| self.owner.escape[i].effort)
        };
        let mut choices: Vec<_> = self
            .owner
            .capture_policies
            .par_iter()
            .filter_map(|policy| {
                let actor = find(policy.person)?;
                let target = find(policy.target)?;
                let &(region, _) = slots.get(&policy.site)?;
                let willing = policy.willing
                    || policy
                        .pressured_by
                        .is_some_and(|id| pressured(actor.id, id, policy.pressure_threshold));
                let target_busy = if target.controlled {
                    self.controlled_unavailable
                        .binary_search(&target.id)
                        .is_ok()
                } else {
                    self.busy.binary_search(&target.id).is_ok()
                };
                if !willing
                    || actor.controlled
                    || actor.health == 0
                    || self.busy.binary_search(&actor.id).is_ok()
                    || target_busy
                    || self.owner.held(actor.id)
                    || self.owner.held(target.id)
                    || actor.region != Some(region)
                    || target.region != Some(region)
                    || (target.health == 0 && target.immortal.is_none())
                {
                    return None;
                }
                let response =
                    ordered_index(&self.owner.capture_responses, target.id, |row| row.person)
                        .map(|i| &self.owner.capture_responses[i]);
                let yields = response.is_some_and(|response| {
                    response.yield_willingly
                        || pressured(target.id, actor.id, response.pressure_threshold)
                });
                Some(Choice {
                    actor: actor.id,
                    target: target.id,
                    site: policy.site,
                    priority: game_parallel_genetics::random(
                        self.seed ^ 0x63617074757265,
                        actor.id,
                        self.tick,
                    ),
                    succeeds: target.health == 0 || yields || effort(actor.id) > effort(target.id),
                })
            })
            .collect();
        let shared_target = choices.first().is_some_and(|first| {
            choices
                .par_iter()
                .all(|choice| choice.target == first.target)
        });
        let disjoint = if shared_target {
            false
        } else {
            let mut targets: Vec<_> = choices.par_iter().map(|choice| choice.target).collect();
            if targets.par_windows(2).any(|pair| pair[0] > pair[1]) {
                targets.par_sort_unstable();
            }
            targets.par_windows(2).all(|pair| pair[0] != pair[1])
                && targets.par_iter().all(|id| {
                    ordered_index(&self.owner.capture_policies, *id, |p| p.person).is_none()
                })
        };
        let selected: Vec<_> = if shared_target {
            choices
                .par_iter()
                .min_by_key(|choice| (choice.priority, choice.actor, choice.target))
                .copied()
                .into_iter()
                .collect()
        } else if disjoint {
            choices
        } else {
            choices
                .par_sort_unstable_by_key(|choice| (choice.priority, choice.actor, choice.target));
            let mut claims: Vec<_> = choices
                .par_iter()
                .enumerate()
                .flat_map_iter(|(index, choice)| [(choice.actor, index), (choice.target, index)])
                .collect();
            claims.par_sort_unstable();
            choices
                .into_par_iter()
                .enumerate()
                .filter_map(|(index, choice)| {
                    let actor = claims.partition_point(|claim| claim.0 < choice.actor);
                    let target = claims.partition_point(|claim| claim.0 < choice.target);
                    (claims[actor].1 == index && claims[target].1 == index).then_some(choice)
                })
                .collect()
        };
        let mut successes: Vec<_> = selected
            .par_iter()
            .copied()
            .filter(|choice| choice.succeeds)
            .collect();
        let success_count = successes.len();
        let approved: Vec<_> = if successes.first().is_some_and(|first| {
            successes.len() as u128 <= u128::from(slots[&first.site].1)
                && successes.par_iter().all(|choice| choice.site == first.site)
        }) {
            successes
        } else {
            successes
                .par_sort_unstable_by_key(|choice| (choice.site, choice.priority, choice.actor));
            successes
                .par_iter()
                .enumerate()
                .filter_map(|(index, choice)| {
                    let start = successes.partition_point(|row| row.site < choice.site);
                    (((index - start) as u128) < u128::from(slots[&choice.site].1))
                        .then_some(*choice)
                })
                .collect()
        };
        let all_approved = approved.len() == success_count;
        let mut approved_actors: Vec<_> = if all_approved {
            Vec::new()
        } else {
            approved.par_iter().map(|choice| choice.actor).collect()
        };
        if approved_actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            approved_actors.par_sort_unstable();
        }
        self.owner
            .events
            .par_extend(selected.par_iter().map(|choice| Event {
                tick: self.tick,
                person: choice.target,
                actor: Some(choice.actor),
                site: choice.site,
                kind: if !choice.succeeds {
                    EventKind::CaptureResisted
                } else if all_approved || approved_actors.binary_search(&choice.actor).is_ok() {
                    EventKind::Confined
                } else {
                    EventKind::CaptureUnavailable
                },
            }));
        if self.owner.events.par_windows(2).any(|pair| {
            (pair[0].person, pair[0].actor, pair[0].kind)
                > (pair[1].person, pair[1].actor, pair[1].kind)
        }) {
            self.owner
                .events
                .par_sort_unstable_by_key(|event| (event.person, event.actor, event.kind));
        }
        self.owner
            .holds
            .par_extend(approved.par_iter().map(|choice| Hold {
                person: choice.target,
                site: choice.site,
                born_inside: false,
            }));
        if self
            .owner
            .holds
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            self.owner
                .holds
                .par_sort_unstable_by_key(|hold| hold.person);
        }
        let mut participants: Vec<_> = selected.par_iter().map(|choice| choice.actor).collect();
        participants.par_extend(selected.par_iter().map(|choice| choice.target));
        if participants.par_windows(2).any(|pair| pair[0] > pair[1]) {
            participants.par_sort_unstable();
        }
        let mut captured: Vec<_> = approved.par_iter().map(|choice| choice.target).collect();
        if captured.par_windows(2).any(|pair| pair[0] > pair[1]) {
            captured.par_sort_unstable();
        }
        Outcome {
            participants,
            captured,
        }
    }
}

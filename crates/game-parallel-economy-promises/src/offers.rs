//! Individual choices to propose future gifts, without compelling anyone to comply.
use game_parallel_agreements::{Agreements, Proposal, Status};
use game_parallel_economy_person::{Person, travel::Colocation};
use game_parallel_opinions::Opinions;
use game_parallel_work_requests::Requests;
use rayon::prelude::*;
use std::collections::HashSet;

#[derive(Default)]
pub struct Plan {
    proposals: Vec<Proposal>,
    reviews: Vec<(u64, u64, Option<u64>)>,
}
impl Plan {
    pub fn proposals(&self) -> &[Proposal] {
        &self.proposals
    }
    pub fn actors(&self) -> impl Iterator<Item = u64> + '_ {
        self.proposals.iter().map(|proposal| proposal.debtor)
    }
}
/// Proposals are personal communication; recipients choose separately at a later boundary.
pub struct Context<'a> {
    pub owner: Option<&'a Agreements>,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub requests: Option<&'a Requests>,
    pub locations: &'a Colocation,
    pub busy: &'a [u64],
    pub max_hunger: u32,
    pub tick: u64,
}
pub fn prepare(context: Context<'_>) -> Result<Plan, String> {
    let Context {
        owner,
        people,
        opinions,
        requests,
        locations,
        busy,
        max_hunger,
        tick,
    } = context;
    let Some(owner) = owner.filter(|o| {
        o.offering
            .par_iter()
            .any(|p| p.willing || p.pressure_threshold.is_some())
    }) else {
        return Ok(Plan::default());
    };
    let pending: HashSet<_> = owner
        .contracts
        .par_iter()
        .filter(|c| matches!(c.status, Status::Proposed | Status::Active))
        .map(|c| (c.debtor, c.creditor, c.good))
        .collect();
    let opinions = opinions.map(Opinions::index);
    let mut work_memories: Vec<_> = if owner
        .offering
        .par_iter()
        .any(|p| p.after_work_recipe.is_some())
    {
        requests
            .map_or(&[][..], |r| r.memories.as_slice())
            .par_iter()
            .collect()
    } else {
        Vec::new()
    };
    if work_memories.par_windows(2).any(|pair| {
        (pair[0].observer, pair[0].worker, pair[0].recipe)
            > (pair[1].observer, pair[1].worker, pair[1].recipe)
    }) {
        work_memories.par_sort_unstable_by_key(|m| (m.observer, m.worker, m.recipe));
    }
    let mut choices: Vec<_> = owner
        .offering
        .par_iter()
        .filter_map(|policy| {
            let actor = people
                .binary_search_by_key(&policy.person, |p| p.id)
                .ok()
                .map(|i| &people[i])?;
            let recipient = people
                .binary_search_by_key(&policy.recipient, |p| p.id)
                .ok()
                .map(|i| &people[i])?;
            let observed_work = match policy.after_work_recipe {
                Some(recipe) => {
                    let i = work_memories
                        .binary_search_by_key(&(policy.person, policy.recipient, recipe), |m| {
                            (m.observer, m.worker, m.recipe)
                        })
                        .ok()?;
                    let completed = work_memories[i].last_success_at?;
                    if policy
                        .last_work_offer_at
                        .is_some_and(|old| old >= completed)
                    {
                        return None;
                    }
                    Some(completed)
                }
                None => None,
            };
            let pressured = policy.pressure_threshold.is_some_and(|threshold| {
                opinions.as_ref().is_some_and(|index| {
                    let (fear, obligation) = index.feelings(actor.id, recipient.id);
                    fear.saturating_add(obligation) >= threshold
                })
            });
            let trusts = policy.minimum_trust.is_none_or(|threshold| {
                opinions
                    .as_ref()
                    .is_some_and(|index| index.trust(actor.id, recipient.id) >= threshold)
            });
            (tick >= policy.next_review
                && !actor.controlled
                && actor.health > 0
                && recipient.health > 0
                && actor.hunger <= max_hunger
                && recipient.hunger >= policy.recipient_hunger_at_least
                && locations
                    .region((actor.id, actor.region), (recipient.id, recipient.region))
                    .is_some()
                && busy.binary_search(&actor.id).is_err()
                && (pressured || (policy.willing && trusts))
                && !pending.contains(&(actor.id, recipient.id, policy.good)))
            .then_some((policy, observed_work))
        })
        .collect();
    choices.par_sort_unstable_by_key(|(p, _)| p.person);
    let rows: Vec<_> = choices
        .par_iter()
        .map(|(policy, observed_work)| {
            Ok((
                Proposal {
                    debtor: policy.person,
                    creditor: policy.recipient,
                    good: policy.good,
                    quantity: policy.quantity,
                    due: tick
                        .checked_add(policy.due_after)
                        .ok_or("promise due overflow")?,
                },
                (
                    policy.person,
                    tick.checked_add(policy.retry_ticks)
                        .ok_or("promise review overflow")?,
                    *observed_work,
                ),
            ))
        })
        .collect::<Result<_, String>>()?;
    Ok(Plan {
        proposals: rows.par_iter().map(|r| r.0.clone()).collect(),
        reviews: rows.par_iter().map(|r| r.1).collect(),
    })
}
/// Publish proposals after previous decisions; the actual offer never transfers goods.
pub fn publish(
    owner: &mut Agreements,
    plan: &Plan,
    tick: u64,
    opinions: Option<&mut Opinions>,
) -> Result<(), String> {
    if plan.proposals.is_empty() {
        return Ok(());
    }
    owner
        .append_validated(&plan.proposals, tick)
        .map_err(str::to_owned)?;
    owner.offering.par_iter_mut().for_each(|policy| {
        if let Ok(i) = plan
            .reviews
            .binary_search_by_key(&policy.person, |row| row.0)
        {
            policy.next_review = plan.reviews[i].1;
            policy.last_work_offer_at = plan.reviews[i].2;
        }
    });
    super::learn_promises(opinions, &plan.proposals);
    Ok(())
}

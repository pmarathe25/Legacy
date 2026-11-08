//! Individual choices to propose future gifts, without compelling anyone to comply.
use game_agreements::{Agreements, Proposal, Status};
use game_economy_person::{Person, travel::Colocation};
use game_opinions::Opinions;
use game_work_requests::Requests;
use rayon::prelude::*;
use std::collections::HashSet;

#[derive(Default)]
pub struct Plan {
    proposals: Vec<Proposal>,
    reviews: Vec<(u64, u64, Option<u64>, Option<u64>)>,
    work_reviews: Vec<(u64, u64, u64)>,
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
    /// The recipient's own bounded memories of actual food received previously.
    pub help_memories: &'a [HelpMemory],
    pub locations: &'a Colocation,
    pub together: &'a (dyn Fn(u64, u64) -> bool + Sync),
    pub busy: &'a [u64],
    pub max_hunger: u32,
    pub tick: u64,
}
#[derive(Clone, Copy)]
pub struct HelpMemory {
    pub person: u64,
    pub helper: u64,
    pub received_at: u64,
}
pub fn prepare(context: Context<'_>) -> Result<Plan, String> {
    let Context {
        owner,
        people,
        opinions,
        requests,
        help_memories,
        locations,
        together,
        busy,
        max_hunger,
        tick,
    } = context;
    let Some(owner) = owner.filter(|o| {
        o.offering
            .par_iter()
            .any(|p| p.willing || p.pressure_threshold.is_some())
            || o.work_offering
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
    let opinions = opinions
        .filter(|_| {
            owner
                .offering
                .par_iter()
                .any(|p| p.minimum_trust.is_some() || p.pressure_threshold.is_some())
                || owner
                    .work_offering
                    .par_iter()
                    .any(|p| p.minimum_trust.is_some() || p.pressure_threshold.is_some())
        })
        .map(Opinions::index);
    let last_person = people.last().map(|p| p.id);
    let find_person = |id| {
        let suffix = last_person
            .and_then(|last| last.checked_sub(id))
            .and_then(|distance| usize::try_from(distance).ok())
            .and_then(|distance| distance.checked_add(1))
            .and_then(|distance| people.len().checked_sub(distance))
            .filter(|&index| people[index].id == id);
        suffix
            .or_else(|| people.binary_search_by_key(&id, |p| p.id).ok())
            .map(|i| &people[i])
    };
    let ordered_help = help_memories
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
        .then(|| {
            let mut rows: Vec<_> = help_memories.par_iter().copied().collect();
            rows.par_sort_unstable_by_key(|row| row.person);
            rows
        });
    let help = ordered_help.as_deref().unwrap_or(help_memories);
    let last_helper = help.last().map(|row| row.person);
    let find_help = |id| {
        let suffix = last_helper
            .and_then(|last| last.checked_sub(id))
            .and_then(|distance| usize::try_from(distance).ok())
            .and_then(|distance| distance.checked_add(1))
            .and_then(|distance| help.len().checked_sub(distance))
            .filter(|&index| help[index].person == id);
        suffix
            .or_else(|| help.binary_search_by_key(&id, |row| row.person).ok())
            .map(|i| &help[i])
    };
    let mut work_memories: Vec<_> = if owner
        .offering
        .par_iter()
        .any(|p| p.after_work_recipe.is_some())
        || !owner.work_offering.is_empty()
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
            let actor = find_person(policy.person)?;
            let recipient = find_person(policy.recipient)?;
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
            let observed_help = if policy.after_help {
                let memory = find_help(policy.person)?;
                if memory.helper != policy.recipient
                    || memory.received_at >= tick
                    || policy
                        .last_help_offer_at
                        .is_some_and(|old| old >= memory.received_at)
                {
                    return None;
                }
                Some(memory.received_at)
            } else {
                None
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
                && together(actor.id, recipient.id)
                && busy.binary_search(&actor.id).is_err()
                && (pressured || (policy.willing && trusts))
                && !pending.contains(&(actor.id, recipient.id, policy.good)))
            .then_some((policy, observed_work, observed_help))
        })
        .collect();
    if choices
        .par_windows(2)
        .any(|pair| pair[0].0.person > pair[1].0.person)
    {
        choices.par_sort_unstable_by_key(|(p, _, _)| p.person);
    }
    let rows: Vec<_> = choices
        .par_iter()
        .map(|(policy, observed_work, observed_help)| {
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
                    *observed_help,
                ),
            ))
        })
        .collect::<Result<_, String>>()?;
    let reviews: Vec<_> = rows.par_iter().map(|r| r.1).collect();
    let work_rows: Vec<_> = owner
        .work_offering
        .par_iter()
        .filter_map(|policy| {
            if tick < policy.next_review
                || busy.binary_search(&policy.person).is_ok()
                || reviews
                    .binary_search_by_key(&policy.person, |row| row.0)
                    .is_ok()
            {
                return None;
            }
            let actor = find_person(policy.person)?;
            if actor.controlled || actor.health == 0 || actor.hunger > max_hunger {
                return None;
            }
            let start = work_memories.partition_point(|m| m.observer < policy.person);
            let end = work_memories.partition_point(|m| m.observer <= policy.person);
            let (completed, recipient, _) = work_memories[start..end]
                .iter()
                .filter_map(|memory| {
                    let completed = if policy.after_refusal {
                        memory.last_refusal_at?
                    } else {
                        memory.last_success_at?
                    };
                    if policy
                        .last_work_offer_at
                        .is_some_and(|old| old >= completed)
                        || pending.contains(&(actor.id, memory.worker, policy.good))
                    {
                        return None;
                    }
                    let recipient = find_person(memory.worker)?;
                    if recipient.health == 0
                        || !together(actor.id, recipient.id)
                        || locations
                            .region((actor.id, actor.region), (recipient.id, recipient.region))
                            .is_none()
                    {
                        return None;
                    }
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
                    (pressured || (policy.willing && trusts)).then_some((
                        completed,
                        recipient.id,
                        memory.recipe,
                    ))
                })
                .max_by_key(|&(completed, recipient, recipe)| {
                    (
                        completed,
                        std::cmp::Reverse(recipient),
                        std::cmp::Reverse(recipe),
                    )
                })?;
            Some((policy, completed, recipient))
        })
        .collect();
    let mut work_rows: Vec<_> = work_rows
        .into_par_iter()
        .map(|(policy, completed, recipient)| {
            Ok((
                Proposal {
                    debtor: policy.person,
                    creditor: recipient,
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
                    completed,
                ),
            ))
        })
        .collect::<Result<_, String>>()?;
    if work_rows
        .par_windows(2)
        .any(|pair| pair[0].0.debtor > pair[1].0.debtor)
    {
        work_rows.par_sort_unstable_by_key(|row| row.0.debtor);
    }
    let mut proposals: Vec<_> = rows.into_par_iter().map(|r| r.0).collect();
    proposals.par_extend(work_rows.par_iter().map(|row| row.0.clone()));
    if proposals
        .par_windows(2)
        .any(|pair| pair[0].debtor > pair[1].debtor)
    {
        proposals.par_sort_unstable_by_key(|row| row.debtor);
    }
    Ok(Plan {
        proposals,
        reviews,
        work_reviews: work_rows.into_par_iter().map(|row| row.1).collect(),
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
            policy.last_help_offer_at = plan.reviews[i].3;
        }
    });
    owner.work_offering.par_iter_mut().for_each(|policy| {
        if let Ok(i) = plan
            .work_reviews
            .binary_search_by_key(&policy.person, |row| row.0)
        {
            policy.next_review = plan.work_reviews[i].1;
            policy.last_work_offer_at = Some(plan.work_reviews[i].2);
        }
    });
    super::learn_promises(opinions, &plan.proposals);
    Ok(())
}

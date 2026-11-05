//! Personal construction of work-and-goods proposals from bounded experience.
//! These records describe intentions; they do not create work, goods, or duties.
use crate::{Case, Decision, Mind as PersonalMind};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A person's own inclinations and bounded records, not an organization.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub invent: bool,
    pub approve: bool,
    /// Optional personal interpretation of the author's relationship.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<Evaluation>,
    pub willing: bool,
    pub ask_willing: bool,
    pub review_interval: u64,
    pub proposal_lifetime: u64,
    #[serde(default)]
    pub reserve: u64,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
    #[serde(default)]
    pub next_review: u64,
    #[serde(default)]
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authored: Option<Proposal>,
    /// One personally remembered request made using these terms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engagement: Option<Engagement>,
    /// Directly experienced gifts, independent of whether the good is food.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gifts: Vec<Gift>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inbox: Vec<Received>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evaluation {
    pub base: i64,
    #[serde(default)]
    pub affinity_weight: i64,
    #[serde(default)]
    pub trust_weight: i64,
    pub threshold: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Received {
    pub proposal: Proposal,
    pub delivered: u64,
    pub decision: Decision,
    pub approved: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Engagement {
    pub proposal: Proposal,
    pub request: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gift {
    pub tick: u64,
    pub other: u64,
    pub good: u32,
    pub quantity: u64,
    pub given: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GiftObservation {
    pub person: u64,
    pub gift: Gift,
}

pub fn valid(
    mind: &Mind,
    person: u64,
    people: &HashSet<u64>,
    goods: &HashSet<u32>,
    now: u64,
) -> bool {
    let valid_proposal = |p: &Proposal| {
        p.author != p.worker
            && people.contains(&p.author)
            && people.contains(&p.worker)
            && p.sequence > 0
            && p.recipe > 0
            && goods.contains(&p.good)
            && p.quantity > 0
            && p.created <= now
            && p.expires > p.created
    };
    mind.review_interval > 0
        && mind.proposal_lifetime > 0
        && mind.pressure_threshold != Some(0)
        && mind.inbox.len() <= crate::MAX_PROPOSALS
        && mind.gifts.len() <= crate::MAX_CASES
        && mind
            .gifts
            .windows(2)
            .all(|pair| pair[0].tick <= pair[1].tick)
        && mind.gifts.iter().all(|gift| {
            gift.tick <= now
                && gift.other != person
                && people.contains(&gift.other)
                && goods.contains(&gift.good)
                && gift.quantity > 0
        })
        && mind
            .authored
            .as_ref()
            .is_none_or(|p| valid_proposal(p) && p.author == person && p.sequence <= mind.sequence)
        && mind.engagement.as_ref().is_none_or(|entry| {
            entry.request > 0
                && valid_proposal(&entry.proposal)
                && entry.proposal.author == person
                && entry.proposal.sequence <= mind.sequence
        })
        && mind.inbox.iter().enumerate().all(|(i, r)| {
            valid_proposal(&r.proposal)
                && r.proposal.worker == person
                && r.delivered <= now
                && r.delivered >= r.proposal.created
                && r.delivered <= r.proposal.expires
                && (r.decision == Decision::Adopted || !r.approved)
                && !mind.inbox[..i].iter().any(|old| {
                    (old.proposal.author, old.proposal.sequence)
                        == (r.proposal.author, r.proposal.sequence)
                })
        })
}

/// A personally witnessed successful requested job, supplied by the work owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Witness {
    pub observer: u64,
    pub worker: u64,
    pub recipe: u32,
    pub request: u64,
    pub job: u64,
    pub tick: u64,
}

/// Terms proposed by one person to another; execution remains separate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub author: u64,
    pub sequence: u64,
    pub worker: u64,
    pub recipe: u32,
    pub good: u32,
    pub quantity: u64,
    pub reserve: u64,
    pub created: u64,
    pub expires: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Schedule {
    pub sequence: u64,
    pub lifetime: u64,
    pub reserve: u64,
    pub tick: u64,
}

pub struct Context<'a, C, P> {
    pub goals: &'a [(u64, u32)],
    pub witnesses: &'a [Witness],
    pub contacts: &'a [(u64, u64)],
    pub tick: u64,
    pub can_decide: C,
    pub pressure: P,
}

/// Record only direct observations, after current-boundary choices have been made.
pub fn remember_gifts(next: &mut crate::Practices, observations: &[GiftObservation]) {
    if observations.is_empty() {
        return;
    }
    let mut sorted = observations.to_vec();
    sorted.par_sort_unstable_by_key(|row| {
        (
            row.person,
            row.gift.tick,
            row.gift.other,
            row.gift.good,
            row.gift.quantity,
            row.gift.given,
        )
    });
    next.minds.par_iter_mut().for_each(|person| {
        let start = sorted.partition_point(|row| row.person < person.person);
        let end = sorted.partition_point(|row| row.person <= person.person);
        if start == end || person.work.is_none() {
            return;
        }
        let work = std::sync::Arc::make_mut(person.work.as_mut().expect("work mind"));
        let first = end.saturating_sub(crate::MAX_CASES).max(start);
        work.gifts
            .extend(sorted[first..end].iter().map(|row| row.gift));
        work.gifts.sort_unstable_by_key(|gift| {
            (gift.tick, gift.other, gift.good, gift.quantity, gift.given)
        });
        if work.gifts.len() > crate::MAX_CASES {
            work.gifts.drain(..work.gifts.len() - crate::MAX_CASES);
        }
    });
}

/// Deliver constructed proposals only to actual contacts; assessment is later.
/// The caller supplies committed personal evidence and a separately staged next generation.
pub fn advance<C, P>(
    prior: &crate::Practices,
    next: &mut crate::Practices,
    context: Context<'_, C, P>,
) -> Result<(), &'static str>
where
    C: Fn(u64) -> bool + Sync,
    P: Fn(u64, u64) -> u64 + Sync,
{
    advance_with_relationships(prior, next, context, |_, _| (0, 0))
}

/// Relationship values belong to the evaluating person and are never objective truth.
pub fn advance_with_relationships<C, P, R>(
    prior: &crate::Practices,
    next: &mut crate::Practices,
    context: Context<'_, C, P>,
    relationship: R,
) -> Result<(), &'static str>
where
    C: Fn(u64) -> bool + Sync,
    P: Fn(u64, u64) -> u64 + Sync,
    R: Fn(u64, u64) -> (i32, i32) + Sync,
{
    if !prior.minds.par_iter().any(|person| person.work.is_some()) {
        return Ok(());
    }
    let may_propose = prior.minds.par_iter().any(|person| {
        person.work.as_ref().is_some_and(|work| {
            work.invent
                && context.tick >= work.next_review
                && work
                    .authored
                    .as_ref()
                    .is_none_or(|p| p.expires < context.tick)
                && (context.can_decide)(person.person)
        })
    });
    if !may_propose {
        return next
            .minds
            .par_iter_mut()
            .try_for_each(|person| update_person(person, &[], None, &context, &relationship));
    }
    let mut minds: Vec<_> = prior.minds.par_iter().collect();
    if minds
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        minds.par_sort_unstable_by_key(|m| m.person);
    }
    let mut goals = context.goals.to_vec();
    goals.par_sort_unstable();
    let mut witnesses = context.witnesses.to_vec();
    witnesses
        .par_sort_unstable_by_key(|w| (w.observer, w.worker, w.recipe, w.tick, w.request, w.job));
    let mut contacts: Vec<_> = context
        .contacts
        .par_iter()
        .flat_map_iter(|&(a, b)| [(a, b), (b, a)])
        .collect();
    contacts.par_sort_unstable();
    let mut offers: Vec<_> = minds
        .par_iter()
        .filter_map(|person| {
            let work = person.work.as_ref()?;
            if !work.invent
                || context.tick < work.next_review
                || work
                    .authored
                    .as_ref()
                    .is_some_and(|p| p.expires >= context.tick)
                || !(context.can_decide)(person.person)
            {
                return None;
            }
            let g0 = goals.partition_point(|row| row.0 < person.person);
            let g1 = goals.partition_point(|row| row.0 <= person.person);
            let w0 = witnesses.partition_point(|row| row.observer < person.person);
            let w1 = witnesses.partition_point(|row| row.observer <= person.person);
            let sequence = match work.sequence.checked_add(1) {
                Some(value) => value,
                None => return Some(Err("work practice sequence overflow")),
            };
            let schedule = Schedule {
                sequence,
                lifetime: work.proposal_lifetime,
                reserve: work.reserve,
                tick: context.tick,
            };
            match propose_for_goals(
                person,
                |recipe| {
                    goals[g0..g1]
                        .binary_search(&(person.person, recipe))
                        .is_ok()
                },
                &witnesses[w0..w1],
                |a, b| {
                    contacts.binary_search(&(a, b)).is_ok()
                        && minds
                            .binary_search_by_key(&b, |m| m.person)
                            .ok()
                            .is_some_and(|i| minds[i].work.is_some())
                },
                schedule,
            ) {
                Ok(Some(proposal)) => Some(Ok(proposal)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect::<Result<_, _>>()?;
    offers.par_sort_unstable_by_key(|p| (p.worker, p.author, p.sequence));
    let offers: Vec<_> = offers
        .par_chunk_by(|a, b| a.worker == b.worker)
        .flat_map_iter(|group| {
            let target = minds[minds
                .binary_search_by_key(&group[0].worker, |m| m.person)
                .expect("known worker")];
            let retained = target
                .work
                .as_ref()
                .expect("work mind")
                .inbox
                .iter()
                .filter(|r| r.proposal.expires >= context.tick)
                .count();
            group
                .iter()
                .take(crate::MAX_PROPOSALS.saturating_sub(retained))
                .cloned()
        })
        .collect();
    let mut by_author: Vec<_> = offers.par_iter().collect();
    by_author.par_sort_unstable_by_key(|p| p.author);
    next.minds.par_iter_mut().try_for_each(|person| {
        let start = offers.partition_point(|p| p.worker < person.person);
        let end = offers.partition_point(|p| p.worker <= person.person);
        let authored = by_author
            .binary_search_by_key(&person.person, |p| p.author)
            .ok()
            .map(|i| by_author[i]);
        update_person(
            person,
            &offers[start..end],
            authored,
            &context,
            &relationship,
        )
    })
}

fn update_person<C, P, R>(
    person: &mut PersonalMind,
    offers: &[Proposal],
    authored: Option<&Proposal>,
    context: &Context<'_, C, P>,
    relationship: &R,
) -> Result<(), &'static str>
where
    C: Fn(u64) -> bool + Sync,
    P: Fn(u64, u64) -> u64 + Sync,
    R: Fn(u64, u64) -> (i32, i32) + Sync,
{
    let Some(existing) = person.work.as_ref() else {
        return Ok(());
    };
    let assess = existing.inbox.iter().any(|r| {
        r.decision == Decision::Pending
            && r.delivered < context.tick
            && r.proposal.expires >= context.tick
    }) && (context.can_decide)(person.person);
    let expired = existing
        .authored
        .as_ref()
        .is_some_and(|p| p.expires < context.tick)
        || existing.engagement.as_ref().is_some_and(|entry| {
            context.tick.saturating_sub(entry.proposal.expires) > existing.proposal_lifetime
        })
        || existing
            .inbox
            .iter()
            .any(|r| r.proposal.expires < context.tick);
    if !assess && !expired && authored.is_none() && offers.is_empty() {
        return Ok(());
    }
    let work = std::sync::Arc::make_mut(person.work.as_mut().expect("configured work mind"));
    if assess {
        for item in &mut work.inbox {
            if item.decision != Decision::Pending
                || item.delivered >= context.tick
                || item.proposal.expires < context.tick
            {
                continue;
            }
            item.approved = work.evaluation.map_or(work.approve, |evaluation| {
                let (affinity, trust) = relationship(person.person, item.proposal.author);
                i128::from(evaluation.base)
                    + i128::from(affinity) * i128::from(evaluation.affinity_weight)
                    + i128::from(trust) * i128::from(evaluation.trust_weight)
                    >= i128::from(evaluation.threshold)
            });
            let pressured = work.pressure_threshold.is_some_and(|threshold| {
                (context.pressure)(person.person, item.proposal.author) >= threshold
            });
            item.decision = if item.approved || pressured {
                Decision::Adopted
            } else {
                Decision::Declined
            };
        }
    }
    if work
        .authored
        .as_ref()
        .is_some_and(|p| p.expires < context.tick)
    {
        work.authored = None;
    }
    if work.engagement.as_ref().is_some_and(|entry| {
        context.tick.saturating_sub(entry.proposal.expires) > work.proposal_lifetime
    }) {
        work.engagement = None;
    }
    work.inbox.retain(|r| r.proposal.expires >= context.tick);
    if let Some(proposal) = authored {
        work.sequence = proposal.sequence;
        work.next_review = context
            .tick
            .checked_add(work.review_interval)
            .ok_or("work practice review overflow")?;
        work.authored = Some(proposal.clone());
    }
    for proposal in offers {
        work.inbox.push(Received {
            proposal: proposal.clone(),
            delivered: context.tick,
            decision: Decision::Pending,
            approved: false,
        });
    }
    Ok(())
}

/// Construct one candidate from this person's own goal, witnessed work, and
/// remembered gift. The host supplies only this person's bounded evidence.
pub fn propose(
    mind: &PersonalMind,
    goals: &[u32],
    witnesses: &[Witness],
    contact: impl Fn(u64, u64) -> bool,
    schedule: Schedule,
) -> Result<Option<Proposal>, &'static str> {
    propose_for_goals(
        mind,
        |recipe| goals.contains(&recipe),
        witnesses,
        contact,
        schedule,
    )
}

fn propose_for_goals(
    mind: &PersonalMind,
    has_goal: impl Fn(u32) -> bool,
    witnesses: &[Witness],
    contact: impl Fn(u64, u64) -> bool,
    schedule: Schedule,
) -> Result<Option<Proposal>, &'static str> {
    let Schedule {
        sequence,
        lifetime,
        reserve,
        tick,
    } = schedule;
    if sequence == 0 || lifetime == 0 {
        return Err("invalid work practice schedule");
    }
    let worker = witnesses
        .iter()
        .filter(|w| {
            w.observer == mind.person
                && w.worker != mind.person
                && w.request > 0
                && w.job > 0
                && w.tick <= tick
                && has_goal(w.recipe)
                && contact(mind.person, w.worker)
        })
        .max_by_key(|w| {
            (
                w.tick,
                std::cmp::Reverse(w.worker),
                std::cmp::Reverse(w.recipe),
                w.request,
                w.job,
            )
        });
    let gift = mind
        .cases
        .iter()
        .filter_map(|case| match *case {
            Case::Received {
                tick: observed,
                good,
                quantity,
                ..
            }
            | Case::Gave {
                tick: observed,
                good,
                quantity,
                ..
            } if observed <= tick && quantity > 0 => Some((observed, good, quantity)),
            _ => None,
        })
        .chain(
            mind.work
                .iter()
                .flat_map(|work| work.gifts.iter())
                .filter(|gift| gift.tick <= tick && gift.quantity > 0)
                .map(|gift| (gift.tick, gift.good, gift.quantity)),
        )
        .max();
    let Some((worker, (_, good, quantity))) = worker.zip(gift) else {
        return Ok(None);
    };
    Ok(Some(Proposal {
        author: mind.person,
        sequence,
        worker: worker.worker,
        recipe: worker.recipe,
        good,
        quantity,
        reserve,
        created: tick,
        expires: tick
            .checked_add(lifetime)
            .ok_or("work practice expiry overflow")?,
    }))
}

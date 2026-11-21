//! Connect personal practices to actual needs, contacts, requests, and gifts.
use crate::{Giving, Person, giving::Plan};
use game_lot_runtime::Events;
use game_lots::{Place, Transaction, View};
use game_opinions::Opinions;
use game_practices::{Case, Observation, Practices};
use game_production_model::Good;
use game_social::Encounter;
use rayon::prelude::*;
use std::collections::HashSet;

pub struct Context<'a> {
    pub giving: &'a Giving,
    pub people: &'a [Person],
    pub goods: &'a [Good],
    pub view: &'a View,
    pub opinions: Option<&'a Opinions>,
    pub goals: Option<&'a game_goals::Goals>,
    pub requests: Option<&'a game_autowork::requests::Requests>,
    pub work_asks: &'a [game_autowork::requests::Request],
    pub practice_requests: &'a [game_economy_work::PracticeRequest],
    pub started_work: &'a [game_economy_work::StartedRequest],
    pub early_start_times: &'a [(u64, u64, u64, u64)],
    pub early_finished: &'a [u64],
    pub early_failed: &'a [u64],
    pub early_gifts: &'a [(u64, u64, u32, u64, u64, u64)],
    pub encounters: &'a [Encounter],
    pub gifts: &'a Plan,
    /// Other personally received transfers, such as delivery of a promise.
    pub promise_deliveries: &'a [Transaction],
    pub events: &'a Events,
    pub tick: u64,
}
pub fn advance(
    owner: &Practices,
    context: Context<'_>,
) -> Result<(Practices, Vec<game_practices::work::outcome::Reaction>), String> {
    let food: Vec<_> = context
        .goods
        .iter()
        .filter_map(|good| good.food.as_ref().map(|f| (good.id, f.portion)))
        .collect();
    let food_ids: HashSet<_> = food.iter().map(|row| row.0).collect();
    let mut observations: Vec<_> = owner
        .minds
        .par_iter()
        .filter_map(|mind| {
            let person = context
                .people
                .binary_search_by_key(&mind.person, |p| p.id)
                .ok()
                .map(|i| &context.people[i])?;
            let holder = context.view.container(Place::Person(person.id))?;
            (person.health > 0
                && person.hunger >= mind.shortage_threshold
                && !holder.goods.iter().any(|good| {
                    food.iter()
                        .any(|&(id, portion)| good.good == id && good.quantity >= portion)
                }))
            .then_some(Observation {
                person: person.id,
                case: Case::Shortage {
                    tick: context.tick,
                    hunger: person.hunger,
                },
            })
        })
        .collect();
    let gifts = context.gifts.practice_gifts(context.events);
    observations.par_extend(
        gifts
            .par_iter()
            .filter(|row| food_ids.contains(&row.3))
            .map(|&(_, helper, person, good, quantity)| Observation {
                person,
                case: Case::Received {
                    tick: context.tick,
                    helper,
                    good,
                    quantity,
                },
            }),
    );
    observations.par_extend(
        gifts
            .par_iter()
            .filter(|row| food_ids.contains(&row.3))
            .map(|&(_, person, recipient, good, quantity)| Observation {
                person,
                case: Case::Gave {
                    tick: context.tick,
                    recipient,
                    good,
                    quantity,
                },
            }),
    );
    observations.par_extend(
        context
            .giving
            .requests
            .par_iter()
            .filter(|request| request.expires_at < context.tick)
            .map(|request| Observation {
                person: request.requester,
                case: Case::Unanswered {
                    tick: context.tick,
                    target: request.target,
                },
            }),
    );
    let contacts: Vec<_> = context
        .encounters
        .par_iter()
        .filter(|event| event.tick == context.tick)
        .map(|event| (event.a, event.b))
        .collect();
    let opinions = context.opinions.map(Opinions::index);
    let mut next = owner
        .prepare_with_relationships(
            &contacts,
            &observations,
            context.tick,
            (
                |person, other| {
                    opinions.as_ref().map_or(0, |index| {
                        let (fear, obligation) = index.feelings(person, other);
                        fear.saturating_add(obligation)
                    })
                },
                |person, other| {
                    opinions
                        .as_ref()
                        .map_or(0, |index| index.trust(person, other))
                },
                |person, other| {
                    opinions
                        .as_ref()
                        .map_or(0, |index| index.affinity(person, other))
                },
            ),
            |person| {
                context
                    .people
                    .binary_search_by_key(&person, |p| p.id)
                    .ok()
                    .is_some_and(|i| context.people[i].health > 0 && !context.people[i].controlled)
            },
            |person, good, needed| {
                context
                    .view
                    .container(Place::Person(person))
                    .is_some_and(|holder| holder.quantity(good) >= needed)
            },
        )
        .map_err(str::to_owned)?;
    let mut reactions = Vec::new();
    if owner.minds.par_iter().any(|mind| mind.work.is_some()) {
        let may_invent = owner.minds.par_iter().any(|mind| {
            mind.work.as_ref().is_some_and(|work| {
                work.invent
                    && context.tick >= work.next_review
                    && work
                        .authored
                        .as_ref()
                        .is_none_or(|p| p.expires < context.tick)
                    && context
                        .people
                        .binary_search_by_key(&mind.person, |p| p.id)
                        .ok()
                        .is_some_and(|i| {
                            context.people[i].health > 0 && !context.people[i].controlled
                        })
            })
        });
        let goals: Vec<_> = if may_invent {
            context
                .goals
                .map_or(&[][..], |owner| owner.people.as_slice())
                .par_iter()
                .filter(|goal| goal.enabled && goal.progress < goal.target)
                .map(|goal| (goal.person, goal.recipe))
                .collect()
        } else {
            Vec::new()
        };
        let witnesses: Vec<_> = if may_invent {
            context
                .requests
                .map_or(&[][..], |owner| owner.memories.as_slice())
                .par_iter()
                .filter_map(|memory| {
                    let receipt = memory.last_success?;
                    Some(game_practices::work::Witness {
                        observer: memory.observer,
                        worker: memory.worker,
                        recipe: memory.recipe,
                        request: receipt.request,
                        job: receipt.job,
                        tick: memory.last_success_at?,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        game_practices::work::advance_with_relationships(
            owner,
            &mut next,
            game_practices::work::Context {
                goals: &goals,
                witnesses: &witnesses,
                contacts: &contacts,
                tick: context.tick,
                can_decide: |person| {
                    context
                        .people
                        .binary_search_by_key(&person, |p| p.id)
                        .ok()
                        .is_some_and(|i| {
                            context.people[i].health > 0 && !context.people[i].controlled
                        })
                },
                pressure: |person, other| {
                    opinions.as_ref().map_or(0, |index| {
                        let (fear, obligation) = index.feelings(person, other);
                        fear.saturating_add(obligation)
                    })
                },
            },
            |person, other| {
                opinions.as_ref().map_or((0, 0), |index| {
                    (index.affinity(person, other), index.trust(person, other))
                })
            },
        )
        .map_err(str::to_owned)?;
        let mut intentions = context.practice_requests.to_vec();
        intentions.par_sort_unstable_by_key(|entry| (entry.requester, entry.worker, entry.recipe));
        let mut asks: Vec<_> = context.work_asks.par_iter().collect();
        asks.par_sort_unstable_by_key(|request| request.requester);
        next.minds.par_iter_mut().for_each(|person| {
            let Some(work) = person.work.as_ref() else {
                return;
            };
            let Some(proposal) = work.authored.as_ref() else {
                return;
            };
            let Ok(i) = asks.binary_search_by_key(&person.person, |request| request.requester)
            else {
                return;
            };
            let request = asks[i];
            let key = (request.requester, request.worker, request.recipe);
            if work.engagement.is_some()
                || intentions
                    .binary_search_by_key(&key, |entry| {
                        (entry.requester, entry.worker, entry.recipe)
                    })
                    .is_err()
                || (proposal.worker, proposal.recipe) != (request.worker, request.recipe)
            {
                return;
            }
            let engagement = game_practices::work::Engagement {
                proposal: proposal.clone(),
                request: request.id,
                delivered: 0,
            };
            std::sync::Arc::make_mut(person.work.as_mut().expect("work mind")).engagement =
                Some(engagement);
        });
        let mut completed = context.gifts.completed_work_gifts(context.events);
        completed.par_sort_unstable();
        if !completed.is_empty() {
            next.minds.par_iter_mut().for_each(|person| {
                let Some(work) = person.work.as_ref() else {
                    return;
                };
                let Some(entry) = work.engagement.as_ref() else {
                    return;
                };
                let Ok(index) = completed
                    .binary_search_by_key(&(person.person, entry.request), |row| (row.0, row.1))
                else {
                    return;
                };
                let work = std::sync::Arc::make_mut(person.work.as_mut().expect("work mind"));
                let entry = work.engagement.as_mut().expect("engagement");
                entry.delivered = entry.delivered.saturating_add(completed[index].2);
                if entry.delivered >= entry.proposal.quantity {
                    work.engagement = None;
                }
            });
        }
        let observed_gifts: Vec<_> = gifts
            .par_iter()
            .flat_map_iter(|&(action, giver, recipient, good, quantity)| {
                use game_practices::work::{Gift, GiftObservation};
                [
                    GiftObservation {
                        person: giver,
                        action,
                        gift: Gift {
                            tick: context.tick,
                            other: recipient,
                            good,
                            quantity,
                            given: true,
                        },
                    },
                    GiftObservation {
                        person: recipient,
                        action,
                        gift: Gift {
                            tick: context.tick,
                            other: giver,
                            good,
                            quantity,
                            given: false,
                        },
                    },
                ]
            })
            .collect();
        if owner.minds.par_iter().any(|mind| {
            mind.work
                .as_ref()
                .is_some_and(|work| work.outcome_policy.is_some() || !work.experiences.is_empty())
        }) {
            // A worker can notice a real receipt even when it was delivered
            // through a promise rather than the direct-gift action. The
            // sender's private reason for transferring it is not consulted.
            let mut receipts = std::borrow::Cow::Borrowed(observed_gifts.as_slice());
            let mut gift_times = Vec::new();
            if !context.early_gifts.is_empty() {
                let base = context
                    .events
                    .physical_actions
                    .par_iter()
                    .map(|row| row.id)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or("practice gift action overflow")?;
                base.checked_add(context.early_gifts.len() as u64)
                    .ok_or("practice gift action overflow")?;
                let early_tick = context
                    .tick
                    .checked_sub(1)
                    .ok_or("invalid early gift tick")?;
                gift_times = context
                    .early_gifts
                    .par_iter()
                    .enumerate()
                    .map(|(index, &(person, _, _, _, offset, order))| {
                        (person, base + index as u64, early_tick, offset, order)
                    })
                    .collect();
                receipts
                    .to_mut()
                    .par_extend(context.early_gifts.par_iter().enumerate().map(
                        |(index, &(person, giver, good, quantity, _, _))| {
                            game_practices::work::GiftObservation {
                                person,
                                action: base + index as u64,
                                gift: game_practices::work::Gift {
                                    tick: early_tick,
                                    other: giver,
                                    good,
                                    quantity,
                                    given: false,
                                },
                            }
                        },
                    ));
            }
            if !context.promise_deliveries.is_empty() {
                let accepted: HashSet<_> = context
                    .events
                    .physical_actions
                    .par_iter()
                    .filter(|event| event.accepted)
                    .map(|event| event.id)
                    .collect();
                receipts.to_mut().par_extend(
                    context
                        .promise_deliveries
                        .par_iter()
                        .filter(|action| accepted.contains(&action.id))
                        .flat_map_iter(|action| {
                            action.movements.iter().filter_map(|movement| {
                                let (Place::Person(giver), Some(Place::Person(recipient))) =
                                    (movement.from, movement.to)
                                else {
                                    return None;
                                };
                                Some(game_practices::work::GiftObservation {
                                    person: recipient,
                                    action: action.id,
                                    gift: game_practices::work::Gift {
                                        tick: context.tick,
                                        other: giver,
                                        good: movement.good,
                                        quantity: movement.quantity,
                                        given: false,
                                    },
                                })
                            })
                        }),
                );
            }
            let starts: Vec<_> = context
                .started_work
                .par_iter()
                .map(|row| game_practices::work::outcome::Start {
                    requester: row.requester,
                    worker: row.worker,
                    recipe: row.recipe,
                    request: row.request,
                    job: row.job,
                })
                .collect();
            let mut failed: Vec<_> = context
                .events
                .interrupted
                .par_iter()
                .chain(context.events.abandoned.par_iter())
                .copied()
                .collect();
            failed.par_extend(context.early_failed.par_iter().copied());
            let mut finished = context.events.finished.clone();
            finished.par_extend(context.early_finished.par_iter().copied());
            reactions = game_practices::work::outcome::advance_with_timing(
                owner,
                &mut next,
                &starts,
                &finished,
                &failed,
                &receipts,
                (
                    context.tick,
                    Some(&game_practices::work::outcome::Timing {
                        starts: context.early_start_times,
                        gifts: &gift_times,
                    }),
                ),
            );
        }
        game_practices::work::remember_gifts(&mut next, &observed_gifts);
    }
    Ok((next, reactions))
}

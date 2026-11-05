//! Connect personal practices to actual needs, contacts, requests, and gifts.
use crate::{Giving, Person, giving::Plan};
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Place, Transaction, View};
use game_parallel_opinions::Opinions;
use game_parallel_practices::{Case, Observation, Practices};
use game_parallel_production_model::Good;
use game_parallel_social::Encounter;
use rayon::prelude::*;
use std::collections::HashSet;

pub struct Context<'a> {
    pub giving: &'a Giving,
    pub people: &'a [Person],
    pub goods: &'a [Good],
    pub view: &'a View,
    pub opinions: Option<&'a Opinions>,
    pub goals: Option<&'a game_parallel_goals::Goals>,
    pub requests: Option<&'a game_parallel_autowork::requests::Requests>,
    pub work_asks: &'a [game_parallel_autowork::requests::Request],
    pub practice_requests: &'a [game_parallel_economy_work::PracticeRequest],
    pub started_work: &'a [game_parallel_economy_work::StartedRequest],
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
) -> Result<
    (
        Practices,
        Vec<game_parallel_practices::work::outcome::Reaction>,
    ),
    String,
> {
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
                    Some(game_parallel_practices::work::Witness {
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
        game_parallel_practices::work::advance_with_relationships(
            owner,
            &mut next,
            game_parallel_practices::work::Context {
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
            let engagement = game_parallel_practices::work::Engagement {
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
                use game_parallel_practices::work::{Gift, GiftObservation};
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
                                Some(game_parallel_practices::work::GiftObservation {
                                    person: recipient,
                                    action: action.id,
                                    gift: game_parallel_practices::work::Gift {
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
                .map(|row| game_parallel_practices::work::outcome::Start {
                    requester: row.requester,
                    worker: row.worker,
                    recipe: row.recipe,
                    request: row.request,
                    job: row.job,
                })
                .collect();
            let failed: Vec<_> = context
                .events
                .interrupted
                .par_iter()
                .chain(context.events.abandoned.par_iter())
                .copied()
                .collect();
            reactions = game_parallel_practices::work::outcome::advance(
                owner,
                &mut next,
                &starts,
                &context.events.finished,
                &failed,
                &receipts,
                context.tick,
            );
        }
        game_parallel_practices::work::remember_gifts(&mut next, &observed_gifts);
    }
    Ok((next, reactions))
}

/// Reclaim independent old personal generations without serializing a large tick.
pub fn reclaim(owner: Option<Giving>) {
    if let Some(mut giving) = owner {
        if let Some(practices) = giving.practices.take() {
            practices.minds.into_par_iter().for_each(drop);
        }
        drop(giving);
    }
}

macro_rules! methods {
    () => {
        /// Administrative access; player views expose only the current person's mind.
        pub fn personal_practice(&self, person: u64) -> Option<&game_parallel_practices::Mind> {
            self.giving
                .as_ref()?
                .practices
                .as_ref()?
                .minds
                .iter()
                .find(|mind| mind.person == person)
        }
        fn edit_practice(
            &mut self,
            person: u64,
            edit: impl FnOnce(&mut game_parallel_practices::Mind) -> Result<(), String>,
        ) -> Result<(), String> {
            if self
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .is_none_or(|i| self.people[i].health == 0)
            {
                return Err("person must be alive".into());
            }
            let mut giving = self.giving.clone().ok_or("giving is disabled")?;
            let mind = giving
                .practices
                .as_mut()
                .ok_or("personal practices are disabled")?
                .minds
                .iter_mut()
                .find(|mind| mind.person == person)
                .ok_or("person has no practice mind")?;
            edit(mind)?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.giving = Some(giving);
            self.epoch = epoch;
            Ok(())
        }
        pub fn set_practice_invention(&mut self, person: u64, enabled: bool) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                mind.invent = enabled;
                Ok(())
            })
        }
        pub fn set_practice_approval(&mut self, person: u64, enabled: bool) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                mind.approve = enabled;
                Ok(())
            })
        }
        pub fn set_practice_willingness(
            &mut self,
            person: u64,
            enabled: bool,
        ) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                mind.willing = enabled;
                Ok(())
            })
        }
        pub fn set_practice_asking(&mut self, person: u64, enabled: bool) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                mind.ask_willing = enabled;
                Ok(())
            })
        }
        /// Change this person's own present choices; no proposal compels them.
        pub fn set_work_practice_choice(
            &mut self,
            person: u64,
            choice: &str,
            enabled: bool,
        ) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                let work = mind
                    .work
                    .as_mut()
                    .ok_or("person has no work practice mind")?;
                let work = std::sync::Arc::make_mut(work);
                match choice {
                    "invent" => work.invent = enabled,
                    "approve" => {
                        work.approve = enabled;
                        work.evaluation = None;
                    }
                    "willing" => work.willing = enabled,
                    "ask" => work.ask_willing = enabled,
                    _ => return Err("unknown work practice choice".into()),
                }
                Ok(())
            })
        }
        /// Set or clear this person's signed relationship interpretation.
        pub fn set_work_practice_evaluation(
            &mut self,
            person: u64,
            evaluation: Option<game_parallel_practices::work::Evaluation>,
        ) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                let work = mind
                    .work
                    .as_mut()
                    .ok_or("person has no work practice mind")?;
                std::sync::Arc::make_mut(work).evaluation = evaluation;
                Ok(())
            })
        }
        pub fn set_work_outcome_policy(
            &mut self,
            person: u64,
            policy: Option<game_parallel_practices::work::outcome::Policy>,
        ) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                let work = mind
                    .work
                    .as_mut()
                    .ok_or("person has no work practice mind")?;
                std::sync::Arc::make_mut(work).outcome_policy = policy;
                Ok(())
            })
        }
        pub fn set_work_practice_reserve(
            &mut self,
            person: u64,
            reserve: u64,
        ) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                let work = mind
                    .work
                    .as_mut()
                    .ok_or("person has no work practice mind")?;
                std::sync::Arc::make_mut(work).reserve = reserve;
                Ok(())
            })
        }
        /// Independently accept or reject a proposal this person actually received.
        pub fn decide_work_practice(
            &mut self,
            person: u64,
            author: u64,
            sequence: u64,
            approve: bool,
        ) -> Result<(), String> {
            let now = self.tick();
            self.edit_practice(person, |mind| {
                let work = mind
                    .work
                    .as_mut()
                    .ok_or("person has no work practice mind")?;
                let work = std::sync::Arc::make_mut(work);
                let item = work
                    .inbox
                    .iter_mut()
                    .find(|item| {
                        item.proposal.author == author
                            && item.proposal.sequence == sequence
                            && item.proposal.expires >= now
                    })
                    .ok_or("no current received work proposal")?;
                item.approved = approve;
                item.decision = if approve {
                    game_parallel_practices::Decision::Adopted
                } else {
                    game_parallel_practices::Decision::Declined
                };
                Ok(())
            })
        }
        pub fn set_practice_reporting(
            &mut self,
            person: u64,
            reporting: game_parallel_practices::Reporting,
        ) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                mind.reporting = reporting;
                Ok(())
            })
        }
        pub fn set_practice_report_attitude(
            &mut self,
            person: u64,
            attitude: game_parallel_practices::ReportAttitude,
        ) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                mind.report_attitude = attitude;
                Ok(())
            })
        }
        pub fn set_practice_require_report(
            &mut self,
            person: u64,
            enabled: bool,
        ) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                mind.require_believed_ready = enabled;
                Ok(())
            })
        }
        pub fn set_practice_unanswered_belief(
            &mut self,
            person: u64,
            belief: Option<game_parallel_practices::Belief>,
        ) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                mind.unanswered_report_belief = belief;
                Ok(())
            })
        }
        pub fn set_practice_value_learning(
            &mut self,
            person: u64,
            value: Option<game_parallel_practices::ValueLearning>,
        ) -> Result<(), String> {
            if value.is_some_and(|v| v.value < v.minimum || v.value > v.maximum) {
                return Err("invalid personal practice value".into());
            }
            self.edit_practice(person, |mind| {
                mind.value_learning = value.map(std::sync::Arc::new);
                Ok(())
            })
        }
        pub fn set_practice_precedent(
            &mut self,
            person: u64,
            precedent: Option<game_parallel_practices::PrecedentReasoning>,
        ) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                mind.precedent = precedent.map(std::sync::Arc::new);
                Ok(())
            })
        }
        pub fn assess_practice_report(
            &mut self,
            person: u64,
            speaker: u64,
            sequence: u64,
            belief: game_parallel_practices::Belief,
        ) -> Result<(), String> {
            self.edit_practice(person, |mind| {
                mind.assess(speaker, sequence, belief)
                    .map_err(str::to_owned)
            })
        }
        pub fn decide_practice(
            &mut self,
            person: u64,
            author: u64,
            sequence: u64,
            approve: bool,
        ) -> Result<(), String> {
            let now = self.tick();
            self.edit_practice(person, |mind| {
                mind.decide(author, sequence, approve, now)
                    .map_err(str::to_owned)
            })
        }
        pub fn withdraw_practice(
            &mut self,
            person: u64,
            author: u64,
            sequence: u64,
        ) -> Result<(), String> {
            let now = self.tick();
            self.edit_practice(person, |mind| {
                mind.withdraw(author, sequence, now).map_err(str::to_owned)
            })
        }
        /// Propose personal work terms at a real meeting, without checking the speaker's stock.
        pub fn propose_work_practice(
            &mut self,
            author: u64,
            worker: u64,
            recipe: u32,
            good: u32,
            quantity: u64,
            lifetime: u64,
        ) -> Result<bool, String> {
            if self.campaign.as_ref().is_some_and(|c| {
                c.lineage.current == Some(author)
                    && c.lineage.controlled_person(self.tick()) != Some(author)
            }) {
                return Err("player control is suspended".into());
            }
            game_parallel_economy_campaign::preflight!(self);
            if quantity == 0
                || lifetime == 0
                || author == worker
                || !self.physical.recipes().iter().any(|row| row.id == recipe)
                || !self.physical.goods().iter().any(|row| row.id == good)
            {
                return Err("invalid personal work terms".into());
            }
            let tick = self.tick().checked_add(1).ok_or("tick overflow")?;
            let practices = self
                .giving
                .as_ref()
                .and_then(|giving| giving.practices.as_ref())
                .ok_or("personal practices are disabled")?;
            let own = practices
                .minds
                .iter()
                .find(|mind| mind.person == author)
                .and_then(|mind| mind.work.as_ref())
                .ok_or("person has no work practice mind")?;
            if practices
                .minds
                .iter()
                .find(|mind| mind.person == worker)
                .and_then(|mind| mind.work.as_ref())
                .is_none()
            {
                return Err("recipient has no work practice mind".into());
            }
            let sequence = own
                .sequence
                .checked_add(1)
                .ok_or("work practice sequence overflow")?;
            let expires = tick
                .checked_add(lifetime)
                .ok_or("work practice expiry overflow")?;
            let next_review = tick
                .checked_add(own.review_interval)
                .ok_or("work practice review overflow")?;
            let reserve = own.reserve;
            let mut draft = self.prepare_interaction(
                Some(game_parallel_economy_interaction::Request {
                    actor: author,
                    target: worker,
                }),
                None,
                None,
                None,
            )?;
            let met = draft
                .dynamics
                .as_ref()
                .and_then(|d| d.social.as_ref())
                .is_some_and(|social| {
                    social
                        .events
                        .iter()
                        .any(|event| event.a == author.min(worker) && event.b == author.max(worker))
                });
            let mut delivered = false;
            if met {
                let practices = draft
                    .giving
                    .as_mut()
                    .and_then(|giving| giving.practices.as_mut())
                    .ok_or("personal practices are disabled")?;
                let recipient = practices
                    .minds
                    .iter()
                    .position(|mind| mind.person == worker)
                    .ok_or("unknown recipient")?;
                let has_room = practices.minds[recipient]
                    .work
                    .as_ref()
                    .is_some_and(|work| work.inbox.len() < game_parallel_practices::MAX_PROPOSALS);
                if has_room {
                    let proposal = game_parallel_practices::work::Proposal {
                        author,
                        sequence,
                        worker,
                        recipe,
                        good,
                        quantity,
                        reserve,
                        created: tick,
                        expires,
                    };
                    let sender = practices
                        .minds
                        .iter_mut()
                        .find(|mind| mind.person == author)
                        .and_then(|mind| mind.work.as_mut())
                        .ok_or("person has no work practice mind")?;
                    let sender = std::sync::Arc::make_mut(sender);
                    sender.sequence = sequence;
                    sender.next_review = next_review;
                    sender.authored = Some(proposal.clone());
                    let target = std::sync::Arc::make_mut(
                        practices.minds[recipient].work.as_mut().expect("work mind"),
                    );
                    target.inbox.push(game_parallel_practices::work::Received {
                        proposal,
                        delivered: tick,
                        decision: game_parallel_practices::Decision::Pending,
                        approved: false,
                    });
                    delivered = true;
                }
            }
            self.commit(draft)?;
            Ok(delivered)
        }
        /// Attempt a real meeting and, if it happens, communicate the chosen claim.
        pub fn report_practice(
            &mut self,
            speaker: u64,
            listener: u64,
            sequence: u64,
            claimed_ready: bool,
        ) -> Result<bool, String> {
            if self.campaign.as_ref().is_some_and(|c| {
                c.lineage.current == Some(speaker)
                    && c.lineage.controlled_person(self.tick()) != Some(speaker)
            }) {
                return Err("player control is suspended".into());
            }
            game_parallel_economy_campaign::preflight!(self);
            let next_tick = self.tick().checked_add(1).ok_or("tick overflow")?;
            self.giving
                .as_ref()
                .and_then(|giving| giving.practices.as_ref())
                .ok_or("personal practices are disabled")?
                .check_speech(speaker, listener, sequence, next_tick)
                .map_err(str::to_owned)?;
            let mut draft = self.prepare_interaction(
                Some(game_parallel_economy_interaction::Request {
                    actor: speaker,
                    target: listener,
                }),
                None,
                None,
                None,
            )?;
            let met = draft
                .dynamics
                .as_ref()
                .and_then(|d| d.social.as_ref())
                .is_some_and(|social| {
                    social.events.iter().any(|event| {
                        event.a == speaker.min(listener) && event.b == speaker.max(listener)
                    })
                });
            if met {
                let tick = draft.physical.events().tick;
                draft
                    .giving
                    .as_mut()
                    .and_then(|giving| giving.practices.as_mut())
                    .ok_or("personal practices are disabled")?
                    .speak(speaker, listener, sequence, claimed_ready, tick)
                    .map_err(str::to_owned)?;
            }
            self.commit(draft)?;
            Ok(met)
        }
    };
}
pub(crate) use methods;

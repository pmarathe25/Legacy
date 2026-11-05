//! Connect personal practices to actual needs, contacts, requests, and gifts.
use crate::{Giving, Person, giving::Plan};
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Place, View};
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
    pub encounters: &'a [Encounter],
    pub gifts: &'a Plan,
    pub events: &'a Events,
    pub tick: u64,
}
pub fn advance(owner: &Practices, context: Context<'_>) -> Result<Practices, String> {
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
            .filter(|row| food_ids.contains(&row.2))
            .map(|&(helper, person, good, quantity)| Observation {
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
            .filter(|row| food_ids.contains(&row.2))
            .map(|&(person, recipient, good, quantity)| Observation {
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
    owner
        .prepare_with_evidence(
            &contacts,
            &observations,
            context.tick,
            |person, other| {
                opinions.as_ref().map_or(0, |index| {
                    let (fear, obligation) = index.feelings(person, other);
                    fear.saturating_add(obligation)
                })
            },
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
        .map_err(str::to_owned)
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

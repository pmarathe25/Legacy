//! An accepted historical controller acts from its own memories and goals.
use crate::{Burden, PreparedControl, Runtime, archive};
use game_characters::decisions::{
    Aspiration, ChildSignal, Goal, Personality, Relationship, SiteKnowledge,
};
use game_core::*;
use std::collections::BTreeMap;

pub type RouteMemory = BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>;
#[derive(Clone, Copy)]
pub struct ControllerMemory<'a> {
    pub goal: Goal,
    pub dream: Aspiration,
    pub stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
    pub sites: &'a BTreeMap<SiteId, SiteKnowledge>,
    pub relationships: &'a BTreeMap<PersonId, Relationship>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerActivity {
    Other,
    Eat,
    Load,
    Care,
    Tell,
    Lesson,
    Memory,
    Work { site: SiteId },
}
impl ControllerActivity {
    fn protected(self) -> bool {
        matches!(
            self,
            Self::Eat | Self::Load | Self::Care | Self::Tell | Self::Lesson | Self::Memory
        )
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ControllerPerson {
    pub person: PersonId,
    pub alive: bool,
    pub contact: bool,
}
pub struct ControllerInput<'a, I> {
    pub actor: PersonId,
    pub changed: bool,
    pub in_combat: bool,
    pub independent: bool,
    pub area: Option<AreaId>,
    pub activity: ControllerActivity,
    pub hunger: u64,
    pub meal_relief: u64,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub personality: &'a Personality,
    pub past: ControllerMemory<'a>,
    pub current_stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
    pub current_sites: &'a BTreeMap<SiteId, SiteKnowledge>,
    pub child_signals: &'a BTreeMap<PersonId, ChildSignal>,
    /// Local bodies in the factual provider's deterministic roster order.
    pub local_people: I,
    pub current_routes: &'a RouteMemory,
    pub historical_routes: &'a [&'a RouteMemory],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerAction {
    Rest,
    Cancel,
    Eat {
        source: InventoryId,
    },
    Move {
        destination: AreaId,
    },
    Engage {
        target: PersonId,
    },
    Care {
        child: PersonId,
        source: InventoryId,
    },
    Work {
        site: SiteId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerDecision {
    Keep,
    Act {
        action: ControllerAction,
        ready: bool,
    },
}
impl ControllerDecision {
    /// Failed physical admission causes a rest attempt, never a different
    /// candidate selected from privileged actual stock or location knowledge.
    pub fn after_failure(self) -> Option<ControllerAction> {
        match self {
            Self::Act {
                action: ControllerAction::Cancel,
                ..
            }
            | Self::Keep => None,
            Self::Act { .. } => Some(ControllerAction::Rest),
        }
    }
}
fn overlay<'a, K: Ord, V>(
    past: &'a BTreeMap<K, V>,
    current: &'a BTreeMap<K, V>,
) -> impl Iterator<Item = (&'a K, &'a V)> {
    let mut past = past.iter().peekable();
    let mut current = current.iter().peekable();
    std::iter::from_fn(move || match (past.peek(), current.peek()) {
        (Some((a, _)), Some((b, _))) => match a.cmp(b) {
            std::cmp::Ordering::Less => past.next(),
            std::cmp::Ordering::Greater => current.next(),
            std::cmp::Ordering::Equal => {
                past.next();
                current.next()
            }
        },
        (Some(_), None) => past.next(),
        (None, Some(_)) => current.next(),
        (None, None) => None,
    })
}
pub fn recalled_routes<'a>(
    current: &RouteMemory,
    historical: impl IntoIterator<Item = &'a RouteMemory>,
) -> RouteMemory {
    let mut routes = current.clone();
    for graph in historical {
        for (from, links) in graph {
            for (to, duration) in links {
                routes
                    .entry(*from)
                    .or_default()
                    .entry(*to)
                    .or_insert(*duration);
            }
        }
    }
    routes
}
pub fn controller_personality(past: &Personality, burden: &Burden) -> Personality {
    let mut result = past.clone();
    result.discipline = result.discipline.saturating_sub(
        burden
            .temporary_identity
            .saturating_add(burden.permanent_identity)
            .saturating_mul(5),
    );
    result.aggression = result
        .aggression
        .saturating_add(burden.emotional_disruption)
        .min(1000);
    result.patience = result.patience.saturating_sub(burden.emotional_disruption);
    result
}
pub fn choose_controller_action<I: IntoIterator<Item = ControllerPerson>>(
    input: ControllerInput<'_, I>,
) -> ControllerDecision {
    use ControllerAction::*;
    if input.in_combat || !input.independent {
        return ControllerDecision::Keep;
    }
    let Some(area) = input.area else {
        return if input.changed && input.past.dream == Aspiration::Comfort {
            ControllerDecision::Act {
                action: Cancel,
                ready: true,
            }
        } else {
            ControllerDecision::Keep
        };
    };
    if !input.changed && input.activity.protected() {
        return ControllerDecision::Keep;
    }
    let stock = |id| {
        input
            .current_stocks
            .get(id)
            .or_else(|| input.past.stocks.get(id))
    };
    let mut ready = true;
    let action = if input.hunger >= input.meal_relief
        || input.past.goal == Goal::Survival && input.hunger >= input.meal_relief / 3
    {
        let routes = recalled_routes(
            input.current_routes,
            input.historical_routes.iter().copied(),
        );
        overlay(input.past.stocks, input.current_stocks)
            .map(|(_, row)| row)
            .filter(|row| row.good == Good::Food && row.accessible && row.quantity.0 >= 1000)
            .filter_map(|row| {
                if row.area == area {
                    Some((
                        0,
                        Eat {
                            source: row.inventory,
                        },
                    ))
                } else {
                    game_characters::decisions::known_path(&routes, area, row.area)
                        .ok()
                        .map(|(_, duration)| {
                            (
                                duration.0,
                                Move {
                                    destination: row.area,
                                },
                            )
                        })
                }
            })
            .min_by_key(|(cost, _)| *cost)
            .map(|(_, action)| action)
            .unwrap_or(Rest)
    } else if input.fatigue > input.fatigue_limit / 2
        || input.past.dream == Aspiration::Comfort
        || input.personality.hedonism > input.personality.discipline
    {
        Rest
    } else if input.personality.aggression >= 800 && input.personality.risk_tolerance >= 600 {
        input
            .local_people
            .into_iter()
            .filter(|person| {
                person.alive
                    && person.person != input.actor
                    && input
                        .past
                        .relationships
                        .get(&person.person)
                        .is_some_and(|row| row.trust < 0)
            })
            .take(64)
            .next()
            .map_or(Rest, |person| {
                ready = person.contact;
                Engage {
                    target: person.person,
                }
            })
    } else if (input.past.dream == Aspiration::SupportOthers || input.past.goal == Goal::Care)
        && input.personality.empathy >= 500
    {
        input
            .child_signals
            .iter()
            .filter(|(_, signal)| signal.location == Location::AtArea(area) && signal.urgency > 0)
            .find_map(|(child, _)| {
                overlay(input.past.stocks, input.current_stocks)
                    .map(|(_, row)| row)
                    .find(|row| {
                        row.area == area
                            && row.good == Good::Food
                            && row.accessible
                            && row.quantity.0 >= 1000
                    })
                    .map(|row| Care {
                        child: *child,
                        source: row.inventory,
                    })
            })
            .unwrap_or(Rest)
    } else {
        overlay(input.past.sites, input.current_sites)
            .map(|(_, row)| row)
            .filter(|row| row.access)
            .filter(|row| {
                stock(&row.input)
                    .is_some_and(|known| known.accessible && known.quantity >= row.input_quantity)
                    && stock(&row.tool)
                        .is_some_and(|known| known.accessible && known.quantity.0 >= 1000)
            })
            .map(|row| {
                if row.area == area {
                    Work { site: row.site }
                } else {
                    Move {
                        destination: row.area,
                    }
                }
            })
            .next()
            .unwrap_or(Rest)
    };
    if !input.changed
        && matches!(action,Work{site} if input.activity==ControllerActivity::Work{site})
    {
        ControllerDecision::Keep
    } else {
        ControllerDecision::Act { action, ready }
    }
}
#[cfg(test)]
mod tests;

/// Actual body facts for a controller turn. Historical and current memory joins
/// are resolved by Invocation from their canonical component rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControllerFacts {
    pub in_combat: bool,
    pub independent: bool,
    pub area: Option<AreaId>,
    pub activity: ControllerActivity,
    pub hunger: u64,
    pub meal_relief: u64,
    pub fatigue: u64,
    pub fatigue_limit: u64,
}
impl Runtime {
    pub fn controller_decision<A: game_ecs::Component>(
        &self,
        store: &game_ecs::Store,
        prepared: &PreparedControl,
        facts: ControllerFacts,
        local_people: impl IntoIterator<Item = ControllerPerson>,
    ) -> Result<ControllerDecision, Error> {
        prepared.check(&self.control, store)?;
        let subject = prepared.controller_subject().ok_or(Error::Unavailable)?;
        let active = self.control.active(store).ok_or(Error::Unavailable)?;
        let past = archive::<A>(store, subject.life).ok_or(Error::Unavailable)?;
        let current = game_characters::decisions::mind::<A>(store, subject.actor)
            .ok_or(Error::Unavailable)?;
        let current_stocks =
            game_characters::decisions::stocks(store, subject.actor).ok_or(Error::Unavailable)?;
        let historical_routes = active
            .lives
            .iter()
            .map(|life| {
                archive::<A>(store, *life)
                    .map(|past| &past.mind.routes)
                    .ok_or(Error::Unavailable)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let personality = prepared
            .controller_personality::<A>(store)
            .ok_or(Error::Unavailable)?;
        Ok(choose_controller_action(ControllerInput {
            actor: subject.actor,
            changed: subject.changed,
            in_combat: facts.in_combat,
            independent: facts.independent,
            area: facts.area,
            activity: facts.activity,
            hunger: facts.hunger,
            meal_relief: facts.meal_relief,
            fatigue: facts.fatigue,
            fatigue_limit: facts.fatigue_limit,
            personality: &personality,
            past: ControllerMemory {
                goal: past.mind.goal,
                dream: past.mind.dream,
                stocks: &past.stocks.known,
                sites: &past.mind.sites,
                relationships: &past.mind.relationships,
            },
            current_stocks: &current_stocks.known,
            current_sites: &current.sites,
            child_signals: &current.child_signals,
            local_people,
            current_routes: &current.routes,
            historical_routes: &historical_routes,
        }))
    }
}

/// A body port supplies actual facts and the native rows producing them. It
/// makes no controller choices or receiving character policy decisions.
pub trait ControllerFactsSource {
    fn facts(&self, actor: PersonId) -> Result<ControllerFacts, Error>;
    fn protect(&self, store: &game_ecs::Store, actor: PersonId, reads: &mut game_ecs::ReadSet);
}
#[derive(Debug)]
pub struct PreparedControllerReview {
    prepared: PreparedControl,
    actor: PersonId,
    facts: ControllerFacts,
    decision: ControllerDecision,
    reads: game_ecs::ReadSet,
}
#[derive(Debug)]
pub struct ControllerReviewReceipt {
    control: crate::ControlReceipt,
    reads: game_ecs::ReadGuard,
    pub actor: PersonId,
    pub decision: ControllerDecision,
}
impl Runtime {
    pub fn prepare_controller_review<A: game_ecs::Component>(
        &self,
        store: &game_ecs::Store,
        prepared: PreparedControl,
        source: &impl ControllerFactsSource,
        local_people: impl IntoIterator<Item = ControllerPerson>,
    ) -> Result<PreparedControllerReview, Error> {
        let actor = prepared
            .controller_subject()
            .ok_or(Error::Unavailable)?
            .actor;
        let facts = source.facts(actor)?;
        let mut reads = game_ecs::ReadSet::new();
        source.protect(store, actor, &mut reads);
        reads.read::<game_characters::decisions::Mind<A>>(store, game_ecs::StableId(actor.0));
        reads.read::<game_characters::decisions::Stocks>(store, game_ecs::StableId(actor.0));
        reads.read::<Burden>(store, game_ecs::StableId(actor.0));
        for life in &self.control.active(store).ok_or(Error::Unavailable)?.lives {
            reads.read::<crate::PastLife<A>>(store, game_ecs::StableId(life.0));
        }
        let people = local_people
            .into_iter()
            .inspect(|person| source.protect(store, person.person, &mut reads));
        let decision = self.controller_decision::<A>(store, &prepared, facts, people)?;
        Ok(PreparedControllerReview {
            prepared,
            actor,
            facts,
            decision,
            reads,
        })
    }
}
impl PreparedControllerReview {
    pub fn actor(&self) -> PersonId {
        self.actor
    }
    pub fn decision(&self) -> ControllerDecision {
        self.decision
    }
    pub fn stage_into(
        self,
        control: &crate::ControlState,
        store: &game_ecs::Store,
        source: &impl ControllerFactsSource,
        batch: &mut game_ecs::Batch,
    ) -> Result<ControllerReviewReceipt, Error> {
        if source.facts(self.actor)? != self.facts {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        let receipt = self.prepared.stage_into(control, store, batch)?;
        Ok(ControllerReviewReceipt {
            control: receipt,
            reads: self
                .reads
                .bind(store, batch, &game_ecs::ReadFootprint::new())?,
            actor: self.actor,
            decision: self.decision,
        })
    }
}
impl ControllerReviewReceipt {
    pub fn validate_batch(
        &self,
        control: &crate::ControlState,
        store: &game_ecs::Store,
        batch: &game_ecs::Batch,
    ) -> Result<(), Error> {
        self.control.validate_batch(control, store, batch)?;
        self.reads.validate(store, batch)
    }
}

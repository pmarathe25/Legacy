//! Bounded persistent order changes over immutable incoming-strike topology.
use game_parallel_conflict::{Command, Conflict, Event, Order};
use game_parallel_conflict_packed::{Damage, Plan as Base, Round as BaseRound};
use rayon::prelude::*;
use std::{collections::BTreeMap, sync::Arc};
const LIMIT: usize = 4096;
#[derive(Clone)]
struct Strike {
    encounter: u64,
    region: u64,
    attacker: u64,
    target: u64,
    damage: u32,
}
#[derive(Clone)]
pub struct Plan {
    base: Base,
    edits: Arc<BTreeMap<u64, Option<Strike>>>,
    damage: u32,
}
pub struct Round {
    base: BaseRound,
    extra: Vec<Event>,
}
impl Plan {
    pub fn compile(conflict: &Conflict) -> Result<Self, &'static str> {
        Ok(Self {
            base: Base::compile(conflict)?,
            edits: Arc::new(BTreeMap::new()),
            damage: conflict.damage,
        })
    }
    /// Commands have already been atomically validated by the conflict owner.
    /// False asks the caller to rebuild once the bounded overlay fills.
    pub fn orders(&mut self, conflict: &Conflict, commands: &[Command]) -> bool {
        let edits = Arc::make_mut(&mut self.edits);
        let mut actors = Vec::new();
        let mut defense = Vec::new();
        for command in commands {
            let fight = conflict
                .encounters
                .iter()
                .find(|fight| fight.id == command.encounter)
                .expect("validated encounter");
            let fighter = &fight.fighters[fight
                .fighters
                .binary_search_by_key(&command.person, |fighter| fighter.person)
                .expect("sorted validated fighter")];
            actors.push(command.person);
            defense.push((command.person, matches!(fighter.order, Order::Defend)));
            let strike = if let Order::Attack { target } = fighter.order {
                Some(Strike {
                    encounter: fight.id,
                    region: fight.region,
                    attacker: fighter.person,
                    target,
                    damage: self.damage,
                })
            } else {
                None
            };
            edits.insert(command.person, strike);
        }
        if edits.len() > LIMIT {
            return false;
        }
        for strike in edits.values_mut().flatten() {
            let fight = conflict
                .encounters
                .iter()
                .find(|fight| fight.id == strike.encounter)
                .expect("current encounter");
            let target = &fight.fighters[fight
                .fighters
                .binary_search_by_key(&strike.target, |fighter| fighter.person)
                .expect("validated target")];
            strike.damage = if matches!(target.order, Order::Defend) {
                (self.damage / 2).max(1)
            } else {
                self.damage
            };
        }
        self.base.patch(&actors, &defense);
        true
    }
    pub fn prepare_with_filter(
        &self,
        tick: u64,
        available: impl Fn(u64, u64) -> bool + Sync,
        eligible: impl Fn(u64, u64) -> bool + Sync,
    ) -> Round {
        let mut base = self.base.prepare_with_filter(tick, &available, &eligible);
        let mut extra: Vec<_> = self
            .edits
            .values()
            .flatten()
            .filter(|strike| {
                available(strike.attacker, strike.region)
                    && available(strike.target, strike.region)
                    && eligible(strike.attacker, strike.target)
            })
            .map(|strike| Event {
                tick,
                encounter: strike.encounter,
                attacker: strike.attacker,
                target: strike.target,
                damage: strike.damage,
            })
            .collect();
        extra.sort_unstable_by_key(key);
        let mut missing = BTreeMap::<u64, u32>::new();
        for event in &extra {
            match base
                .damage
                .binary_search_by_key(&event.target, |damage| damage.target)
            {
                Ok(index) => {
                    base.damage[index].amount =
                        base.damage[index].amount.saturating_add(event.damage)
                }
                Err(_) => {
                    let total = missing.entry(event.target).or_default();
                    *total = total.saturating_add(event.damage);
                }
            }
        }
        if !missing.is_empty() {
            let mut previous = std::mem::take(&mut base.damage).into_iter().peekable();
            for (target, amount) in missing {
                while previous.peek().is_some_and(|damage| damage.target < target) {
                    base.damage.push(previous.next().unwrap());
                }
                base.damage.push(Damage { target, amount });
            }
            base.damage.extend(previous);
        }
        Round { base, extra }
    }
    pub fn filtered(
        &self,
        survives: impl Fn(u64) -> bool + Sync,
        defends: impl Fn(u64, u64) -> bool + Sync,
    ) -> Self {
        let edits = self
            .edits
            .iter()
            .filter(|(actor, _)| survives(**actor))
            .map(|(&actor, strike)| {
                let strike = strike
                    .as_ref()
                    .filter(|strike| survives(strike.target))
                    .map(|strike| {
                        let mut next = strike.clone();
                        if defends(next.encounter, next.target) {
                            next.damage = (self.damage / 2).max(1);
                        }
                        next
                    });
                (actor, strike)
            })
            .collect();
        Self {
            base: self.base.filtered(survives, defends),
            edits: Arc::new(edits),
            damage: self.damage,
        }
    }
}
fn key(event: &Event) -> (u64, u64, u64) {
    (event.target, event.encounter, event.attacker)
}
impl Round {
    pub fn damage(&self) -> &[Damage] {
        self.base.damage()
    }
    pub fn event_count(&self) -> u64 {
        self.base.event_count() + self.extra.len() as u64
    }
    pub fn events(&self) -> impl Iterator<Item = Event> + '_ {
        let mut base = self.base.events().peekable();
        let mut extra = self.extra.iter().cloned().peekable();
        std::iter::from_fn(move || match (base.peek(), extra.peek()) {
            (Some(left), Some(right)) if key(right) < key(left) => extra.next(),
            (Some(_), _) => base.next(),
            (_, Some(_)) => extra.next(),
            _ => None,
        })
    }
    pub fn expand(&self) -> Vec<Event> {
        if self.extra.is_empty() {
            return self.base.expand();
        }
        let mut events = self.base.expand();
        events.extend_from_slice(&self.extra);
        events.par_sort_unstable_by_key(key);
        events
    }
}

#[cfg(test)]
#[path = "../tests/support/orders.rs"]
mod integration_tests;

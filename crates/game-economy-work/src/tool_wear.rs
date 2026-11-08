//! Physical, identified tools retain condition across changes of custody.
use crate::tools::Requirement;
use game_economy_person::Person;
use game_lot_runtime::Runtime;
use game_lots::{Movement, Place, Transaction, View};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub good: u32,
    pub durability: u64,
    pub wear: u64,
    pub repair_good: u32,
    pub repair_quantity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub item: u64,
    pub good: u32,
    pub damage: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairChoice {
    pub person: u64,
    pub item: u64,
    pub willing: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairEvent {
    pub tick: u64,
    pub person: u64,
    pub item: u64,
    pub accepted: bool,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolWear {
    pub rules: Vec<Rule>,
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub repairs: Vec<RepairChoice>,
    #[serde(default)]
    pub events: Vec<RepairEvent>,
}
impl Clone for ToolWear {
    fn clone(&self) -> Self {
        Self {
            rules: self.rules.par_iter().cloned().collect(),
            conditions: self.conditions.par_iter().cloned().collect(),
            repairs: self.repairs.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
        }
    }
}
impl ToolWear {
    pub fn validate(
        &self,
        physical: &Runtime,
        goods: &HashSet<u32>,
        people: &HashSet<u64>,
        tick: u64,
    ) -> Result<(), String> {
        if self.rules.par_windows(2).any(|p| p[0].good >= p[1].good)
            || self
                .conditions
                .par_windows(2)
                .any(|p| p[0].item >= p[1].item)
            || self
                .repairs
                .par_windows(2)
                .any(|p| p[0].person >= p[1].person)
            || self.rules.par_iter().any(|r| {
                r.durability == 0
                    || r.wear == 0
                    || r.repair_quantity == 0
                    || !goods.contains(&r.good)
                    || !goods.contains(&r.repair_good)
                    || r.good == r.repair_good
            })
            || self.conditions.par_iter().any(|c| {
                self.rule(c.good).is_none_or(|r| c.damage > r.durability)
                    || physical
                        .item_place(c.item)
                        .and_then(|p| physical.custody(p))
                        .is_none_or(|holder| {
                            holder
                                .lots
                                .binary_search_by_key(&(c.good, true, 0, Some(c.item)), |lot| {
                                    (
                                        lot.good,
                                        lot.expires_at.is_none(),
                                        lot.expires_at.unwrap_or(0),
                                        lot.item,
                                    )
                                })
                                .is_err()
                        })
            })
            || self.repairs.par_iter().any(|p| {
                !people.contains(&p.person)
                    || self
                        .conditions
                        .binary_search_by_key(&p.item, |c| c.item)
                        .is_err()
            })
            || self
                .events
                .par_iter()
                .any(|e| e.tick > tick || !people.contains(&e.person))
        {
            return Err("invalid tool wear state".into());
        }
        Ok(())
    }
    fn rule(&self, good: u32) -> Option<&Rule> {
        self.rules
            .binary_search_by_key(&good, |r| r.good)
            .ok()
            .map(|i| &self.rules[i])
    }
    /// Select actual working items in stable custody order. Unconfigured goods retain
    /// ordinary quantity semantics; configured goods require tracked identities.
    pub fn select(
        &self,
        requirements: &[Requirement],
        physical: &Runtime,
        view: &View,
        person: u64,
        recipe: u32,
    ) -> Option<Vec<u64>> {
        let first = requirements.partition_point(|r| r.recipe < recipe);
        let last = requirements.partition_point(|r| r.recipe <= recipe);
        let mut selected = Vec::new();
        for requirement in &requirements[first..last] {
            if let Some(rule) = self.rule(requirement.good) {
                let holder = physical.custody(Place::Person(person))?;
                let mut count = 0u64;
                let first = holder
                    .lots
                    .partition_point(|lot| lot.good < requirement.good);
                let last = holder
                    .lots
                    .partition_point(|lot| lot.good <= requirement.good);
                for lot in &holder.lots[first..last] {
                    if let Some(item) = lot.item
                        && self
                            .conditions
                            .binary_search_by_key(&item, |c| c.item)
                            .ok()
                            .is_some_and(|i| self.conditions[i].damage < rule.durability)
                    {
                        selected.push(item);
                        count += 1;
                        if count == requirement.quantity {
                            break;
                        }
                    }
                }
                if count < requirement.quantity {
                    return None;
                }
            } else if view
                .container(Place::Person(person))
                .is_none_or(|holder| holder.quantity(requirement.good) < requirement.quantity)
            {
                return None;
            }
        }
        Some(selected)
    }
    pub fn prepare_repairs(
        &self,
        physical: &Runtime,
        people: &[Person],
        busy: &[u64],
        paused_workers: &[u64],
        max_hunger: u32,
        base: u64,
    ) -> Result<Vec<(u64, u64, Transaction)>, String> {
        let choices: Vec<_> = self
            .repairs
            .par_iter()
            .filter_map(|choice| {
                let person = people
                    .binary_search_by_key(&choice.person, |p| p.id)
                    .ok()
                    .map(|i| &people[i])?;
                if !choice.willing
                    || person.controlled
                    || person.health == 0
                    || person.region.is_none()
                    || person.hunger > max_hunger
                    || (busy.binary_search(&choice.person).is_ok()
                        && paused_workers.binary_search(&choice.person).is_err())
                    || physical.item_place(choice.item) != Some(Place::Person(choice.person))
                {
                    return None;
                }
                let condition = self
                    .conditions
                    .binary_search_by_key(&choice.item, |c| c.item)
                    .ok()
                    .map(|i| &self.conditions[i])?;
                let rule = self.rule(condition.good)?;
                (condition.damage > 0).then_some((choice.person, choice.item, rule))
            })
            .collect();
        base.checked_add(choices.len() as u64)
            .ok_or("action count overflow")?;
        Ok(choices
            .into_par_iter()
            .enumerate()
            .map(|(i, (person, item, rule))| {
                (
                    person,
                    item,
                    Transaction {
                        id: base + i as u64,
                        movements: vec![Movement {
                            id: 1,
                            from: Place::Person(person),
                            to: None,
                            good: rule.repair_good,
                            quantity: rule.repair_quantity,
                            item: None,
                        }],
                        creations: vec![],
                    },
                )
            })
            .collect())
    }
    /// Both repairs and wear use opening-boundary custody; transfers take effect
    /// afterwards, so condition stays with the same physical identity.
    pub fn advance(
        &mut self,
        selected: &[(u64, u64)],
        repairs: &[(u64, u64, Transaction)],
        outcomes: &[game_lots::TransactionOutcome],
        item_updates: &[(u64, Option<Place>)],
        tick: u64,
    ) {
        let mut used: Vec<_> = selected.par_iter().map(|row| row.1).collect();
        if used.par_windows(2).any(|pair| pair[0] > pair[1]) {
            used.par_sort_unstable();
        }
        self.conditions.par_iter_mut().for_each(|condition| {
            if used.binary_search(&condition.item).is_ok() {
                let rule = &self.rules[self
                    .rules
                    .binary_search_by_key(&condition.good, |r| r.good)
                    .expect("validated rule")];
                condition.damage = condition
                    .damage
                    .saturating_add(rule.wear)
                    .min(rule.durability);
            }
        });
        self.events = repairs
            .par_iter()
            .map(|(person, item, action)| RepairEvent {
                tick,
                person: *person,
                item: *item,
                accepted: outcomes
                    .binary_search_by_key(&action.id, |outcome| outcome.id)
                    .ok()
                    .is_some_and(|i| outcomes[i].accepted),
            })
            .collect();
        let mut repaired: Vec<_> = self
            .events
            .par_iter()
            .filter(|e| e.accepted)
            .map(|e| e.item)
            .collect();
        if repaired.par_windows(2).any(|pair| pair[0] > pair[1]) {
            repaired.par_sort_unstable();
        }
        self.conditions.par_iter_mut().for_each(|condition| {
            if repaired.binary_search(&condition.item).is_ok() {
                condition.damage = 0;
            }
        });
        if item_updates.par_iter().any(|row| row.1.is_none()) {
            self.conditions = self
                .conditions
                .par_iter()
                .filter(|condition| {
                    item_updates
                        .binary_search_by_key(&condition.item, |row| row.0)
                        .ok()
                        .is_none_or(|i| item_updates[i].1.is_some())
                })
                .cloned()
                .collect();
            self.repairs = self
                .repairs
                .par_iter()
                .filter(|choice| {
                    self.conditions
                        .binary_search_by_key(&choice.item, |condition| condition.item)
                        .is_ok()
                })
                .cloned()
                .collect();
        }
    }
}

//! Individual gathering from finite or renewable local physical resources.
use game_parallel_economy_person::Person;
use game_parallel_lot_content::Catalog;
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Place, Transaction, View};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::Good;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: u64,
    pub region: u64,
    pub good: u32,
    pub available: u64,
    pub capacity: u64,
    pub regrowth: u64,
    pub yield_per_action: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub node: u64,
    pub willing: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gathering {
    pub max_hunger: u32,
    #[serde(default)]
    pub inherit_births: bool,
    pub nodes: Vec<Node>,
    pub policies: Vec<Policy>,
}
impl Clone for Gathering {
    fn clone(&self) -> Self {
        Self {
            max_hunger: self.max_hunger,
            inherit_births: self.inherit_births,
            nodes: self.nodes.par_iter().cloned().collect(),
            policies: self.policies.par_iter().cloned().collect(),
        }
    }
}
/// A child may inherit a personal inclination, never another person's pressure.
pub fn enroll(owner: Option<&mut Gathering>, family: Option<&crate::Families>, tick: u64) {
    let Some((owner, family)) = owner
        .zip(family)
        .filter(|(owner, family)| owner.inherit_births && family.rules.enroll_births)
    else {
        return;
    };
    let mut preferences: Vec<_> = owner.policies.par_iter().collect();
    preferences.par_sort_unstable_by_key(|p| p.person);
    let newborns: Vec<_> = family
        .members
        .par_iter()
        .filter(|m| m.born == Some(tick))
        .filter_map(|m| {
            let parent = m.parents.iter().find_map(|id| {
                preferences
                    .binary_search_by_key(id, |p| p.person)
                    .ok()
                    .map(|i| preferences[i])
            })?;
            Some(Policy {
                person: m.id,
                node: parent.node,
                willing: parent.willing,
                pressured_by: None,
                pressure_threshold: None,
            })
        })
        .collect();
    owner.policies.par_extend(newborns);
}
impl Gathering {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        view: &View,
    ) -> Result<(), String> {
        let mut nodes: Vec<_> = self.nodes.par_iter().map(|n| n.id).collect();
        let mut policies: Vec<_> = self.policies.par_iter().map(|p| p.person).collect();
        nodes.par_sort_unstable();
        policies.par_sort_unstable();
        if nodes.first() == Some(&0)
            || nodes.par_windows(2).any(|p| p[0] == p[1])
            || policies.par_windows(2).any(|p| p[0] == p[1])
            || self.nodes.par_iter().any(|n| {
                n.capacity == 0
                    || n.available > n.capacity
                    || n.yield_per_action == 0
                    || n.yield_per_action > n.capacity
                    || !goods.contains(&n.good)
                    || view
                        .container(Place::Stock {
                            region: n.region,
                            good: n.good,
                        })
                        .is_none()
            })
            || self.policies.par_iter().any(|p| {
                !people.contains(&p.person)
                    || nodes.binary_search(&p.node).is_err()
                    || p.pressured_by
                        .is_some_and(|id| id == p.person || !people.contains(&id))
                    || p.pressure_threshold == Some(0)
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
            })
        {
            return Err("invalid gathering state".into());
        }
        Ok(())
    }
}
pub struct Plan {
    pub actors: Vec<u64>,
    pub actions: Vec<Transaction>,
    records: Vec<(u64, u64, u64, u64)>,
}
pub struct Context<'a> {
    pub owner: &'a Gathering,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub busy: &'a [u64],
    pub goods: &'a [Good],
    pub view: &'a View,
    pub tick_millis: u64,
    pub seed: u64,
    pub tick: u64,
    pub manual: Option<(u64, u64)>,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        let mut nodes: Vec<_> = self.owner.nodes.par_iter().collect();
        nodes.par_sort_unstable_by_key(|n| n.id);
        let node = |id| {
            nodes
                .binary_search_by_key(&id, |n| n.id)
                .ok()
                .map(|i| nodes[i])
        };
        let opinions = self.opinions.map(Opinions::index);
        let mut candidates: Vec<_> = self
            .owner
            .policies
            .par_iter()
            .filter_map(|policy| {
                let person = self
                    .people
                    .binary_search_by_key(&policy.person, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])?;
                let resource = node(policy.node)?;
                let stock = self.view.container(Place::Stock {
                    region: resource.region,
                    good: resource.good,
                })?;
                if resource.available < resource.yield_per_action
                    || stock.capacity.saturating_sub(stock.used) < resource.yield_per_action
                {
                    return None;
                }
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(person.id, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                (person.health > 0
                    && !person.controlled
                    && person.region == Some(resource.region)
                    && person.hunger <= self.owner.max_hunger
                    && self.busy.binary_search(&person.id).is_err()
                    && (policy.willing || pressured))
                    .then_some((
                        resource.id,
                        game_parallel_genetics::random(
                            self.seed ^ resource.id,
                            person.id,
                            self.tick,
                        ),
                        person.id,
                    ))
            })
            .collect();
        if let Some((actor, resource)) = self.manual {
            let person = self
                .people
                .binary_search_by_key(&actor, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown person")?;
            let resource = node(resource).ok_or("unknown resource site")?;
            if !person.controlled
                || person.health == 0
                || person.region != Some(resource.region)
                || person.hunger > self.owner.max_hunger
            {
                return Err("person cannot gather here".into());
            }
            candidates.push((
                resource.id,
                game_parallel_genetics::random(self.seed ^ resource.id, actor, self.tick),
                actor,
            ));
        }
        candidates.par_sort_unstable();
        let actors = candidates.par_iter().map(|row| row.2).collect();
        let selected: Vec<_> = candidates
            .par_chunk_by(|a, b| a.0 == b.0)
            .flat_map(|group| {
                let resource = node(group[0].0).expect("validated resource site");
                let limit = usize::try_from(resource.available / resource.yield_per_action)
                    .unwrap_or(usize::MAX)
                    .min(group.len());
                group[..limit].par_iter().map(move |row| (resource, row.2))
            })
            .collect();
        let count = u64::try_from(selected.len()).map_err(|_| "gathering action overflow")?;
        base.checked_add(count).ok_or("gathering action overflow")?;
        let catalog = Catalog::from_tick_millis(self.goods, self.tick_millis)?;
        let results: Vec<_> = selected
            .par_iter()
            .enumerate()
            .map(|(i, (resource, actor))| -> Result<_, String> {
                let id = base + i as u64;
                let action = Transaction {
                    id,
                    movements: vec![],
                    creations: vec![catalog.creation(
                        id,
                        Place::Stock {
                            region: resource.region,
                            good: resource.good,
                        },
                        resource.good,
                        resource.yield_per_action,
                        self.tick,
                    )?],
                };
                Ok(((resource.id, id, resource.yield_per_action, *actor), action))
            })
            .collect::<Result<_, _>>()?;
        let (records, actions) = results.into_par_iter().unzip();
        Ok(Plan {
            actors,
            actions,
            records,
        })
    }
}
impl Plan {
    pub fn outputs(&self, owner: &Gathering) -> Vec<(u64, u64, u64, u32)> {
        let mut nodes: Vec<_> = owner.nodes.par_iter().collect();
        nodes.par_sort_unstable_by_key(|n| n.id);
        self.records
            .par_iter()
            .filter_map(|&(id, action, _, actor)| {
                nodes
                    .binary_search_by_key(&id, |n| n.id)
                    .ok()
                    .map(|i| (action, actor, nodes[i].region, nodes[i].good))
            })
            .collect()
    }
    pub fn admitted(&self, actor: u64, events: &Events) -> bool {
        self.records
            .iter()
            .find(|row| row.3 == actor)
            .is_some_and(|row| {
                events
                    .physical_actions
                    .iter()
                    .any(|decision| decision.id == row.1 && decision.accepted)
            })
    }
    pub fn publish(mut self, owner: &Gathering, events: &Events) -> Result<Gathering, String> {
        let mut decisions: Vec<_> = events.physical_actions.par_iter().collect();
        if decisions.par_windows(2).any(|w| w[0].id > w[1].id) {
            decisions.par_sort_unstable_by_key(|d| d.id);
        }
        self.records.par_sort_unstable_by_key(|r| r.0);
        let nodes = owner
            .nodes
            .par_iter()
            .map(|node| -> Result<_, String> {
                let start = self.records.partition_point(|r| r.0 < node.id);
                let end = self.records.partition_point(|r| r.0 <= node.id);
                let gathered = self.records[start..end]
                    .par_iter()
                    .map(|(_, id, amount, _)| {
                        decisions
                            .binary_search_by_key(id, |d| d.id)
                            .map(|i| if decisions[i].accepted { *amount } else { 0 })
                            .map_err(|_| "missing gathering outcome")
                    })
                    .try_reduce(
                        || 0u64,
                        |a, b| a.checked_add(b).ok_or("gathering quantity overflow"),
                    )?;
                let mut next = node.clone();
                next.available = node
                    .available
                    .checked_sub(gathered)
                    .ok_or("gathering exceeds resource")?
                    .saturating_add(node.regrowth)
                    .min(node.capacity);
                Ok(next)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Gathering {
            max_hunger: owner.max_hunger,
            inherit_births: owner.inherit_births,
            nodes,
            policies: owner.policies.par_iter().cloned().collect(),
        })
    }
}

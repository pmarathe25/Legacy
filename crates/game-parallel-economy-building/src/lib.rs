//! Personal choices to assemble physical sites from actual local kits.
pub mod demolition;
use game_parallel_economy_claims::{Index as Claims, Use};
use game_parallel_economy_person::Person;
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Movement, Place, Transaction, View};
use game_parallel_opinions::Opinions;
use game_parallel_work_sites::{Facilities, Site};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blueprint {
    pub id: u32,
    pub name: String,
    pub kind: String,
    pub capacity: u64,
    pub kit: u32,
    pub quantity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub blueprint: u32,
    pub target_sites: u64,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
    #[serde(default)]
    pub disregard_claims: bool,
}
/// A person's willingness to build sites that their own planned work needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalBuilder {
    pub person: u64,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub disregard_claims: bool,
}
#[derive(Clone, Copy)]
pub struct GoalNeed {
    pub person: u64,
    pub recipe: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub blueprint: u32,
    pub site: Option<u64>,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Construction {
    pub blueprints: Vec<Blueprint>,
    #[serde(default)]
    pub policies: Vec<Policy>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub demolition: Vec<demolition::Policy>,
    #[serde(default)]
    pub demolition_events: Vec<demolition::Event>,
    #[serde(default)]
    pub goal_builders: Vec<GoalBuilder>,
}
impl Clone for Construction {
    fn clone(&self) -> Self {
        Self {
            blueprints: self.blueprints.par_iter().cloned().collect(),
            policies: self.policies.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
            demolition: self.demolition.par_iter().cloned().collect(),
            demolition_events: self.demolition_events.par_iter().cloned().collect(),
            goal_builders: self.goal_builders.par_iter().cloned().collect(),
        }
    }
}
impl Construction {
    pub fn normalize(&mut self) {
        self.blueprints.par_sort_unstable_by_key(|b| b.id);
        self.policies.par_sort_unstable_by_key(|p| p.person);
        self.demolition.par_sort_unstable_by_key(|p| p.person);
        self.goal_builders.par_sort_unstable_by_key(|p| p.person);
    }
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        sites: &Facilities,
        tick: u64,
    ) -> Result<(), String> {
        if self
            .goal_builders
            .par_windows(2)
            .any(|w| w[0].person >= w[1].person)
            || self
                .goal_builders
                .par_iter()
                .any(|builder| !people.contains(&builder.person))
            || !demolition::valid(&self.demolition, &self.demolition_events, people, tick)
            || self.blueprints.par_windows(2).any(|w| w[0].id >= w[1].id)
            || self.blueprints.par_iter().any(|b| {
                b.id == 0
                    || b.name.is_empty()
                    || b.kind.is_empty()
                    || b.capacity == 0
                    || b.quantity == 0
                    || !goods.contains(&b.kit)
            })
            || self
                .policies
                .par_windows(2)
                .any(|w| w[0].person >= w[1].person)
            || self.policies.par_iter().any(|p| {
                !people.contains(&p.person)
                    || p.target_sites == 0
                    || self
                        .blueprints
                        .binary_search_by_key(&p.blueprint, |b| b.id)
                        .is_err()
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
                    || p.pressure_threshold == Some(0)
                    || p.pressured_by
                        .is_some_and(|id| id == p.person || !people.contains(&id))
            })
            || self.events.par_iter().any(|e| {
                e.tick > tick
                    || !people.contains(&e.person)
                    || self
                        .blueprints
                        .binary_search_by_key(&e.blueprint, |b| b.id)
                        .is_err()
                    || e.site.is_some_and(|id| id == 0 || id >= sites.next_id)
            })
        {
            return Err("invalid personal construction state".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct Manual {
    pub person: u64,
    pub blueprint: u32,
}
struct Choice<'a> {
    person: u64,
    region: u64,
    blueprint: &'a Blueprint,
    source: Place,
}
struct Row {
    action: u64,
    person: u64,
    blueprint: u32,
    site: Site,
}
pub struct Plan {
    pub actions: Vec<Transaction>,
    pub actors: Vec<u64>,
    pub uses: Vec<Use>,
    rows: Vec<Row>,
    next_id: u64,
}
pub struct Context<'a> {
    pub owner: &'a Construction,
    pub sites: &'a Facilities,
    pub people: &'a [Person],
    pub view: &'a View,
    pub opinions: Option<&'a Opinions>,
    pub claims: Option<&'a Claims<'a>>,
    pub busy: &'a [u64],
    pub max_hunger: u32,
    pub tick: u64,
    pub manual: Option<Manual>,
    /// Active personal goal plans, sorted by person and excluding direct work policies.
    pub goal_needs: &'a [GoalNeed],
    pub unwilling: &'a [u64],
}
impl Context<'_> {
    fn source(&self, person: &Person, blueprint: &Blueprint, disregard: bool) -> Option<Place> {
        let region = person.region?;
        let personal = Place::Person(person.id);
        if self
            .view
            .container(personal)
            .is_some_and(|c| c.quantity(blueprint.kit) >= blueprint.quantity)
        {
            return Some(personal);
        }
        if !disregard
            && self
                .claims
                .is_some_and(|c| !c.allows(person.id, region, blueprint.kit))
        {
            return None;
        }
        let stock = Place::Stock {
            region,
            good: blueprint.kit,
        };
        self.view
            .container(stock)
            .filter(|c| c.quantity(blueprint.kit) >= blueprint.quantity)
            .map(|_| stock)
    }
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        if self.manual.is_none()
            && !self
                .owner
                .policies
                .par_iter()
                .any(|p| p.willing || p.pressure_threshold.is_some())
            && (self.goal_needs.is_empty()
                || !self.owner.goal_builders.par_iter().any(|p| p.enabled))
        {
            return Ok(Plan {
                actions: Vec::new(),
                actors: Vec::new(),
                uses: Vec::new(),
                rows: Vec::new(),
                next_id: self.sites.next_id,
            });
        }
        let mut counts: Vec<_> = self
            .sites
            .sites
            .par_iter()
            .map(|s| (s.region, s.kind.as_str()))
            .collect();
        if counts.par_windows(2).any(|w| w[0] > w[1]) {
            counts.par_sort_unstable();
        }
        let opinions = self.opinions.map(Opinions::index);
        let mut choices: Vec<_> = self
            .owner
            .policies
            .par_iter()
            .filter_map(|policy| {
                if self.busy.binary_search(&policy.person).is_ok() {
                    return None;
                }
                let person = self
                    .people
                    .binary_search_by_key(&policy.person, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])?;
                let region = person.region?;
                if person.controlled || person.health == 0 || person.hunger > self.max_hunger {
                    return None;
                }
                let blueprint = self
                    .owner
                    .blueprints
                    .binary_search_by_key(&policy.blueprint, |b| b.id)
                    .ok()
                    .map(|i| &self.owner.blueprints[i])?;
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|o| {
                            let (fear, duty) = o.feelings(person.id, other);
                            fear.saturating_add(duty) >= threshold
                        })
                    });
                let key = (region, blueprint.kind.as_str());
                let present =
                    counts.partition_point(|r| *r <= key) - counts.partition_point(|r| *r < key);
                if !(policy.willing || pressured) || present as u64 >= policy.target_sites {
                    return None;
                }
                let source = self.source(person, blueprint, policy.disregard_claims)?;
                Some(Choice {
                    person: person.id,
                    region,
                    blueprint,
                    source,
                })
            })
            .collect();
        if !self.goal_needs.is_empty() && !self.owner.goal_builders.is_empty() {
            let chosen: HashSet<_> = choices.par_iter().map(|choice| choice.person).collect();
            let mut site_sizes: Vec<_> = self
                .sites
                .sites
                .par_iter()
                .map(|site| (site.region, site.kind.as_str(), site.capacity))
                .collect();
            site_sizes.par_sort_unstable();
            let mut suitable: Vec<_> = self
                .sites
                .requirements
                .par_iter()
                .map(|requirement| {
                    let blueprints: Vec<_> = self
                        .owner
                        .blueprints
                        .iter()
                        .filter(|blueprint| {
                            blueprint.kind == requirement.kind
                                && blueprint.capacity >= requirement.slots
                        })
                        .collect();
                    (requirement.recipe, requirement, blueprints)
                })
                .collect();
            suitable.par_sort_unstable_by_key(|row| row.0);
            let additions: Vec<_> = self
                .owner
                .goal_builders
                .par_iter()
                .filter_map(|policy| {
                    if !policy.enabled
                        || chosen.contains(&policy.person)
                        || self.busy.binary_search(&policy.person).is_ok()
                        || self.unwilling.binary_search(&policy.person).is_ok()
                    {
                        return None;
                    }
                    let need = self
                        .goal_needs
                        .binary_search_by_key(&policy.person, |n| n.person)
                        .ok()
                        .map(|i| self.goal_needs[i])?;
                    let person = self
                        .people
                        .binary_search_by_key(&policy.person, |p| p.id)
                        .ok()
                        .map(|i| &self.people[i])?;
                    let region = person.region?;
                    if person.controlled
                        || person.health == 0
                        || !person.willing_to_work
                        || person.hunger > self.max_hunger
                    {
                        return None;
                    }
                    let (_, requirement, blueprints) = suitable
                        .binary_search_by_key(&need.recipe, |row| row.0)
                        .ok()
                        .map(|i| &suitable[i])?;
                    let key = (region, requirement.kind.as_str(), requirement.slots);
                    let index = site_sizes.partition_point(|row| *row < key);
                    if site_sizes
                        .get(index)
                        .is_some_and(|row| row.0 == region && row.1 == requirement.kind)
                    {
                        return None;
                    }
                    let (blueprint, source) = blueprints.iter().find_map(|&blueprint| {
                        self.source(person, blueprint, policy.disregard_claims)
                            .map(|source| (blueprint, source))
                    })?;
                    Some(Choice {
                        person: person.id,
                        region,
                        blueprint,
                        source,
                    })
                })
                .collect();
            choices.par_extend(additions);
        }
        if let Some(manual) = self.manual {
            let person = self
                .people
                .binary_search_by_key(&manual.person, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .filter(|p| p.controlled && p.health > 0)
                .ok_or("construction requires a living controlled person")?;
            let region = person.region.ok_or("construction requires a local site")?;
            let blueprint = self
                .owner
                .blueprints
                .binary_search_by_key(&manual.blueprint, |b| b.id)
                .ok()
                .map(|i| &self.owner.blueprints[i])
                .ok_or("unknown construction blueprint")?;
            let source = self
                .source(person, blueprint, false)
                .ok_or("construction kit is unavailable")?;
            choices.push(Choice {
                person: person.id,
                region,
                blueprint,
                source,
            });
        }
        choices.par_sort_unstable_by_key(|c| c.person);
        let count = u64::try_from(choices.len()).map_err(|_| "construction count overflow")?;
        let next_id = self
            .sites
            .next_id
            .checked_add(count)
            .ok_or("work site identity overflow")?;
        base.checked_add(count)
            .ok_or("construction action overflow")?;
        let actions: Vec<_> = choices
            .par_iter()
            .enumerate()
            .map(|(i, c)| {
                let id = base + i as u64;
                Transaction {
                    id,
                    movements: vec![Movement {
                        id,
                        from: c.source,
                        to: None,
                        good: c.blueprint.kit,
                        quantity: c.blueprint.quantity,
                    }],
                    creations: vec![],
                }
            })
            .collect();
        let rows = choices
            .par_iter()
            .enumerate()
            .map(|(i, c)| Row {
                action: base + i as u64,
                person: c.person,
                blueprint: c.blueprint.id,
                site: Site {
                    id: self.sites.next_id + i as u64,
                    region: c.region,
                    kind: c.blueprint.kind.clone(),
                    capacity: c.blueprint.capacity,
                },
            })
            .collect();
        let uses = if self.claims.is_some() {
            choices
                .par_iter()
                .enumerate()
                .filter_map(|(i, c)| match c.source {
                    Place::Stock { region, good } => Some(Use {
                        action: base + i as u64,
                        person: c.person,
                        region,
                        good,
                        quantity: c.blueprint.quantity,
                        job: false,
                    }),
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(Plan {
            actions,
            actors: choices.par_iter().map(|c| c.person).collect(),
            uses,
            rows,
            next_id,
        })
    }
}
impl Plan {
    /// Publish sites only after their physical kit consumptions have succeeded.
    pub fn publish(self, owner: &mut Construction, sites: &mut Facilities, events: &Events) {
        let accepted: HashSet<_> = events
            .physical_actions
            .par_iter()
            .filter(|r| r.accepted)
            .map(|r| r.id)
            .collect();
        let built: Vec<_> = self
            .rows
            .par_iter()
            .filter(|r| accepted.contains(&r.action))
            .map(|r| r.site.clone())
            .collect();
        owner.events = self
            .rows
            .par_iter()
            .map(|r| Event {
                tick: events.tick,
                person: r.person,
                blueprint: r.blueprint,
                site: accepted.contains(&r.action).then_some(r.site.id),
            })
            .collect();
        sites.sites.par_extend(built);
        if sites.sites.par_windows(2).any(|w| w[0].id > w[1].id) {
            sites.sites.par_sort_unstable_by_key(|s| s.id);
        }
        sites.next_id = self.next_id;
    }
}

//! Individual gathering from finite or renewable local physical resources.
use game_economy_person::Person;
use game_lot_runtime::Events;
use game_lots::{Place, View};
use game_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Renewable units per explicit elapsed duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenewalRate {
    pub units: u64,
    pub per_millis: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: u64,
    pub region: u64,
    pub good: u32,
    pub available: u64,
    pub capacity: u64,
    /// Legacy units per physical boundary; omit when using `renewal_rate`.
    #[serde(default)]
    pub regrowth: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renewal_rate: Option<RenewalRate>,
    /// Fractional numerator over `per_millis * 1000`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub renewal_remainder: u64,
    /// Scale renewal by the current season's food-growth factor.
    #[serde(default)]
    pub seasonal: bool,
    pub yield_per_action: u64,
}
fn is_zero(value: &u64) -> bool {
    *value == 0
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
    /// Publish renewable growth as simulated time passes inside the interval.
    #[serde(default)]
    pub elapsed_renewal: bool,
    #[serde(default)]
    pub inherit_births: bool,
    pub nodes: Vec<Node>,
    pub policies: Vec<Policy>,
}
impl Clone for Gathering {
    fn clone(&self) -> Self {
        Self {
            max_hunger: self.max_hunger,
            elapsed_renewal: self.elapsed_renewal,
            inherit_births: self.inherit_births,
            nodes: self.nodes.par_iter().cloned().collect(),
            policies: self.policies.par_iter().cloned().collect(),
        }
    }
}
/// A child may inherit a personal inclination, never another person's pressure.
pub fn enroll(
    owner: Option<&mut Gathering>,
    family: Option<&crate::Families>,
    batch: game_economy_family::BirthBatch<'_>,
) {
    let Some((owner, family)) = owner
        .zip(family)
        .filter(|(owner, family)| owner.inherit_births && family.rules.enroll_births)
    else {
        return;
    };
    let mut preferences: Vec<_> = owner.policies.par_iter().collect();
    preferences.par_sort_unstable_by_key(|p| (p.person, p.node));
    let newborns: Vec<_> = family
        .members
        .par_iter()
        .filter(|m| batch.contains(m))
        .flat_map_iter(|m| {
            let parent = m
                .parents
                .iter()
                .find(|id| preferences.binary_search_by_key(*id, |p| p.person).is_ok());
            parent
                .into_iter()
                .flat_map(|id| {
                    let start = preferences.partition_point(|policy| policy.person < *id);
                    let end = preferences.partition_point(|policy| policy.person <= *id);
                    preferences[start..end].iter().map(|policy| Policy {
                        person: m.id,
                        node: policy.node,
                        willing: policy.willing,
                        pressured_by: None,
                        pressure_threshold: None,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    owner.policies.par_extend(newborns);
}
impl Gathering {
    /// Prepare independent node updates for a segment with no withdrawals.
    pub fn progress_renewal(
        &self,
        millis: u64,
        seasonal_exposure: Option<u128>,
    ) -> Result<Option<Vec<Node>>, String> {
        if !self.elapsed_renewal || millis == 0 {
            return Ok(None);
        }
        let neutral = u128::from(millis) * 1000;
        let nodes = self
            .nodes
            .par_iter()
            .map(|node| {
                let mut next = node.clone();
                if let Some(rate) = node.renewal_rate {
                    let exposure = if node.seasonal {
                        seasonal_exposure.unwrap_or(neutral)
                    } else {
                        neutral
                    };
                    (next.available, next.renewal_remainder) = integrate_renewal(
                        rate,
                        node.renewal_remainder,
                        exposure,
                        node.available,
                        node.capacity,
                    )?;
                }
                Ok(next)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Some(nodes))
    }

    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        view: &View,
    ) -> Result<(), String> {
        let mut nodes: Vec<_> = self.nodes.par_iter().map(|n| n.id).collect();
        let mut policies: Vec<_> = self
            .policies
            .par_iter()
            .map(|p| (p.person, p.node))
            .collect();
        nodes.par_sort_unstable();
        policies.par_sort_unstable();
        if nodes.first() == Some(&0)
            || nodes.par_windows(2).any(|p| p[0] == p[1])
            || policies.par_windows(2).any(|p| p[0] == p[1])
            || self.nodes.par_iter().any(|n| {
                n.capacity == 0
                    || (self.elapsed_renewal && n.regrowth > 0)
                    || n.available > n.capacity
                    || n.yield_per_action == 0
                    || n.yield_per_action > n.capacity
                    || n.renewal_rate.is_some_and(|rate| {
                        rate.per_millis == 0
                            || rate.per_millis.checked_mul(1000).is_none()
                            || n.regrowth != 0
                            || n.renewal_remainder >= rate.per_millis * 1000
                    })
                    || (n.renewal_rate.is_none() && n.renewal_remainder != 0)
                    || (n.available == n.capacity && n.renewal_remainder != 0)
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
    records: Vec<(u64, u64, u64, u64)>,
    elapsed_millis: u64,
}
/// One person's possible local gathering action before resource arbitration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opportunity {
    pub person: u64,
    pub node: u64,
    pub good: u32,
    pub quantity: u64,
}
pub struct Context<'a> {
    pub owner: &'a Gathering,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub busy: &'a [u64],
    pub view: &'a View,
    pub tick_millis: u64,
    pub seed: u64,
    pub tick: u64,
    pub manual: Option<(u64, u64)>,
}
impl Context<'_> {
    /// Expose individual alternatives without reserving a shared resource.
    pub fn opportunities(&self, max_hunger: u32) -> Vec<Opportunity> {
        let mut nodes: Vec<_> = self.owner.nodes.par_iter().collect();
        nodes.par_sort_unstable_by_key(|node| node.id);
        let opinions = self.opinions.map(Opinions::index);
        let mut opportunities: Vec<_> = self
            .owner
            .policies
            .par_iter()
            .filter_map(|policy| {
                let person = self
                    .people
                    .binary_search_by_key(&policy.person, |person| person.id)
                    .ok()
                    .map(|index| &self.people[index])?;
                let resource = nodes
                    .binary_search_by_key(&policy.node, |node| node.id)
                    .ok()
                    .map(|index| nodes[index])?;
                self.view.container(Place::Stock {
                    region: resource.region,
                    good: resource.good,
                })?;
                let capacity = self
                    .view
                    .container(Place::Person(person.id))
                    .map_or(0, |holder| holder.capacity.saturating_sub(holder.used));
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(person.id, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                (resource.available >= resource.yield_per_action
                    && capacity >= resource.yield_per_action
                    && person.health > 0
                    && !person.controlled
                    && person.region == Some(resource.region)
                    && person.hunger <= max_hunger
                    && self.busy.binary_search(&person.id).is_err()
                    && (policy.willing || pressured))
                    .then_some(Opportunity {
                        person: person.id,
                        node: resource.id,
                        good: resource.good,
                        quantity: resource.yield_per_action,
                    })
            })
            .collect();
        opportunities.par_sort_unstable_by_key(|row| (row.person, row.node));
        opportunities
    }

    /// Only exact, previously selected intentions may act for profiled people.
    /// Other actors retain the legacy policy path until they receive profiles.
    pub fn prepare_selected(
        &self,
        base: u64,
        profiled: &[u64],
        selected: &[Opportunity],
    ) -> Result<Plan, String> {
        let mut nodes: Vec<_> = self.owner.nodes.par_iter().collect();
        nodes.par_sort_unstable_by_key(|n| n.id);
        let node = |id| {
            nodes
                .binary_search_by_key(&id, |n| n.id)
                .ok()
                .map(|i| nodes[i])
        };
        let mut eligible: Vec<_> = self
            .opportunities(if profiled.is_empty() {
                self.owner.max_hunger
            } else {
                u32::MAX
            })
            .into_par_iter()
            .filter(|opportunity| {
                if profiled.binary_search(&opportunity.person).is_ok() {
                    selected
                        .binary_search_by_key(&opportunity.person, |row| row.person)
                        .ok()
                        .is_some_and(|index| selected[index] == *opportunity)
                } else {
                    self.people
                        .binary_search_by_key(&opportunity.person, |row| row.id)
                        .ok()
                        .is_some_and(|index| self.people[index].hunger <= self.owner.max_hunger)
                }
            })
            .collect();
        // An unprofiled legacy actor with alternatives has not made a choice.
        if eligible
            .par_windows(2)
            .any(|pair| pair[0].person == pair[1].person)
        {
            eligible = eligible
                .par_chunk_by(|a, b| a.person == b.person)
                .filter_map(|group| (group.len() == 1).then_some(group[0]))
                .collect();
        }
        let mut candidates: Vec<_> = eligible
            .into_par_iter()
            .map(|opportunity| {
                (
                    opportunity.node,
                    game_genetics::random(
                        self.seed ^ opportunity.node,
                        opportunity.person,
                        self.tick,
                    ),
                    opportunity.person,
                )
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
                game_genetics::random(self.seed ^ resource.id, actor, self.tick),
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
        let records = selected
            .par_iter()
            .enumerate()
            .map(|(i, (resource, actor))| {
                (
                    resource.id,
                    base + i as u64,
                    resource.yield_per_action,
                    *actor,
                )
            })
            .collect();
        Ok(Plan {
            actors,
            records,
            elapsed_millis: self.tick_millis,
        })
    }
}
impl Plan {
    pub fn outcomes(&self, events: &Events) -> Vec<(u64, bool)> {
        let decisions: std::collections::HashMap<_, _> = events
            .physical_actions
            .par_iter()
            .map(|row| (row.id, row.accepted))
            .collect();
        let accepted: HashSet<_> = self
            .records
            .par_iter()
            .filter_map(|row| {
                decisions
                    .get(&row.1)
                    .copied()
                    .unwrap_or(false)
                    .then_some(row.3)
            })
            .collect();
        let mut outcomes: Vec<_> = self
            .actors
            .par_iter()
            .map(|&actor| (actor, accepted.contains(&actor)))
            .collect();
        outcomes.par_sort_unstable_by_key(|row| row.0);
        outcomes
    }
    pub fn carried_outputs(&self, owner: &Gathering) -> Vec<game_lot_runtime::Gathered> {
        let mut nodes: Vec<_> = owner.nodes.par_iter().collect();
        nodes.par_sort_unstable_by_key(|node| node.id);
        self.records
            .par_iter()
            .filter_map(|&(id, action, quantity, person)| {
                let index = nodes.binary_search_by_key(&id, |node| node.id).ok()?;
                Some(game_lot_runtime::Gathered {
                    action,
                    person,
                    good: nodes[index].good,
                    quantity,
                })
            })
            .collect()
    }
    pub fn publish(
        mut self,
        owner: &Gathering,
        events: &Events,
        season: Option<&crate::Season>,
        food_growth_exposure: Option<u128>,
    ) -> Result<Gathering, String> {
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
                let after_gathering = node
                    .available
                    .checked_sub(gathered)
                    .ok_or("gathering exceeds resource")?;
                let factor = if node.seasonal {
                    season.map_or(1000, |s| s.food_growth_per_thousand)
                } else {
                    1000
                };
                if owner.elapsed_renewal {
                    next.available = after_gathering;
                } else if let Some(rate) = node.renewal_rate {
                    let exposure = if node.seasonal {
                        food_growth_exposure
                            .unwrap_or(u128::from(self.elapsed_millis) * u128::from(factor))
                    } else {
                        u128::from(self.elapsed_millis) * 1000
                    };
                    let (available, remainder) = integrate_renewal(
                        rate,
                        node.renewal_remainder,
                        exposure,
                        after_gathering,
                        node.capacity,
                    )?;
                    next.available = available;
                    next.renewal_remainder = remainder;
                } else {
                    next.available = after_gathering
                        .saturating_add(if node.seasonal {
                            renewal(node.regrowth, factor, events.tick)
                        } else {
                            node.regrowth
                        })
                        .min(node.capacity);
                }
                Ok(next)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Gathering {
            max_hunger: owner.max_hunger,
            elapsed_renewal: owner.elapsed_renewal,
            inherit_births: owner.inherit_births,
            nodes,
            policies: owner.policies.par_iter().cloned().collect(),
        })
    }
}

fn integrate_renewal(
    rate: RenewalRate,
    remainder: u64,
    per_thousand_millis: u128,
    available: u64,
    capacity: u64,
) -> Result<(u64, u64), String> {
    let denominator = rate
        .per_millis
        .checked_mul(1000)
        .filter(|value| *value > 0)
        .ok_or("invalid renewal period")?;
    if remainder >= denominator || available > capacity {
        return Err("invalid renewal state".into());
    }
    // A numerator larger than u128 already exceeds any u64 capacity times
    // any supported denominator, so clamping it here still fills the node.
    let numerator = u128::from(rate.units)
        .saturating_mul(per_thousand_millis)
        .saturating_add(u128::from(remainder));
    let quantity = numerator / u128::from(denominator);
    let room = u128::from(capacity - available);
    if quantity >= room {
        return Ok((capacity, 0));
    }
    Ok((
        available + quantity as u64,
        (numerator % u128::from(denominator)) as u64,
    ))
}

/// Distribute fractional renewal deterministically without losing low rates to rounding.
fn renewal(rate: u64, factor: u32, tick: u64) -> u64 {
    let scaled = u128::from(rate) * u128::from(factor);
    let whole = scaled / 1000;
    let remainder = scaled % 1000;
    let fraction = (u128::from(tick) * remainder / 1000)
        - (u128::from(tick.saturating_sub(1)) * remainder / 1000);
    (whole + fraction).min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod elapsed_tests {
    use super::*;

    fn elapsed_renewal(
        rate: RenewalRate,
        remainder: u64,
        millis: u64,
        factor: u32,
        available: u64,
        capacity: u64,
    ) -> Result<(u64, u64), String> {
        integrate_renewal(
            rate,
            remainder,
            u128::from(millis) * u128::from(factor),
            available,
            capacity,
        )
    }

    #[test]
    fn renewal_preserves_fraction_across_arbitrary_partitions_and_saturation() {
        let rate = RenewalRate {
            units: 3,
            per_millis: 3_600_000,
        };
        let whole = elapsed_renewal(rate, 0, 3_600_000, 1000, 0, 20).unwrap();
        let mut split = (0, 0);
        let mut elapsed = 0;
        while elapsed < 3_600_000 {
            let duration = 17_000.min(3_600_000 - elapsed);
            split = elapsed_renewal(rate, split.1, duration, 1000, split.0, 20).unwrap();
            elapsed += duration;
        }
        assert_eq!(whole, (3, 0));
        assert_eq!(split, whole);
        assert_eq!(
            elapsed_renewal(rate, 0, 3_600_000, 1000, 19, 20).unwrap(),
            (20, 0)
        );
        assert!(
            elapsed_renewal(
                RenewalRate {
                    units: 1,
                    per_millis: 0
                },
                0,
                1,
                1000,
                0,
                1
            )
            .is_err()
        );
        assert!(elapsed_renewal(rate, 3_600_000_000, 1, 1000, 0, 1).is_err());
    }
}

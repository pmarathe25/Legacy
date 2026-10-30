//! Individual, revocable choices to carry actual goods between regions.
use game_parallel_economy_claims::{Index as Claims, Use};
use game_parallel_economy_person::{
    Person,
    travel::{Colocation, Departure, Travel, routes},
};
use game_parallel_lots::{Movement, Place, Transaction, View};
use game_parallel_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub source: u64,
    pub destination: u64,
    pub good: u32,
    pub quantity: u64,
    pub willing: bool,
    #[serde(default)]
    pub requires_request: bool,
    #[serde(default)]
    pub request_pressure_threshold: Option<u64>,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hauling {
    pub policies: Vec<Policy>,
    #[serde(default)]
    pub asking: Vec<Asking>,
    #[serde(default)]
    pub requests: Vec<Request>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asking {
    pub person: u64,
    pub carrier: u64,
    pub source: u64,
    pub destination: u64,
    pub good: u32,
    pub enabled: bool,
    pub retry_ticks: u64,
    pub request_lifetime: u64,
    #[serde(default)]
    pub next_review: u64,
}
/// A personal request for ongoing help; no action is compelled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub requester: u64,
    pub carrier: u64,
    pub source: u64,
    pub destination: u64,
    pub good: u32,
    pub expires_at: u64,
}
impl Clone for Hauling {
    fn clone(&self) -> Self {
        Self {
            policies: self.policies.par_iter().cloned().collect(),
            asking: self.asking.par_iter().cloned().collect(),
            requests: self.requests.par_iter().cloned().collect(),
        }
    }
}
impl Hauling {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        view: &View,
        travel: Option<&Travel>,
    ) -> Result<(), String> {
        let travel = travel.ok_or("hauling needs travel routes")?;
        let mut destinations: Vec<_> = self
            .policies
            .par_iter()
            .flat_map_iter(|p| [p.source, p.destination])
            .chain(
                self.asking
                    .par_iter()
                    .flat_map_iter(|p| [p.source, p.destination]),
            )
            .chain(
                self.requests
                    .par_iter()
                    .flat_map_iter(|p| [p.source, p.destination]),
            )
            .collect();
        destinations.par_sort_unstable();
        destinations.dedup();
        let routes = routes(travel, &destinations);
        let mut ids: Vec<_> = self.policies.par_iter().map(|p| p.person).collect();
        ids.par_sort_unstable();
        let mut requesters: Vec<_> = self.requests.par_iter().map(|r| r.requester).collect();
        requesters.par_sort_unstable();
        let mut askers: Vec<_> = self.asking.par_iter().map(|p| p.person).collect();
        askers.par_sort_unstable();
        if ids.par_windows(2).any(|p| p[0] == p[1])
            || requesters.par_windows(2).any(|p| p[0] == p[1])
            || askers.par_windows(2).any(|p| p[0] == p[1])
            || self.asking.par_iter().any(|p| {
                !people.contains(&p.person)
                    || !people.contains(&p.carrier)
                    || p.person == p.carrier
                    || !goods.contains(&p.good)
                    || p.retry_ticks == 0
                    || p.request_lifetime == 0
                    || p.source == p.destination
                    || !routes.contains_key(&(p.source, p.destination))
                    || !routes.contains_key(&(p.destination, p.source))
                    || view
                        .container(Place::Stock {
                            region: p.source,
                            good: p.good,
                        })
                        .is_none()
                    || view
                        .container(Place::Stock {
                            region: p.destination,
                            good: p.good,
                        })
                        .is_none()
            })
            || self.requests.par_iter().any(|r| {
                !people.contains(&r.requester)
                    || !people.contains(&r.carrier)
                    || r.requester == r.carrier
                    || !goods.contains(&r.good)
                    || r.source == r.destination
                    || !routes.contains_key(&(r.source, r.destination))
                    || !routes.contains_key(&(r.destination, r.source))
                    || view
                        .container(Place::Stock {
                            region: r.source,
                            good: r.good,
                        })
                        .is_none()
                    || view
                        .container(Place::Stock {
                            region: r.destination,
                            good: r.good,
                        })
                        .is_none()
            })
            || self.policies.par_iter().any(|p| {
                !people.contains(&p.person)
                    || !goods.contains(&p.good)
                    || p.quantity == 0
                    || p.source == p.destination
                    || !routes.contains_key(&(p.source, p.destination))
                    || !routes.contains_key(&(p.destination, p.source))
                    || view
                        .container(Place::Stock {
                            region: p.source,
                            good: p.good,
                        })
                        .is_none()
                    || view
                        .container(Place::Stock {
                            region: p.destination,
                            good: p.good,
                        })
                        .is_none()
                    || p.pressured_by
                        .is_some_and(|id| id == p.person || !people.contains(&id))
                    || p.pressure_threshold == Some(0)
                    || p.request_pressure_threshold == Some(0)
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
            })
        {
            return Err("invalid personal hauling preference".into());
        }
        Ok(())
    }
    pub fn ask(
        &mut self,
        people: &[Person],
        busy: &[u64],
        locations: &Colocation,
        tick: u64,
    ) -> Result<Vec<u64>, String> {
        if self.asking.is_empty() {
            return Ok(Vec::new());
        }
        let mut pending: Vec<_> = self.requests.par_iter().map(|r| r.requester).collect();
        pending.par_sort_unstable();
        let mut choices: Vec<_> = self
            .asking
            .par_iter()
            .filter_map(|p| {
                let actor = people
                    .binary_search_by_key(&p.person, |p| p.id)
                    .ok()
                    .map(|i| &people[i])?;
                let carrier = people
                    .binary_search_by_key(&p.carrier, |p| p.id)
                    .ok()
                    .map(|i| &people[i])?;
                (p.enabled
                    && tick >= p.next_review
                    && !actor.controlled
                    && actor.health > 0
                    && carrier.health > 0
                    && busy.binary_search(&actor.id).is_err()
                    && pending.binary_search(&actor.id).is_err()
                    && locations
                        .region((actor.id, actor.region), (carrier.id, carrier.region))
                        .is_some())
                .then_some(p)
            })
            .map(|p| {
                Ok((
                    p.person,
                    p.carrier,
                    p.source,
                    p.destination,
                    p.good,
                    tick.checked_add(p.request_lifetime)
                        .ok_or("hauling request expiry overflow")?,
                    tick.checked_add(p.retry_ticks)
                        .ok_or("hauling request review overflow")?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        choices.par_sort_unstable_by_key(|r| r.0);
        self.asking.par_iter_mut().for_each(|p| {
            if let Ok(i) = choices.binary_search_by_key(&p.person, |r| r.0) {
                p.next_review = choices[i].6;
            }
        });
        self.requests
            .par_extend(choices.par_iter().map(|r| Request {
                requester: r.0,
                carrier: r.1,
                source: r.2,
                destination: r.3,
                good: r.4,
                expires_at: r.5,
            }));
        Ok(choices.into_par_iter().map(|r| r.0).collect())
    }
}
pub struct Plan {
    pub actors: Vec<u64>,
    pub actions: Vec<Transaction>,
    pub departures: Vec<Departure>,
    pub uses: Vec<Use>,
}
pub struct Context<'a> {
    pub owner: &'a Hauling,
    pub travel: &'a Travel,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub claims: Option<&'a Claims<'a>>,
    pub view: &'a View,
    pub busy: &'a [u64],
    pub max_hunger: u32,
    pub tick: u64,
    pub seed: u64,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        let mut destinations: Vec<_> = self
            .owner
            .policies
            .par_iter()
            .flat_map_iter(|p| [p.source, p.destination])
            .collect();
        destinations.par_sort_unstable();
        destinations.dedup();
        let routes = routes(self.travel, &destinations);
        let opinions = self.opinions.map(Opinions::index);
        let chosen_requests: Vec<_> = if self.owner.requests.is_empty() {
            Vec::new()
        } else {
            let mut policies: Vec<_> = self.owner.policies.par_iter().collect();
            if policies.par_windows(2).any(|p| p[0].person > p[1].person) {
                policies.par_sort_unstable_by_key(|p| p.person);
            }
            let mut requests: Vec<_> = self
                .owner
                .requests
                .par_iter()
                .filter_map(|request| {
                    if request.expires_at < self.tick {
                        return None;
                    }
                    let policy = policies
                        .binary_search_by_key(&request.carrier, |p| p.person)
                        .ok()
                        .map(|i| policies[i])?;
                    let feels = |other, threshold| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(policy.person, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    };
                    let motivated = policy.willing
                        || policy
                            .pressured_by
                            .zip(policy.pressure_threshold)
                            .is_some_and(|(other, threshold)| feels(other, threshold))
                        || policy
                            .request_pressure_threshold
                            .is_some_and(|threshold| feels(request.requester, threshold));
                    (policy.source == request.source
                        && policy.destination == request.destination
                        && policy.good == request.good
                        && motivated)
                        .then_some((
                            request.carrier,
                            game_parallel_genetics::random(
                                self.seed ^ request.carrier,
                                request.requester,
                                self.tick,
                            ),
                            request.requester,
                        ))
                })
                .collect();
            requests.par_sort_unstable();
            requests
                .par_chunk_by(|a, b| a.0 == b.0)
                .map(|group| (group[0].0, group[0].2))
                .collect()
        };
        // Each policy proposes at most one physical transfer or departure.
        let mut choices: Vec<_> =
            self.owner
                .policies
                .par_iter()
                .filter_map(|p| {
                    let actor = self
                        .people
                        .binary_search_by_key(&p.person, |person| person.id)
                        .ok()
                        .map(|i| &self.people[i])?;
                    let region = actor.region?;
                    let pressured = p.pressured_by.zip(p.pressure_threshold).is_some_and(
                        |(other, threshold)| {
                            opinions.as_ref().is_some_and(|index| {
                                let (fear, obligation) = index.feelings(actor.id, other);
                                fear.saturating_add(obligation) >= threshold
                            })
                        },
                    );
                    let requester = chosen_requests
                        .binary_search_by_key(&actor.id, |r| r.0)
                        .ok()
                        .map(|i| chosen_requests[i].1);
                    let request_pressure = requester.zip(p.request_pressure_threshold).is_some_and(
                        |(other, threshold)| {
                            opinions.as_ref().is_some_and(|index| {
                                let (fear, obligation) = index.feelings(actor.id, other);
                                fear.saturating_add(obligation) >= threshold
                            })
                        },
                    );
                    if actor.controlled
                        || actor.health == 0
                        || actor.hunger > self.max_hunger
                        || self.busy.binary_search(&actor.id).is_ok()
                        || (p.requires_request && requester.is_none())
                        || (!p.willing && !pressured && !request_pressure)
                    {
                        return None;
                    }
                    let holder = self.view.container(Place::Person(actor.id))?;
                    let carried = holder.quantity(p.good);
                    if region == p.source && carried == 0 {
                        if self
                            .claims
                            .is_some_and(|claims| !claims.allows(actor.id, region, p.good))
                        {
                            return None;
                        }
                        let stock = self.view.container(Place::Stock {
                            region,
                            good: p.good,
                        })?;
                        let amount = p
                            .quantity
                            .min(stock.quantity(p.good))
                            .min(holder.capacity.saturating_sub(holder.used));
                        (amount > 0).then_some((
                            actor.id,
                            Some((
                                Place::Stock {
                                    region,
                                    good: p.good,
                                },
                                Place::Person(actor.id),
                                p.good,
                                amount,
                            )),
                            None,
                        ))
                    } else if region == p.destination && carried > 0 {
                        let stock = self.view.container(Place::Stock {
                            region,
                            good: p.good,
                        })?;
                        let amount = p
                            .quantity
                            .min(carried)
                            .min(stock.capacity.saturating_sub(stock.used));
                        (amount > 0).then_some((
                            actor.id,
                            Some((
                                Place::Person(actor.id),
                                Place::Stock {
                                    region,
                                    good: p.good,
                                },
                                p.good,
                                amount,
                            )),
                            None,
                        ))
                    } else {
                        let target = if carried > 0 { p.destination } else { p.source };
                        routes
                            .get(&(region, target))
                            .map(|&(_, next)| (actor.id, None, Some(next)))
                    }
                })
                .collect();
        choices.par_sort_unstable_by_key(|row| row.0);
        let transfers: Vec<_> = choices
            .par_iter()
            .filter_map(|&(person, movement, _)| movement.map(|m| (person, m)))
            .collect();
        base.checked_add(transfers.len() as u64)
            .ok_or("hauling action overflow")?;
        let actions: Vec<_> = transfers
            .par_iter()
            .enumerate()
            .map(|(i, &(_, (from, to, good, quantity)))| {
                let id = base + i as u64;
                Transaction {
                    id,
                    movements: vec![Movement {
                        id,
                        from,
                        to: Some(to),
                        good,
                        quantity,
                    }],
                    creations: vec![],
                }
            })
            .collect();
        let uses = transfers
            .par_iter()
            .enumerate()
            .filter_map(|(i, &(person, (from, _, good, quantity)))| {
                let Place::Stock { region, .. } = from else {
                    return None;
                };
                self.claims.map(|_| Use {
                    action: base + i as u64,
                    person,
                    region,
                    good,
                    quantity,
                    job: false,
                })
            })
            .collect();
        Ok(Plan {
            actors: choices.par_iter().map(|row| row.0).collect(),
            actions,
            uses,
            departures: choices
                .par_iter()
                .filter_map(|&(person, _, destination)| {
                    destination.map(|destination| Departure {
                        person,
                        destination,
                    })
                })
                .collect(),
        })
    }
}

//! Personal help moves actual possessions; no institution or promise is required.
use game_parallel_economy_person::{Person, travel::Colocation};
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Movement, Place, Transaction, View};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::Good;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub recipient: u64,
    pub good: u32,
    pub quantity: u64,
    pub recipient_hunger_at_least: u32,
    pub willing: bool,
    #[serde(default)]
    pub requires_request: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Giving {
    pub policies: Vec<Policy>,
    #[serde(default)]
    pub responses: Vec<Response>,
    #[serde(default)]
    pub learning: Vec<SharingLearning>,
    #[serde(default)]
    pub asking: Vec<Asking>,
    #[serde(default)]
    pub requests: Vec<Request>,
}
/// Personal experience may develop an inclination to help, never a binding rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharingLearning {
    pub person: u64,
    pub enabled: bool,
    pub threshold: u32,
    pub capacity: u32,
    #[serde(default)]
    pub experiences: u32,
    /// A remembered actual helper, used only for this person's own decisions.
    #[serde(default)]
    pub last_helper: Option<u64>,
    /// Offer food to that helper if they are hungry, without waiting for a request.
    #[serde(default)]
    pub reciprocate_hunger: Option<u32>,
    /// A learned way to ask the last helper when personally hungry.
    #[serde(default)]
    pub ask_helper: Option<HelperAsking>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelperAsking {
    pub min_hunger: u32,
    pub retry_ticks: u64,
    pub request_lifetime: u64,
    #[serde(default)]
    pub next_review: u64,
}
/// A person's willingness to respond to requests from different individuals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub person: u64,
    pub good: u32,
    pub quantity: u64,
    pub recipient_hunger_at_least: u32,
    pub willing: bool,
    #[serde(default)]
    pub minimum_trust: Option<i32>,
    /// Positive directed affinity can motivate help, separately from fear or obligation.
    #[serde(default)]
    pub affinity_threshold: Option<i32>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}
/// A person's inclination to ask a particular acquaintance for food.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asking {
    pub person: u64,
    pub target: u64,
    /// Other known people to consider after an unanswered request.
    #[serde(default)]
    pub alternatives: Vec<u64>,
    #[serde(default)]
    pub next_target: usize,
    pub min_hunger: u32,
    pub enabled: bool,
    pub retry_ticks: u64,
    pub request_lifetime: u64,
    #[serde(default)]
    pub next_review: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub requester: u64,
    pub target: u64,
    pub expires_at: u64,
}
impl Clone for Giving {
    fn clone(&self) -> Self {
        Self {
            policies: self.policies.par_iter().cloned().collect(),
            responses: self.responses.par_iter().cloned().collect(),
            learning: self.learning.par_iter().cloned().collect(),
            asking: self.asking.par_iter().cloned().collect(),
            requests: self.requests.par_iter().cloned().collect(),
        }
    }
}
impl Giving {
    pub fn validate(&self, people: &HashSet<u64>, catalog: &[Good]) -> Result<(), String> {
        let goods: HashSet<_> = catalog.iter().map(|g| g.id).collect();
        let food: HashSet<_> = catalog
            .iter()
            .filter(|g| g.food.is_some())
            .map(|g| g.id)
            .collect();
        let mut ids: Vec<_> = self
            .policies
            .par_iter()
            .map(|p| p.person)
            .chain(self.responses.par_iter().map(|p| p.person))
            .collect();
        ids.par_sort_unstable();
        let mut learners: Vec<_> = self.learning.par_iter().map(|p| p.person).collect();
        learners.par_sort_unstable();
        let mut askers: Vec<_> = self.asking.par_iter().map(|p| p.person).collect();
        askers.par_sort_unstable();
        let mut requested: Vec<_> = self.requests.par_iter().map(|r| r.requester).collect();
        requested.par_sort_unstable();
        if ids.par_windows(2).any(|p| p[0] == p[1])
            || learners.par_windows(2).any(|p| p[0] == p[1])
            || self.learning.par_iter().any(|p| {
                !people.contains(&p.person)
                    || p.threshold == 0
                    || p.capacity < p.threshold
                    || p.experiences > p.capacity
                    || p.last_helper
                        .is_some_and(|id| id == p.person || !people.contains(&id))
                    || p.ask_helper
                        .as_ref()
                        .is_some_and(|a| a.retry_ticks == 0 || a.request_lifetime == 0)
                    || ids.binary_search(&p.person).is_err()
            })
            || askers.par_windows(2).any(|p| p[0] == p[1])
            || requested.par_windows(2).any(|p| p[0] == p[1])
            || self.asking.par_iter().any(|p| {
                !people.contains(&p.person)
                    || !people.contains(&p.target)
                    || p.person == p.target
                    || p.alternatives.len() > 15
                    || p.next_target > p.alternatives.len()
                    || p.alternatives.iter().enumerate().any(|(i, &id)| {
                        !people.contains(&id)
                            || id == p.person
                            || id == p.target
                            || p.alternatives[..i].contains(&id)
                    })
                    || p.retry_ticks == 0
                    || p.request_lifetime == 0
            })
            || self.requests.par_iter().any(|r| {
                !people.contains(&r.requester)
                    || !people.contains(&r.target)
                    || r.requester == r.target
            })
            || self.responses.par_iter().any(|p| {
                !people.contains(&p.person)
                    || !food.contains(&p.good)
                    || p.quantity == 0
                    || p.pressure_threshold == Some(0)
                    || p.affinity_threshold.is_some_and(|n| n <= 0)
            })
            || self.policies.par_iter().any(|p| {
                !people.contains(&p.person)
                    || !people.contains(&p.recipient)
                    || p.person == p.recipient
                    || !goods.contains(&p.good)
                    || (p.requires_request && !food.contains(&p.good))
                    || p.quantity == 0
                    || p.pressured_by
                        .is_some_and(|id| id == p.person || !people.contains(&id))
                    || p.pressure_threshold == Some(0)
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
            })
        {
            return Err("invalid personal giving preference".into());
        }
        Ok(())
    }
    /// Requests are personal communications, not obligations or guaranteed deliveries.
    pub fn ask(
        &mut self,
        people: &[Person],
        goods: &[Good],
        view: &View,
        busy: &[u64],
        tick: u64,
        locations: &Colocation,
    ) -> Result<Vec<u64>, String> {
        let mut expired: Vec<_> = self
            .requests
            .par_iter()
            .filter(|r| r.expires_at < tick)
            .map(|r| r.requester)
            .collect();
        expired.par_sort_unstable();
        self.asking.par_iter_mut().for_each(|p| {
            if expired.binary_search(&p.person).is_ok() {
                p.next_target = (p.next_target + 1) % (p.alternatives.len() + 1);
            }
        });
        self.requests = self
            .requests
            .par_iter()
            .filter(|r| r.expires_at >= tick)
            .cloned()
            .collect();
        let mut pending: Vec<_> = self.requests.par_iter().map(|r| r.requester).collect();
        pending.par_sort_unstable();
        let food: HashSet<_> = goods
            .iter()
            .filter_map(|g| g.food.as_ref().map(|f| (g.id, f.portion)))
            .collect();
        let choices: Vec<_> = self
            .asking
            .par_iter()
            .filter_map(|p| {
                let actor = people
                    .binary_search_by_key(&p.person, |x| x.id)
                    .ok()
                    .map(|i| &people[i])?;
                if !p.enabled
                    || tick < p.next_review
                    || actor.controlled
                    || actor.health == 0
                    || actor.hunger < p.min_hunger
                    || busy.binary_search(&actor.id).is_ok()
                    || pending.binary_search(&actor.id).is_ok()
                {
                    return None;
                }
                let holding = view.container(Place::Person(actor.id))?;
                if holding.goods.iter().any(|g| {
                    food.iter()
                        .any(|&(id, portion)| id == g.good && g.quantity >= portion)
                }) {
                    return None;
                }
                let count = p.alternatives.len() + 1;
                let (target_index, target) = (0..count).find_map(|offset| {
                    let index = (p.next_target + offset) % count;
                    let id = if index == 0 {
                        p.target
                    } else {
                        p.alternatives[index - 1]
                    };
                    let target = people
                        .binary_search_by_key(&id, |x| x.id)
                        .ok()
                        .map(|i| &people[i])?;
                    (target.health > 0
                        && locations
                            .region((actor.id, actor.region), (target.id, target.region))
                            .is_some())
                    .then_some((index, target))
                })?;
                Some((
                    p.person,
                    target.id,
                    p.retry_ticks,
                    p.request_lifetime,
                    target_index,
                ))
            })
            .collect();
        let mut updates: Vec<_> = choices
            .into_par_iter()
            .map(|(person, target, retry, lifetime, target_index)| {
                Ok((
                    person,
                    target,
                    tick.checked_add(retry)
                        .ok_or("food request review overflow")?,
                    tick.checked_add(lifetime)
                        .ok_or("food request expiry overflow")?,
                    target_index,
                ))
            })
            .collect::<Result<_, String>>()?;
        updates.par_sort_unstable_by_key(|r| r.0);
        self.asking.par_iter_mut().for_each(|p| {
            if let Ok(i) = updates.binary_search_by_key(&p.person, |r| r.0) {
                p.next_review = updates[i].2;
                p.next_target = updates[i].4;
            }
        });
        // Construct a request from this person's own remembered helper; no target is authored.
        // An explicit personal asking preference takes priority, including an explicit refusal.
        let mut explicit: Vec<_> = self.asking.par_iter().map(|p| p.person).collect();
        explicit.par_sort_unstable();
        let helper_choices: Vec<_> = self
            .learning
            .par_iter()
            .filter_map(|learning| {
                let asking = learning.ask_helper.as_ref()?;
                let target_id = learning.last_helper?;
                let actor = people
                    .binary_search_by_key(&learning.person, |p| p.id)
                    .ok()
                    .map(|i| &people[i])?;
                let target = people
                    .binary_search_by_key(&target_id, |p| p.id)
                    .ok()
                    .map(|i| &people[i])?;
                if !learning.enabled
                    || learning.experiences < learning.threshold
                    || tick < asking.next_review
                    || actor.controlled
                    || actor.health == 0
                    || actor.hunger < asking.min_hunger
                    || target.health == 0
                    || explicit.binary_search(&actor.id).is_ok()
                    || busy.binary_search(&actor.id).is_ok()
                    || pending.binary_search(&actor.id).is_ok()
                    || locations
                        .region((actor.id, actor.region), (target.id, target.region))
                        .is_none()
                {
                    return None;
                }
                let holding = view.container(Place::Person(actor.id))?;
                if holding.goods.iter().any(|g| {
                    food.iter()
                        .any(|&(id, portion)| id == g.good && g.quantity >= portion)
                }) {
                    return None;
                }
                Some((
                    actor.id,
                    target.id,
                    asking.retry_ticks,
                    asking.request_lifetime,
                ))
            })
            .collect();
        let mut helper_updates: Vec<_> = helper_choices
            .into_par_iter()
            .map(|(person, target, retry, lifetime)| {
                Ok((
                    person,
                    target,
                    tick.checked_add(retry)
                        .ok_or("food request review overflow")?,
                    tick.checked_add(lifetime)
                        .ok_or("food request expiry overflow")?,
                ))
            })
            .collect::<Result<_, String>>()?;
        helper_updates.par_sort_unstable_by_key(|r| r.0);
        self.learning.par_iter_mut().for_each(|p| {
            if let (Ok(i), Some(asking)) = (
                helper_updates.binary_search_by_key(&p.person, |r| r.0),
                &mut p.ask_helper,
            ) {
                asking.next_review = helper_updates[i].2;
            }
        });
        self.requests
            .par_extend(helper_updates.par_iter().map(|r| Request {
                requester: r.0,
                target: r.1,
                expires_at: r.3,
            }));
        self.requests
            .par_extend(updates.par_iter().map(|r| Request {
                requester: r.0,
                target: r.1,
                expires_at: r.3,
            }));
        Ok(updates
            .into_par_iter()
            .map(|r| r.0)
            .chain(helper_updates.into_par_iter().map(|r| r.0))
            .collect())
    }
}
pub struct Plan {
    pub actors: Vec<u64>,
    pub actions: Vec<Transaction>,
    records: Vec<(u64, u64, u64, u32)>,
}
pub struct Context<'a> {
    pub owner: Option<&'a Giving>,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub goods: &'a [Good],
    pub view: &'a View,
    pub busy: &'a [u64],
    pub manual: Option<(u64, u64, u32, u64)>,
    pub tick: u64,
    pub seed: u64,
    pub locations: &'a Colocation,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        let find = |id| {
            self.people
                .binary_search_by_key(&id, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
        };
        let opinions = self.opinions.map(Opinions::index);
        let food_goods: HashSet<_> = self
            .goods
            .iter()
            .filter(|g| g.food.is_some())
            .map(|g| g.id)
            .collect();
        let mut learners: Vec<_> = self
            .owner
            .map_or(&[][..], |o| o.learning.as_slice())
            .par_iter()
            .filter(|p| p.enabled && p.experiences >= p.threshold)
            .map(|p| p.person)
            .collect();
        learners.par_sort_unstable();
        let policies = self.owner.map_or(&[][..], |o| o.policies.as_slice());
        let requests: HashSet<_> = self
            .owner
            .map_or(&[][..], |o| o.requests.as_slice())
            .par_iter()
            .filter(|r| r.expires_at >= self.tick)
            .map(|r| (r.requester, r.target))
            .collect();
        let mut chosen: Vec<_> = policies
            .par_iter()
            .filter_map(|policy| {
                let sender = find(policy.person)?;
                let recipient = find(policy.recipient)?;
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(sender.id, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                let from = self.view.container(Place::Person(sender.id))?;
                let to = self.view.container(Place::Person(recipient.id))?;
                (!sender.controlled
                    && sender.health > 0
                    && recipient.health > 0
                    && self
                        .locations
                        .region((sender.id, sender.region), (recipient.id, recipient.region))
                        .is_some()
                    && recipient.hunger >= policy.recipient_hunger_at_least
                    && self.busy.binary_search(&sender.id).is_err()
                    && (policy.willing || pressured || learners.binary_search(&sender.id).is_ok())
                    && (!policy.requires_request
                        || (food_goods.contains(&policy.good)
                            && requests.contains(&(recipient.id, sender.id))))
                    && from.quantity(policy.good) >= policy.quantity
                    && to.quantity(policy.good) < policy.quantity
                    && to.capacity.saturating_sub(to.used) >= policy.quantity)
                    .then_some((sender.id, recipient.id, policy.good, policy.quantity))
            })
            .collect();
        let mut responses: Vec<_> = self
            .owner
            .map_or(&[][..], |o| o.responses.as_slice())
            .par_iter()
            .collect();
        responses.par_sort_unstable_by_key(|p| p.person);
        let mut candidates: Vec<_> = self
            .owner
            .map_or(&[][..], |o| o.requests.as_slice())
            .par_iter()
            .filter_map(|request| {
                if request.expires_at < self.tick {
                    return None;
                }
                let policy = responses
                    .binary_search_by_key(&request.target, |p| p.person)
                    .ok()
                    .map(|i| responses[i])?;
                let sender = find(policy.person)?;
                let recipient = find(request.requester)?;
                let from = self.view.container(Place::Person(sender.id))?;
                let to = self.view.container(Place::Person(recipient.id))?;
                let trust = opinions
                    .as_ref()
                    .map_or(0, |index| index.trust(sender.id, recipient.id));
                let affinity = opinions
                    .as_ref()
                    .map_or(0, |index| index.affinity(sender.id, recipient.id));
                let likes = policy
                    .affinity_threshold
                    .is_some_and(|threshold| affinity >= threshold);
                let pressured = policy.pressure_threshold.is_some_and(|threshold| {
                    opinions.as_ref().is_some_and(|index| {
                        let (fear, obligation) = index.feelings(sender.id, recipient.id);
                        fear.saturating_add(obligation) >= threshold
                    })
                });
                (!sender.controlled
                    && sender.health > 0
                    && recipient.health > 0
                    && self
                        .locations
                        .region((sender.id, sender.region), (recipient.id, recipient.region))
                        .is_some()
                    && recipient.hunger >= policy.recipient_hunger_at_least
                    && self.busy.binary_search(&sender.id).is_err()
                    && (pressured
                        || ((policy.willing
                            || likes
                            || learners.binary_search(&sender.id).is_ok())
                            && policy.minimum_trust.is_none_or(|minimum| trust >= minimum)))
                    && food_goods.contains(&policy.good)
                    && from.quantity(policy.good) >= policy.quantity
                    && to.quantity(policy.good) < policy.quantity
                    && to.capacity.saturating_sub(to.used) >= policy.quantity)
                    .then_some((
                        sender.id,
                        std::cmp::Reverse(affinity),
                        game_parallel_genetics::random(
                            self.seed ^ sender.id,
                            recipient.id,
                            self.tick,
                        ),
                        recipient.id,
                        policy.good,
                        policy.quantity,
                    ))
            })
            .collect();
        candidates.par_sort_unstable();
        chosen.par_extend(
            candidates
                .par_chunk_by(|a, b| a.0 == b.0)
                .map(|group| (group[0].0, group[0].3, group[0].4, group[0].5)),
        );
        // A remembered helper can become the target of an unsolicited personal offer.
        // Existing requested help takes priority, so each person still chooses one action.
        let mut already_chosen: Vec<_> = chosen.par_iter().map(|row| row.0).collect();
        already_chosen.par_sort_unstable();
        let reciprocal: Vec<_> = self
            .owner
            .map_or(&[][..], |o| o.learning.as_slice())
            .par_iter()
            .filter_map(|learning| {
                if !learning.enabled
                    || learning.experiences < learning.threshold
                    || already_chosen.binary_search(&learning.person).is_ok()
                {
                    return None;
                }
                let minimum = learning.reciprocate_hunger?;
                let sender = find(learning.person)?;
                let recipient = find(learning.last_helper?)?;
                let policy = responses
                    .binary_search_by_key(&sender.id, |p| p.person)
                    .ok()
                    .map(|i| responses[i])?;
                let from = self.view.container(Place::Person(sender.id))?;
                let to = self.view.container(Place::Person(recipient.id))?;
                let trust = opinions
                    .as_ref()
                    .map_or(0, |index| index.trust(sender.id, recipient.id));
                (!sender.controlled
                    && sender.health > 0
                    && recipient.health > 0
                    && recipient.hunger >= minimum
                    && self.busy.binary_search(&sender.id).is_err()
                    && self
                        .locations
                        .region((sender.id, sender.region), (recipient.id, recipient.region))
                        .is_some()
                    && policy.minimum_trust.is_none_or(|min| trust >= min)
                    && from.quantity(policy.good) >= policy.quantity
                    && to.quantity(policy.good) < policy.quantity
                    && to.capacity.saturating_sub(to.used) >= policy.quantity)
                    .then_some((sender.id, recipient.id, policy.good, policy.quantity))
            })
            .collect();
        chosen.par_extend(reciprocal);
        if let Some((sender, recipient, good, quantity)) = self.manual {
            let a = find(sender).ok_or("unknown person")?;
            let b = find(recipient).ok_or("unknown recipient")?;
            if !a.controlled
                || a.health == 0
                || b.health == 0
                || sender == recipient
                || self
                    .locations
                    .region((a.id, a.region), (b.id, b.region))
                    .is_none()
                || quantity == 0
                || !self.goods.iter().any(|g| g.id == good)
            {
                return Err(
                    "giving requires a living controlled person and a local recipient".into(),
                );
            }
            chosen.push((sender, recipient, good, quantity));
        }
        chosen.par_sort_unstable();
        base.checked_add(chosen.len() as u64)
            .ok_or("giving action overflow")?;
        let actions = chosen
            .par_iter()
            .enumerate()
            .map(|(i, &(sender, recipient, good, quantity))| {
                let id = base + i as u64;
                Transaction {
                    id,
                    movements: vec![Movement {
                        id,
                        from: Place::Person(sender),
                        to: Some(Place::Person(recipient)),
                        good,
                        quantity,
                    }],
                    creations: vec![],
                }
            })
            .collect();
        let actors = chosen.par_iter().map(|row| row.0).collect();
        let records = chosen
            .par_iter()
            .enumerate()
            .map(|(i, row)| (base + i as u64, row.0, row.1, row.2))
            .collect();
        Ok(Plan {
            actors,
            actions,
            records,
        })
    }
}
impl Plan {
    /// Learn only from food that was actually received in this boundary.
    pub fn learn_sharing(&self, giving: &mut Giving, events: &Events, goods: &[Good]) {
        if giving.learning.is_empty() || self.records.is_empty() {
            return;
        }
        let accepted: HashSet<_> = events
            .physical_actions
            .par_iter()
            .filter(|e| e.accepted)
            .map(|e| e.id)
            .collect();
        let food: HashSet<_> = goods
            .iter()
            .filter(|g| g.food.is_some())
            .map(|g| g.id)
            .collect();
        let mut recipients: Vec<_> = self
            .records
            .par_iter()
            .filter(|r| accepted.contains(&r.0) && food.contains(&r.3))
            .map(|r| (r.2, r.0, r.1))
            .collect();
        recipients.par_sort_unstable();
        giving
            .learning
            .par_iter_mut()
            .filter(|p| p.enabled)
            .for_each(|p| {
                let start = recipients.partition_point(|row| row.0 < p.person);
                let end = recipients.partition_point(|row| row.0 <= p.person);
                let count = u32::try_from(end - start).unwrap_or(u32::MAX);
                p.experiences = p.experiences.saturating_add(count).min(p.capacity);
                if end > start {
                    p.last_helper = Some(recipients[end - 1].2);
                }
            });
    }
    pub fn clear_fulfilled_requests(&self, giving: &mut Giving, events: &Events, goods: &[Good]) {
        let accepted: HashSet<_> = events
            .physical_actions
            .par_iter()
            .filter(|e| e.accepted)
            .map(|e| e.id)
            .collect();
        let food: HashSet<_> = goods
            .iter()
            .filter(|g| g.food.is_some())
            .map(|g| g.id)
            .collect();
        let fulfilled: HashSet<_> = self
            .records
            .par_iter()
            .filter(|r| accepted.contains(&r.0) && food.contains(&r.3))
            .map(|r| (r.2, r.1))
            .collect();
        giving.requests = giving
            .requests
            .par_iter()
            .filter(|r| !fulfilled.contains(&(r.requester, r.target)))
            .cloned()
            .collect();
    }
    pub fn delivered(&self, actor: u64, events: &Events) -> bool {
        self.records.iter().find(|r| r.1 == actor).is_some_and(|r| {
            events
                .physical_actions
                .iter()
                .any(|d| d.id == r.0 && d.accepted)
        })
    }
    /// Each recipient personally interprets an accepted gift.
    pub fn help(&self, events: &Events) -> Vec<(u64, u64)> {
        let accepted: HashSet<_> = events
            .physical_actions
            .par_iter()
            .filter(|event| event.accepted)
            .map(|event| event.id)
            .collect();
        self.records
            .par_iter()
            .filter(|record| accepted.contains(&record.0))
            .map(|record| (record.2, record.1))
            .collect()
    }
}

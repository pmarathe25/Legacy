//! Persistent autonomous actor metadata and reusable review-clock scratch.
use game_parallel_autonomy::{Actor, Autonomy, Decision, Event, Outcome, Path, Rules, Situation};
use game_parallel_knowledge::Record;
use rayon::prelude::*;
use std::{
    cmp::Reverse,
    sync::atomic::{AtomicU64, Ordering},
};
const BLOCK: usize = 1024;
static INSTANCE: AtomicU64 = AtomicU64::new(1);
pub trait SituationView: Sync {
    fn read(&self, person: u64) -> Situation;
}
pub struct Runtime {
    actors: Vec<Actor>,
    order: Vec<usize>,
    reviews: Vec<u64>,
    paths: Vec<Path>,
    rules: Rules,
    events: Vec<Event>,
    tick: u64,
    epoch: u64,
    instance: u64,
}
pub struct Draft {
    pub decisions: Vec<Decision>,
    tick: u64,
    epoch: u64,
    instance: u64,
}
impl Runtime {
    /// Host validates actor identities, rule values, routes and imported receipts.
    pub fn compile_validated(
        owner: &Autonomy,
        paths: &[Path],
        tick: u64,
    ) -> Result<Self, &'static str> {
        if owner.rules.review_interval == 0 {
            return Err("invalid autonomy rules");
        }
        let mut actors = owner.actors.clone();
        actors.par_sort_unstable_by_key(|a| a.person);
        if actors.windows(2).any(|a| a[0].person == a[1].person) {
            return Err("duplicate autonomous actor");
        }
        let order = owner
            .actors
            .par_iter()
            .map(|a| {
                actors
                    .binary_search_by_key(&a.person, |a| a.person)
                    .unwrap()
            })
            .collect();
        let reviews = actors.iter().map(|a| a.next_review).collect();
        let mut paths = paths.to_vec();
        paths.par_sort_unstable_by_key(|p| (p.source, p.destination));
        Ok(Self {
            actors,
            order,
            reviews,
            paths,
            rules: owner.rules.clone(),
            events: owner.events.clone(),
            tick,
            epoch: 0,
            instance: INSTANCE.fetch_add(1, Ordering::Relaxed),
        })
    }
    pub fn export(&self) -> Autonomy {
        Autonomy {
            rules: self.rules.clone(),
            actors: self
                .order
                .par_iter()
                .map(|&i| self.actors[i].clone())
                .collect(),
            events: self.events.clone(),
        }
    }
    pub fn prepare<'a>(
        &mut self,
        view: &impl SituationView,
        records_for: impl Fn(u64) -> Option<&'a [Record]> + Sync,
        now: u64,
    ) -> Result<Draft, &'static str> {
        if now != self.tick {
            return Err("autonomy phase must advance one tick");
        }
        let tick = now.checked_add(1).ok_or("autonomy clock overflow")?;
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("autonomy generation overflow")?;
        let actors = &self.actors;
        let rules = &self.rules;
        let paths = &self.paths;
        let chunks = self
            .reviews
            .par_chunks_mut(BLOCK)
            .enumerate()
            .map(|(block, reviews)| {
                let mut decisions = Vec::new();
                for (offset, review) in reviews.iter_mut().enumerate() {
                    let actor = &actors[block * BLOCK + offset];
                    *review = actor.next_review;
                    let situation = view.read(actor.person);
                    let Some(source) = situation.region else {
                        continue;
                    };
                    if !actor.enabled
                        || situation.busy
                        || situation.health == 0
                        || situation.health < rules.min_health
                        || actor.next_review > now
                    {
                        continue;
                    }
                    *review = now
                        .checked_add(rules.review_interval)
                        .ok_or("autonomy review overflow")?;
                    if situation.hunger < rules.min_hunger.saturating_sub(actor.hunger_urgency)
                        || paths.is_empty()
                    {
                        continue;
                    }
                    let Some(records) = records_for(actor.person) else {
                        continue;
                    };
                    let valid = |r: &&Record| {
                        r.observed_at <= now && now - r.observed_at <= rules.max_observation_age
                    };
                    let Some(local) = records.iter().filter(valid).find(|r| r.region == source)
                    else {
                        continue;
                    };
                    let start = paths.partition_point(|p| p.source < source);
                    let end = paths.partition_point(|p| p.source <= source);
                    let best = paths[start..end]
                        .iter()
                        .filter_map(|path| {
                            let record = records
                                .iter()
                                .filter(valid)
                                .find(|r| r.region == path.destination)?;
                            (record.food >= rules.min_destination_food
                                && u64::from(record.food)
                                    >= u64::from(local.food)
                                        + u64::from(rules.min_food_gain)
                                        + u64::from(actor.travel_caution))
                            .then_some((
                                record.food,
                                Reverse(path.ticks),
                                Reverse(path.destination),
                                path.destination,
                            ))
                        })
                        .max();
                    if let Some((_, _, _, destination)) = best {
                        decisions.push(Decision {
                            person: actor.person,
                            source,
                            destination,
                        });
                    }
                }
                Ok(decisions)
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        let decisions = chunks.into_par_iter().flatten().collect();
        Ok(Draft {
            decisions,
            tick,
            epoch: self.epoch,
            instance: self.instance,
        })
    }
    pub fn check(&self, draft: &Draft) -> Result<(), &'static str> {
        if draft.instance != self.instance
            || draft.epoch != self.epoch
            || self.tick.checked_add(1) != Some(draft.tick)
        {
            return Err("stale autonomy draft");
        }
        Ok(())
    }
    /// Movement admission has already resolved against the unpublished world boundary.
    pub fn commit(
        &mut self,
        draft: Draft,
        available: &[bool],
        result: &Result<(), String>,
    ) -> Result<(), &'static str> {
        self.check(&draft)?;
        if available.len() != draft.decisions.len() {
            return Err("invalid autonomous outcomes");
        }
        let events = draft
            .decisions
            .par_iter()
            .zip(available)
            .map(|(d, &yes)| Event {
                tick: draft.tick,
                person: d.person,
                source: d.source,
                destination: d.destination,
                outcome: if !yes {
                    Outcome::Unavailable
                } else if result.is_ok() {
                    Outcome::Started
                } else {
                    Outcome::Rejected
                },
                reason: if yes {
                    result.as_ref().err().cloned()
                } else {
                    Some("person unavailable at departure".into())
                },
            })
            .collect();
        self.actors
            .par_iter_mut()
            .zip(&self.reviews)
            .for_each(|(actor, &review)| actor.next_review = review);
        self.events = events;
        self.tick = draft.tick;
        Ok(())
    }
    /// Append validated newborn actors in canonical identity order.
    pub fn enroll(&mut self, births: &[Actor]) -> Result<(), &'static str> {
        if births.is_empty() {
            return Ok(());
        }
        if births.par_windows(2).any(|p| p[0].person >= p[1].person)
            || self
                .actors
                .last()
                .is_some_and(|a| a.person >= births[0].person)
        {
            return Err("newborn actors must append canonical identities");
        }
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("autonomy generation overflow")?;
        let base = self.actors.len();
        self.order
            .par_extend((base..base + births.len()).into_par_iter());
        self.reviews
            .par_extend(births.par_iter().map(|a| a.next_review));
        self.actors.par_extend(births.par_iter().cloned());
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/support/differential.rs"]
mod differential;

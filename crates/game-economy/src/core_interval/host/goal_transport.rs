//! Personal supply trips for an active craft goal.
use super::*;
use game_economy_local_transport::{
    Endpoint, LocalTransport,
    goal::{Context, Need},
    walking::{Request, Walking},
};
use game_lots::{Place, View};
use std::collections::BTreeSet;

pub(super) struct GoalTravel {
    pub requests: Vec<Request>,
    pub searches: Vec<crate::local_walking::SearchProposal>,
}

pub(super) fn search_expiries(
    owner: Option<&LocalTransport>,
    now: u128,
) -> Result<BTreeSet<(u128, u64)>, String> {
    let Some(owner) = owner else {
        return Ok(BTreeSet::new());
    };
    let expiries: BTreeSet<_> = owner
        .goal_fetchers
        .par_iter()
        .filter_map(|preference| {
            Some((
                preference.search.as_ref()?.elapsed?.expires_at?,
                preference.person,
            ))
        })
        .collect();
    if expiries.first().is_some_and(|row| row.0 < now) {
        return Err("local goal search missed its expiry".into());
    }
    Ok(expiries)
}

impl CoreHost {
    pub(super) fn next_local_goal_search_expiry(&self) -> Option<u64> {
        let &(at, _) = self.local_goal_search_expiries.first()?;
        u64::try_from(at.saturating_sub(self.core.simulated_millis())).ok()
    }

    pub(super) fn expire_local_goal_searches(&mut self) -> Vec<u64> {
        let now = self.core.simulated_millis();
        if self
            .local_goal_search_expiries
            .first()
            .is_none_or(|row| row.0 > now)
        {
            return Vec::new();
        }
        let future = now.checked_add(1).map_or_else(BTreeSet::new, |time| {
            self.local_goal_search_expiries.split_off(&(time, 0))
        });
        let mut searches: Vec<_> = std::mem::replace(&mut self.local_goal_search_expiries, future)
            .into_iter()
            .map(|(_, person)| person)
            .collect();
        searches.par_sort_unstable();
        let Some(owner) = self.local_transport.as_mut() else {
            return Vec::new();
        };
        let walking = owner.walking.as_ref().expect("timed local walking");
        let core = &self.core;
        let mut halt: Vec<_> = owner
            .goal_fetchers
            .par_chunks_mut(256)
            .flat_map_iter(|chunk| {
                let first = searches.partition_point(|id| *id < chunk[0].person);
                let end = searches.partition_point(|id| *id <= chunk[chunk.len() - 1].person);
                searches[first..end]
                    .iter()
                    .filter_map(move |&person| {
                        let index = chunk
                            .binary_search_by_key(&person, |row| row.person)
                            .expect("known due search");
                        let search = chunk[index].search.as_mut().expect("active timed search");
                        let owns_walk = search.elapsed.is_some_and(|clock| clock.owns_walk);
                        search.stop();
                        let actor = core.person(person)?;
                        let region = actor.region?;
                        (owns_walk && !actor.controlled && walking.moving(person, region))
                            .then_some(person)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        if !halt.is_empty()
            && let Some(clock) = owner
                .walking
                .as_mut()
                .and_then(|walking| walking.elapsed.as_mut())
        {
            clock.paused.append(&mut halt);
            clock.paused.par_sort_unstable();
            clock.paused.dedup();
        }
        self.work_checked_revision = None;
        searches
    }

    pub(super) fn publish_local_goal_searches(
        &mut self,
        mut edits: Vec<crate::local_walking::SearchProposal>,
    ) {
        if edits.is_empty() && self.local_goal_search_expiries.is_empty() {
            return;
        }
        edits.par_sort_unstable_by_key(|row| row.person);
        let owner = self
            .local_transport
            .as_mut()
            .expect("known local transport");
        let changes: Vec<_> = owner
            .goal_fetchers
            .par_iter_mut()
            .filter_map(|preference| {
                let previous = preference
                    .search
                    .as_ref()
                    .and_then(|search| search.elapsed?.expires_at);
                if let Ok(index) = edits.binary_search_by_key(&preference.person, |row| row.person)
                {
                    let edit = &edits[index];
                    let mut search = edit.search.clone();
                    if let Some(clock) = &mut search.elapsed {
                        clock.owns_walk = edit.request.is_some();
                    }
                    preference.search = Some(search);
                } else if let Some(clock) = preference
                    .search
                    .as_mut()
                    .and_then(|search| search.elapsed.as_mut())
                {
                    clock.owns_walk = false;
                }
                let current = preference
                    .search
                    .as_ref()
                    .and_then(|search| search.elapsed?.expires_at);
                (current != previous).then_some((preference.person, previous, current))
            })
            .collect();
        for (person, previous, current) in changes {
            if let Some(previous) = previous {
                self.local_goal_search_expiries.remove(&(previous, person));
            }
            if let Some(current) = current {
                self.local_goal_search_expiries.insert((current, person));
            }
        }
    }

    pub(super) fn goal_fetch_needs(&self, owner: &LocalTransport) -> Vec<Need> {
        let Some(goals) = self
            .goals
            .as_ref()
            .filter(|_| !owner.goal_fetchers.is_empty())
        else {
            return Vec::new();
        };
        let mut active: Vec<_> = goals
            .people
            .par_iter()
            .filter(|goal| {
                goal.enabled
                    && goal.progress < goal.target
                    && owner
                        .goal_fetchers
                        .binary_search_by_key(&goal.person, |preference| preference.person)
                        .ok()
                        .is_some_and(|index| owner.goal_fetchers[index].enabled)
            })
            .collect();
        active.par_sort_unstable_by_key(|goal| {
            (goal.person, std::cmp::Reverse(goal.priority), goal.recipe)
        });
        active
            .par_chunk_by(|a, b| a.person == b.person)
            .map(|rows| Need {
                person: rows[0].person,
                recipe: rows[0].recipe,
            })
            .collect()
    }

    pub(super) fn goal_carry_context<'a>(
        &'a self,
        owner: &'a LocalTransport,
        view: &'a View,
        busy: &'a [u64],
        needs: &'a [Need],
    ) -> Context<'a> {
        let unit = owner
            .walking
            .as_ref()
            .and_then(|walking| walking.elapsed.as_ref())
            .map_or_else(
                || {
                    self.schedule
                        .as_ref()
                        .map_or(1, |schedule| schedule.period_millis)
                },
                |clock| clock.unit_millis,
            );
        let now =
            u64::try_from(self.core.simulated_millis() / u128::from(unit)).unwrap_or(u64::MAX);
        Context {
            owner,
            needs,
            recipes: self.core.physical.recipes(),
            people: &self.core.people,
            view,
            busy,
            unwilling: &[],
            max_hunger: u32::MAX,
            tick: now,
            plan_inputs: self.goals.as_ref().is_some_and(|goals| goals.plan_inputs),
            review: self
                .goals
                .as_ref()
                .map_or(0, |goals| goals.review_ordinal(self.core.simulated_millis(), now)),
        }
    }

    pub(super) fn plan_local_goal_routes(
        &self,
        owner: &LocalTransport,
        walking: &Walking,
        view: &View,
        busy: &[u64],
    ) -> Result<GoalTravel, String> {
        let needs = self.goal_fetch_needs(owner);
        let elapsed = walking.elapsed.as_ref().expect("elapsed walking");
        let now = u64::try_from(self.core.simulated_millis() / u128::from(elapsed.unit_millis))
            .map_err(|_| "local supply search time overflow")?;
        let mut requests: Vec<_> = needs
            .par_iter()
            .filter_map(|need| {
                if busy.binary_search(&need.person).is_ok() {
                    return None;
                }
                let person = &self.core.people[self
                    .core
                    .people
                    .binary_search_by_key(&need.person, |p| p.id)
                    .ok()?];
                let region = person.region?;
                if person.health == 0
                    || person.controlled
                    || !walking.can_advance(person.id, region, Endpoint::Stock)
                {
                    return None;
                }
                let recipes = self.core.physical.recipes();
                let recipe = &recipes[recipes
                    .binary_search_by_key(&need.recipe, |row| row.id)
                    .ok()?];
                let holder = view.container(Place::Person(person.id))?;
                recipe
                    .inputs
                    .iter()
                    .any(|input| holder.quantity(input.good) > 0)
                    .then_some(Request {
                        person: person.id,
                        region,
                        destination: Endpoint::Stock,
                    })
            })
            .collect();
        let mut excluded = busy.to_vec();
        excluded.par_extend(requests.par_iter().map(|request| request.person));
        excluded.par_sort_unstable();
        excluded.dedup();
        let searches =
            crate::local_walking::goal_search(crate::local_walking::GoalSearchContext {
                owner,
                walking,
                people: &self.core.people,
                view,
                recipes: self.core.physical.recipes(),
                needs: &needs,
                busy: &excluded,
                underage: &[],
                max_hunger: u32::MAX,
                tick: now,
                now: self.core.simulated_millis(),
                honors_work_inclination: false,
            })?;
        requests.par_extend(searches.par_iter().filter_map(|row| row.request));
        requests.par_sort_unstable_by_key(|request| request.person);
        Ok(GoalTravel { requests, searches })
    }
}

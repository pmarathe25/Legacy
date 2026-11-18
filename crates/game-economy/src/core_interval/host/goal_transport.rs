//! Personal supply trips for an active craft goal.
use super::*;
use game_economy_local_transport::{
    Endpoint, LocalTransport,
    goal::{Context, Need},
    walking::{Request, Walking},
};
use game_lots::{Place, View};

pub(super) struct GoalTravel {
    pub requests: Vec<Request>,
    pub searches: Vec<crate::local_walking::SearchProposal>,
}

impl CoreHost {
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
                .map_or(0, |goals| now / goals.retry_interval),
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
                honors_work_inclination: false,
            })?;
        requests.par_extend(searches.par_iter().filter_map(|row| row.request));
        requests.par_sort_unstable_by_key(|request| request.person);
        Ok(GoalTravel { requests, searches })
    }
}

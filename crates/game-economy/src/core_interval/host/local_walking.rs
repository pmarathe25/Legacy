//! Personal movement between local endpoints.
use super::*;
use game_economy_local_transport::{
    Endpoint,
    walking::{Event, Location, Outcome, Progress, Request, Walking},
};

impl CoreHost {
    pub(super) fn walking(&self) -> Option<&Walking> {
        self.local_transport.as_ref()?.walking.as_ref()
    }

    pub fn local_location(&self, person: u64) -> Option<Location> {
        let actor = self
            .core
            .people
            .binary_search_by_key(&person, |row| row.id)
            .ok()?;
        Some(
            self.walking()?
                .location(person, self.core.people[actor].region?),
        )
    }

    pub(super) fn regional_stock_reachable(&self, person: u64, region: u64) -> bool {
        self.walking()
            .is_none_or(|walking| walking.stock_reachable(person, region))
    }

    pub(super) fn local_partition(&self, person: u64) -> (u64, u64) {
        match self.local_location(person) {
            Some(Location::At {
                endpoint: Endpoint::Store { id },
                ..
            }) => (1, id),
            Some(Location::Walking { .. }) => (2, person),
            _ => (0, 0),
        }
    }

    pub(super) fn same_local_endpoint(&self, a: u64, b: u64) -> bool {
        let a = self.local_partition(a);
        a.0 != 2 && a == self.local_partition(b)
    }

    pub(super) fn clear_local_positions(&mut self, departed: &[u64]) {
        if departed.is_empty() {
            return;
        }
        if let Some(walking) = self
            .local_transport
            .as_mut()
            .and_then(|owner| owner.walking.as_mut())
        {
            let departed: std::collections::HashSet<_> = departed.par_iter().copied().collect();
            walking.positions = std::mem::take(&mut walking.positions)
                .into_par_iter()
                .filter(|position| !departed.contains(&position.person))
                .collect();
            if let Some(clock) = &mut walking.elapsed {
                clock.paused.retain(|person| !departed.contains(person));
            }
        }
    }

    pub(super) fn block_away_from_stock(&self, busy: &[u64]) -> Option<Vec<u64>> {
        let walking = self.walking()?;
        let mut excluded: Vec<_> = walking
            .positions
            .par_iter()
            .filter_map(|position| {
                (!matches!(
                    position.location,
                    Location::At {
                        endpoint: Endpoint::Stock,
                        ..
                    }
                ))
                .then_some(position.person)
            })
            .collect();
        if excluded.is_empty() {
            return None;
        }
        excluded.par_extend(busy.par_iter().copied());
        excluded.par_sort_unstable();
        excluded.dedup();
        Some(excluded)
    }

    fn can_walk(&self, person: u64) -> bool {
        self.core
            .people
            .binary_search_by_key(&person, |row| row.id)
            .ok()
            .is_some_and(|index| {
                self.core.people[index].health > 0 && self.core.people[index].region.is_some()
            })
    }

    pub(super) fn next_local_arrival(&self) -> Option<u64> {
        self.walking()?.next_arrival(|person| self.can_walk(person))
    }

    pub(super) fn local_travelers(&self) -> impl ParallelIterator<Item = u64> + '_ {
        let walking = self.walking();
        walking
            .map_or(&[][..], |owner| owner.positions.as_slice())
            .par_iter()
            .filter_map(move |position| {
                let Location::Walking { region, .. } = position.location else {
                    return None;
                };
                (walking.is_some_and(|walking| walking.moving(position.person, region))
                    && self.can_walk(position.person))
                .then_some(position.person)
            })
    }

    pub(super) fn stage_local_progress(&self, consumed: u64) -> Option<Progress> {
        self.walking()?.stage_elapsed(
            |person| self.can_walk(person),
            consumed,
            self.core.physical.tick(),
        )
    }

    pub(super) fn publish_local_progress(&mut self, progress: Option<Progress>) -> Vec<Event> {
        let Some(progress) = progress else {
            return Vec::new();
        };
        let walking = self
            .local_transport
            .as_mut()
            .and_then(|owner| owner.walking.as_mut())
            .expect("walking owner");
        walking.positions = progress.positions;
        if !progress.arrivals.is_empty() {
            walking.events = progress.arrivals.par_iter().cloned().collect();
            self.work_checked_revision = None;
        }
        progress.arrivals
    }

    fn local_walk_exclusions(&self, blocked: &[u64]) -> Vec<u64> {
        let resting = self.blocked_with_rest(blocked);
        let mut busy = resting.as_deref().unwrap_or(blocked).to_vec();
        busy.par_extend(self.core.physical.work().par_iter().filter_map(|row| {
            (row.phase == game_lot_runtime::Phase::Working).then_some(row.job.worker)
        }));
        if let Some(shared) = &self.shared_work {
            busy.par_extend(shared.sessions.par_iter().map(|row| row.event.person));
        }
        busy.par_sort_unstable();
        busy.dedup();
        busy
    }

    pub(super) fn walk_autonomously(&mut self, blocked: &[u64]) -> Result<Vec<Event>, String> {
        let Some(owner) = &self.local_transport else {
            return Ok(Vec::new());
        };
        let Some(walking) = &owner.walking else {
            return Ok(Vec::new());
        };
        let tick = self.core.physical.tick();
        let busy = self.local_walk_exclusions(blocked);
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let view = self.core.physical.view(tick)?;
        let mut choices = game_economy_local_transport::walking::personal_requests(
            owner,
            walking,
            &self.core.people,
            &view,
            (&busy, &[]),
            self.opinions.as_ref(),
            (u32::MAX, self.core.physical.goods(), claims.as_ref()),
        );
        let goals = if owner.goal_fetchers.is_empty() {
            None
        } else {
            let mut occupied = busy.clone();
            occupied.par_extend(choices.par_iter().map(|option| option.person));
            occupied.par_sort_unstable();
            occupied.dedup();
            let plan = self.plan_local_goal_routes(owner, walking, &view, &occupied)?;
            choices.par_extend(plan.requests.par_iter().copied());
            choices.par_sort_unstable_by_key(|request| request.person);
            Some(plan)
        };
        let mut halting: Vec<_> = walking
            .positions
            .par_iter()
            .filter_map(|position| {
                let Location::Walking { region, .. } = position.location else {
                    return None;
                };
                let id = position.person;
                let index = self
                    .core
                    .people
                    .binary_search_by_key(&id, |person| person.id)
                    .ok()?;
                (!self.core.people[index].controlled
                    && walking.moving(id, region)
                    && choices
                        .binary_search_by_key(&id, |request| request.person)
                        .is_err())
                .then_some(id)
            })
            .collect();
        let starts: Vec<_> = choices
            .into_par_iter()
            .filter(|request| !walking.moving(request.person, request.region))
            .collect();
        if starts.is_empty()
            && halting.is_empty()
            && goals.as_ref().is_none_or(|plan| plan.searches.is_empty())
        {
            return Ok(Vec::new());
        }
        let mut next = walking.prepare(&self.core.people, &starts, tick)?;
        let clock = next.elapsed.as_mut().expect("elapsed walking");
        clock.paused.append(&mut halting);
        clock.paused.par_sort_unstable();
        clock.paused.dedup();
        let events = next.events.par_iter().cloned().collect();
        let owner = self.local_transport.as_mut().expect("local transport");
        owner.walking = Some(next);
        if let Some(plan) = goals {
            let mut edits = plan.searches;
            edits.par_sort_unstable_by_key(|row| row.person);
            owner.goal_fetchers.par_iter_mut().for_each(|preference| {
                if let Ok(index) = edits.binary_search_by_key(&preference.person, |row| row.person)
                {
                    preference.search = Some(edits[index].search.clone());
                }
            });
        }
        self.work_checked_revision = None;
        Ok(events)
    }

    pub fn walk_controlled(
        &mut self,
        person: u64,
        destination: Endpoint,
    ) -> Result<Outcome, String> {
        if self
            .core
            .elapsed_millis()
            .is_none_or(|offset| offset >= self.core.physical.tick_millis())
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before walking".into());
        }
        let actor = self
            .core
            .people
            .binary_search_by_key(&person, |row| row.id)
            .ok()
            .map(|index| &self.core.people[index])
            .filter(|row| row.controlled && row.health > 0)
            .ok_or("walking requires an available controlled person")?;
        let region = actor.region.ok_or("person is between regions")?;
        if self
            .local_walk_exclusions(&[])
            .binary_search(&person)
            .is_ok()
        {
            return Err("person is occupied".into());
        }
        let owner = self
            .local_transport
            .as_mut()
            .and_then(|owner| owner.walking.as_mut())
            .ok_or("local walking is disabled")?;
        let next = owner.prepare(
            &self.core.people,
            &[Request {
                person,
                region,
                destination,
            }],
            self.core.physical.tick(),
        )?;
        let outcome = next.events[0].outcome;
        *owner = next;
        self.work_checked_revision = None;
        Ok(outcome)
    }

    pub fn pause_controlled_walk(&mut self, person: u64, paused: bool) -> Result<bool, String> {
        let actor = self
            .core
            .people
            .binary_search_by_key(&person, |row| row.id)
            .ok()
            .map(|index| &self.core.people[index])
            .filter(|row| row.controlled && row.health > 0)
            .ok_or("walking requires an available controlled person")?;
        let region = actor.region.ok_or("person is between regions")?;
        if !paused
            && self
                .local_walk_exclusions(&[])
                .binary_search(&person)
                .is_ok()
        {
            return Err("person is occupied".into());
        }
        let owner = self
            .local_transport
            .as_mut()
            .and_then(|owner| owner.walking.as_mut())
            .ok_or("local walking is disabled")?;
        let changed = owner.pause(person, region, paused);
        if changed {
            self.work_checked_revision = None;
        }
        Ok(changed)
    }
}

//! Accumulate actual elapsed activity for boundary-weighted fatigue.
use super::*;
use game_economy_fatigue::TimedActivity;

impl CoreHost {
    pub(super) fn stage_fatigue(
        &self,
        people: &mut [Person],
    ) -> Result<Option<game_economy_fatigue::TimedFatigue>, String> {
        let Some(owner) = &self.fatigue else {
            return Ok(None);
        };
        let draft = owner.prepare(
            &self.activity,
            self.core.physical.tick_millis(),
            |id| {
                people
                    .binary_search_by_key(&id, |person| person.id)
                    .ok()
                    .is_some_and(|i| people[i].health > 0)
            },
            |id| {
                people
                    .binary_search_by_key(&id, |person| person.id)
                    .ok()
                    .is_some_and(|i| people[i].controlled)
            },
        )?;
        people.par_iter_mut().for_each(|person| {
            if let Ok(index) = draft
                .consequences
                .binary_search_by_key(&person.id, |row| row.id)
            {
                person.health = person
                    .health
                    .saturating_sub(draft.consequences[index].damage);
            }
        });
        Ok(Some(draft.state))
    }
    pub(super) fn blocked_with_rest(&self, blocked: &[u64]) -> Option<Vec<u64>> {
        let mut occupied = self
            .fatigue
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.fatigue.resting());
        occupied.par_extend(self.assisting_people());
        if occupied
            .par_iter()
            .all(|id| blocked.binary_search(id).is_ok())
        {
            return None;
        }
        occupied.par_extend(blocked.par_iter().copied());
        occupied.par_sort_unstable();
        occupied.dedup();
        Some(occupied)
    }
    /// A controlled person can change their own rest choice at this offset.
    pub fn set_controlled_rest(&mut self, person: u64, resting: bool) -> Result<(), String> {
        let actor = self
            .core
            .people
            .binary_search_by_key(&person, |row| row.id)
            .ok()
            .map(|index| &self.core.people[index])
            .ok_or("unknown person")?;
        if !actor.controlled || actor.health == 0 || actor.region.is_none() {
            return Err("person cannot choose rest here".into());
        }
        let owner = self.fatigue.as_mut().ok_or("fatigue is disabled")?;
        owner.fatigue.people.par_sort_unstable_by_key(|row| row.id);
        match owner
            .fatigue
            .people
            .binary_search_by_key(&person, |row| row.id)
        {
            Ok(index) => owner.fatigue.people[index].resting = resting,
            Err(index) => owner
                .fatigue
                .people
                .insert(index, game_fatigue_person(person, resting)),
        }
        self.work_checked_revision = None;
        Ok(())
    }

    pub(super) fn progress_with_activity(
        &mut self,
        requested: u64,
        externally_busy: &[u64],
    ) -> Result<u64, String> {
        let elapsed = self.core.elapsed_millis().ok_or("no physical interval")?;
        let quantum = self.core.physical.tick_millis();
        let consumed = requested.min(
            quantum
                .checked_sub(elapsed)
                .ok_or("invalid interval elapsed time")?,
        );
        let durations = self.assist_durations(consumed)?;
        let Some(owner) = &self.fatigue else {
            let credits = self.plan_skill_practice(requested);
            let advanced = self.core.progress(requested)?;
            if advanced > 0 {
                self.record_assist_durations(&durations);
                self.remember_skill_practice(credits);
            }
            return Ok(advanced);
        };
        let elapsed = self.core.elapsed_millis().ok_or("no physical interval")?;
        let quantum = self.core.physical.tick_millis();
        let consumed = requested.min(
            quantum
                .checked_sub(elapsed)
                .ok_or("invalid interval elapsed time")?,
        );
        if consumed == 0 {
            return Ok(0);
        }
        let skill_practice = self.plan_skill_practice(consumed);
        let rates = &self
            .core
            .physical
            .work_interval()
            .ok_or("no physical interval")?
            .rates;
        let mut work: Vec<_> = rates
            .par_iter()
            .map(|&(job, rate)| -> Result<_, String> {
                let index = self
                    .core
                    .physical
                    .work()
                    .binary_search_by_key(&job, |row| row.job.id)
                    .map_err(|_| "missing activity job")?;
                let row = &self.core.physical.work()[index];
                let duration = row
                    .completion_after(rate, quantum)
                    .map_err(str::to_owned)?
                    .map_or(consumed, |after| after.min(u128::from(consumed)) as u64);
                Ok((row.job.worker, duration))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(owner) = &self.shared_work {
            work.par_extend(
                owner
                    .sessions
                    .par_iter()
                    .zip(durations.par_iter())
                    .map(|(session, duration)| (session.event.person, *duration)),
            );
        }
        work.par_sort_unstable_by_key(|row| row.0);
        let mut travel: Vec<_> = self.core.travel.as_ref().map_or(Ok(Vec::new()), |travel| {
            travel
                .journeys
                .par_iter()
                .map(|journey| {
                    journey
                        .arrival_after(quantum)
                        .map(|after| (journey.person, after.min(u128::from(consumed)) as u64))
                        .map_err(str::to_owned)
                })
                .collect::<Result<Vec<_>, _>>()
        })?;
        travel.par_sort_unstable_by_key(|row| row.0);
        let resting = owner.fatigue.resting();
        let chunks: Vec<_> = self
            .activity
            .par_chunks(4096)
            .zip(self.core.people.par_chunks(4096))
            .map(|(activity, people)| -> Result<Vec<TimedActivity>, String> {
                let first = people[0].id;
                let mut work_index = work.partition_point(|entry| entry.0 < first);
                let mut travel_index = travel.partition_point(|entry| entry.0 < first);
                let mut rest_index = resting.partition_point(|&id| id < first);
                let mut busy_index = externally_busy.partition_point(|&id| id < first);
                let mut rows = Vec::with_capacity(people.len());
                for (old, person) in activity.iter().zip(people) {
                    while work_index < work.len() && work[work_index].0 < person.id {
                        work_index += 1;
                    }
                    while travel_index < travel.len() && travel[travel_index].0 < person.id {
                        travel_index += 1;
                    }
                    while rest_index < resting.len() && resting[rest_index] < person.id {
                        rest_index += 1;
                    }
                    while busy_index < externally_busy.len()
                        && externally_busy[busy_index] < person.id
                    {
                        busy_index += 1;
                    }
                    let mut row = old.clone();
                    if person.health > 0 {
                        let work_millis = work
                            .get(work_index)
                            .filter(|entry| entry.0 == person.id)
                            .map_or(0, |entry| entry.1);
                        let travel_millis = travel
                            .get(travel_index)
                            .filter(|entry| entry.0 == person.id)
                            .map_or(0, |entry| entry.1);
                        if u128::from(work_millis) + u128::from(travel_millis)
                            > u128::from(consumed)
                        {
                            return Err("overlapping work and travel activity".into());
                        }
                        let available = if person.region.is_some()
                            && externally_busy.get(busy_index) != Some(&person.id)
                        {
                            consumed - work_millis - travel_millis
                        } else {
                            0
                        };
                        row.alive_millis = row
                            .alive_millis
                            .checked_add(consumed)
                            .ok_or("activity overflow")?;
                        row.work_millis = row
                            .work_millis
                            .checked_add(work_millis)
                            .ok_or("activity overflow")?;
                        row.travel_millis = row
                            .travel_millis
                            .checked_add(travel_millis)
                            .ok_or("activity overflow")?;
                        if resting.get(rest_index) == Some(&person.id) {
                            row.rest_millis = row
                                .rest_millis
                                .checked_add(available)
                                .ok_or("activity overflow")?;
                        } else {
                            row.idle_millis = row
                                .idle_millis
                                .checked_add(available)
                                .ok_or("activity overflow")?;
                        }
                    }
                    rows.push(row);
                }
                Ok(rows)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let next = chunks.into_par_iter().flatten().collect();
        let advanced = self.core.progress(consumed)?;
        self.activity = next;
        self.record_assist_durations(&durations);
        self.remember_skill_practice(skill_practice);
        Ok(advanced)
    }
}

fn game_fatigue_person(id: u64, resting: bool) -> game_economy_fatigue::FatiguePerson {
    game_economy_fatigue::FatiguePerson {
        id,
        fatigue: 0,
        resting,
    }
}

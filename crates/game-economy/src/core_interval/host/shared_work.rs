//! Personal helper assignments to real, reserved work.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreAssistSession {
    pub event: game_economy_work::shared::Event,
    pub elapsed_millis: u64,
    pub base_rate: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreAssistRecord {
    pub event: game_economy_work::shared::Event,
    pub elapsed_millis: u64,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSharedWork {
    pub owner: game_economy_work::shared::SharedWork,
    #[serde(default)]
    pub sessions: Vec<CoreAssistSession>,
    #[serde(default)]
    pub records: Vec<CoreAssistRecord>,
}
impl Clone for CoreSharedWork {
    fn clone(&self) -> Self {
        Self {
            owner: self.owner.clone(),
            sessions: self.sessions.par_iter().cloned().collect(),
            records: self.records.par_iter().cloned().collect(),
        }
    }
}
impl CoreSharedWork {
    pub(super) fn validate(
        &self,
        core: &CoreInterval,
        sites: Option<&CoreSites>,
    ) -> Result<(), String> {
        let sites = sites.ok_or("shared work requires work sites")?;
        let people: HashSet<_> = core.people.par_iter().map(|p| p.id).collect();
        let recipes: HashSet<_> = core.physical.recipes().par_iter().map(|r| r.id).collect();
        self.owner
            .validate(&people, &recipes, core.physical.tick())?;
        if self.owner.succession.is_some() {
            return Err("shared work succession is not yet supported".into());
        }
        let interval = core
            .physical
            .work_interval()
            .ok_or("shared work requires an interval")?;
        let mut unavailable: Vec<_> = core
            .physical
            .work()
            .par_iter()
            .filter(|work| work.phase == game_lot_runtime::Phase::Working)
            .map(|work| work.job.worker)
            .collect();
        if let Some(travel) = &core.travel {
            unavailable.par_extend(travel.journeys.par_iter().map(|journey| journey.person));
        }
        unavailable.par_sort_unstable();
        unavailable.dedup();
        let mut helpers: Vec<_> = self
            .sessions
            .par_iter()
            .map(|session| session.event.person)
            .collect();
        helpers.par_sort_unstable();
        if helpers.par_windows(2).any(|pair| pair[0] == pair[1])
            || self.sessions.par_windows(2).any(|pair| {
                (pair[0].event.job, pair[0].event.person)
                    >= (pair[1].event.job, pair[1].event.person)
            })
            || self.sessions.par_iter().any(|session| {
                let event = &session.event;
                session.base_rate == 0
                    || session.elapsed_millis > interval.elapsed_millis
                    || event.tick != core.physical.tick()
                    || event.progress == 0
                    || event.progress != event.offered
                    || event.person == event.lead
                    || event.finished
                    || event.interrupted
                    || !people.contains(&event.person)
                    || unavailable.binary_search(&event.person).is_ok()
                    || core
                        .physical
                        .work()
                        .binary_search_by_key(&event.job, |row| row.job.id)
                        .ok()
                        .is_none_or(|i| {
                            let job = &core.physical.work()[i];
                            job.phase != game_lot_runtime::Phase::Working
                                || core
                                    .people
                                    .binary_search_by_key(&event.person, |person| person.id)
                                    .ok()
                                    .is_none_or(|i| {
                                        core.people[i].health == 0
                                            || core.people[i].region != Some(job.job.region)
                                    })
                                || job.job.worker != event.lead
                                || job.job.recipe != event.recipe
                        })
            })
        {
            return Err("invalid shared work sessions".into());
        }
        if self.records.par_iter().any(|record| {
            record.elapsed_millis == 0
                || record.elapsed_millis > core.physical.tick_millis()
                || !people.contains(&record.event.person)
                || record.event.tick > core.physical.tick()
                || record.event.progress
                    != ((u128::from(record.event.offered) * u128::from(record.elapsed_millis))
                        / u128::from(core.physical.tick_millis())) as u64
        }) {
            return Err("invalid shared work history".into());
        }
        let limits: std::collections::HashMap<_, _> = self
            .owner
            .rules
            .par_iter()
            .map(|rule| (rule.recipe, rule.max_helpers))
            .collect();
        self.sessions
            .par_chunk_by(|a, b| a.event.job == b.event.job)
            .try_for_each(|group| -> Result<(), String> {
                if group
                    .par_iter()
                    .any(|row| row.base_rate != group[0].base_rate)
                {
                    return Err("invalid shared work rate".into());
                }
                let binding = sites
                    .facilities
                    .bindings
                    .binary_search_by_key(&group[0].event.job, |binding| binding.job)
                    .ok()
                    .map(|i| &sites.facilities.bindings[i])
                    .ok_or("shared job lacks a site")?;
                let maximum = *limits
                    .get(&group[0].event.recipe)
                    .ok_or("shared job lacks a crew rule")?;
                if group.len() as u128 > u128::from(maximum.min(binding.slots.saturating_sub(1))) {
                    return Err("shared crew exceeds its reserved capacity".into());
                }
                let sum = u128::from(group[0].base_rate)
                    + group
                        .par_iter()
                        .map(|row| u128::from(row.event.progress))
                        .sum::<u128>();
                let sum = u64::try_from(sum).map_err(|_| "invalid shared work rate")?;
                if interval
                    .rates
                    .binary_search_by_key(&group[0].event.job, |row| row.0)
                    .ok()
                    .is_none_or(|i| interval.rates[i].1 != sum)
                {
                    return Err("invalid shared work rate".into());
                }
                Ok(())
            })?;
        Ok(())
    }
}

impl CoreHost {
    pub(super) fn assisting_people(&self) -> Vec<u64> {
        let mut people: Vec<_> = self.shared_work.as_ref().map_or_else(Vec::new, |owner| {
            owner.sessions.par_iter().map(|s| s.event.person).collect()
        });
        people.par_sort_unstable();
        people
    }
    /// Each helper makes a personal choice; no institution or agreement assigns them.
    pub fn assist_work_autonomously(&mut self, blocked: &[u64]) -> Result<Vec<u64>, String> {
        if blocked.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.shared_work.as_ref() else {
            return Ok(Vec::new());
        };
        if !owner.sessions.is_empty() {
            return Err("helper assignments are already active".into());
        }
        if owner.owner.preferences.is_empty() {
            return Ok(Vec::new());
        }
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before assisting".into());
        }
        let sites = self
            .sites
            .as_ref()
            .ok_or("shared work requires work sites")?;
        let mut busy = blocked.to_vec();
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|work| work.phase == game_lot_runtime::Phase::Working)
                .map(|work| work.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let resting = self
            .fatigue
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.fatigue.resting());
        let unavailable = sites
            .maintenance
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.broken(&sites.facilities));
        let claims = self.claims.as_ref().map(|owner| owner.index());
        let classes = claims
            .as_ref()
            .map_or_else(Vec::new, |index| index.site_classes());
        let view = self.core.physical.view(self.core.physical.tick())?;
        let rates = &self
            .core
            .physical
            .work_interval()
            .ok_or("no physical interval")?
            .rates;
        let mut events = game_economy_work::shared::Context {
            owner: &owner.owner,
            people: &self.core.people,
            work: self.core.physical.work(),
            rates,
            busy: &busy,
            resting: &resting,
            manual: None,
            sites: &sites.facilities,
            unavailable_sites: &unavailable,
            site_classes: &classes,
            claims: claims.as_ref(),
            skills: self.skills.as_ref().map(|owner| &owner.owner),
            injuries: None,
            family: self.family.as_ref(),
            tools: &self.tool_requirements,
            tool_wear: self.tool_wear.as_ref(),
            physical: &self.core.physical,
            view: &view,
            opinions: self.opinions.as_ref(),
            max_hunger: self.core.rules.max_work_hunger,
            seed: self.core.seed,
            tick: self.core.physical.tick(),
        }
        .prepare_interval();
        events.retain(|event| event.progress > 0);
        if events.is_empty() {
            return Ok(Vec::new());
        }
        events.par_sort_unstable_by_key(|event| (event.job, event.person));
        let sessions: Vec<_> = events
            .par_iter()
            .map(|event| {
                let rate = rates
                    .binary_search_by_key(&event.job, |row| row.0)
                    .expect("selected active job");
                CoreAssistSession {
                    event: event.clone(),
                    elapsed_millis: 0,
                    base_rate: rates[rate].1,
                }
            })
            .collect();
        sessions
            .par_chunk_by(|a, b| a.event.job == b.event.job)
            .try_for_each(|group| -> Result<(), String> {
                let sum = u128::from(group[0].base_rate)
                    + group
                        .par_iter()
                        .map(|row| u128::from(row.event.progress))
                        .sum::<u128>();
                u64::try_from(sum).map_err(|_| "shared work rate overflow")?;
                Ok(())
            })?;
        let mut effective = rates.clone();
        game_economy_work::shared::apply_rates(&events, &mut effective);
        self.core.physical.replace_interval_rates(&effective)?;
        self.shared_work
            .as_mut()
            .expect("checked shared work")
            .sessions = sessions;
        self.work_checked_revision = None;
        Ok(self.assisting_people())
    }
}

impl CoreSharedWork {
    fn finish(&mut self, jobs: Option<&[u64]>, completed: &[u64], quantum: u64) {
        let selected = |session: &CoreAssistSession| {
            jobs.is_none_or(|ids| ids.binary_search(&session.event.job).is_ok())
        };
        let mut records: Vec<_> = self
            .sessions
            .par_iter()
            .filter(|session| selected(session) && session.elapsed_millis > 0)
            .map(|session| {
                let mut event = session.event.clone();
                event.progress = ((u128::from(event.progress) * u128::from(session.elapsed_millis))
                    / u128::from(quantum)) as u64;
                event.finished = completed.binary_search(&event.job).is_ok();
                event.interrupted = jobs.is_some() && !event.finished;
                CoreAssistRecord {
                    event,
                    elapsed_millis: session.elapsed_millis,
                }
            })
            .collect();
        records.par_sort_unstable_by_key(|record| (record.event.job, record.event.person));
        self.owner
            .events
            .par_extend(records.par_iter().map(|record| record.event.clone()));
        self.records.par_extend(records);
        self.sessions = self
            .sessions
            .par_iter()
            .filter(|session| !selected(session))
            .cloned()
            .collect();
    }
}
impl CoreHost {
    pub(super) fn assist_durations(&self, consumed: u64) -> Result<Vec<u64>, String> {
        let Some(owner) = &self.shared_work else {
            return Ok(Vec::new());
        };
        let interval = self
            .core
            .physical
            .work_interval()
            .ok_or("no physical interval")?;
        owner
            .sessions
            .par_iter()
            .map(|session| {
                let job = self
                    .core
                    .physical
                    .work()
                    .binary_search_by_key(&session.event.job, |work| work.job.id)
                    .map_err(|_| "missing shared job")?;
                let rate = interval
                    .rates
                    .binary_search_by_key(&session.event.job, |rate| rate.0)
                    .map_err(|_| "missing shared rate")?;
                let after = self.core.physical.work()[job]
                    .completion_after(interval.rates[rate].1, self.core.physical.tick_millis())
                    .map_err(str::to_owned)?;
                let duration =
                    after.map_or(consumed, |after| after.min(u128::from(consumed)) as u64);
                session
                    .elapsed_millis
                    .checked_add(duration)
                    .ok_or("shared work duration overflow")?;
                Ok(duration)
            })
            .collect()
    }
    pub(super) fn record_assist_durations(&mut self, durations: &[u64]) {
        if let Some(owner) = &mut self.shared_work {
            owner
                .sessions
                .par_iter_mut()
                .zip(durations.par_iter())
                .for_each(|(session, duration)| session.elapsed_millis += duration);
        }
    }
    /// End assignments at a decision while preserving the leads' original rates.
    pub fn release_shared_helpers(&mut self) -> Result<(), String> {
        let Some(owner) = self
            .shared_work
            .as_ref()
            .filter(|owner| !owner.sessions.is_empty())
        else {
            return Ok(());
        };
        let mut rates = self
            .core
            .physical
            .work_interval()
            .ok_or("no physical interval")?
            .rates
            .clone();
        rates.par_iter_mut().for_each(|row| {
            if let Ok(i) = owner
                .sessions
                .binary_search_by_key(&row.0, |session| session.event.job)
            {
                row.1 = owner.sessions[i].base_rate;
            }
        });
        let mut completed: Vec<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|work| {
                work.phase == game_lot_runtime::Phase::Working && work.job.remaining == 0
            })
            .map(|work| work.job.id)
            .collect();
        completed.par_sort_unstable();
        let mut next = owner.clone();
        next.finish(None, &completed, self.core.physical.tick_millis());
        self.core.physical.replace_interval_rates(&rates)?;
        self.shared_work = Some(next);
        self.work_checked_revision = None;
        Ok(())
    }
    pub(super) fn finished_shared_jobs(&mut self, jobs: &[u64], finished: &[u64]) {
        if let Some(owner) = &mut self.shared_work {
            owner.finish(Some(jobs), finished, self.core.physical.tick_millis());
        }
    }
    pub(super) fn stage_shared_boundary(
        &self,
        finished: &[u64],
        interrupted: &[u64],
    ) -> Option<CoreSharedWork> {
        let owner = self.shared_work.as_ref()?;
        if owner.sessions.is_empty() && owner.owner.events.is_empty() && owner.records.is_empty() {
            return None;
        }
        let mut next = owner.clone();
        let tick = self.core.physical.tick();
        next.owner.events = owner
            .owner
            .events
            .par_iter()
            .filter(|event| event.tick == tick)
            .cloned()
            .collect();
        next.records = owner
            .records
            .par_iter()
            .filter(|record| record.event.tick == tick)
            .cloned()
            .collect();
        next.finish(None, finished, self.core.physical.tick_millis());
        next.owner
            .events
            .par_iter_mut()
            .for_each(|event| event.interrupted = interrupted.binary_search(&event.job).is_ok());
        next.records.par_iter_mut().for_each(|record| {
            record.event.interrupted = interrupted.binary_search(&record.event.job).is_ok()
        });
        Some(next)
    }
    pub(super) fn base_shared_rates(&self, rates: &mut [(u64, u64)]) {
        if let Some(owner) = &self.shared_work {
            rates.par_iter_mut().for_each(|row| {
                if let Ok(i) = owner
                    .sessions
                    .binary_search_by_key(&row.0, |session| session.event.job)
                {
                    row.1 = owner.sessions[i].base_rate;
                }
            });
        }
    }
}

//! Personal helper assignments to real, reserved work.
use super::*;

type FatalAssistanceStage = (Vec<(u64, u64)>, Option<CoreSharedWork>);

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
        if let Some(succession) = &self.owner.succession {
            succession.validate_links(core.physical.work())?;
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
                                        (core.people[i].health == 0 && job.job.remaining != 0)
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
                || !people.contains(&record.event.person)
                || record.event.tick > core.physical.tick()
                || record.event.progress
                    != ((u128::from(record.event.offered) * u128::from(record.elapsed_millis))
                        / u128::from(core.physical.work_rate_period_millis()))
                    .min(u128::from(u64::MAX)) as u64
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
        self.assist_unprofiled_work(blocked, |_| true)
    }

    pub(super) fn assist_unprofiled_work(
        &mut self,
        blocked: &[u64],
        allowed: impl Fn(u64) -> bool + Sync,
    ) -> Result<Vec<u64>, String> {
        if self
            .shared_work
            .as_ref()
            .is_some_and(|owner| !owner.sessions.is_empty())
        {
            return Err("helper assignments are already active".into());
        }
        let candidates = self.assistance_candidates(blocked, false, |person, _| allowed(person))?;
        self.admit_assistance(candidates)
    }

    pub(super) fn assist_selected_work(
        &mut self,
        blocked: &[u64],
        selections: &[(u64, u64)],
    ) -> Result<(Vec<u64>, Vec<u64>), String> {
        if selections.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let candidates = self.assistance_candidates(blocked, true, |person, job| {
            selections.binary_search(&(person, job)).is_ok()
        })?;
        let mut available: Vec<_> = candidates.par_iter().map(|c| c.person).collect();
        available.par_sort_unstable();
        Ok((available, self.admit_assistance(candidates)?))
    }

    pub(super) fn assistance_candidates(
        &self,
        blocked: &[u64],
        personal: bool,
        allowed: impl Fn(u64, u64) -> bool + Sync,
    ) -> Result<Vec<game_economy_work::shared::Candidate>, String> {
        if blocked.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.shared_work.as_ref() else {
            return Ok(Vec::new());
        };
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
        busy.par_extend(
            owner
                .sessions
                .par_iter()
                .map(|session| session.event.person),
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
        let occupancies: Vec<_> = owner
            .sessions
            .par_chunk_by(|first, second| first.event.job == second.event.job)
            .map(|crew| (crew[0].event.job, crew.len()))
            .collect();
        let mut candidates = game_economy_work::shared::Context {
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
            family: if personal { None } else { self.family.as_ref() },
            tools: &self.tool_requirements,
            tool_wear: self.tool_wear.as_ref(),
            physical: &self.core.physical,
            view: &view,
            opinions: self.opinions.as_ref(),
            max_hunger: if personal {
                u32::MAX
            } else {
                self.core.rules.max_work_hunger
            },
            seed: self.core.seed,
            tick: self.core.physical.tick(),
        }
        .interval_candidates(&occupancies, allowed);
        if self.walking().is_some() || self.confinement().is_some() {
            candidates = candidates
                .into_par_iter()
                .filter(|row| self.same_local_endpoint(row.person, row.lead))
                .collect();
        }
        if personal && self.work_capabilities.is_some() && !candidates.is_empty() {
            let attempts: Vec<_> = candidates
                .par_iter()
                .map(|row| (row.person, row.recipe))
                .collect();
            let permitted = self.work_capability_checks(&attempts);
            candidates = candidates
                .into_par_iter()
                .zip(permitted)
                .filter_map(|(row, physically_capable)| physically_capable.then_some(row))
                .collect();
        }
        Ok(candidates)
    }

    fn admit_assistance(
        &mut self,
        candidates: Vec<game_economy_work::shared::Candidate>,
    ) -> Result<Vec<u64>, String> {
        use game_economy_work::shared;
        let events = shared::admit_interval(candidates, self.core.physical.tick());
        if events.is_empty() {
            return Ok(Vec::new());
        }
        let owner = self.shared_work.as_ref().expect("assistance owner");
        let rates = &self
            .core
            .physical
            .work_interval()
            .ok_or("no physical interval")?
            .rates;
        events
            .par_chunk_by(|a, b| a.job == b.job)
            .try_for_each(|group| -> Result<(), String> {
                let rate = rates
                    .binary_search_by_key(&group[0].job, |row| row.0)
                    .map_err(|_| "missing shared job rate")?;
                let sum = u128::from(rates[rate].1)
                    + group
                        .par_iter()
                        .map(|event| u128::from(event.progress))
                        .sum::<u128>();
                u64::try_from(sum).map_err(|_| "shared work rate overflow")?;
                Ok(())
            })?;
        let sessions: Vec<_> = events
            .par_iter()
            .map(|event| {
                let base_rate = owner
                    .sessions
                    .binary_search_by_key(&event.job, |session| session.event.job)
                    .ok()
                    .map_or_else(
                        || {
                            let rate = rates
                                .binary_search_by_key(&event.job, |row| row.0)
                                .expect("selected active job");
                            rates[rate].1
                        },
                        |index| owner.sessions[index].base_rate,
                    );
                CoreAssistSession {
                    event: event.clone(),
                    elapsed_millis: 0,
                    base_rate,
                }
            })
            .collect();
        let mut effective = rates.clone();
        shared::apply_rates(&events, &mut effective);
        self.core.physical.replace_interval_rates(&effective)?;
        let current = &mut self
            .shared_work
            .as_mut()
            .expect("checked shared work")
            .sessions;
        current.par_extend(sessions);
        current.par_sort_unstable_by_key(|session| (session.event.job, session.event.person));
        self.work_checked_revision = None;
        let mut admitted: Vec<_> = events.into_par_iter().map(|row| row.person).collect();
        admitted.par_sort_unstable();
        Ok(admitted)
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
                    / u128::from(quantum))
                .min(u128::from(u64::MAX)) as u64;
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
    /// Prepare fatal helper withdrawals without publishing rates or records.
    pub(super) fn stage_fatal_assistance(
        &self,
        deaths: &[u64],
        durations: &[u64],
        consumed: u64,
    ) -> Result<FatalAssistanceStage, String> {
        let Some(owner) = &self.shared_work else {
            return Ok((Vec::new(), None));
        };
        if deaths.is_empty()
            || owner.sessions.par_iter().all(|session| {
                deaths.binary_search(&session.event.person).is_err()
                    && deaths.binary_search(&session.event.lead).is_err()
            })
        {
            return Ok((Vec::new(), None));
        }
        if durations.len() != owner.sessions.len() {
            return Err("invalid shared work durations".into());
        }
        let interval = self
            .core
            .physical
            .work_interval()
            .ok_or("no physical interval")?;
        let period = self.core.physical.work_rate_period_millis();
        let selected: Vec<_> = owner
            .sessions
            .par_iter()
            .enumerate()
            .map(|(index, session)| -> Result<_, String> {
                if deaths.binary_search(&session.event.person).is_err()
                    && deaths.binary_search(&session.event.lead).is_err()
                {
                    return Ok(None);
                }
                let job = self
                    .core
                    .physical
                    .work()
                    .binary_search_by_key(&session.event.job, |work| work.job.id)
                    .map_err(|_| "missing shared job")?;
                let rate = interval
                    .rates
                    .binary_search_by_key(&session.event.job, |rate| rate.0)
                    .ok();
                let complete = if let Some(rate) = rate {
                    self.core.physical.work()[job]
                        .completion_after(interval.rates[rate].1, period)
                        .map_err(str::to_owned)?
                        .is_some_and(|deadline| deadline <= u128::from(consumed))
                } else {
                    self.core.physical.work()[job].job.remaining == 0
                };
                Ok((!complete).then_some((index, rate.is_some())))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_par_iter()
            .flatten()
            .collect();
        if selected.is_empty() {
            return Ok((Vec::new(), None));
        }
        let mut next = owner.clone();
        next.sessions
            .par_iter_mut()
            .zip(durations.par_iter())
            .try_for_each(|(session, duration)| -> Result<(), String> {
                session.elapsed_millis = session
                    .elapsed_millis
                    .checked_add(*duration)
                    .ok_or("shared work duration overflow")?;
                Ok(())
            })?;
        let active: Vec<_> = selected
            .par_iter()
            .filter_map(|&(index, active)| {
                active.then_some((
                    next.sessions[index].event.job,
                    next.sessions[index].event.progress,
                ))
            })
            .collect();
        let reductions: Vec<_> = active
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                let sum: u128 = group.par_iter().map(|row| u128::from(row.1)).sum();
                u64::try_from(sum)
                    .map(|sum| (group[0].0, sum))
                    .map_err(|_| "shared work rate overflow".to_owned())
            })
            .collect::<Result<_, _>>()?;
        let records: Vec<_> = selected
            .par_iter()
            .filter_map(|&(index, _)| {
                let session = &next.sessions[index];
                if session.elapsed_millis > 0 {
                    let mut event = session.event.clone();
                    event.progress = ((u128::from(event.offered)
                        * u128::from(session.elapsed_millis))
                        / u128::from(period))
                    .min(u128::from(u64::MAX)) as u64;
                    event.interrupted = true;
                    Some(CoreAssistRecord {
                        event,
                        elapsed_millis: session.elapsed_millis,
                    })
                } else {
                    None
                }
            })
            .collect();
        next.owner
            .events
            .par_extend(records.par_iter().map(|record| record.event.clone()));
        next.records.par_extend(records);
        let selected_ids: Vec<_> = selected.par_iter().map(|&(index, _)| index).collect();
        next.sessions = next
            .sessions
            .into_par_iter()
            .enumerate()
            .filter_map(|(index, session)| {
                selected_ids
                    .binary_search(&index)
                    .is_err()
                    .then_some(session)
            })
            .collect();
        Ok((reductions, Some(next)))
    }

    /// End only the named helpers' current contributions, preserving other helpers.
    /// The sorted list represents personal decisions, not a lead's instruction.
    pub fn withdraw_shared_helpers(&mut self, people: &[u64]) -> Result<(), String> {
        if people.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid withdrawing people".into());
        }
        if people.is_empty() {
            return Ok(());
        }
        let owner = self.shared_work.as_ref().ok_or("shared work is disabled")?;
        let mut selected: Vec<_> = owner
            .sessions
            .par_iter()
            .filter(|session| people.binary_search(&session.event.person).is_ok())
            .collect();
        if selected.len() != people.len() {
            return Err("person is not assisting".into());
        }
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before withdrawing".into());
        }
        selected.par_sort_unstable_by_key(|session| (session.event.job, session.event.person));
        let removed: Vec<(u64, u64)> = selected
            .par_chunk_by(|a, b| a.event.job == b.event.job)
            .map(|group| {
                let sum: u128 = group
                    .par_iter()
                    .map(|session| u128::from(session.event.progress))
                    .sum();
                u64::try_from(sum)
                    .map(|sum| (group[0].event.job, sum))
                    .map_err(|_| "shared work rate overflow".to_owned())
            })
            .collect::<Result<_, _>>()?;
        let mut rates = self
            .core
            .physical
            .work_interval()
            .ok_or("no physical interval")?
            .rates
            .clone();
        removed
            .par_iter()
            .try_for_each(|&(job, amount)| -> Result<(), String> {
                let index = rates
                    .binary_search_by_key(&job, |row| row.0)
                    .map_err(|_| "missing shared rate")?;
                rates[index]
                    .1
                    .checked_sub(amount)
                    .filter(|rate| *rate > 0)
                    .ok_or("invalid shared rate")?;
                Ok(())
            })?;
        rates.par_iter_mut().for_each(|row| {
            if let Ok(index) = removed.binary_search_by_key(&row.0, |entry| entry.0) {
                row.1 -= removed[index].1;
            }
        });
        let quantum = self.core.physical.work_rate_period_millis();
        let mut records: Vec<_> = selected
            .par_iter()
            .filter(|session| session.elapsed_millis > 0)
            .map(|session| {
                let mut event = session.event.clone();
                event.progress = ((u128::from(event.offered) * u128::from(session.elapsed_millis))
                    / u128::from(quantum))
                .min(u128::from(u64::MAX)) as u64;
                event.interrupted = true;
                CoreAssistRecord {
                    event,
                    elapsed_millis: session.elapsed_millis,
                }
            })
            .collect();
        records.par_sort_unstable_by_key(|record| (record.event.job, record.event.person));
        self.core.physical.replace_interval_rates(&rates)?;
        let owner = self.shared_work.as_mut().expect("shared work checked");
        owner
            .owner
            .events
            .par_extend(records.par_iter().map(|record| record.event.clone()));
        owner.records.par_extend(records);
        owner.sessions = owner
            .sessions
            .par_iter()
            .filter(|session| people.binary_search(&session.event.person).is_err())
            .cloned()
            .collect();
        self.work_checked_revision = None;
        Ok(())
    }

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
                    .completion_after(
                        interval.rates[rate].1,
                        self.core.physical.work_rate_period_millis(),
                    )
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
        next.finish(
            None,
            &completed,
            self.core.physical.work_rate_period_millis(),
        );
        self.core.physical.replace_interval_rates(&rates)?;
        self.shared_work = Some(next);
        self.work_checked_revision = None;
        Ok(())
    }
    pub(super) fn finished_shared_jobs(&mut self, jobs: &[u64], finished: &[u64]) {
        if let Some(owner) = &mut self.shared_work {
            owner.finish(
                Some(jobs),
                finished,
                self.core.physical.work_rate_period_millis(),
            );
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
        next.finish(None, finished, self.core.physical.work_rate_period_millis());
        next.owner
            .events
            .par_iter_mut()
            .for_each(|event| event.interrupted |= interrupted.binary_search(&event.job).is_ok());
        next.records.par_iter_mut().for_each(|record| {
            record.event.interrupted |= interrupted.binary_search(&record.event.job).is_ok()
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

impl CoreHost {
    /// A lead may leave unfinished reserved work available for someone else.
    pub fn suspend_shared_work(&mut self, requests: &[(u64, u64)]) -> Result<(), String> {
        let succession = self
            .shared_work
            .as_ref()
            .and_then(|owner| owner.owner.succession.as_ref())
            .ok_or("work succession is disabled")?;
        if requests.is_empty() {
            return Ok(());
        }
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before suspending work".into());
        }
        let until = tick
            .checked_add(succession.retention)
            .and_then(|tick| tick.checked_add(1))
            .ok_or("work retention overflow")?;
        let facilities = &self
            .sites
            .as_ref()
            .ok_or("shared work requires work sites")?
            .facilities;
        if requests.par_iter().any(|(job, _)| {
            facilities
                .bindings
                .binary_search_by_key(job, |binding| binding.job)
                .is_err()
        }) {
            return Err("work suspension requires a reserved site".into());
        }
        let transitions: Vec<_> = requests
            .par_iter()
            .map(|&(job, worker)| game_lot_runtime::Suspension { job, worker, until })
            .collect();
        let outcomes: Vec<_> = requests.par_iter().map(|&(job, _)| (job, false)).collect();
        let resolved = self.stage_request_resolution(&outcomes, tick, &self.core.people, false);
        let mut ended: Vec<_> = requests.par_iter().map(|row| row.0).collect();
        ended.par_sort_unstable();
        self.core.physical.suspend_interval_work(&transitions)?;
        self.finished_shared_jobs(&ended, &[]);
        self.paused_work_rates
            .retain(|row| ended.binary_search(&row.0).is_err());
        self.record_early_work_outcomes(&[], &ended);
        self.publish_request_resolution(resolved);
        self.prune_successions();
        self.work_checked_revision = None;
        Ok(())
    }
    /// Select successors by their own preferences and actual access to the reserved job.
    pub fn take_over_work_autonomously(
        &mut self,
        blocked: &[u64],
        rate: u64,
    ) -> Result<Vec<game_lot_runtime::Adoption>, String> {
        if rate == 0 || blocked.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid work succession batch".into());
        }
        let Some(owner) = self
            .shared_work
            .as_ref()
            .and_then(|owner| owner.owner.succession.as_ref())
        else {
            return Ok(Vec::new());
        };
        if owner.preferences.is_empty()
            || !self
                .core
                .physical
                .work()
                .par_iter()
                .any(|work| matches!(work.phase, game_lot_runtime::Phase::Suspended { .. }))
        {
            return Ok(Vec::new());
        }
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before succession".into());
        }
        let sites = self
            .sites
            .as_ref()
            .ok_or("shared work requires work sites")?;
        let mut busy = self
            .blocked_with_rest(blocked)
            .unwrap_or_else(|| blocked.to_vec());
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
        let view = self.core.physical.view(tick)?;
        let adoptions = game_economy_work::succession::Context {
            owner,
            people: &self.core.people,
            work: self.core.physical.work(),
            busy: &busy,
            resting: &resting,
            manual: None,
            sites: &sites.facilities,
            unavailable_sites: &unavailable,
            site_classes: &classes,
            claims: claims.as_ref(),
            injuries: None,
            family: self.family.as_ref(),
            tools: &self.tool_requirements,
            tool_wear: self.tool_wear.as_ref(),
            physical: &self.core.physical,
            view: &view,
            opinions: self.opinions.as_ref(),
            max_hunger: self.core.rules.max_work_hunger,
            seed: self.core.seed,
            tick,
        }
        .prepare();
        if adoptions.is_empty() {
            return Ok(adoptions);
        }
        let requests: Vec<_> = adoptions
            .par_iter()
            .map(|adoption| -> Result<_, String> {
                let job = self
                    .core
                    .physical
                    .work()
                    .binary_search_by_key(&adoption.job, |work| work.job.id)
                    .map_err(|_| "missing suspended job")?;
                let recipe = self.core.physical.work()[job].job.recipe;
                let multiplier = self
                    .skill_rates
                    .as_ref()
                    .map_or(1, |index| index.rate(adoption.worker, recipe));
                Ok((
                    *adoption,
                    rate.checked_mul(multiplier)
                        .ok_or("skill work rate overflow")?,
                ))
            })
            .collect::<Result<_, _>>()?;
        self.core.physical.adopt_interval_work(&requests)?;
        let succession = self
            .shared_work
            .as_mut()
            .expect("checked shared work")
            .owner
            .succession
            .as_mut()
            .expect("checked succession");
        succession
            .engagements
            .par_extend(adoptions.par_iter().map(|row| {
                game_economy_work::succession::Engagement {
                    job: row.job,
                    person: row.worker,
                    former_worker: row.previous_worker,
                }
            }));
        succession
            .engagements
            .par_sort_unstable_by_key(|row| row.job);
        self.work_checked_revision = None;
        Ok(adoptions)
    }
    pub(super) fn prune_successions(&mut self) {
        if let Some(succession) = self
            .shared_work
            .as_mut()
            .and_then(|owner| owner.owner.succession.as_mut())
        {
            let work = self.core.physical.work();
            succession.engagements = succession
                .engagements
                .par_iter()
                .filter(|row| {
                    work.binary_search_by_key(&row.job, |work| work.job.id)
                        .ok()
                        .is_some_and(|i| {
                            work[i].phase == game_lot_runtime::Phase::Working
                                && work[i].job.worker == row.person
                        })
                })
                .cloned()
                .collect();
        }
    }
}

impl CoreHost {
    pub(super) fn succession_death_suspensions(
        &self,
        deceased: &[u64],
        tick: u64,
    ) -> Result<(Vec<u64>, u64), String> {
        if deceased.is_empty() {
            return Ok((Vec::new(), 0));
        }
        let Some((owner, sites)) = self
            .shared_work
            .as_ref()
            .and_then(|owner| owner.owner.succession.as_ref())
            .zip(self.sites.as_ref())
        else {
            return Ok((Vec::new(), 0));
        };
        let broken = sites
            .maintenance
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.broken(&sites.facilities));
        let eligible: Vec<_> = sites
            .facilities
            .bindings
            .par_iter()
            .filter(|binding| {
                broken.binary_search(&binding.site).is_err()
                    && self
                        .core
                        .physical
                        .work()
                        .binary_search_by_key(&binding.job, |work| work.job.id)
                        .ok()
                        .is_some_and(|i| {
                            let work = &self.core.physical.work()[i];
                            work.phase == game_lot_runtime::Phase::Working
                                && work.job.remaining > 0
                                && deceased.binary_search(&work.job.worker).is_ok()
                        })
            })
            .map(|binding| binding.job)
            .collect();
        if eligible.is_empty() {
            return Ok((eligible, 0));
        }
        let until = tick
            .checked_add(owner.retention)
            .and_then(|tick| tick.checked_add(1))
            .ok_or("work retention overflow")?;
        Ok((eligible, until))
    }
}

//! Bounded event-driven advancement of the saved, limited core host.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum DecisionPhase {
    Meals,
    Delivery { meal_people: Vec<u64> },
    Offers { meal_people: Vec<u64> },
    Acceptance { meal_people: Vec<u64> },
    Asking { meal_people: Vec<u64> },
    Requests { meal_people: Vec<u64> },
    Work { meal_people: Vec<u64> },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSchedule {
    pub period_millis: u64,
    pub work_rate: u64,
    /// Offset from this quantum's opening; it may lie in a later quantum.
    pub next_decision_millis: Option<u128>,
    pub phase: DecisionPhase,
    /// Rates retained across boundaries, including temporarily paused work.
    #[serde(default)]
    pub carried_rates: Vec<(u64, u64)>,
}
impl CoreSchedule {
    pub(super) fn validate(&self, core: &CoreInterval) -> Result<(), String> {
        let offset = core
            .elapsed_millis()
            .ok_or("scheduled host needs an active interval")?;
        let quantum = core.physical.tick_millis();
        if self.period_millis == 0
            || self.work_rate == 0
            || offset > quantum
            || self
                .next_decision_millis
                .is_some_and(|next| next < u128::from(offset))
            || self
                .carried_rates
                .par_windows(2)
                .any(|rows| rows[0].0 >= rows[1].0)
            || self
                .carried_rates
                .par_iter()
                .any(|&(id, rate)| id == 0 || rate == 0)
        {
            return Err("invalid core decision schedule".into());
        }
        if let DecisionPhase::Delivery { meal_people }
        | DecisionPhase::Offers { meal_people }
        | DecisionPhase::Acceptance { meal_people }
        | DecisionPhase::Asking { meal_people }
        | DecisionPhase::Requests { meal_people }
        | DecisionPhase::Work { meal_people } = &self.phase
            && (self.next_decision_millis != Some(u128::from(offset))
                || offset == quantum
                || meal_people.par_windows(2).any(|rows| rows[0] >= rows[1])
                || meal_people
                    .par_iter()
                    .any(|id| core.people.binary_search_by_key(id, |p| p.id).is_err()))
        {
            return Err("invalid pending work decision".into());
        }
        Ok(())
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum CoreRunEvent {
    Arrivals(CoreArrivals),
    Deferred(Vec<u64>),
    Work(game_lot_runtime::EarlyWorkEvents),
    Meals(Vec<(u64, u32)>),
    Deliveries(Vec<game_lots::TransactionOutcome>),
    Offers(Vec<u64>),
    Acceptances(Vec<u64>),
    Asks(Vec<u64>),
    Requests(Vec<game_autowork::requests::Event>),
    Starts(Vec<(u64, bool)>),
    Boundary(Box<Events>),
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreRun {
    pub elapsed_millis: u64,
    pub boundary: bool,
    pub boundaries: u64,
    pub decisions: u64,
    pub events: Vec<CoreRunEvent>,
    /// Earlier events and elapsed time remain committed when an action fails.
    pub failure: Option<String>,
}
impl CoreHost {
    /// Begin periodic personal decisions at the current partial offset.
    pub fn enable_schedule(&mut self, period_millis: u64, work_rate: u64) -> Result<(), String> {
        if self.schedule.is_some() {
            return Err("core schedule already active".into());
        }
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let schedule = CoreSchedule {
            period_millis,
            work_rate,
            next_decision_millis: Some(u128::from(offset)),
            phase: DecisionPhase::Meals,
            carried_rates: self
                .core
                .physical
                .work_interval()
                .expect("active interval")
                .rates
                .clone(),
        };
        schedule.validate(&self.core)?;
        self.schedule = Some(schedule);
        Ok(())
    }
    /// Stop scheduling between completed personal decision batches.
    pub fn disable_schedule(&mut self) -> Result<(), String> {
        let schedule = self.schedule.as_ref().ok_or("no core schedule")?;
        if !matches!(schedule.phase, DecisionPhase::Meals) {
            return Err("finish the pending work decision first".into());
        }
        self.schedule = None;
        Ok(())
    }

    /// Advance within this quantum, publishing due work and arrivals and
    /// running scheduled personal choices. The host supplies sorted occupied
    /// people from owners not included in this limited core.
    pub fn advance_scheduled(
        &mut self,
        requested_millis: u64,
        blocked: &[u64],
    ) -> Result<CoreRun, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        self.schedule
            .as_ref()
            .ok_or("no core schedule")?
            .validate(&self.core)?;
        let start = self.core.elapsed_millis().ok_or("no physical interval")?;
        let quantum = self.core.physical.tick_millis();
        let mut run = CoreRun::default();
        macro_rules! attempt {
            ($label:lifetime, $operation:expr) => {
                match $operation {
                    Ok(value) => value,
                    Err(error) => {
                        run.failure = Some(error);
                        break $label;
                    }
                }
            };
        }
        'run: loop {
            let offset = self.core.elapsed_millis().expect("active interval");
            if offset == quantum {
                run.boundary = true;
                break;
            }
            let deadline = attempt!('run, self.core.next_deadline(None));
            if deadline.after_millis == 0 {
                let mut handled = false;
                if deadline.arrival
                    && let Some(arrivals) = attempt!('run, self.publish_arrivals())
                {
                    run.events.push(CoreRunEvent::Arrivals(arrivals));
                    handled = true;
                }
                if deadline.work {
                    let perishable: HashSet<_> = self
                        .core
                        .physical
                        .goods()
                        .par_iter()
                        .filter_map(|good| {
                            good.food
                                .as_ref()
                                .and_then(|food| food.shelf_days.map(|_| good.id))
                        })
                        .collect();
                    let rates = &self
                        .core
                        .physical
                        .work_interval()
                        .expect("active interval")
                        .rates;
                    let defer: Vec<_> = self
                        .core
                        .physical
                        .work()
                        .par_iter()
                        .filter(|row| {
                            row.phase == game_lot_runtime::Phase::Working
                                && row.job.remaining == 0
                                && rates
                                    .binary_search_by_key(&row.job.id, |rate| rate.0)
                                    .is_ok()
                                && row
                                    .job
                                    .outputs
                                    .iter()
                                    .any(|output| perishable.contains(&output.good))
                        })
                        .map(|row| row.job.id)
                        .collect();
                    if !defer.is_empty() {
                        attempt!('run, self.defer_work(self.core.physical.tick(), offset, &defer));
                        run.events.push(CoreRunEvent::Deferred(defer));
                        handled = true;
                    }
                    if let Some(events) = attempt!('run, self.publish_early_work()) {
                        run.events.push(CoreRunEvent::Work(events));
                        handled = true;
                    }
                }
                if !handled {
                    run.failure = Some("unresolved core deadline".into());
                    break;
                }
                continue;
            }
            let schedule = self.schedule.as_ref().expect("active schedule").clone();
            if schedule.next_decision_millis == Some(u128::from(offset)) {
                match schedule.phase {
                    DecisionPhase::Meals => {
                        let meals = attempt!('run, self.eat_autonomously(blocked));
                        let people: Vec<_> = meals.iter().map(|meal| meal.person).collect();
                        if !meals.is_empty() {
                            run.events.push(CoreRunEvent::Meals(
                                meals.iter().map(|meal| (meal.person, meal.good)).collect(),
                            ));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            if self.agreements.is_some() {
                                DecisionPhase::Delivery {
                                    meal_people: people,
                                }
                            } else if self
                                .autowork
                                .as_ref()
                                .is_some_and(|owner| owner.requests.is_some())
                            {
                                DecisionPhase::Asking {
                                    meal_people: people,
                                }
                            } else {
                                DecisionPhase::Work {
                                    meal_people: people,
                                }
                            };
                    }
                    DecisionPhase::Delivery { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.deliver_agreements_autonomously(&busy));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.outcomes.is_empty() {
                            run.events.push(CoreRunEvent::Deliveries(result.outcomes));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::Offers { meal_people };
                    }
                    DecisionPhase::Offers { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.propose_agreements_autonomously(&busy));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.agreements.is_empty() {
                            run.events.push(CoreRunEvent::Offers(result.agreements));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::Acceptance { meal_people };
                    }
                    DecisionPhase::Acceptance { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.accept_agreements_autonomously(&busy));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.agreements.is_empty() {
                            run.events
                                .push(CoreRunEvent::Acceptances(result.agreements));
                        }
                        self.schedule.as_mut().expect("active schedule").phase = if self
                            .autowork
                            .as_ref()
                            .is_some_and(|owner| owner.requests.is_some())
                        {
                            DecisionPhase::Asking { meal_people }
                        } else {
                            DecisionPhase::Work { meal_people }
                        };
                    }
                    DecisionPhase::Asking { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let actors = attempt!('run, self.ask_for_work(&busy));
                        if !actors.is_empty() {
                            meal_people.extend(actors.iter().copied());
                            meal_people.par_sort_unstable();
                            meal_people.dedup();
                            run.events.push(CoreRunEvent::Asks(actors));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::Requests { meal_people };
                    }
                    DecisionPhase::Requests { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.respond_to_work_requests(&busy, schedule.work_rate));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.events.is_empty() {
                            run.events.push(CoreRunEvent::Requests(result.events));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::Work { meal_people };
                    }
                    DecisionPhase::Work { meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people);
                        busy.par_sort_unstable();
                        busy.dedup();
                        let starts =
                            attempt!('run, self.start_autonomous_work(&busy, schedule.work_rate));
                        if !starts.is_empty() {
                            run.events.push(CoreRunEvent::Starts(starts));
                        }
                        run.decisions += 1;
                        let next = Some(u128::from(offset) + u128::from(schedule.period_millis));
                        let schedule = self.schedule.as_mut().expect("active schedule");
                        schedule.next_decision_millis = next;
                        schedule.phase = DecisionPhase::Meals;
                    }
                }
                continue;
            }
            let spent = offset - start;
            if spent >= requested_millis {
                break;
            }
            let until_decision = schedule.next_decision_millis.map_or(u64::MAX, |next| {
                (next - u128::from(offset)).min(u128::from(u64::MAX)) as u64
            });
            let step = (requested_millis - spent)
                .min(deadline.after_millis)
                .min(until_decision);
            if step == 0 {
                run.failure = Some("core schedule made no progress".into());
                break;
            }
            attempt!('run, self.core.progress(step));
        }
        run.elapsed_millis = self.core.elapsed_millis().expect("active interval") - start;
        Ok(run)
    }

    /// Continue the limited host across boundaries, retaining decision timing
    /// and work rates. Boundary events are emitted in order. A failure after a
    /// boundary leaves the settled boundary committed and reports that failure.
    pub fn advance_continuous(
        &mut self,
        requested_millis: u64,
        blocked: &[u64],
    ) -> Result<CoreRun, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        self.schedule
            .as_ref()
            .ok_or("no core schedule")?
            .validate(&self.core)?;
        let mut combined = CoreRun::default();
        loop {
            let remaining = requested_millis - combined.elapsed_millis;
            let mut segment = self.advance_scheduled(remaining, blocked)?;
            combined.elapsed_millis += segment.elapsed_millis;
            combined.events.append(&mut segment.events);
            let Some(decisions) = combined.decisions.checked_add(segment.decisions) else {
                combined.failure = Some("decision count overflow".into());
                combined.boundary = segment.boundary;
                break;
            };
            combined.decisions = decisions;
            if let Some(error) = segment.failure {
                combined.failure = Some(error);
                combined.boundary = segment.boundary;
                break;
            }
            if !segment.boundary {
                break;
            }
            let mut schedule = self.schedule.clone().expect("active schedule");
            let quantum = self.core.physical.tick_millis();
            let current_rates = self
                .core
                .physical
                .work_interval()
                .expect("active interval")
                .rates
                .clone();
            let events = match self.settle() {
                Ok(events) => events,
                Err(error) => {
                    combined.failure = Some(error);
                    combined.boundary = true;
                    break;
                }
            };
            combined
                .events
                .push(CoreRunEvent::Boundary(Box::new(events)));
            let Some(boundaries) = combined.boundaries.checked_add(1) else {
                combined.failure = Some("boundary count overflow".into());
                combined.boundary = true;
                break;
            };
            combined.boundaries = boundaries;
            schedule.next_decision_millis = schedule
                .next_decision_millis
                .map(|next| next - u128::from(quantum));
            let mut candidates: Vec<_> = schedule
                .carried_rates
                .par_iter()
                .map(|&(id, rate)| (id, 0u8, rate))
                .collect();
            candidates.par_extend(current_rates.par_iter().map(|&(id, rate)| (id, 1u8, rate)));
            candidates.par_sort_unstable_by_key(|row| (row.0, row.1));
            schedule.carried_rates = candidates
                .par_chunk_by(|a, b| a.0 == b.0)
                .filter_map(|group| {
                    let &(id, _, rate) = group.last().expect("nonempty group");
                    self.core
                        .physical
                        .work()
                        .binary_search_by_key(&id, |row| row.job.id)
                        .ok()
                        .filter(|&index| {
                            let row = &self.core.physical.work()[index];
                            row.phase == game_lot_runtime::Phase::Working && row.job.remaining > 0
                        })
                        .map(|_| (id, rate))
                })
                .collect();
            let rates: Vec<_> = schedule
                .carried_rates
                .par_iter()
                .copied()
                .filter(|(id, _)| {
                    let index = self
                        .core
                        .physical
                        .work()
                        .binary_search_by_key(id, |row| row.job.id)
                        .expect("carried job");
                    let job = &self.core.physical.work()[index].job;
                    self.core
                        .people
                        .binary_search_by_key(&job.worker, |person| person.id)
                        .ok()
                        .is_some_and(|person| {
                            let person = &self.core.people[person];
                            person.health > 0
                                && person.hunger <= self.core.rules.max_work_hunger
                                && person.region == Some(job.region)
                                && blocked.binary_search(&person.id).is_err()
                        })
                })
                .collect();
            let next = CoreInterval::begin(self.core.snapshot(), &rates).and_then(|core| {
                schedule.validate(&core)?;
                Ok(core)
            });
            match next {
                Ok(core) => {
                    self.core = core;
                    self.schedule = Some(schedule);
                }
                Err(error) => {
                    combined.failure = Some(error);
                    combined.boundary = true;
                    break;
                }
            }
        }
        Ok(combined)
    }
}

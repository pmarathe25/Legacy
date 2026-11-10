//! Bounded event-driven advancement of the saved, limited core host.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum DecisionPhase {
    Meals,
    Work { meal_people: Vec<u64> },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSchedule {
    pub period_millis: u64,
    pub work_rate: u64,
    pub next_decision_millis: Option<u64>,
    pub phase: DecisionPhase,
}
impl CoreSchedule {
    pub(super) fn validate(&self, core: &CoreInterval) -> Result<(), String> {
        let offset = core
            .elapsed_millis()
            .ok_or("scheduled host needs an active interval")?;
        let quantum = core.physical.tick_millis();
        if self.period_millis == 0
            || self.work_rate == 0
            || offset == 0
            || offset > quantum
            || self
                .next_decision_millis
                .is_some_and(|next| next < offset || next >= quantum)
        {
            return Err("invalid core decision schedule".into());
        }
        if let DecisionPhase::Work { meal_people } = &self.phase
            && (self.next_decision_millis != Some(offset)
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
    Starts(Vec<(u64, bool)>),
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreRun {
    pub elapsed_millis: u64,
    pub boundary: bool,
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
            next_decision_millis: Some(offset),
            phase: DecisionPhase::Meals,
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
            if schedule.next_decision_millis == Some(offset) {
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
                            DecisionPhase::Work {
                                meal_people: people,
                            };
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
                        let next = offset
                            .checked_add(schedule.period_millis)
                            .filter(|&time| time < quantum);
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
            let until_decision = schedule
                .next_decision_millis
                .map_or(u64::MAX, |next| next - offset);
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
}

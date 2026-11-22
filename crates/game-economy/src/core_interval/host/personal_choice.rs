//! Opt-in competition between construction, carrying, maintenance, and work starts.
//! Other scheduler domains keep their existing behavior until separately migrated.
use super::*;
use game_personal_choice::{Key, Person, Proposal};

pub(super) const CONSTRUCTION: u32 = 1;
pub(super) const CARRYING: u32 = 2;
pub(super) const MAINTENANCE: u32 = 3;
pub(super) const WORK: u32 = 4;
pub(super) const CONTINUE_WORK: u32 = 5;
pub(super) const EXPERIMENT: u32 = 6;
pub(super) const MEAL: u32 = 7;
pub(super) const TOOL_REPAIR: u32 = 8;
pub(super) const FOOD_PICKUP: u32 = 9;
pub(super) const SELF_TREATMENT: u32 = 10;
pub(super) const CULTIVATION: u32 = 11;
pub(super) const MECHANICAL_LOAD: u32 = 12;
pub(super) const MECHANICAL_DESIGN: u32 = 13;
pub(super) const MECHANICAL_WATCH: u32 = 14;
pub(super) const PERSONAL_ATTACK: u32 = 15;
pub(super) const WOUND_CARE: u32 = 16;
pub(super) const WRITTEN_FORM: u32 = 17;
pub(super) const MARK_INSCRIPTION: u32 = 18;
pub(super) const REMEMBERED_FORM: u32 = 19;
pub(super) const GATHERING: u32 = 20;
pub(super) const REST: u32 = 21;
pub(super) const RESUME: u32 = 22;
pub(super) const PRACTICE_LESSON: u32 = 23;
pub(super) const CONFINEMENT: u32 = 24;
pub(super) const READ_TEXT: u32 = 25;
pub(super) const ASSIST: u32 = 26;
pub(super) const PASTIME: u32 = 27;

fn index_by_person<T>(rows: &[T], id: u64, key: impl Fn(&T) -> u64) -> Option<usize> {
    rows.first()
        .and_then(|first| id.checked_sub(key(first)))
        .and_then(|offset| usize::try_from(offset).ok())
        .filter(|&index| rows.get(index).is_some_and(|row| key(row) == id))
        .or_else(|| rows.binary_search_by_key(&id, key).ok())
}

fn candidate_index(rows: &[CoreChoiceIntent], actor: u64, key: Key) -> Option<usize> {
    index_by_person(rows, actor, |row| row.actor)
        .filter(|&index| rows[index].key == key)
        .or_else(|| {
            rows.binary_search_by_key(&(actor, key), |row| (row.actor, row.key))
                .ok()
        })
}

pub(super) fn selected_key(rows: &[CoreChoiceIntent], actor: u64) -> Option<Key> {
    index_by_person(rows, actor, |row| row.actor).map(|index| rows[index].key)
}

/// A selected work start; job identity is assigned only during admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreWorkIntent {
    pub region: u64,
    pub recipe: u32,
    pub units: u32,
    pub base_rate: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreChoiceIntent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barrier: Option<BarrierAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_form: Option<game_language::writing::Association>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike: Option<game_personal_conflict::Strike>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch: Option<MechanicalWitnessRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<game_mechanics::Assembly>,
    pub actor: u64,
    pub key: Key,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cultivation: Option<game_economy_agriculture::opportunities::Opportunity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mechanical_load: Option<MechanicalLoadRequest>,
    /// Exact physical movements except transient action and new-site identities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement: Option<game_lots::Movement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub second_movement: Option<game_lots::Movement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<CoreWorkIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment: Option<game_mechanics::Experiment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_value: Option<i64>,
    /// The person's observed hunger when choosing a meal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meal_value: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_damage: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_value: Option<i64>,
    /// The actor's projected fatigue when considering rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fatigue_value: Option<i64>,
}

impl CoreChoiceIntent {
    pub(super) fn magnitude(&self) -> i64 {
        if self.key.domain == GATHERING {
            return 1;
        }
        if self.cultivation.is_some() || self.barrier.is_some() {
            return 1;
        }
        if let Some(value) = self
            .experiment_value
            .or(self.meal_value)
            .or(self.health_value)
            .or(self.fatigue_value)
        {
            return value;
        }
        if let Some(damage) = self.tool_damage {
            return damage.min(i64::MAX as u64) as i64;
        }
        (self.movement.map_or(0, |row| u128::from(row.quantity))
            + self.work.map_or(0, |row| u128::from(row.units))
            + self
                .second_movement
                .map_or(0, |row| u128::from(row.quantity)))
        .min(i64::MAX as u128) as i64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreChoiceOutcome {
    NoLocalAction,
    Pending,
    Cancelled,
    Stale,
    Accepted,
    Denied,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreChoiceEvent {
    pub tick: u64,
    pub offset_millis: u64,
    pub actor: u64,
    pub key: Option<Key>,
    pub outcome: CoreChoiceOutcome,
}

pub(super) fn intention_permitted(person: &Person, intent: &CoreChoiceIntent) -> bool {
    let Some(score) = game_personal_choice::score(person, intent.key, intent.magnitude()) else {
        return false;
    };
    score > i128::from(person.idle) || (score == i128::from(person.idle) && person.act_on_tie)
}

impl CoreHost {
    /// Let each person compare their own rest or resumption inclination with idleness.
    pub(super) fn reconsider_rest(
        &mut self,
        people: &[Person],
        blocked: &[u64],
    ) -> Result<(Vec<u64>, Vec<CoreChoiceEvent>), String> {
        let Some(owner) = &self.fatigue else {
            return Ok((Vec::new(), Vec::new()));
        };
        if !people.par_iter().any(|person| {
            person
                .preferences
                .par_iter()
                .any(|preference| matches!(preference.domain, REST | RESUME) && preference.enabled)
        }) {
            return Ok((Vec::new(), Vec::new()));
        }
        let quantum = self.core.physical.tick_millis();
        let projected = if owner.elapsed_damage_remainders.is_some() {
            owner.clone()
        } else {
            owner
                .prepare(
                    &self.activity,
                    quantum,
                    |id| {
                        self.core
                            .people
                            .binary_search_by_key(&id, |person| person.id)
                            .ok()
                            .is_some_and(|i| self.core.people[i].health > 0)
                    },
                    |id| {
                        self.core
                            .people
                            .binary_search_by_key(&id, |person| person.id)
                            .ok()
                            .is_some_and(|i| self.core.people[i].controlled)
                    },
                )?
                .state
        };
        let resting = owner.fatigue.resting();
        let mut occupied = blocked.to_vec();
        if let Some(schedule) = &self.schedule {
            occupied.par_extend(
                schedule
                    .pending_cultivation
                    .par_iter()
                    .map(|row| row.person),
            );
            occupied.par_extend(schedule.pending_gathering.par_iter().map(|row| row.person));
        }
        occupied.par_sort_unstable();
        occupied.dedup();
        let mut jobs: Vec<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|work| work.phase == game_lot_runtime::Phase::Working)
            .map(|work| (work.job.worker, work.job.id, work.job.remaining))
            .collect();
        jobs.par_sort_unstable_by_key(|row| row.0);
        let proposals: Vec<_> = people
            .par_iter()
            .flat_map_iter(|profile| {
                let Some(actor) = self
                    .core
                    .people
                    .binary_search_by_key(&profile.id, |person| person.id)
                    .ok()
                    .map(|index| &self.core.people[index])
                else {
                    return Vec::new();
                };
                if actor.controlled
                    || actor.health == 0
                    || actor.region.is_none()
                    || occupied.binary_search(&actor.id).is_ok()
                {
                    return Vec::new();
                }
                let is_resting = resting.binary_search(&actor.id).is_ok();
                let job = jobs.binary_search_by_key(&actor.id, |row| row.0).ok();
                if !is_resting && job.is_none() {
                    return Vec::new();
                }
                let fatigue = projected
                    .fatigue
                    .people
                    .binary_search_by_key(&actor.id, |person| person.id)
                    .ok()
                    .map_or(0, |index| projected.fatigue.people[index].fatigue);
                let mut options = vec![Proposal {
                    actor: actor.id,
                    key: Key {
                        domain: if is_resting { RESUME } else { REST },
                        action: 1,
                    },
                    value: if is_resting {
                        projected.fatigue.rules.capacity.saturating_sub(fatigue)
                    } else {
                        fatigue
                    } as i64,
                }];
                if !is_resting && let Some(index) = job {
                    options.push(Proposal {
                        actor: actor.id,
                        key: Key {
                            domain: CONTINUE_WORK,
                            action: jobs[index].1,
                        },
                        value: jobs[index].2.min(i64::MAX as u64) as i64,
                    });
                }
                options
            })
            .collect();
        let selections = game_personal_choice::select(
            people,
            &proposals,
            self.core.seed,
            self.core.decision_time_key(),
        )
        .map_err(str::to_owned)?;
        let choices: Vec<_> = selections
            .par_iter()
            .filter_map(|selection| {
                let key = selection.action?;
                match key.domain {
                    REST => Some(game_economy_fatigue::RestChoice {
                        person: selection.actor,
                        resting: true,
                    }),
                    RESUME => Some(game_economy_fatigue::RestChoice {
                        person: selection.actor,
                        resting: false,
                    }),
                    _ => None,
                }
            })
            .collect();
        if choices.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let events = self.choose_personal_rest(&choices, &occupied)?;
        let actors = choices.par_iter().map(|choice| choice.person).collect();
        Ok((actors, events))
    }

    /// Withdraw a person's pending gathering intention before settlement.
    pub fn cancel_gathering_intention(
        &mut self,
        person: u64,
    ) -> Result<Option<CoreChoiceEvent>, String> {
        let tick = self.core.physical.tick();
        let offset_millis = self.core.elapsed_millis().ok_or("no physical interval")?;
        let schedule = self.schedule.as_mut().ok_or("no core schedule")?;
        let Ok(index) = schedule
            .pending_gathering
            .binary_search_by_key(&person, |row| row.person)
        else {
            return Ok(None);
        };
        let intention = schedule.pending_gathering.remove(index);
        Ok(Some(CoreChoiceEvent {
            tick,
            offset_millis,
            actor: person,
            key: Some(Key {
                domain: GATHERING,
                action: intention.node,
            }),
            outcome: CoreChoiceOutcome::Cancelled,
        }))
    }
    /// Withdraw one person's pending cultivation intention before settlement.
    pub fn cancel_cultivation_intention(
        &mut self,
        person: u64,
    ) -> Result<Option<CoreChoiceEvent>, String> {
        let tick = self.core.physical.tick();
        let offset_millis = self.core.elapsed_millis().ok_or("no physical interval")?;
        let schedule = self.schedule.as_mut().ok_or("no core schedule")?;
        let Ok(index) = schedule
            .pending_cultivation
            .binary_search_by_key(&person, |row| row.person)
        else {
            return Ok(None);
        };
        let intention = schedule.pending_cultivation.remove(index);
        Ok(Some(CoreChoiceEvent {
            tick,
            offset_millis,
            actor: person,
            key: Some(Key {
                domain: CULTIVATION,
                action: intention.field,
            }),
            outcome: CoreChoiceOutcome::Cancelled,
        }))
    }
    /// People with an explicit continuation preference may leave an ordinary
    /// active job. Shared sessions and succession are reviewed by their owners.
    pub(super) fn reconsider_ordinary_work(
        &mut self,
        people: &[Person],
        blocked: &[u64],
    ) -> Result<(Vec<u64>, Vec<CoreChoiceEvent>), String> {
        if !people.par_iter().any(|person| {
            person
                .preferences
                .binary_search_by_key(&CONTINUE_WORK, |preference| preference.domain)
                .is_ok()
        }) {
            return Ok((Vec::new(), Vec::new()));
        }
        let rest = self.blocked_with_rest(blocked);
        let blocked = rest.as_deref().unwrap_or(blocked);
        let mut shared: std::collections::HashSet<u64> = self
            .shared_work
            .as_ref()
            .map_or_else(std::collections::HashSet::new, |owner| {
                owner.sessions.par_iter().map(|row| row.event.job).collect()
            });
        if let Some(succession) = self
            .shared_work
            .as_ref()
            .and_then(|owner| owner.owner.succession.as_ref())
        {
            shared.par_extend(succession.engagements.par_iter().map(|row| row.job));
        }
        let mut jobs: Vec<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|row| {
                row.phase == game_lot_runtime::Phase::Working
                    && !shared.contains(&row.job.id)
                    && blocked.binary_search(&row.job.worker).is_err()
                    && self
                        .core
                        .people
                        .binary_search_by_key(&row.job.worker, |person| person.id)
                        .ok()
                        .is_some_and(|i| {
                            self.core.people[i].health > 0 && !self.core.people[i].controlled
                        })
                    && people
                        .binary_search_by_key(&row.job.worker, |person| person.id)
                        .ok()
                        .is_some_and(|i| {
                            people[i]
                                .preferences
                                .binary_search_by_key(&CONTINUE_WORK, |preference| {
                                    preference.domain
                                })
                                .is_ok()
                        })
            })
            .map(|row| (row.job.worker, row.job.id, row.job.remaining))
            .collect();
        jobs.par_sort_unstable_by_key(|row| row.0);
        if jobs.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let decision_makers: Vec<_> = people
            .par_iter()
            .filter(|person| jobs.binary_search_by_key(&person.id, |row| row.0).is_ok())
            .cloned()
            .collect();
        let proposals: Vec<_> = jobs
            .par_iter()
            .map(|&(actor, job, remaining)| Proposal {
                actor,
                key: Key {
                    domain: CONTINUE_WORK,
                    action: job,
                },
                value: remaining.min(i64::MAX as u64) as i64,
            })
            .collect();
        let tick = self.core.physical.tick();
        let offset_millis = self.core.elapsed_millis().ok_or("no physical interval")?;
        let round = self.core.decision_time_key();
        let decisions =
            game_personal_choice::select(&decision_makers, &proposals, self.core.seed, round)
                .map_err(str::to_owned)?;
        let mut stops: Vec<_> = decisions
            .par_iter()
            .filter(|decision| decision.action.is_none())
            .map(|decision| {
                let i = jobs
                    .binary_search_by_key(&decision.actor, |row| row.0)
                    .expect("known job");
                (jobs[i].1, decision.actor)
            })
            .collect();
        if stops.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        stops.par_sort_unstable_by_key(|row| row.0);
        self.abandon_work(&stops)?;
        let mut actors: Vec<_> = stops.par_iter().map(|row| row.1).collect();
        actors.par_sort_unstable();
        let mut events: Vec<_> = stops
            .into_par_iter()
            .map(|(job, actor)| CoreChoiceEvent {
                tick,
                offset_millis,
                actor,
                key: Some(Key {
                    domain: CONTINUE_WORK,
                    action: job,
                }),
                outcome: CoreChoiceOutcome::Stopped,
            })
            .collect();
        events.par_sort_unstable_by_key(|event| event.actor);
        Ok((actors, events))
    }

    /// Admit already chosen work without choosing a replacement policy.
    pub(super) fn start_selected_work(
        &mut self,
        intents: &[CoreChoiceIntent],
        blocked: &[u64],
    ) -> Result<Vec<(u64, u64, bool)>, String> {
        if intents.is_empty() {
            return Ok(Vec::new());
        }
        let owner = self
            .autowork
            .as_ref()
            .ok_or("work choice requires an owner")?;
        let after_live = self
            .core
            .physical
            .work()
            .last()
            .map_or(Some(0), |row| row.job.id.checked_add(1))
            .ok_or("job id overflow")?;
        let first = owner.next_id.max(after_live);
        let next_id = first
            .checked_add(u64::try_from(intents.len()).map_err(|_| "job count overflow")?)
            .ok_or("job id overflow")?;
        let starts: Vec<_> = intents
            .par_iter()
            .enumerate()
            .map(|(index, intent)| {
                let work = intent.work.expect("validated selected work");
                (
                    game_production_model::Start {
                        id: first + index as u64,
                        worker: intent.actor,
                        region: work.region,
                        recipe: work.recipe,
                        units: work.units,
                    },
                    work.base_rate,
                )
            })
            .collect();
        let raw: Vec<_> = starts.par_iter().map(|row| row.0).collect();
        let uses = if self
            .claims
            .as_ref()
            .is_some_and(|claims| !claims.claims.is_empty())
        {
            game_economy_claims::work(&raw, self.core.physical.recipes())?
        } else {
            Vec::new()
        };
        let effective = self.effective_autonomous_work(blocked)?;
        let selected: std::collections::HashSet<_> = intents
            .par_iter()
            .map(|row| (row.actor, row.key.action as u32))
            .collect();
        let candidates: Vec<_> = effective
            .goal_candidates
            .into_par_iter()
            .filter(|row| selected.contains(row))
            .collect();
        let attempts: Vec<_> = starts
            .par_iter()
            .map(|(start, _)| (start.worker, start.recipe))
            .collect();
        let permitted = self.work_capability_checks(&attempts);
        let allowed: Vec<_> = starts
            .par_iter()
            .copied()
            .zip(permitted.par_iter().copied())
            .filter_map(|(start, allowed)| allowed.then_some(start))
            .collect();
        let mut outcomes = if allowed.is_empty() {
            Vec::new()
        } else {
            self.start_with_sites_hunger_limit(&allowed, u32::MAX, true, None)?
        };
        outcomes.par_extend(
            starts
                .par_iter()
                .zip(permitted.par_iter())
                .filter_map(|((start, _), allowed)| (!allowed).then_some((start.id, false))),
        );
        outcomes.par_sort_unstable_by_key(|row| row.0);
        self.autowork.as_mut().expect("owner checked").next_id = next_id;
        self.publish_claim_work(&uses, &outcomes, self.core.physical.tick());
        self.record_goal_setbacks(&candidates, &outcomes);
        let accepted: std::collections::HashMap<_, _> = outcomes.into_iter().collect();
        Ok(starts
            .into_par_iter()
            .map(|(start, _)| {
                (
                    start.worker,
                    start.id,
                    accepted.get(&start.id).copied().unwrap_or(false),
                )
            })
            .collect())
    }

    /// Inspect an uncommitted choice round from the current opening state.
    /// No result is a physical action or an instruction to another person.
    pub fn preview_personal_choices(
        &self,
        blocked: &[u64],
    ) -> Result<(Vec<CoreChoiceIntent>, Vec<CoreChoiceEvent>), String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let people = self
            .schedule
            .as_ref()
            .and_then(|schedule| schedule.personal_choices.as_deref())
            .ok_or("personal choices are not enabled")?;
        self.choose_local_actions(blocked, people)
    }

    /// Enable voluntary competition at a clean decision boundary.
    pub fn enable_personal_choices(&mut self, people: Vec<Person>) -> Result<(), String> {
        let schedule = self.schedule.as_ref().ok_or("no core schedule")?;
        if !matches!(schedule.phase, DecisionPhase::Meals)
            || !schedule.pending_cultivation.is_empty()
            || !schedule.pending_gathering.is_empty()
        {
            return Err("finish the pending decision before changing personal choices".into());
        }
        game_personal_choice::validate_people(&people).map_err(str::to_owned)?;
        if people.par_iter().any(|p| {
            self.core
                .people
                .binary_search_by_key(&p.id, |row| row.id)
                .is_err()
        }) {
            return Err("personal choices require known people".into());
        }
        self.schedule
            .as_mut()
            .expect("checked schedule")
            .personal_choices = Some(std::sync::Arc::new(people));
        Ok(())
    }

    /// Configure inclinations for future births without changing existing people.
    pub fn enable_birth_choices(
        &mut self,
        profile: game_personal_choice::BirthProfile,
    ) -> Result<(), String> {
        profile.validate().map_err(str::to_owned)?;
        let schedule = self.schedule.as_mut().ok_or("no core schedule")?;
        if !matches!(schedule.phase, DecisionPhase::Meals) || schedule.personal_choices.is_none() {
            return Err("newborn preferences require an idle personal-choice schedule".into());
        }
        schedule.birth_choices = Some(profile);
        Ok(())
    }

    pub(super) fn choice_busy(&self, blocked: &[u64]) -> Vec<u64> {
        let rest = self.blocked_with_rest(blocked);
        let mut busy = rest.as_deref().unwrap_or(blocked).to_vec();
        if let Some(schedule) = &self.schedule {
            busy.par_extend(
                schedule
                    .pending_cultivation
                    .par_iter()
                    .map(|row| row.person),
            );
            busy.par_extend(schedule.pending_gathering.par_iter().map(|row| row.person));
        }
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
        busy
    }

    pub(super) fn queue_cultivation(
        &mut self,
        intents: &[CoreChoiceIntent],
    ) -> (Vec<u64>, Vec<CoreChoiceEvent>) {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().expect("active interval");
        let resolved = self.resolve_cultivation_fields(intents);
        let schedule = self.schedule.as_mut().expect("active schedule");
        schedule.pending_cultivation.par_extend(
            resolved
                .par_iter()
                .filter_map(|(row, accepted)| accepted.then_some(*row)),
        );
        schedule
            .pending_cultivation
            .par_sort_unstable_by_key(|row| row.person);
        let actors = resolved.par_iter().map(|row| row.0.person).collect();
        let events = resolved
            .into_par_iter()
            .map(|(row, accepted)| CoreChoiceEvent {
                tick,
                offset_millis: offset,
                actor: row.person,
                key: Some(Key {
                    domain: CULTIVATION,
                    action: row.field,
                }),
                outcome: if accepted {
                    CoreChoiceOutcome::Pending
                } else {
                    CoreChoiceOutcome::Denied
                },
            })
            .collect();
        (actors, events)
    }

    pub(super) fn resolve_cultivation_fields(
        &self,
        intents: &[CoreChoiceIntent],
    ) -> Vec<(game_economy_agriculture::opportunities::Opportunity, bool)> {
        let round = self.core.decision_time_key();
        let schedule = self.schedule.as_ref().expect("active schedule");
        let mut occupied: Vec<_> = schedule
            .pending_cultivation
            .par_iter()
            .map(|row| row.field)
            .collect();
        occupied.par_sort_unstable();
        let mut choices: Vec<_> = intents
            .par_iter()
            .filter_map(|intent| intent.cultivation)
            .collect();
        choices.par_sort_unstable_by_key(|row| {
            (
                row.field,
                game_genetics::random(self.core.seed ^ round, row.field, row.person),
                row.person,
            )
        });
        let mut resolved: Vec<_> = choices
            .par_chunk_by(|a, b| a.field == b.field)
            .flat_map_iter(|group| {
                let available = occupied.binary_search(&group[0].field).is_err();
                group
                    .iter()
                    .enumerate()
                    .map(move |(index, row)| (*row, available && index == 0))
            })
            .collect();
        resolved.par_sort_unstable_by_key(|row| row.0.person);
        resolved
    }

    pub(super) fn queue_gathering(
        &mut self,
        intents: &[CoreChoiceIntent],
    ) -> (Vec<u64>, Vec<CoreChoiceEvent>) {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().expect("active interval");
        let Some(owner) = &self.gathering else {
            return (Vec::new(), Vec::new());
        };
        let nodes: std::collections::HashMap<_, _> =
            owner.nodes.par_iter().map(|node| (node.id, node)).collect();
        let mut selected: Vec<_> = intents
            .par_iter()
            .filter(|intent| intent.key.domain == GATHERING)
            .filter_map(|intent| {
                let node = nodes.get(&intent.key.action)?;
                Some(crate::gathering::Opportunity {
                    person: intent.actor,
                    node: node.id,
                    good: node.good,
                    quantity: node.yield_per_action,
                })
            })
            .collect();
        selected.par_sort_unstable_by_key(|row| row.person);
        let actors = selected.par_iter().map(|row| row.person).collect();
        let events = selected
            .par_iter()
            .map(|row| CoreChoiceEvent {
                tick,
                offset_millis: offset,
                actor: row.person,
                key: Some(Key {
                    domain: GATHERING,
                    action: row.node,
                }),
                outcome: CoreChoiceOutcome::Pending,
            })
            .collect();
        let schedule = self.schedule.as_mut().expect("active schedule");
        schedule.pending_gathering.par_extend(selected);
        schedule
            .pending_gathering
            .par_sort_unstable_by_key(|row| row.person);
        (actors, events)
    }

    /// Read candidates against one opening state; no resource is reserved here.
    fn choice_candidates(
        &self,
        blocked: &[u64],
        domain: Option<u32>,
    ) -> Result<Vec<CoreChoiceIntent>, String> {
        let tick = self.core.physical.tick();
        let needs_provision = domain.is_none_or(|domain| domain == FOOD_PICKUP)
            && self.food_search.is_some()
            && self
                .schedule
                .as_ref()
                .and_then(|s| s.personal_choices.as_ref())
                .is_some_and(|people| {
                    people.par_iter().any(|person| {
                        person
                            .preferences
                            .binary_search_by_key(&FOOD_PICKUP, |p| p.domain)
                            .is_ok()
                    })
                });
        let on_site_meals = domain.is_none_or(|domain| domain == MEAL)
            && self
                .schedule
                .as_ref()
                .is_some_and(|schedule| schedule.on_site_meals);
        let needs_view = needs_provision
            || on_site_meals
            || (domain.is_none_or(|domain| domain == CONSTRUCTION)
                && self.construction.as_ref().is_some_and(|owner| {
                    !owner.policies.is_empty() || !owner.goal_builders.is_empty()
                }))
            || (domain.is_none_or(|domain| domain == CARRYING)
                && self.local_transport.as_ref().is_some_and(|owner| owner.has_choices()))
            || (domain.is_none_or(|domain| domain == GATHERING)
                && self
                    .gathering
                    .as_ref()
                    .is_some_and(|owner| !owner.policies.is_empty()));
        let view = if needs_view {
            Some(self.core.physical.view(tick)?)
        } else {
            None
        };
        let busy = self.choice_busy(blocked);
        let stock_busy = self.block_away_from_stock(&busy);
        let regional = stock_busy.as_deref().unwrap_or(&busy);
        let gathering: Vec<_> = if domain.is_none_or(|domain| domain == GATHERING) {
            self.gathering
                .as_ref()
                .filter(|owner| !owner.policies.is_empty())
                .map_or_else(Vec::new, |owner| {
                    crate::gathering::Context {
                        owner,
                        people: &self.core.people,
                        opinions: self.opinions.as_ref(),
                        busy: regional,
                        view: view.as_ref().expect("gathering view"),
                        tick_millis: self.core.physical.tick_millis(),
                        seed: self.core.seed,
                        tick,
                        manual: None,
                    }
                    .opportunities(u32::MAX)
                    .into_par_iter()
                    .map(|opportunity| CoreChoiceIntent {
                        fatigue_value: None,
                        actor: opportunity.person,
                        key: Key {
                            domain: GATHERING,
                            action: opportunity.node,
                        },
                        barrier: None,
                        written_form: None,
                        strike: None,
                        watch: None,
                        assembly: None,
                        cultivation: None,
                        mechanical_load: None,
                        movement: None,
                        second_movement: None,
                        work: None,
                        experiment: None,
                        experiment_value: None,
                        meal_value: None,
                        tool_damage: None,
                        health_value: None,
                    })
                    .collect()
                })
        } else {
            Vec::new()
        };
        let cultivation = if domain.is_none_or(|domain| domain == CULTIVATION) {
            let mut eligible: Vec<_> = self
                .schedule
                .as_ref()
                .and_then(|s| s.personal_choices.as_ref())
                .map_or_else(Vec::new, |people| {
                    people
                        .par_iter()
                        .filter(|person| {
                            person
                                .preferences
                                .binary_search_by_key(&CULTIVATION, |preference| preference.domain)
                                .ok()
                                .is_some_and(|index| person.preferences[index].enabled)
                        })
                        .map(|person| person.id)
                        .collect()
                });
            eligible.par_sort_unstable();
            if let Some(owner) = self.agriculture.as_ref().filter(|_| !eligible.is_empty()) {
                let round = self.core.decision_time_key();
                game_economy_agriculture::opportunities::discover(
                    owner,
                    &self.core.people,
                    &eligible,
                    &busy,
                    self.current_season(tick),
                    self.core.seed,
                    round,
                )
                .map_err(str::to_owned)?
                .into_par_iter()
                .map(|opportunity| CoreChoiceIntent {
                    fatigue_value: None,
                    barrier: None,
                    written_form: None,
                    strike: None,
                    watch: None,
                    assembly: None,
                    mechanical_load: None,
                    actor: opportunity.person,
                    key: Key {
                        domain: CULTIVATION,
                        action: opportunity.field,
                    },
                    cultivation: Some(opportunity),
                    movement: None,
                    second_movement: None,
                    work: None,
                    experiment: None,
                    experiment_value: None,
                    meal_value: None,
                    tool_damage: None,
                    health_value: None,
                })
                .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let construction = || -> Result<Vec<CoreChoiceIntent>, String> {
            if domain.is_none_or(|domain| domain == CONSTRUCTION)
                && let Some(owner) = &self.construction
                && (!owner.policies.is_empty() || !owner.goal_builders.is_empty())
            {
                #[cfg(feature = "profile")]
                let _span = game_diagnostics::span("choice.preview.construction");
                let sites = self
                    .sites
                    .as_ref()
                    .ok_or("construction requires work sites")?;
                let goal_needs = self.goal_building_needs(owner);
                let opportunities = game_economy_building::Context {
                    owner,
                    sites: &sites.facilities,
                    people: &self.core.people,
                    view: view.as_ref().expect("construction view"),
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy: regional,
                    max_hunger: u32::MAX,
                    tick,
                    manual: None,
                    goal_needs: &goal_needs,
                    unwilling: &[],
                }
                .opportunities();
                let rows: Vec<_> = opportunities
                    .into_par_iter()
                    .map(|opportunity| {
                        let retained = (opportunity.salvage > 0).then_some(game_lots::Movement {
                            batch: None,
                            id: 0,
                            from: opportunity.source,
                            to: Some(game_lots::Place::Site(0)),
                            good: opportunity.good,
                            quantity: opportunity.salvage,
                            item: None,
                        });
                        let consumed = (opportunity.salvage < opportunity.quantity).then(|| {
                            game_lots::Movement {
                                batch: None,
                                id: 0,
                                from: opportunity.source,
                                to: None,
                                good: opportunity.good,
                                quantity: opportunity.quantity - opportunity.salvage,
                                item: None,
                            }
                        });
                        CoreChoiceIntent {
                            fatigue_value: None,
                            barrier: None,
                            written_form: None,
                            strike: None,
                            watch: None,
                            assembly: None,
                            mechanical_load: None,
                            cultivation: None,
                            actor: opportunity.person,
                            key: Key {
                                domain: CONSTRUCTION,
                                action: u64::from(opportunity.blueprint),
                            },
                            movement: Some(
                                retained
                                    .or(consumed)
                                    .expect("positive construction quantity"),
                            ),
                            second_movement: retained.and(consumed),
                            work: None,
                            experiment: None,
                            experiment_value: None,
                            meal_value: None,
                            tool_damage: None,
                            health_value: None,
                        }
                    })
                    .collect();
                Ok(rows)
            } else {
                Ok(Vec::new())
            }
        };
        let carrying = || -> Vec<CoreChoiceIntent> {
            if domain.is_none_or(|domain| domain == CARRYING)
                && let Some(owner) = &self.local_transport
                && owner.has_choices()
            {
                #[cfg(feature = "profile")]
                let _span = game_diagnostics::span("choice.preview.carrying");
                let needs = self.goal_fetch_needs(owner);
                let view = view.as_ref().expect("carrying view");
                let held = self.block_held(&busy);
                let busy = held.as_deref().unwrap_or(&busy);
                let goal =
                    (!needs.is_empty()).then(|| self.goal_carry_context(owner, view, busy, &needs));
                let opportunities = game_economy_local_transport::Context {
                    now: self.core.simulated_millis(),
                    owner,
                    people: &self.core.people,
                    view,
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy,
                    max_hunger: u32::MAX,
                    manual: None,
                    walking: owner.walking.as_ref(),
                    storage_celsius: &|place| self.core.physical.storage_celsius(place),
                }
                .opportunities(goal.as_ref());
                let rows: Vec<_> = opportunities
                    .into_par_iter()
                    .map(|opportunity| CoreChoiceIntent {
                        fatigue_value: None,
                        barrier: None,
                        written_form: None,
                        strike: None,
                        watch: None,
                        assembly: None,
                        mechanical_load: None,
                        cultivation: None,
                        actor: opportunity.person,
                        key: Key {
                            domain: CARRYING,
                            action: u64::from(opportunity.good),
                        },
                        movement: Some(game_lots::Movement {
                            batch: None,
                            id: 0,
                            from: opportunity.from,
                            to: Some(opportunity.to),
                            good: opportunity.good,
                            quantity: opportunity.quantity,
                            item: None,
                        }),
                        second_movement: None,
                        work: None,
                        experiment: None,
                        experiment_value: None,
                        meal_value: None,
                        tool_damage: None,
                        health_value: None,
                    })
                    .collect();
                rows
            } else {
                Vec::new()
            }
        };
        let maintenance = || -> Vec<CoreChoiceIntent> {
            if domain.is_none_or(|domain| domain == MAINTENANCE)
                && let Some(sites) = &self.sites
                && let Some(owner) = &sites.maintenance
            {
                game_economy_building::maintenance::Context {
                    owner,
                    sites: &sites.facilities,
                    people: &self.core.people,
                    opinions: self.opinions.as_ref(),
                    claims: None,
                    busy: &busy,
                    max_hunger: u32::MAX,
                    manual: None,
                    manual_destroy: None,
                    seed: self.core.seed,
                    tick,
                }
                .direct_opportunities()
                .into_par_iter()
                .map(|opportunity| CoreChoiceIntent {
                    fatigue_value: None,
                    barrier: None,
                    written_form: None,
                    strike: None,
                    watch: None,
                    assembly: None,
                    mechanical_load: None,
                    cultivation: None,
                    actor: opportunity.person,
                    key: Key {
                        domain: MAINTENANCE,
                        action: opportunity.site,
                    },
                    movement: Some(game_lots::Movement {
                        batch: None,
                        id: 0,
                        from: game_lots::Place::Person(opportunity.person),
                        to: None,
                        good: opportunity.good,
                        quantity: opportunity.quantity,
                        item: None,
                    }),
                    second_movement: None,
                    work: None,
                    experiment: None,
                    experiment_value: None,
                    meal_value: None,
                    tool_damage: None,
                    health_value: None,
                })
                .collect()
            } else {
                Vec::new()
            }
        };
        let work = || -> Result<Vec<CoreChoiceIntent>, String> {
            let schedule = self.schedule.as_ref().ok_or("no core schedule")?;
            if domain.is_none_or(|domain| domain == WORK)
                && schedule.personal_choices.as_ref().is_some_and(|people| {
                    people.par_iter().any(|person| {
                        person
                            .preferences
                            .binary_search_by_key(&WORK, |preference| preference.domain)
                            .is_ok()
                    })
                })
            {
                Ok(self
                    .preview_work_with_hunger_limit(&busy, schedule.work_rate, u32::MAX, true)?
                    .into_par_iter()
                    .map(|opportunity| CoreChoiceIntent {
                        fatigue_value: None,
                        barrier: None,
                        written_form: None,
                        strike: None,
                        watch: None,
                        assembly: None,
                        mechanical_load: None,
                        cultivation: None,
                        actor: opportunity.person,
                        key: Key {
                            domain: WORK,
                            action: u64::from(opportunity.recipe),
                        },
                        movement: None,
                        second_movement: None,
                        work: Some(CoreWorkIntent {
                            region: opportunity.region,
                            recipe: opportunity.recipe,
                            units: 1,
                            base_rate: schedule.work_rate,
                        }),
                        experiment: None,
                        experiment_value: None,
                        meal_value: None,
                        tool_damage: None,
                        health_value: None,
                    })
                    .collect())
            } else {
                Ok(Vec::new())
            }
        };
        let experiments = || -> Result<Vec<CoreChoiceIntent>, String> {
            let schedule = self.schedule.as_ref().ok_or("no core schedule")?;
            if domain.is_none_or(|domain| domain == EXPERIMENT)
                && schedule.personal_choices.as_ref().is_some_and(|people| {
                    people.par_iter().any(|person| {
                        person
                            .preferences
                            .binary_search_by_key(&EXPERIMENT, |preference| preference.domain)
                            .is_ok()
                    })
                })
            {
                let mut people: Vec<_> = schedule
                    .personal_choices
                    .as_ref()
                    .expect("choice profile")
                    .par_iter()
                    .filter(|person| {
                        person
                            .preferences
                            .binary_search_by_key(&EXPERIMENT, |preference| preference.domain)
                            .ok()
                            .is_some_and(|index| person.preferences[index].enabled)
                    })
                    .map(|person| person.id)
                    .collect();
                people.par_sort_unstable();
                let mut policies = self.preview_mechanical_experiments(&busy)?;
                policies.par_extend(self.preview_personal_device_trials(&people, &busy)?);
                Ok(policies
                    .into_par_iter()
                    .map(|policy| CoreChoiceIntent {
                        fatigue_value: None,
                        barrier: None,
                        written_form: None,
                        strike: None,
                        watch: None,
                        assembly: None,
                        mechanical_load: None,
                        cultivation: None,
                        actor: policy.experiment.actor,
                        key: Key {
                            domain: EXPERIMENT,
                            action: policy.id,
                        },
                        movement: None,
                        second_movement: None,
                        work: None,
                        experiment: Some(policy.experiment),
                        experiment_value: Some(policy.value),
                        meal_value: None,
                        tool_damage: None,
                        health_value: None,
                    })
                    .collect())
            } else {
                Ok(Vec::new())
            }
        };
        let personal_attacks = || -> Result<Vec<CoreChoiceIntent>, String> {
            if domain.is_some_and(|domain| domain != PERSONAL_ATTACK)
                || !self
                    .schedule
                    .as_ref()
                    .and_then(|schedule| schedule.personal_choices.as_ref())
                    .is_some_and(|people| {
                        people.par_iter().any(|person| {
                            person
                                .preferences
                                .binary_search_by_key(&PERSONAL_ATTACK, |preference| {
                                    preference.domain
                                })
                                .ok()
                                .is_some_and(|index| person.preferences[index].enabled)
                        })
                    })
            {
                return Ok(Vec::new());
            }
            Ok(self
                .preview_personal_attacks(&busy)?
                .into_par_iter()
                .map(|policy| CoreChoiceIntent {
                    fatigue_value: None,
                    actor: policy.actor,
                    key: Key {
                        domain: PERSONAL_ATTACK,
                        action: policy.id,
                    },
                    barrier: None,
                    written_form: None,
                    strike: Some(game_personal_conflict::Strike {
                        actor: policy.actor,
                        target: policy.target,
                        force: policy.force,
                    }),
                    watch: None,
                    assembly: None,
                    cultivation: None,
                    mechanical_load: None,
                    movement: None,
                    second_movement: None,
                    work: None,
                    experiment: None,
                    experiment_value: Some(policy.value),
                    meal_value: None,
                    tool_damage: None,
                    health_value: None,
                })
                .collect())
        };
        let mechanical_watching = || -> Result<Vec<CoreChoiceIntent>, String> {
            if domain.is_some_and(|domain| domain != MECHANICAL_WATCH)
                || !self
                    .schedule
                    .as_ref()
                    .and_then(|schedule| schedule.personal_choices.as_ref())
                    .is_some_and(|people| {
                        people.par_iter().any(|person| {
                            person
                                .preferences
                                .binary_search_by_key(&MECHANICAL_WATCH, |preference| {
                                    preference.domain
                                })
                                .ok()
                                .is_some_and(|index| person.preferences[index].enabled)
                        })
                    })
            {
                return Ok(Vec::new());
            }
            Ok(self
                .preview_mechanical_watching(&busy)?
                .into_par_iter()
                .map(|policy| CoreChoiceIntent {
                    fatigue_value: None,
                    barrier: None,
                    written_form: None,
                    strike: None,
                    actor: policy.observer,
                    key: Key {
                        domain: MECHANICAL_WATCH,
                        action: policy.id,
                    },
                    watch: Some(MechanicalWitnessRequest {
                        observer: policy.observer,
                        actor: policy.actor,
                    }),
                    assembly: None,
                    cultivation: None,
                    mechanical_load: None,
                    movement: None,
                    second_movement: None,
                    work: None,
                    experiment: None,
                    experiment_value: Some(policy.value),
                    meal_value: None,
                    tool_damage: None,
                    health_value: None,
                })
                .collect())
        };
        let mechanical_designs = || -> Result<Vec<CoreChoiceIntent>, String> {
            if !domain.is_none_or(|domain| domain == MECHANICAL_DESIGN) {
                return Ok(Vec::new());
            }
            let mut designers: Vec<_> = self
                .schedule
                .as_ref()
                .and_then(|s| s.personal_choices.as_ref())
                .map_or_else(Vec::new, |people| {
                    people
                        .par_iter()
                        .filter(|person| {
                            person
                                .preferences
                                .binary_search_by_key(&MECHANICAL_DESIGN, |preference| {
                                    preference.domain
                                })
                                .ok()
                                .is_some_and(|index| person.preferences[index].enabled)
                        })
                        .map(|person| person.id)
                        .collect()
                });
            designers.par_sort_unstable();
            Ok(self
                .preview_mechanical_designs(&designers, &busy)?
                .into_par_iter()
                .map(|assembly| {
                    let value = self.mechanical_design_value(&assembly);
                    CoreChoiceIntent {
                        fatigue_value: None,
                        barrier: None,
                        written_form: None,
                        strike: None,
                        watch: None,
                        actor: assembly.actor,
                        key: Key {
                            domain: MECHANICAL_DESIGN,
                            action: assembly.apparatus.id,
                        },
                        assembly: Some(assembly),
                        cultivation: None,
                        mechanical_load: None,
                        movement: None,
                        second_movement: None,
                        work: None,
                        experiment: None,
                        experiment_value: Some(value),
                        meal_value: None,
                        tool_damage: None,
                        health_value: None,
                    }
                })
                .collect())
        };
        let mechanical_loads = || -> Result<Vec<CoreChoiceIntent>, String> {
            let schedule = self.schedule.as_ref().ok_or("no core schedule")?;
            if domain.is_none_or(|domain| domain == MECHANICAL_LOAD)
                && schedule.personal_choices.as_ref().is_some_and(|people| {
                    people.par_iter().any(|person| {
                        person
                            .preferences
                            .binary_search_by_key(&MECHANICAL_LOAD, |preference| preference.domain)
                            .ok()
                            .is_some_and(|index| person.preferences[index].enabled)
                    })
                })
            {
                Ok(self
                    .preview_mechanical_loads(&busy)?
                    .into_par_iter()
                    .map(|policy| CoreChoiceIntent {
                        fatigue_value: None,
                        barrier: None,
                        written_form: None,
                        strike: None,
                        watch: None,
                        assembly: None,
                        actor: policy.request.actor,
                        key: Key {
                            domain: MECHANICAL_LOAD,
                            action: policy.id,
                        },
                        cultivation: None,
                        mechanical_load: Some(policy.request),
                        movement: None,
                        second_movement: None,
                        work: None,
                        experiment: None,
                        experiment_value: Some(policy.value),
                        meal_value: None,
                        tool_damage: None,
                        health_value: None,
                    })
                    .collect())
            } else {
                Ok(Vec::new())
            }
        };
        let meals = || -> Result<Vec<CoreChoiceIntent>, String> {
            if domain.is_none_or(|domain| domain == MEAL)
                && self
                    .schedule
                    .as_ref()
                    .and_then(|s| s.personal_choices.as_ref())
                    .is_some_and(|people| {
                        people.par_iter().any(|person| {
                            person
                                .preferences
                                .binary_search_by_key(&MEAL, |p| p.domain)
                                .is_ok()
                        })
                    })
            {
                let mut sources: Vec<_> = game_economy_person::meal::opportunities_now(
                    &self.core.people,
                    self.core.physical.goods(),
                    &self.core.physical,
                    &busy,
                )?
                .into_par_iter()
                .map(|food| {
                    (
                        food.person,
                        food.good,
                        food.portion,
                        game_lots::Place::Person(food.person),
                    )
                })
                .collect();
                if on_site_meals {
                    let local_food = game_economy_person::provision::opportunities_to_eat(
                        &self.core.people,
                        self.core.physical.goods(),
                        view.as_ref().expect("on-site meal view"),
                        regional,
                        |person, region, good| {
                            claims
                                .as_ref()
                                .map_or(Some(0), |index| index.food_access(person, region, good))
                        },
                    )?;
                    sources.par_extend(local_food.into_par_iter().map(|food| {
                        (
                            food.person,
                            food.good,
                            food.quantity,
                            game_lots::Place::Stock {
                                region: food.region,
                                good: food.good,
                            },
                        )
                    }));
                }
                Ok(sources
                    .into_par_iter()
                    .map(|(id, good, portion, from)| {
                        let person =
                            index_by_person(&self.core.people, id, |p| p.id).expect("known person");
                        CoreChoiceIntent {
                            fatigue_value: None,
                            barrier: None,
                            written_form: None,
                            strike: None,
                            watch: None,
                            assembly: None,
                            mechanical_load: None,
                            cultivation: None,
                            actor: id,
                            key: Key {
                                domain: MEAL,
                                action: u64::from(good),
                            },
                            movement: Some(game_lots::Movement {
                                batch: None,
                                id: 0,
                                from,
                                to: None,
                                good,
                                quantity: portion,
                                item: None,
                            }),
                            second_movement: None,
                            work: None,
                            experiment: None,
                            experiment_value: None,
                            meal_value: Some(i64::from(self.core.people[person].hunger)),
                            tool_damage: None,
                            health_value: None,
                        }
                    })
                    .collect())
            } else {
                Ok(Vec::new())
            }
        };
        let tool_repairs = || -> Result<Vec<CoreChoiceIntent>, String> {
            let enabled = self
                .schedule
                .as_ref()
                .and_then(|schedule| schedule.personal_choices.as_ref())
                .is_some_and(|people| {
                    people.par_iter().any(|person| {
                        person
                            .preferences
                            .binary_search_by_key(&TOOL_REPAIR, |p| p.domain)
                            .is_ok()
                    })
                });
            if !domain.is_none_or(|domain| domain == TOOL_REPAIR) || !enabled {
                return Ok(Vec::new());
            }
            let Some(owner) = &self.tool_wear else {
                return Ok(Vec::new());
            };
            Ok(owner
                .prepare_repairs(
                    &self.core.physical,
                    &self.core.people,
                    &busy,
                    &[],
                    u32::MAX,
                    1,
                )?
                .into_par_iter()
                .map(|(actor, item, action)| {
                    let index = owner
                        .conditions
                        .binary_search_by_key(&item, |row| row.item)
                        .expect("known condition");
                    let mut movement = action.movements[0];
                    movement.id = 0;
                    CoreChoiceIntent {
                        fatigue_value: None,
                        barrier: None,
                        written_form: None,
                        strike: None,
                        watch: None,
                        assembly: None,
                        mechanical_load: None,
                        cultivation: None,
                        actor,
                        key: Key {
                            domain: TOOL_REPAIR,
                            action: item,
                        },
                        movement: Some(movement),
                        second_movement: None,
                        work: None,
                        experiment: None,
                        experiment_value: None,
                        meal_value: None,
                        tool_damage: Some(owner.conditions[index].damage),
                        health_value: None,
                    }
                })
                .collect())
        };
        let food_pickups = || -> Result<Vec<CoreChoiceIntent>, String> {
            if !needs_provision {
                return Ok(Vec::new());
            }
            Ok(game_economy_person::provision::opportunities_now(
                &self.core.people,
                self.core.physical.goods(),
                view.as_ref().expect("food pickup view"),
                regional,
                |person, region, good| {
                    claims
                        .as_ref()
                        .map_or(Some(0), |index| index.food_access(person, region, good))
                },
            )?
            .into_par_iter()
            .map(|opportunity| {
                let person = index_by_person(&self.core.people, opportunity.person, |p| p.id)
                    .expect("known person");
                CoreChoiceIntent {
                    fatigue_value: None,
                    barrier: None,
                    written_form: None,
                    strike: None,
                    watch: None,
                    assembly: None,
                    mechanical_load: None,
                    cultivation: None,
                    actor: opportunity.person,
                    key: Key {
                        domain: FOOD_PICKUP,
                        action: u64::from(opportunity.good),
                    },
                    movement: Some(game_lots::Movement {
                        batch: None,
                        id: 0,
                        from: game_lots::Place::Stock {
                            region: opportunity.region,
                            good: opportunity.good,
                        },
                        to: Some(game_lots::Place::Person(opportunity.person)),
                        good: opportunity.good,
                        quantity: opportunity.quantity,
                        item: None,
                    }),
                    second_movement: None,
                    work: None,
                    experiment: None,
                    experiment_value: None,
                    meal_value: Some(i64::from(self.core.people[person].hunger)),
                    tool_damage: None,
                    health_value: None,
                }
            })
            .collect())
        };
        let treatments = || -> Result<Vec<CoreChoiceIntent>, String> {
            if !domain.is_none_or(|domain| domain == SELF_TREATMENT)
                || !self
                    .schedule
                    .as_ref()
                    .and_then(|s| s.personal_choices.as_ref())
                    .is_some_and(|people| {
                        people.par_iter().any(|person| {
                            person
                                .preferences
                                .binary_search_by_key(&SELF_TREATMENT, |p| p.domain)
                                .is_ok()
                        })
                    })
            {
                return Ok(Vec::new());
            }
            let Some(health) = &self.health else {
                return Ok(Vec::new());
            };
            Ok(game_economy_treatment::opportunities_now(
                &health.treatment,
                &self.core.people,
                &self.core.physical,
                &busy,
            )?
            .into_par_iter()
            .map(|opportunity| CoreChoiceIntent {
                fatigue_value: None,
                barrier: None,
                written_form: None,
                strike: None,
                watch: None,
                assembly: None,
                mechanical_load: None,
                cultivation: None,
                actor: opportunity.person,
                key: Key {
                    domain: SELF_TREATMENT,
                    action: u64::from(opportunity.medicine),
                },
                movement: Some(game_lots::Movement {
                    batch: None,
                    id: 0,
                    from: game_lots::Place::Person(opportunity.person),
                    to: None,
                    good: opportunity.medicine,
                    quantity: opportunity.quantity,
                    item: None,
                }),
                second_movement: None,
                work: None,
                experiment: None,
                experiment_value: None,
                meal_value: None,
                tool_damage: None,
                health_value: Some(i64::from(
                    self.core
                        .rules
                        .max_health
                        .saturating_sub(opportunity.health),
                )),
            })
            .collect())
        };
        let (
            (construction, carrying),
            (
                maintenance,
                (work, (experiments, (meals, (tool_repairs, (food_pickups, treatments))))),
            ),
        ) = rayon::join(
            || rayon::join(construction, carrying),
            || {
                rayon::join(maintenance, || {
                    rayon::join(work, || {
                        rayon::join(experiments, || {
                            rayon::join(meals, || {
                                rayon::join(tool_repairs, || rayon::join(food_pickups, treatments))
                            })
                        })
                    })
                })
            },
        );
        let mut intents = construction?;
        intents.par_extend(carrying);
        intents.par_extend(maintenance);
        intents.par_extend(work?);
        intents.par_extend(experiments?);
        intents.par_extend(meals?);
        intents.par_extend(tool_repairs?);
        intents.par_extend(food_pickups?);
        intents.par_extend(treatments?);
        intents.par_extend(cultivation);
        intents.par_extend(gathering);
        intents.par_extend(mechanical_loads()?);
        intents.par_extend(mechanical_designs()?);
        intents.par_extend(mechanical_watching()?);
        intents.par_extend(personal_attacks()?);
        if domain.is_none_or(|domain| domain == WOUND_CARE)
            && self
                .schedule
                .as_ref()
                .and_then(|schedule| schedule.personal_choices.as_ref())
                .is_some_and(|people| {
                    people.par_iter().any(|person| {
                        person
                            .preferences
                            .binary_search_by_key(&WOUND_CARE, |row| row.domain)
                            .ok()
                            .is_some_and(|index| person.preferences[index].enabled)
                    })
                })
        {
            let owner = self
                .personal_conflict
                .as_ref()
                .and_then(|owner| owner.wounds.as_ref());
            // A paused job still owns its inputs, but it does not physically
            // occupy the worker. The care owner separately excludes active work.
            let mut care_busy = blocked.to_vec();
            if let Some(schedule) = &self.schedule {
                care_busy.par_extend(
                    schedule
                        .pending_cultivation
                        .par_iter()
                        .map(|row| row.person),
                );
            }
            care_busy.par_sort_unstable();
            care_busy.dedup();
            let care: Vec<_> = self
                .preview_wound_care(&care_busy)?
                .into_par_iter()
                .map(|choice| {
                    let wounds = owner.expect("care requires wounds");
                    let remedy = wounds.remedies[wounds
                        .remedies
                        .binary_search_by_key(&choice.good, |row| row.good)
                        .expect("known remedy")];
                    let case = &wounds.cases[wounds
                        .cases
                        .binary_search_by_key(&choice.person, |row| row.person)
                        .expect("known wound")];
                    CoreChoiceIntent {
                        fatigue_value: None,
                        actor: choice.person,
                        key: Key {
                            domain: WOUND_CARE,
                            action: u64::from(choice.good),
                        },
                        movement: Some(game_lots::Movement {
                            id: 0,
                            from: game_lots::Place::Person(choice.person),
                            to: None,
                            good: choice.good,
                            quantity: remedy.quantity,
                            item: None,
                            batch: None,
                        }),
                        barrier: None,
                        written_form: None,
                        strike: None,
                        watch: None,
                        assembly: None,
                        mechanical_load: None,
                        cultivation: None,
                        second_movement: None,
                        work: None,
                        experiment: None,
                        experiment_value: None,
                        meal_value: None,
                        tool_damage: None,
                        health_value: Some(i64::from(case.severity)),
                    }
                })
                .collect();
            intents.par_extend(care);
        }
        if domain.is_none_or(|domain| domain == WRITTEN_FORM)
            && let Some(people) = self
                .schedule
                .as_ref()
                .and_then(|schedule| schedule.personal_choices.as_ref())
            && let Some(language) = self
                .social
                .as_ref()
                .and_then(|social| social.language.as_ref())
            && let Some(forms) = language.writing.as_ref()
        {
            let round = self.core.decision_time_key();
            let proposed: Vec<_> = people
                .par_iter()
                .flat_map_iter(|person| {
                    let enabled = person
                        .preferences
                        .binary_search_by_key(&WRITTEN_FORM, |row| row.domain)
                        .ok()
                        .is_some_and(|index| person.preferences[index].enabled);
                    let actor = self
                        .core
                        .people
                        .binary_search_by_key(&person.id, |row| row.id)
                        .ok()
                        .map(|index| &self.core.people[index]);
                    let available = enabled
                        && busy.binary_search(&person.id).is_err()
                        && actor.is_some_and(|actor| actor.health > 0 && !actor.controlled);
                    let mind = language
                        .minds
                        .binary_search_by_key(&person.id, |mind| mind.person)
                        .ok()
                        .map(|index| &language.minds[index]);
                    let room = forms
                        .minds
                        .binary_search_by_key(&person.id, |mind| mind.person)
                        .ok()
                        .is_none_or(|index| forms.minds[index].associations.len() < forms.capacity);
                    mind.into_iter()
                        .filter(move |_| available && room)
                        .flat_map(move |mind| {
                            mind.words
                                .iter()
                                .enumerate()
                                .filter_map(move |(index, word)| {
                                    if forms.marks(person.id, &word.sounds).is_some() {
                                        return None;
                                    }
                                    let action = u64::try_from(index).ok()?.checked_add(1)?;
                                    let mark = (game_genetics::random(
                                        self.core.seed,
                                        person.id,
                                        round ^ action,
                                    ) as u32)
                                        .max(1);
                                    Some(CoreChoiceIntent {
                                        fatigue_value: None,
                                        actor: person.id,
                                        key: Key {
                                            domain: WRITTEN_FORM,
                                            action,
                                        },
                                        barrier: None,
                                        written_form: Some(game_language::writing::Association {
                                            sounds: word.sounds.clone(),
                                            marks: vec![mark],
                                        }),
                                        strike: None,
                                        watch: None,
                                        assembly: None,
                                        cultivation: None,
                                        mechanical_load: None,
                                        movement: None,
                                        second_movement: None,
                                        work: None,
                                        experiment: None,
                                        experiment_value: Some(1),
                                        meal_value: None,
                                        tool_damage: None,
                                        health_value: None,
                                    })
                                })
                        })
                })
                .collect();
            intents.par_extend(proposed);
        }
        if domain.is_none_or(|domain| domain == MARK_INSCRIPTION)
            && self.writing.is_some()
            && let Some(people) = self
                .schedule
                .as_ref()
                .and_then(|schedule| schedule.personal_choices.as_ref())
            && let Some(forms) = self
                .social
                .as_ref()
                .and_then(|social| social.language.as_ref())
                .and_then(|language| language.writing.as_ref())
        {
            let proposed: Vec<_> = people
                .par_iter()
                .flat_map_iter(|person| {
                    let enabled = person
                        .preferences
                        .binary_search_by_key(&MARK_INSCRIPTION, |row| row.domain)
                        .ok()
                        .is_some_and(|index| person.preferences[index].enabled);
                    let actor = self
                        .core
                        .people
                        .binary_search_by_key(&person.id, |row| row.id)
                        .ok()
                        .map(|index| &self.core.people[index]);
                    let available = enabled
                        && busy.binary_search(&person.id).is_err()
                        && actor.is_some_and(|actor| {
                            actor.health > 0 && actor.region.is_some() && !actor.controlled
                        });
                    forms
                        .minds
                        .binary_search_by_key(&person.id, |mind| mind.person)
                        .ok()
                        .map(|index| &forms.minds[index])
                        .into_iter()
                        .filter(move |_| available)
                        .flat_map(move |mind| {
                            mind.associations
                                .iter()
                                .enumerate()
                                .filter_map(move |(index, form)| {
                                    let action = u64::try_from(index).ok()?.checked_add(1)?;
                                    Some(CoreChoiceIntent {
                                        fatigue_value: None,
                                        actor: person.id,
                                        key: Key {
                                            domain: MARK_INSCRIPTION,
                                            action,
                                        },
                                        barrier: None,
                                        written_form: Some(form.clone()),
                                        strike: None,
                                        watch: None,
                                        assembly: None,
                                        cultivation: None,
                                        mechanical_load: None,
                                        movement: None,
                                        second_movement: None,
                                        work: None,
                                        experiment: None,
                                        experiment_value: Some(1),
                                        meal_value: None,
                                        tool_damage: None,
                                        health_value: None,
                                    })
                                })
                        })
                })
                .collect();
            intents.par_extend(proposed);
        }
        if domain.is_none_or(|domain| domain == REMEMBERED_FORM)
            && let Some(people) = self
                .schedule
                .as_ref()
                .and_then(|schedule| schedule.personal_choices.as_ref())
            && let Some(forms) = self
                .social
                .as_ref()
                .and_then(|social| social.language.as_ref())
                .and_then(|language| language.writing.as_ref())
        {
            let proposed: Vec<_> = people
                .par_iter()
                .flat_map_iter(|person| {
                    let enabled = person
                        .preferences
                        .binary_search_by_key(&REMEMBERED_FORM, |row| row.domain)
                        .ok()
                        .is_some_and(|index| person.preferences[index].enabled);
                    let actor = self
                        .core
                        .people
                        .binary_search_by_key(&person.id, |row| row.id)
                        .ok()
                        .map(|index| &self.core.people[index]);
                    let available = enabled
                        && busy.binary_search(&person.id).is_err()
                        && actor.is_some_and(|actor| actor.health > 0 && !actor.controlled);
                    let full = forms
                        .minds
                        .binary_search_by_key(&person.id, |mind| mind.person)
                        .ok()
                        .is_some_and(|index| {
                            forms.minds[index].associations.len() >= forms.capacity
                        });
                    let start = forms
                        .observations
                        .partition_point(|row| row.person < person.id);
                    forms.observations[start..]
                        .iter()
                        .take_while(move |row| row.person == person.id)
                        .filter_map(move |observation| {
                            let sounds = observation.heard.as_ref()?;
                            let current = forms.marks(person.id, sounds);
                            if !available
                                || current == Some(observation.marks.as_slice())
                                || (full && current.is_none())
                            {
                                return None;
                            }
                            Some(CoreChoiceIntent {
                                fatigue_value: None,
                                actor: person.id,
                                key: Key {
                                    domain: REMEMBERED_FORM,
                                    action: observation.page,
                                },
                                barrier: None,
                                written_form: Some(game_language::writing::Association {
                                    sounds: sounds.clone(),
                                    marks: observation.marks.clone(),
                                }),
                                strike: None,
                                watch: None,
                                assembly: None,
                                cultivation: None,
                                mechanical_load: None,
                                movement: None,
                                second_movement: None,
                                work: None,
                                experiment: None,
                                experiment_value: Some(1),
                                meal_value: None,
                                tool_damage: None,
                                health_value: None,
                            })
                        })
                })
                .collect();
            intents.par_extend(proposed);
        }
        if domain.is_none_or(|domain| domain == REST)
            && self
                .schedule
                .as_ref()
                .and_then(|schedule| schedule.personal_choices.as_ref())
                .is_some_and(|people| {
                    people.par_iter().any(|person| {
                        person
                            .preferences
                            .par_iter()
                            .any(|preference| preference.domain == REST && preference.enabled)
                    })
                })
            && let Some(owner) = &self.fatigue
        {
            let projected = if owner.elapsed_damage_remainders.is_some() {
                owner.clone()
            } else {
                owner
                    .prepare(
                        &self.activity,
                        self.core.physical.tick_millis(),
                        |id| {
                            self.core
                                .people
                                .binary_search_by_key(&id, |person| person.id)
                                .ok()
                                .is_some_and(|index| self.core.people[index].health > 0)
                        },
                        |id| {
                            self.core
                                .people
                                .binary_search_by_key(&id, |person| person.id)
                                .ok()
                                .is_some_and(|index| self.core.people[index].controlled)
                        },
                    )?
                    .state
            };
            let rest: Vec<_> = self
                .core
                .people
                .par_iter()
                .filter(|person| {
                    person.health > 0
                        && !person.controlled
                        && person.region.is_some()
                        && busy.binary_search(&person.id).is_err()
                })
                .map(|person| {
                    let fatigue = projected
                        .fatigue
                        .people
                        .binary_search_by_key(&person.id, |row| row.id)
                        .ok()
                        .map_or(0, |index| projected.fatigue.people[index].fatigue);
                    CoreChoiceIntent {
                        actor: person.id,
                        key: Key {
                            domain: REST,
                            action: 1,
                        },
                        fatigue_value: Some(i64::from(fatigue)),
                        barrier: None,
                        written_form: None,
                        strike: None,
                        watch: None,
                        assembly: None,
                        cultivation: None,
                        mechanical_load: None,
                        movement: None,
                        second_movement: None,
                        work: None,
                        experiment: None,
                        experiment_value: None,
                        meal_value: None,
                        tool_damage: None,
                        health_value: None,
                    }
                })
                .collect();
            intents.par_extend(rest);
        }
        if domain.is_none_or(|domain| domain == CONFINEMENT) {
            intents.par_extend(self.barrier_choices(&busy).into_par_iter().map(
                |(actor, action)| CoreChoiceIntent {
                    actor,
                    key: Key {
                        domain: CONFINEMENT,
                        action: action.choice_key(),
                    },
                    barrier: Some(action),
                    written_form: None,
                    strike: None,
                    watch: None,
                    assembly: None,
                    cultivation: None,
                    mechanical_load: None,
                    movement: None,
                    second_movement: None,
                    work: None,
                    experiment: None,
                    experiment_value: None,
                    meal_value: None,
                    tool_damage: None,
                    health_value: None,
                    fatigue_value: None,
                },
            ));
        }
        if domain.is_none_or(|domain| domain == ASSIST)
            && self.shared_work.is_some()
            && let Some(people) = self
                .schedule
                .as_ref()
                .and_then(|schedule| schedule.personal_choices.as_deref())
        {
            let preferred = |id| {
                index_by_person(people, id, |row| row.id).is_some_and(|index| {
                    let profile = &people[index];
                    profile
                        .preferences
                        .binary_search_by_key(&ASSIST, |row| row.domain)
                        .ok()
                        .is_some_and(|row| profile.preferences[row].enabled)
                })
            };
            if people.par_iter().any(|profile| preferred(profile.id)) {
                let offers =
                    self.assistance_candidates(&busy, true, |person, _| preferred(person))?;
                intents.par_extend(offers.into_par_iter().map(|candidate| CoreChoiceIntent {
                    actor: candidate.person,
                    key: Key {
                        domain: ASSIST,
                        action: candidate.job,
                    },
                    fatigue_value: None,
                    barrier: None,
                    written_form: None,
                    strike: None,
                    watch: None,
                    assembly: None,
                    cultivation: None,
                    mechanical_load: None,
                    movement: None,
                    second_movement: None,
                    work: None,
                    experiment: None,
                    experiment_value: Some(1),
                    meal_value: None,
                    tool_damage: None,
                    health_value: None,
                }));
            }
        }
        if domain.is_none_or(|domain| domain == READ_TEXT)
            && let Some(owner) = &self.writing
            && let Some(people) = self
                .schedule
                .as_ref()
                .and_then(|schedule| schedule.personal_choices.as_deref())
        {
            let preferred = |id| {
                index_by_person(people, id, |row| row.id).is_some_and(|index| {
                    let profile = &people[index];
                    profile
                        .preferences
                        .binary_search_by_key(&READ_TEXT, |row| row.domain)
                        .ok()
                        .is_some_and(|row| profile.preferences[row].enabled)
                })
            };
            if people.par_iter().any(|row| preferred(row.id)) {
                let reads = owner.read_opportunities(
                    &self.core.people,
                    &busy,
                    &self.core.physical,
                    false,
                    preferred,
                );
                let round = self.core.decision_time_key();
                intents.par_extend(reads.par_chunk_by(|a, b| a.0 == b.0).filter_map(|group| {
                    let actor = group[0].0;
                    let index = index_by_person(people, actor, |row| row.id)?;
                    let profile = &people[index];
                    let key = |item| Key {
                        domain: READ_TEXT,
                        action: item,
                    };
                    let item = group
                        .iter()
                        .map(|row| row.1)
                        .filter_map(|item| {
                            let value = game_personal_choice::score(profile, key(item), 1)?;
                            Some((
                                value,
                                game_genetics::random(self.core.seed ^ actor, item, round),
                                item,
                            ))
                        })
                        .max()?
                        .2;
                    Some(CoreChoiceIntent {
                        actor,
                        key: key(item),
                        fatigue_value: None,
                        barrier: None,
                        written_form: None,
                        strike: None,
                        watch: None,
                        assembly: None,
                        cultivation: None,
                        mechanical_load: None,
                        movement: None,
                        second_movement: None,
                        work: None,
                        experiment: None,
                        experiment_value: Some(1),
                        meal_value: None,
                        tool_damage: None,
                        health_value: None,
                    })
                }));
            }
        }
        if intents
            .par_windows(2)
            .any(|rows| (rows[0].actor, rows[0].key) > (rows[1].actor, rows[1].key))
        {
            intents.par_sort_unstable_by_key(|intent| (intent.actor, intent.key));
        }
        Ok(intents)
    }

    pub(super) fn choose_local_actions(
        &self,
        blocked: &[u64],
        people: &[Person],
    ) -> Result<(Vec<CoreChoiceIntent>, Vec<CoreChoiceEvent>), String> {
        let candidates = self.choice_candidates(blocked, None)?;
        let busy = self.choice_busy(blocked);
        let available = |id: u64| {
            busy.binary_search(&id).is_err()
                && index_by_person(&self.core.people, id, |person| person.id).is_some_and(|i| {
                    self.core.people[i].health > 0 && !self.core.people[i].controlled
                })
        };
        let proposals: Vec<_> = candidates
            .par_iter()
            .filter(|intent| index_by_person(people, intent.actor, |p| p.id).is_some())
            .map(|intent| {
                let observed = intent.magnitude();
                Proposal {
                    actor: intent.actor,
                    key: intent.key,
                    value: observed,
                }
            })
            .collect();
        let round = self.core.decision_time_key();
        #[cfg(feature = "profile")]
        let select_span = game_diagnostics::span("choice.select");
        let selected = game_personal_choice::select(people, &proposals, self.core.seed, round)
            .map_err(str::to_owned)?;
        #[cfg(feature = "profile")]
        drop(select_span);
        let intents: Vec<_> = selected
            .par_iter()
            .filter_map(|row| row.action.map(|key| (row.actor, key)))
            .map(|key| {
                candidate_index(&candidates, key.0, key.1)
                    .ok_or(())
                    .map(|i| candidates[i].clone())
                    .map_err(|_| "selected opportunity disappeared".to_owned())
            })
            .collect::<Result<_, _>>()?;
        let tick = self.core.physical.tick();
        let offset_millis = self.core.elapsed_millis().ok_or("no physical interval")?;
        let idle = selected
            .par_iter()
            .filter(|row| row.action.is_none() && available(row.actor))
            .map(|row| CoreChoiceEvent {
                tick,
                offset_millis,
                actor: row.actor,
                key: None,
                outcome: CoreChoiceOutcome::NoLocalAction,
            })
            .collect();
        Ok((intents, idle))
    }

    /// Revalidate exact selected movements against the current state. A stale
    /// intention never turns into a different action or an implicit fallback.
    pub(super) fn choice_blocked(
        &self,
        blocked: &[u64],
        people: &[Person],
        intents: &[CoreChoiceIntent],
        domain: u32,
    ) -> Result<(Vec<u64>, Vec<CoreChoiceEvent>), String> {
        #[cfg(feature = "profile")]
        let _span = game_diagnostics::span("choice.revalidate");
        let current = self.choice_candidates(blocked, Some(domain))?;
        let checked: Vec<_> = intents
            .par_iter()
            .filter(|row| row.key.domain == domain)
            .map(|intent| {
                let found = candidate_index(&current, intent.actor, intent.key).is_some_and(|i| {
                    current[i].movement == intent.movement
                        && current[i].second_movement == intent.second_movement
                        && current[i].work == intent.work
                        && current[i].experiment == intent.experiment
                        && current[i].experiment_value == intent.experiment_value
                        && current[i].meal_value == intent.meal_value
                        && current[i].tool_damage == intent.tool_damage
                        && current[i].health_value == intent.health_value
                        && current[i].fatigue_value == intent.fatigue_value
                        && current[i].cultivation == intent.cultivation
                        && current[i].mechanical_load == intent.mechanical_load
                        && current[i].assembly == intent.assembly
                        && current[i].watch == intent.watch
                        && current[i].strike == intent.strike
                        && current[i].barrier == intent.barrier
                        && current[i].written_form == intent.written_form
                });
                (intent, found)
            })
            .collect();
        let mut valid: Vec<_> = checked
            .par_iter()
            .filter(|(_, found)| *found)
            .map(|(intent, _)| intent.actor)
            .collect();
        let tick = self.core.physical.tick();
        let offset_millis = self.core.elapsed_millis().ok_or("no physical interval")?;
        let stale = checked
            .par_iter()
            .filter(|(_, found)| !*found)
            .map(|(intent, _)| CoreChoiceEvent {
                tick,
                offset_millis,
                actor: intent.actor,
                key: Some(intent.key),
                outcome: CoreChoiceOutcome::Stale,
            })
            .collect();
        if valid.par_windows(2).any(|rows| rows[0] > rows[1]) {
            valid.par_sort_unstable();
        }
        let mut denied = blocked.to_vec();
        denied.par_extend(
            people
                .par_iter()
                .filter(|p| index_by_person(&valid, p.id, |id| *id).is_none())
                .map(|p| p.id),
        );
        // Legacy actors retain their existing cutoff during this mixed batch.
        // Only people who selected and revalidated a personal intention bypass it.
        if matches!(domain, CONSTRUCTION | CARRYING | MAINTENANCE) {
            denied.par_extend(
                self.core
                    .people
                    .par_iter()
                    .filter(|person| {
                        person.hunger > self.core.rules.max_work_hunger
                            && index_by_person(&valid, person.id, |id| *id).is_none()
                    })
                    .map(|person| person.id),
            );
        }
        denied.par_sort_unstable();
        denied.dedup();
        Ok((denied, stale))
    }
}

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
    pub actor: u64,
    pub key: Key,
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
}

impl CoreChoiceIntent {
    pub(super) fn magnitude(&self) -> i64 {
        if let Some(value) = self
            .experiment_value
            .or(self.meal_value)
            .or(self.health_value)
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
        let round = tick.rotate_left(21) ^ offset_millis;
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
        let outcomes = self.start_with_sites(&starts)?;
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
        if !matches!(schedule.phase, DecisionPhase::Meals) {
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

    fn choice_busy(&self, blocked: &[u64]) -> Vec<u64> {
        let rest = self.blocked_with_rest(blocked);
        let mut busy = rest.as_deref().unwrap_or(blocked).to_vec();
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
        let needs_view = needs_provision
            || (domain.is_none_or(|domain| domain == CONSTRUCTION)
                && self
                    .construction
                    .as_ref()
                    .is_some_and(|owner| !owner.policies.is_empty()))
            || (domain.is_none_or(|domain| domain == CARRYING)
                && self
                    .local_transport
                    .as_ref()
                    .is_some_and(|owner| !owner.policies.is_empty()));
        let view = if needs_view {
            Some(self.core.physical.view(tick)?)
        } else {
            None
        };
        let busy = self.choice_busy(blocked);
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let construction = || -> Result<Vec<CoreChoiceIntent>, String> {
            if domain.is_none_or(|domain| domain == CONSTRUCTION)
                && let Some(owner) = &self.construction
                && !owner.policies.is_empty()
            {
                #[cfg(feature = "profile")]
                let _span = game_diagnostics::span("choice.preview.construction");
                let sites = self
                    .sites
                    .as_ref()
                    .ok_or("construction requires work sites")?;
                let opportunities = game_economy_building::Context {
                    owner,
                    sites: &sites.facilities,
                    people: &self.core.people,
                    view: view.as_ref().expect("construction view"),
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy: &busy,
                    max_hunger: u32::MAX,
                    tick,
                    manual: None,
                    goal_needs: &[],
                    unwilling: &[],
                }
                .direct_opportunities();
                let rows: Vec<_> = opportunities
                    .into_par_iter()
                    .map(|opportunity| {
                        let retained = (opportunity.salvage > 0).then_some(game_lots::Movement {
                            id: 0,
                            from: opportunity.source,
                            to: Some(game_lots::Place::Site(0)),
                            good: opportunity.good,
                            quantity: opportunity.salvage,
                            item: None,
                        });
                        let consumed = (opportunity.salvage < opportunity.quantity).then(|| {
                            game_lots::Movement {
                                id: 0,
                                from: opportunity.source,
                                to: None,
                                good: opportunity.good,
                                quantity: opportunity.quantity - opportunity.salvage,
                                item: None,
                            }
                        });
                        CoreChoiceIntent {
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
                && !owner.policies.is_empty()
            {
                #[cfg(feature = "profile")]
                let _span = game_diagnostics::span("choice.preview.carrying");
                let opportunities = game_economy_local_transport::Context {
                    owner,
                    people: &self.core.people,
                    view: view.as_ref().expect("carrying view"),
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy: &busy,
                    max_hunger: u32::MAX,
                    manual: None,
                    walking: None,
                }
                .direct_opportunities();
                let rows: Vec<_> = opportunities
                    .into_par_iter()
                    .map(|opportunity| CoreChoiceIntent {
                        actor: opportunity.person,
                        key: Key {
                            domain: CARRYING,
                            action: u64::from(opportunity.good),
                        },
                        movement: Some(game_lots::Movement {
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
                    actor: opportunity.person,
                    key: Key {
                        domain: MAINTENANCE,
                        action: opportunity.site,
                    },
                    movement: Some(game_lots::Movement {
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
                    .preview_autonomous_work(&busy, schedule.work_rate)?
                    .into_par_iter()
                    .map(|opportunity| CoreChoiceIntent {
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
                Ok(self
                    .preview_mechanical_experiments(&busy)?
                    .into_par_iter()
                    .map(|policy| CoreChoiceIntent {
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
                Ok(game_economy_person::meal::opportunities_now(
                    &self.core.people,
                    self.core.physical.goods(),
                    &self.core.physical,
                    &busy,
                )?
                .into_par_iter()
                .map(|opportunity| {
                    let person = index_by_person(&self.core.people, opportunity.person, |p| p.id)
                        .expect("known person");
                    CoreChoiceIntent {
                        actor: opportunity.person,
                        key: Key {
                            domain: MEAL,
                            action: u64::from(opportunity.good),
                        },
                        movement: Some(game_lots::Movement {
                            id: 0,
                            from: game_lots::Place::Person(opportunity.person),
                            to: None,
                            good: opportunity.good,
                            quantity: opportunity.portion,
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
                &busy,
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
                    actor: opportunity.person,
                    key: Key {
                        domain: FOOD_PICKUP,
                        action: u64::from(opportunity.good),
                    },
                    movement: Some(game_lots::Movement {
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
                actor: opportunity.person,
                key: Key {
                    domain: SELF_TREATMENT,
                    action: u64::from(opportunity.medicine),
                },
                movement: Some(game_lots::Movement {
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
        let round = self.core.physical.tick().rotate_left(21)
            ^ self.core.elapsed_millis().ok_or("no physical interval")?;
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

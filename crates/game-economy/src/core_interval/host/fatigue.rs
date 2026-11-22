//! Accumulate actual elapsed activity for boundary-weighted fatigue.
use super::*;

#[derive(Default)]
pub(super) struct ElapsedProgress {
    pub millis: u64,
    pub interrupted_preparation: bool,
    pub activity: Option<ActivityFrame>,
    pub incapacitations: Vec<scheduler::Incapacitation>,
    pub family_events: Vec<game_economy_family::FamilyEvent>,
    pub fauna: Option<CoreFaunaTransitions>,
    pub wild_plants: Option<game_economy_agriculture::wild_plants::Cycle>,
    pub agreement_expiries: Vec<game_agreements::Event>,
    pub pastimes: super::pastimes::SettledPastimes,
    pub disease_events: Vec<game_disease::Event>,
    pub local_arrivals: Vec<game_economy_local_transport::walking::Event>,
}
use game_economy_fatigue::{ElapsedActivity, TimedActivity};

pub(super) struct ActivityFrame {
    activity: Vec<ElapsedActivity>,
    people: std::collections::HashSet<u64>,
}

struct ElapsedLimits {
    limit: u64,
    recovery: Option<u64>,
    immortal_recovery: Option<u128>,
    exposure: Option<u64>,
    natural_death: Option<u64>,
    starvation: Option<u64>,
    purpose: Option<u64>,
    delivery: Option<u64>,
    conception: Option<u64>,
    disease: Option<u64>,
    nutrition: Option<u64>,
    food_health: Option<u128>,
    fauna: Option<u64>,
    wild_plants: Option<u64>,
    fear_relaxation: Option<u64>,
    agreement_expiry: Option<u64>,
}

impl From<PreparedDeadline> for ElapsedLimits {
    fn from(plan: PreparedDeadline) -> Self {
        let deadline = plan.deadline;
        let at = |due: bool| due.then_some(deadline.after_millis);
        Self {
            limit: deadline.after_millis,
            recovery: at(plan.ordinary_recovery),
            immortal_recovery: at(deadline.recovery).map(u128::from),
            exposure: at(deadline.exposure),
            natural_death: at(plan.natural_death),
            starvation: at(deadline.death),
            purpose: at(deadline.purpose),
            delivery: at(deadline.delivery),
            conception: at(deadline.conception),
            disease: at(deadline.disease),
            nutrition: at(deadline.nutrition),
            food_health: at(deadline.food_health).map(u128::from),
            fauna: at(deadline.fauna),
            wild_plants: at(deadline.wild_plants),
            fear_relaxation: at(deadline.fear_relaxation),
            agreement_expiry: at(deadline.agreement_expiry),
        }
    }
}

impl ElapsedLimits {
    fn for_unscheduled(host: &CoreHost, requested: u64) -> Result<Self, String> {
        let recovery = host.next_ordinary_recovery();
        let immortal_recovery =
            game_economy_person::next_recovery(&host.core.people).map_err(str::to_owned)?;
        let exposure = host.next_environment_exposure();
        let natural_death = host
            .next_natural_death()
            .and_then(|after| u64::try_from(after).ok());
        let purpose = host.next_purpose();
        let delivery = host
            .next_delivery()
            .and_then(|after| u64::try_from(after).ok());
        let conception = host
            .next_conception()
            .and_then(|after| u64::try_from(after).ok());
        let disease = host.next_disease_deadline();
        let fauna = host.next_fauna_lifecycle();
        let wild_plants = host.next_wild_plant_cycle();
        let fear_relaxation = host.next_fear_relaxation();
        let agreement_expiry = host.next_agreement_expiry();
        let mut limit = host.core.progress_limit(requested)?;
        limit = [
            recovery,
            exposure,
            natural_death,
            purpose,
            delivery,
            conception,
            host.next_dry_field(),
            disease,
            host.next_local_arrival(),
            host.next_social_encounter(),
            host.next_storage_temperature(),
            fauna,
            wild_plants,
            fear_relaxation,
            agreement_expiry,
            host.next_pastime_finish(),
        ]
        .into_iter()
        .flatten()
        .fold(limit, u64::min);
        Ok(Self {
            limit,
            recovery,
            immortal_recovery,
            exposure,
            natural_death,
            starvation: None,
            purpose,
            delivery,
            conception,
            disease,
            nutrition: host.core.next_metabolism(),
            food_health: host.core.next_food_health(),
            fauna,
            wild_plants,
            fear_relaxation,
            agreement_expiry,
        })
    }
}

impl CoreHost {
    /// Apply already selected, individual rest choices after checking the actors.
    pub fn choose_personal_rest(
        &mut self,
        choices: &[game_economy_fatigue::RestChoice],
        blocked: &[u64],
    ) -> Result<Vec<CoreChoiceEvent>, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1])
            || choices
                .par_windows(2)
                .any(|rows| rows[0].person >= rows[1].person)
        {
            return Err("invalid personal rest choices".into());
        }
        let owner = self.fatigue.as_ref().ok_or("fatigue is disabled")?;
        let resting = owner.fatigue.resting();
        let tick = self.core.physical.tick();
        let offset_millis = self.core.elapsed_millis().ok_or("no physical interval")?;
        if choices.par_iter().any(|choice| {
            let current = resting.binary_search(&choice.person).is_ok();
            current == choice.resting
                || blocked.binary_search(&choice.person).is_ok()
                || self
                    .core
                    .people
                    .binary_search_by_key(&choice.person, |person| person.id)
                    .ok()
                    .is_none_or(|index| {
                        let person = &self.core.people[index];
                        person.controlled || person.health == 0 || person.region.is_none()
                    })
        }) {
            return Err("person cannot choose that rest state".into());
        }
        let people: std::collections::HashSet<_> = self
            .core
            .people
            .par_iter()
            .map(|person| person.id)
            .collect();
        let staged = owner.fatigue.choose_rest(choices, &people)?;
        let events = choices
            .par_iter()
            .map(|choice| CoreChoiceEvent {
                tick,
                offset_millis,
                actor: choice.person,
                key: Some(game_personal_choice::Key {
                    domain: if choice.resting {
                        super::personal_choice::REST
                    } else {
                        super::personal_choice::RESUME
                    },
                    action: 1,
                }),
                outcome: CoreChoiceOutcome::Accepted,
            })
            .collect();
        self.fatigue.as_mut().expect("checked fatigue").fatigue = staged;
        if !choices.is_empty() {
            self.record_episode_rest(choices.iter().map(|choice| (choice.person, choice.resting)));
            self.work_checked_revision = None;
        }
        Ok(events)
    }

    pub(super) fn stage_fatigue(
        &self,
        people: &mut [Person],
    ) -> Result<Option<game_economy_fatigue::TimedFatigue>, String> {
        let Some(owner) = &self.fatigue else {
            return Ok(None);
        };
        if owner.elapsed_damage_remainders.is_some() {
            return Ok(Some(owner.clone()));
        }
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
        self.record_episode_rest([(person, resting)]);
        self.work_checked_revision = None;
        Ok(())
    }

    fn elapsed_activity_frame(
        &self,
        owner: &game_economy_fatigue::ElapsedFatigueView<'_>,
        externally_busy: &[u64],
    ) -> Result<ActivityFrame, String> {
        let resting = owner.resting();
        let mut working: Vec<_> = self
            .core
            .physical
            .work_interval()
            .ok_or("no physical interval")?
            .rates
            .par_iter()
            .map(|&(job, _)| {
                self.core
                    .physical
                    .work()
                    .binary_search_by_key(&job, |row| row.job.id)
                    .map(|index| self.core.physical.work()[index].job.worker)
                    .map_err(|_| "missing activity job".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(shared) = &self.shared_work {
            working.par_extend(
                shared
                    .sessions
                    .par_iter()
                    .map(|session| session.event.person),
            );
        }
        working.par_sort_unstable();
        working.dedup();
        let mut traveling: Vec<_> = self.core.travel.as_ref().map_or_else(Vec::new, |travel| {
            travel
                .journeys
                .par_iter()
                .map(|journey| journey.person)
                .collect()
        });
        traveling.par_extend(self.local_travelers());
        traveling.par_sort_unstable();
        let activity: Vec<_> = self
            .core
            .people
            .par_iter()
            .map(|person| {
                use game_economy_fatigue::Exertion;
                let work = working.binary_search(&person.id).is_ok();
                let travel = traveling.binary_search(&person.id).is_ok();
                if person.health > 0 && work && travel {
                    return Err("overlapping work and travel activity".to_string());
                }
                let exertion = if person.health == 0 {
                    Exertion::Inactive
                } else if work {
                    Exertion::Work
                } else if travel {
                    Exertion::Travel
                } else if person.region.is_none()
                    || externally_busy.binary_search(&person.id).is_ok()
                {
                    Exertion::Other
                } else if resting.binary_search(&person.id).is_ok() {
                    Exertion::Rest
                } else {
                    Exertion::Idle
                };
                Ok(game_economy_fatigue::ElapsedActivity {
                    id: person.id,
                    exertion,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let activity = owner.relevant_activity(&activity);
        let people = activity.par_iter().map(|row| row.id).collect();
        Ok(ActivityFrame { activity, people })
    }

    pub(super) fn progress_with_activity(
        &mut self,
        requested: u64,
        externally_busy: &[u64],
        prepared: Option<PreparedDeadline>,
        mut activity: Option<ActivityFrame>,
        quiet_observation: bool,
    ) -> Result<ElapsedProgress, String> {
        let quantum = self.core.physical.tick_millis();
        let scheduled = prepared.is_some();
        if !scheduled {
            self.expire_due_agreements();
        }
        let limits = match prepared {
            Some(plan) => ElapsedLimits::from(plan),
            None => ElapsedLimits::for_unscheduled(self, requested)?,
        };
        let mut consumed = requested.min(limits.limit);
        let (crop_rates, crop_limit) = self.plan_crop_segment(consumed)?;
        consumed = crop_limit;
        let elapsed_owner = self.fatigue.as_ref().and_then(|owner| owner.elapsed_view());
        let mut elapsed_draft = None;
        if let Some(owner) = elapsed_owner {
            // Each elapsed segment ends before an activity transition is published.
            if !scheduled {
                consumed = consumed.min(self.core.next_deadline(None)?.after_millis);
            }
            if consumed == 0 {
                return Ok(ElapsedProgress::default());
            }
            if activity.is_none() {
                activity = Some(self.elapsed_activity_frame(&owner, externally_busy)?);
            }
            let frame = activity.as_ref().expect("prepared elapsed activity");
            #[cfg(feature = "profile")]
            let _phase = game_diagnostics::span("elapsed.progress.fatigue");
            #[cfg(feature = "profile")]
            game_diagnostics::count("elapsed.rows.fatigue", frame.activity.len() as u64);
            let (millis, draft) =
                owner.prepare_segment(&frame.activity, consumed, &frame.people)?;
            consumed = millis;
            elapsed_draft = Some(draft);
        }
        let ordinary_recovery_due = limits.recovery == Some(consumed);
        let recovery_due = ordinary_recovery_due
            || limits
                .immortal_recovery
                .is_some_and(|after| after <= u128::from(consumed));
        let injuries = elapsed_draft
            .as_ref()
            .map_or(&[][..], |draft| draft.consequences.as_slice());
        let has_injuries = !injuries.is_empty();
        let (growth, batches) = self.stage_crop_growth(crop_rates, consumed)?;
        let quiet_observation = quiet_observation && consumed == requested && !has_injuries;
        if !quiet_observation {
            self.core.materialize_vitals();
        }
        let exposure_due = limits.exposure == Some(consumed);
        let natural_death_due = limits.natural_death == Some(consumed);
        let purpose_due = limits.purpose == Some(consumed);
        let delivery_due = limits.delivery == Some(consumed);
        let conception_due = limits.conception == Some(consumed);
        let disease_due = limits.disease == Some(consumed);
        let food_health_due = limits.food_health == Some(u128::from(consumed));
        let mut vitals = (has_injuries
            || exposure_due
            || natural_death_due
            || purpose_due
            || delivery_due
            || conception_due
            || disease_due
            || food_health_due
            || ordinary_recovery_due
            || limits.nutrition == Some(consumed))
        .then(|| self.core.preview_people_progress(consumed, injuries))
        .transpose()?;
        if exposure_due {
            let tick = self.core.physical.tick();
            let view = self.core.physical.view(tick)?;
            let sample = self.core.simulated_millis() + u128::from(consumed) - 1;
            crate::environment::apply(
                self.environment.as_ref(),
                &view,
                self.core.travel.as_ref(),
                &mut vitals.as_mut().expect("staged exposure").people,
                tick,
                Some(sample),
            );
        }
        let event_time = self.core.simulated_millis() + u128::from(consumed);
        let staged_health = if disease_due {
            self.stage_elapsed_disease(
                &mut vitals.as_mut().expect("disease vitals").people,
                event_time,
            )
        } else {
            None
        };
        let natural_deaths: Vec<_> = if natural_death_due {
            let now = self.core.simulated_millis() + u128::from(consumed);
            let due = self
                .family
                .as_ref()
                .unwrap()
                .elapsed_lifetimes
                .as_ref()
                .unwrap()
                .deaths_at(now);
            let people = &mut vitals.as_mut().expect("staged natural mortality").people;
            people
                .par_iter_mut()
                .filter_map(|person| {
                    if person.health > 0
                        && due.binary_search_by_key(&person.id, |row| row.1).is_ok()
                    {
                        person.health = 0;
                        Some(person.id)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        let purpose = if purpose_due {
            let elapsed = self.core.elapsed_millis().expect("active interval");
            let tick = self
                .core
                .physical
                .tick()
                .checked_add(u64::from(elapsed + consumed == quantum))
                .ok_or("purpose eligibility tick overflow")?;
            Some(self.stage_elapsed_purpose(
                &mut vitals.as_mut().expect("staged elapsed purpose").people,
                tick,
            ))
        } else {
            None
        };
        let delivery = if delivery_due || conception_due {
            self.stage_elapsed_family(
                &vitals.as_ref().expect("delivery vitals").people,
                event_time,
                purpose.as_ref(),
                staged_health.as_ref(),
                delivery_due,
                conception_due,
            )?
        } else {
            None
        };
        let mut disease_events: Vec<_> = staged_health.as_ref().map_or_else(Vec::new, |health| {
            health.owner.events.par_iter().cloned().collect()
        });
        if let Some(delivery) = &delivery {
            disease_events.par_extend(delivery.birth_disease_events(event_time));
        }
        let deaths = if let Some(vitals) = &vitals {
            self.core
                .people
                .par_iter()
                .zip(vitals.people.par_iter())
                .filter_map(|(before, after)| {
                    (before.health > 0 && after.health == 0).then_some(before.id)
                })
                .collect()
        } else if !scheduled || limits.starvation == Some(consumed) {
            game_economy_person::starvation_deaths_within(
                &self.core.people,
                &self.core.rules,
                quantum,
                consumed,
            )
            .map_err(str::to_owned)?
        } else {
            Vec::new()
        };
        let incapacitations = deaths
            .par_iter()
            .map(|&person| {
                let index = self
                    .core
                    .people
                    .binary_search_by_key(&person, |row| row.id)
                    .expect("known elapsed casualty");
                scheduler::Incapacitation {
                    person,
                    died: self.core.people[index].immortal.is_none(),
                    natural_mortality: natural_deaths.binary_search(&person).is_ok(),
                }
            })
            .collect();
        let mut pending = self.pending_vital_deaths.clone();
        if let Some(family) = &self.family {
            pending.par_extend(deaths.par_iter().copied().filter(|id| {
                family
                    .members
                    .binary_search_by_key(id, |member| member.id)
                    .ok()
                    .is_some_and(|index| family.members[index].alive)
            }));
            pending.par_sort_unstable();
            pending.dedup();
        }
        let durations = self.assist_durations(consumed)?;
        let (reductions, staged_shared) =
            self.stage_fatal_assistance(&deaths, &durations, consumed)?;
        let paused_after_deaths = self.pause_incapacitated_rates(&deaths, &reductions, consumed)?;
        let renewal = self.stage_resource_renewal(consumed)?;
        let wild_due = limits.wild_plants == Some(consumed);
        let fauna_due = limits.fauna == Some(consumed);
        let mut ecological = (wild_due || fauna_due)
            .then(|| self.resources_for_ecology(renewal.as_ref())).flatten();
        let wild = wild_due.then(|| self.stage_wild_plants(event_time, &mut ecological)).transpose()?;
        let (fauna, fauna_transitions) = if fauna_due {
            let (draft, transitions) = self.stage_elapsed_fauna(&mut ecological, event_time)?;
            (Some(draft), Some(transitions))
        } else {
            (None, None)
        };
        let local_progress = self.stage_local_progress(consumed);
        let soil = self.agriculture.as_ref().and_then(|farm| {
            farm.soil.as_ref()?.prepare_recovery(&farm.fields, consumed)
        });
        let interrupted_preparation = consumed != requested
            || has_injuries
            || recovery_due
            || batches.is_some()
            || limits.agreement_expiry == Some(consumed);
        let moisture = self
            .irrigation
            .as_ref()
            .and_then(|owner| owner.stage_evaporation(consumed));
        let Some(owner) = &self.fatigue else {
            let credits = self.plan_skill_practice(consumed);
            let recovery = self.stage_natural_recovery(consumed, &[], vitals.as_mut())?;
            let advanced = self.core.progress_checked_segment(
                consumed,
                &reductions,
                vitals,
                batches,
                delivery.as_ref().map(|delivery| delivery.physical()),
            quiet_observation,
            )?;
            let mut family_events = Vec::new();
            let mut local_arrivals = Vec::new();
            let mut wild_plants = None;
            if advanced > 0 {
                local_arrivals = self.publish_local_progress(local_progress);
                self.publish_resources(renewal, ecological);
                wild_plants = self.publish_wild_plants(wild);
                self.publish_elapsed_fauna(fauna, fauna_transitions.as_ref());
                self.relax_due_fear(limits.fear_relaxation == Some(advanced));
                self.publish_crop_growth(growth);
                self.publish_soil_recovery(soil);
                if let Some(moisture) = moisture {
                    self.irrigation
                        .as_mut()
                        .expect("staged irrigation")
                        .moisture = moisture;
                }
                if let Some(recovery) = recovery {
                    self.recovery = Some(recovery);
                }
                if let Some(purpose) = purpose {
                    self.purpose = Some(purpose);
                }
                self.pending_vital_deaths = pending;
                if let Some(paused) = paused_after_deaths {
                    self.paused_work_rates = paused;
                }
                if !deaths.is_empty()
                    || recovery_due
                    || exposure_due
                    || purpose_due
                    || food_health_due
                    || delivery_due
                    || disease_due
                {
                    self.work_checked_revision = None;
                }
                if let Some(owner) = staged_shared {
                    self.shared_work = Some(owner);
                } else {
                    self.record_assist_durations(&durations);
                }
                self.remember_skill_practice(credits);
                if let Some(health) = staged_health {
                    self.health = Some(health);
                }
                if let Some(delivery) = delivery {
                    family_events = self.publish_elapsed_family(delivery);
                }
                if !deaths.is_empty() {
                    self.remove_deceased_holds();
                }
            }
            let agreement_expiries = if limits.agreement_expiry == Some(advanced) {
                self.expire_due_agreements()
            } else {
                Vec::new()
            };
            return Ok(ElapsedProgress {
                millis: advanced,
                pastimes: self.settle_pastimes(externally_busy),
                agreement_expiries,
                interrupted_preparation,
                activity,
                incapacitations,
                family_events,
                fauna: fauna_transitions,
                wild_plants,
                disease_events,
                local_arrivals,
            });
        };
        if consumed == 0 {
            return Ok(ElapsedProgress::default());
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
                    .completion_after(rate, self.core.physical.work_rate_period_millis())
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
                        .arrival_after(travel.unit_millis(quantum))
                        .map(|after| (journey.person, after.min(u128::from(consumed)) as u64))
                        .map_err(str::to_owned)
                })
                .collect::<Result<Vec<_>, _>>()
        })?;
        travel.par_extend(self.local_travelers().map(|person| (person, consumed)));
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
        let next: Vec<_> = chunks.into_par_iter().flatten().collect();
        let recovery = self.stage_natural_recovery(consumed, &next, vitals.as_mut())?;
        let advanced = self.core.progress_checked_segment(
            consumed,
            &reductions,
            vitals,
            batches,
            delivery.as_ref().map(|delivery| delivery.physical()),
            quiet_observation,
        )?;
        self.publish_resources(renewal, ecological);
        let wild_plants = self.publish_wild_plants(wild);
        self.publish_elapsed_fauna(fauna, fauna_transitions.as_ref());
        self.relax_due_fear(limits.fear_relaxation == Some(advanced));
        let local_arrivals = self.publish_local_progress(local_progress);
        self.publish_crop_growth(growth);
        self.publish_soil_recovery(soil);
        if let Some(moisture) = moisture {
            self.irrigation
                .as_mut()
                .expect("staged irrigation")
                .moisture = moisture;
        }
        if let Some(recovery) = recovery {
            self.recovery = Some(recovery);
        }
        if let Some(purpose) = purpose {
            self.purpose = Some(purpose);
        }
        if let Some(draft) = elapsed_draft {
            self.fatigue
                .as_mut()
                .expect("checked fatigue")
                .set_elapsed_owner(draft.state);
        }
        self.pending_vital_deaths = pending;
        if let Some(paused) = paused_after_deaths {
            self.paused_work_rates = paused;
        }
        if !deaths.is_empty()
            || recovery_due
            || has_injuries
            || exposure_due
            || purpose_due
            || food_health_due
            || delivery_due
            || disease_due
        {
            self.work_checked_revision = None;
        }
        self.activity = next;
        if let Some(health) = staged_health {
            self.health = Some(health);
        }
        let family_events =
            delivery.map_or_else(Vec::new, |delivery| self.publish_elapsed_family(delivery));
        if !deaths.is_empty() {
            self.remove_deceased_holds();
        }
        if let Some(owner) = staged_shared {
            self.shared_work = Some(owner);
        } else {
            self.record_assist_durations(&durations);
        }
        self.remember_skill_practice(skill_practice);
        let agreement_expiries = if limits.agreement_expiry == Some(advanced) {
            self.expire_due_agreements()
        } else {
            Vec::new()
        };
        Ok(ElapsedProgress {
            millis: advanced,
            pastimes: self.settle_pastimes(externally_busy),
            agreement_expiries,
            interrupted_preparation,
            activity,
            incapacitations,
            family_events,
            fauna: fauna_transitions,
            wild_plants,
            disease_events,
            local_arrivals,
        })
    }

    fn relax_due_fear(&mut self, due: bool) {
        if due && let Some(opinions) = &mut self.opinions {
            opinions.relax_elapsed_fear();
        }
    }

    /// Use actual rested time in this segment, including work interruptions.
    fn stage_natural_recovery(
        &self,
        millis: u64,
        next_activity: &[TimedActivity],
        vitals: Option<&mut VitalsProgress>,
    ) -> Result<Option<crate::RecoveryRules>, String> {
        let Some(owner) = self
            .recovery
            .as_ref()
            .filter(|owner| owner.elapsed.is_some() && millis > 0)
        else {
            return Ok(None);
        };
        let rest: Vec<_> = self
            .activity
            .par_iter()
            .zip(next_activity.par_iter())
            .filter_map(|(old, next)| {
                let delta = next.rest_millis - old.rest_millis;
                (delta > 0).then_some((next.id, delta))
            })
            .collect();
        owner
            .stage_elapsed(
                self.core.simulated_millis(),
                millis,
                &rest,
                vitals.map(|draft| (draft.people.as_mut_slice(), self.core.rules.max_health)),
            )
            .map(Some)
    }

    /// Remember the chosen rate of unfinished jobs while their lead is unable
    /// to work, excluding assistance that ended with the incapacitation.
    fn pause_incapacitated_rates(
        &self,
        deaths: &[u64],
        reductions: &[(u64, u64)],
        consumed: u64,
    ) -> Result<Option<Vec<(u64, u64)>>, String> {
        if deaths.is_empty() {
            return Ok(None);
        }
        let interval = self
            .core
            .physical
            .work_interval()
            .ok_or("no physical interval")?;
        let added: Vec<_> = interval
            .rates
            .par_iter()
            .map(|&(job, rate)| {
                let index = self
                    .core
                    .physical
                    .work()
                    .binary_search_by_key(&job, |row| row.job.id)
                    .map_err(|_| "missing activity job")?;
                let work = &self.core.physical.work()[index];
                if deaths.binary_search(&work.job.worker).is_err()
                    || self
                        .core
                        .people
                        .binary_search_by_key(&work.job.worker, |person| person.id)
                        .ok()
                        .is_none_or(|index| self.core.people[index].immortal.is_none())
                    || work
                        .completion_after(rate, self.core.physical.work_rate_period_millis())
                        .map_err(str::to_owned)?
                        .is_some_and(|after| after <= u128::from(consumed))
                {
                    return Ok(None);
                }
                let reduction = reductions
                    .binary_search_by_key(&job, |row| row.0)
                    .ok()
                    .map_or(0, |index| reductions[index].1);
                let rate = rate
                    .checked_sub(reduction)
                    .ok_or("invalid fatal work reduction")?;
                if rate == 0 {
                    return Err("invalid paused work rate".to_string());
                }
                Ok(Some((job, rate)))
            })
            .collect::<Result<Vec<_>, String>>()?
            .into_par_iter()
            .flatten()
            .collect();
        if added.is_empty() {
            return Ok(None);
        }
        let mut paused = self.paused_work_rates.clone();
        paused.extend(added);
        paused.par_sort_unstable_by_key(|row| row.0);
        if paused.par_windows(2).any(|rows| rows[0].0 == rows[1].0) {
            return Err("duplicate paused work rate".into());
        }
        Ok(Some(paused))
    }
}

fn game_fatigue_person(id: u64, resting: bool) -> game_economy_fatigue::FatiguePerson {
    game_economy_fatigue::FatiguePerson {
        id,
        fatigue: 0,
        resting,
    }
}

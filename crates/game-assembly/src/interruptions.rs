//! Admission for a linked set of interrupted activities. Native owner changes
//! share the caller's batch; metadata and public events retain their old slots.
use crate::commands::ActivityOwner;
use crate::*;
use game_ecs::{Batch, StableId};
use game_lifecycle::Effect as LifeEffect;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Removal {
    ArmyTalk(armies::Talk),
    ArmyApproach(armies::Approach),
    Social(PersonId, social::Interaction),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct MetadataStep {
    actor: PersonId,
    removals: Vec<Removal>,
    physical_due: BTreeSet<PersonId>,
    report_due: BTreeSet<(PersonId, SimTime)>,
    pregnancy_due: Vec<PregnancyId>,
    pregnancies: Vec<PersonId>,
    care: Option<PersonId>,
    events: Vec<EventKind>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct MetadataPlan {
    steps: Vec<MetadataStep>,
    opening: BTreeMap<PersonId, Option<Activity>>,
    due: Vec<(SimTime, Vec<Due>)>,
    activities: BTreeMap<PersonId, Activity>,
}
struct LiftActivityStamps(BTreeMap<PersonId, Option<(PersonId, SimTime)>>);
impl game_transport::LiftActionFacts for LiftActivityStamps {
    fn lifting(&self, actor: PersonId) -> Option<(PersonId, SimTime)> {
        self.0.get(&actor).copied().flatten()
    }
}
impl MetadataPlan {
    fn current(&mut self, sim: &Simulation, actor: PersonId) -> Option<Activity> {
        let opening = self
            .opening
            .entry(actor)
            .or_insert_with(|| sim.host.storage.activity(actor).cloned());
        self.activities
            .get(&actor)
            .cloned()
            .or_else(|| opening.clone())
    }
    fn collect(
        sim: &Simulation,
        actors: &[PersonId],
        health: &BTreeMap<PersonId, &[game_health::HealthEffect]>,
        property: &BTreeMap<PersonId, &[game_property::Effect]>,
        life: &BTreeMap<PersonId, &[LifeEffect]>,
        missions: &BTreeMap<PersonId, &[game_missions::ClearTalk]>,
        preserve_transport: bool,
    ) -> Self {
        let mut plan = Self {
            steps: Vec::new(),
            opening: BTreeMap::new(),
            due: Vec::new(),
            activities: BTreeMap::new(),
        };
        let mut removed = BTreeSet::new();
        for actor in actors.iter().copied() {
            let mut step = MetadataStep {
                actor,
                removals: Vec::new(),
                physical_due: BTreeSet::new(),
                report_due: BTreeSet::new(),
                pregnancy_due: Vec::new(),
                pregnancies: Vec::new(),
                care: None,
                events: Vec::new(),
            };
            if sim.host.state.armies.is_some() {
                let cancellation = game_armies::prepare_interruption_rows(&sim.host.storage, actor);
                for talk in cancellation.talks {
                    if !removed.insert((0, talk.speaker.0)) {
                        continue;
                    }
                    let stamp = match plan.current(sim, talk.speaker) {
                        Some(Activity::ArmyTalk { completes, .. }) => {
                            game_armies::InterruptionActivity::Talk { completes }
                        }
                        _ => game_armies::InterruptionActivity::Other,
                    };
                    if let Some(id) = game_armies::ArmyInterruption::talk_reset(&talk, stamp) {
                        plan.activities.insert(id, Activity::Idle);
                    }
                    step.removals.push(Removal::ArmyTalk(talk));
                }
                if let Some(approach) = cancellation.approach {
                    let stamp = match plan.current(sim, actor) {
                        Some(Activity::ArmyApproach { .. }) => {
                            game_armies::InterruptionActivity::Approach
                        }
                        _ => game_armies::InterruptionActivity::Other,
                    };
                    if let Some(id) =
                        game_armies::ArmyInterruption::approach_reset(&approach, stamp)
                    {
                        plan.activities.insert(id, Activity::Idle);
                    }
                    step.removals.push(Removal::ArmyApproach(approach));
                }
            }
            if let Some(w) = &sim.host.state.realm {
                let cancellation = w.prepare_cancel_talks(&sim.host.storage, &[actor]);
                let resets = cancellation.activity_resets(|speaker| {
                    matches!(plan.current(sim, speaker), Some(Activity::RealmTalk { .. }))
                });
                for speaker in cancellation.speakers() {
                    if !removed.insert((1, speaker.0)) {
                        continue;
                    }
                    if resets.contains(&speaker) {
                        plan.activities.insert(speaker, Activity::Idle);
                        step.physical_due.insert(speaker);
                    }
                    step.events.push(EventKind::Interrupted { actor: speaker });
                }
            }

            if !preserve_transport && let Some(runtime) = &sim.host.state.transport {
                let cancellation = runtime.prepare_cancel_lifts(&sim.host.storage, &[actor]);
                for lift in cancellation.result() {
                    if !removed.insert((2, lift.passenger.0)) {
                        continue;
                    }
                    let stamps = LiftActivityStamps(
                        lift.bearers
                            .iter()
                            .map(|bearer| {
                                let stamp = match plan.current(sim, *bearer) {
                                    Some(Activity::Lift {
                                        passenger,
                                        completes,
                                        ..
                                    }) => Some((passenger, completes)),
                                    _ => None,
                                };
                                (*bearer, stamp)
                            })
                            .collect(),
                    );
                    for bearer in
                        game_transport::prepare_lift_reactions(std::slice::from_ref(lift), &stamps)
                            .resets()
                    {
                        plan.activities.insert(bearer, Activity::Idle);
                        step.physical_due.insert(bearer);
                    }
                }
            }
            if sim.host.state.social.is_some()
                && let Some((key, interaction)) =
                    game_characters::social::cancellation_interaction(&sim.host.storage, actor)
                && removed.insert((3, key.0))
            {
                for id in interaction.actors() {
                    let kind = match plan.current(sim, id) {
                        Some(Activity::Social { .. }) => {
                            game_characters::social::SocialActivityKind::Conversation
                        }
                        Some(Activity::Instruction { .. }) => {
                            game_characters::social::SocialActivityKind::Instruction
                        }
                        Some(Activity::Practice { .. }) => {
                            game_characters::social::SocialActivityKind::Practice
                        }
                        _ => game_characters::social::SocialActivityKind::Other,
                    };
                    if let Some(game_characters::social::ActivityCleanup::Idle { actor }) =
                        game_characters::social::cleanup_activity(id, kind)
                    {
                        plan.activities.insert(actor, Activity::Idle);
                    }
                    step.physical_due.insert(id);
                }
                step.removals
                    .push(Removal::Social(key, interaction.clone()));
            }
            if let Some(effects) = health.get(&actor) {
                for id in game_health::cancellation_activity_resets(effects, |id| {
                    match plan.current(sim, id) {
                        Some(Activity::HealthWork { completes, .. }) => {
                            game_health::HealthActivityState::Working { completes }
                        }
                        _ => game_health::HealthActivityState::Other,
                    }
                }) {
                    plan.activities.insert(id, Activity::Idle);
                }
            }
            for talk in missions
                .get(&actor)
                .into_iter()
                .flat_map(|effects| effects.iter())
            {
                let deadline = match plan.current(sim, talk.speaker) {
                    Some(Activity::MissionTalk { completes }) => Some(completes),
                    _ => None,
                };
                if let Some(id) = talk.activity_reset(deadline) {
                    plan.activities.insert(id, Activity::Idle);
                }
            }
            if let Some(runtime) = &sim.host.state.invocation {
                let ending = runtime.prepare_memory_ending(&sim.host.storage, &[actor]);
                for id in ending.activity_resets(&invocation::MemoryBindings(sim)) {
                    plan.activities.insert(id, Activity::Idle);
                }
            }
            let partners = match plan.current(sim, actor) {
                Some(Activity::Lesson {
                    teacher, student, ..
                }) => Some((teacher, student)),
                _ => None,
            };
            let lesson = game_skills::lesson_interruption(partners, |id| {
                matches!(plan.current(sim, id), Some(Activity::Lesson { .. }))
            });
            for id in lesson.idle {
                plan.activities.insert(id, Activity::Idle);
            }
            step.physical_due.extend(lesson.deadlines);
            for effect in property
                .get(&actor)
                .into_iter()
                .flat_map(|effects| effects.iter())
            {
                match effect {
                    game_property::Effect::TakingInterrupted { incident, actor } => {
                        step.events.push(EventKind::TakingInterrupted {
                            incident: *incident,
                            actor: *actor,
                        });
                    }
                    game_property::Effect::ClearTaking { actor } => {
                        let stamp = match plan.current(sim, *actor) {
                            Some(Activity::Take { .. }) => {
                                game_property::CancellationActivity::Taking
                            }
                            _ => game_property::CancellationActivity::Other,
                        };
                        if let Some(id) = effect.activity_reset(stamp) {
                            plan.activities.insert(id, Activity::Idle);
                        }
                        step.physical_due.insert(*actor);
                    }
                    game_property::Effect::ClearReport {
                        sender,
                        completes,
                        interrupted,
                    } => {
                        let stamp = match plan.current(sim, *sender) {
                            Some(Activity::ReportIncident { completes, .. }) => {
                                game_property::CancellationActivity::Report { completes }
                            }
                            _ => game_property::CancellationActivity::Other,
                        };
                        if let Some(id) = effect.activity_reset(stamp) {
                            plan.activities.insert(id, Activity::Idle);
                        }
                        step.report_due.insert((*sender, *completes));
                        if *interrupted {
                            step.events.push(EventKind::Interrupted { actor: *sender });
                        }
                    }
                    _ => unreachable!("property cancellation effect"),
                }
            }
            for effect in life
                .get(&actor)
                .into_iter()
                .flat_map(|effects| effects.iter())
            {
                let LifeEffect::PregnancyEnded(effect) = effect else {
                    unreachable!("lifecycle interruption effect")
                };
                step.pregnancies.push(effect.parents[0]);
                if effect.was_conceiving {
                    for parent in effect.parents {
                        plan.activities.insert(parent, Activity::Idle);
                        plan.current(sim, parent);
                        step.physical_due.insert(parent);
                    }
                }
                step.pregnancy_due.push(effect.pregnancy);
                step.events.push(EventKind::PregnancyEnded {
                    pregnancy: effect.pregnancy,
                });
            }
            if sim.host.state.physical.is_some() {
                let stamp = match plan.current(sim, actor) {
                    Some(Activity::Care { child, .. }) => {
                        game_physiology::InterruptionActivity::Care { child }
                    }
                    Some(Activity::Comfort { child, .. }) => {
                        game_physiology::InterruptionActivity::Comfort { child }
                    }
                    _ => game_physiology::InterruptionActivity::Other,
                };
                let effect = game_physiology::prepare_activity_interruption(actor, stamp);
                step.care = effect.care;
                plan.activities.insert(effect.idle, Activity::Idle);
                step.physical_due.insert(effect.deadline);
            }
            plan.steps.push(step);
        }
        plan.due = sim
            .host
            .state
            .due
            .iter()
            .filter(|(at, events)| {
                events.iter().any(|event| {
                    plan.steps.iter().any(|step| match event {
                        Due::Physical(actor) => {
                            step.physical_due.contains(actor)
                                || step.report_due.contains(&(*actor, **at))
                        }
                        Due::Birth(id) => step.pregnancy_due.contains(id),
                        _ => false,
                    })
                })
            })
            .map(|(at, events)| (*at, events.clone()))
            .collect();
        plan
    }
}

pub(crate) struct PreparedInterruptions {
    actors: Vec<PersonId>,
    preserve_transport: bool,
    transport: Option<game_transport::PreparedTransport<Vec<game_transport::Lift>>>,
    now: SimTime,
    metadata: MetadataPlan,
    health: Option<game_health::PreparedHealthCancellations>,
    property: Option<game_property::PreparedCancellations>,
    lifecycle: Option<game_lifecycle::PreparedLifecycleInterruptions>,
    genes: Vec<game_genetics::PreparedPregnancyEnd>,
    agriculture: Option<game_agriculture::PreparedCultivationCancellation>,
    ecology: Option<game_ecology::PreparedGatheringCancellation>,
    projects: Option<game_projects::PreparedChange>,
    missions: Option<game_missions::PreparedTalkCancellations>,
    agreements: Option<game_agreements::PreparedReportCancellations>,
    memory: Option<game_invocation::PreparedMemoryEnding>,
    realm: Option<game_realm::PreparedTalkCancellations>,
    enforcement: Option<game_society::enforcement::PreparedInterruption>,
    force: Option<game_conflict::force::PreparedChange>,
}
impl Simulation {
    pub(crate) fn prepare_interruptions(
        &self,
        actors: &[PersonId],
        preserve: ActivityOwner,
    ) -> Result<PreparedInterruptions, Error> {
        self.prepare_interruptions_scoped(
            actors,
            preserve,
            preserve == ActivityOwner::Transport,
            None,
        )
    }

    pub(crate) fn prepare_motion_interruptions(
        &self,
        actors: &[PersonId],
        preserve: ActivityOwner,
    ) -> Result<PreparedInterruptions, Error> {
        self.prepare_interruptions_scoped(actors, preserve, true, None)
    }

    pub(crate) fn prepare_interruptions_after_conception(
        &self,
        actors: &[PersonId],
        preserve: ActivityOwner,
        completed: &game_lifecycle::PreparedLifecycle,
    ) -> Result<PreparedInterruptions, Error> {
        self.prepare_interruptions_scoped(
            actors,
            preserve,
            preserve == ActivityOwner::Transport,
            Some(completed),
        )
    }

    fn prepare_interruptions_scoped(
        &self,
        actors: &[PersonId],
        preserve: ActivityOwner,
        preserve_transport: bool,
        completed: Option<&game_lifecycle::PreparedLifecycle>,
    ) -> Result<PreparedInterruptions, Error> {
        let mut seen = BTreeSet::new();
        let actors: Vec<_> = actors.iter().copied().filter(|a| seen.insert(*a)).collect();
        let now = self.host.clock().time;
        let health = self
            .host
            .state
            .health
            .as_ref()
            .filter(|_| preserve != ActivityOwner::Health)
            .map(|w| w.prepare_cancel_many(&self.host.storage, &actors));
        let mut property = self
            .host
            .state
            .property
            .as_ref()
            .filter(|_| preserve != ActivityOwner::Property)
            .map(|w| w.prepare_cancel_many(&self.host.storage, &actors, now));
        if let Some(change) = &mut property {
            let incidents = change
                .effects()
                .flat_map(|(_, effects)| effects)
                .filter_map(|effect| {
                    if let game_property::Effect::TakingInterrupted { incident, .. } = effect {
                        Some(*incident)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for id in incidents {
                let incident = change
                    .projected_incident(&self.host.storage, id)
                    .ok_or(Error::Unavailable)?;
                let observers = self
                    .property_present(incident.area)
                    .into_iter()
                    .map(|actor| self.property_observer_facts(actor, incident))
                    .collect::<Vec<_>>();
                change.witness_projected(
                    self.host
                        .state
                        .property
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    id,
                    now,
                    &observers,
                )?;
            }
        }
        let requests: Vec<_> = actors
            .iter()
            .map(|actor| {
                let pregnancy = match self.host.storage.activity(*actor) {
                    Some(Activity::Conceive { pregnancy, .. }) => Some(*pregnancy),
                    _ => None,
                };
                game_lifecycle::InterruptionRequest::for_attempt(*actor, pregnancy, completed)
            })
            .collect();
        let lifecycle = self
            .host
            .state
            .development
            .as_ref()
            .map(|w| w.prepare_interruptions(&self.host.storage, &requests));
        let agreements = Some(game_agreements::prepare_cancel_reports(
            &self.host.storage,
            &actors,
        ));
        let missions = self
            .host
            .state
            .missions
            .as_ref()
            .map(|w| w.prepare_cancel_talks(&self.host.storage, &actors));
        let mm = missions
            .as_ref()
            .map(|p| p.effects().collect())
            .unwrap_or_default();
        let hm = health
            .as_ref()
            .map(|p| p.effects().collect())
            .unwrap_or_default();
        let pm = property
            .as_ref()
            .map(|p| p.effects().collect())
            .unwrap_or_default();
        let lm = lifecycle
            .as_ref()
            .map(|p| p.effects().collect())
            .unwrap_or_default();
        let metadata = MetadataPlan::collect(self, &actors, &hm, &pm, &lm, &mm, preserve_transport);
        let genes = self
            .host
            .state
            .genetics
            .as_ref()
            .map(|g| {
                lm.values()
                    .flat_map(|effects| effects.iter())
                    .filter_map(|effect| {
                        if let LifeEffect::PregnancyEnded(effect) = effect {
                            Some(g.prepare_end_pregnancy(&self.host.storage, effect.pregnancy, now))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(PreparedInterruptions {
            force: self
                .host
                .state
                .force
                .as_ref()
                .filter(|_| preserve != ActivityOwner::Force)
                .map(|_| game_conflict::force::prepare_interrupt_many(&self.host.storage, &actors)),
            enforcement: self
                .host
                .state
                .enforcement
                .as_ref()
                .filter(|_| preserve != ActivityOwner::Enforcement)
                .map(|runtime| runtime.prepare_interruptions(&self.host.storage, &actors, now))
                .transpose()?,
            preserve_transport,
            transport: self
                .host
                .state
                .transport
                .as_ref()
                .filter(|_| !preserve_transport)
                .map(|runtime| runtime.prepare_cancel_lifts(&self.host.storage, &actors)),
            missions,
            agreements,
            projects: self
                .host
                .state
                .projects
                .as_ref()
                .filter(|_| preserve != ActivityOwner::Projects)
                .map(|w| w.prepare_interrupt_many(&self.host.storage, &actors))
                .transpose()?,
            agriculture: self
                .host
                .state
                .agriculture
                .as_ref()
                .filter(|_| preserve != ActivityOwner::Agriculture)
                .map(|w| w.prepare_cancel_cultivation(&self.host.storage, &actors)),
            ecology: self
                .host
                .state
                .ecology
                .as_ref()
                .filter(|_| preserve != ActivityOwner::Ecology)
                .map(|w| w.prepare_cancel_gathering(&self.host.storage, &actors)),
            memory: self
                .host
                .state
                .invocation
                .as_ref()
                .map(|runtime| runtime.prepare_memory_ending(&self.host.storage, &actors)),
            realm: self
                .host
                .state
                .realm
                .as_ref()
                .map(|w| w.prepare_cancel_talks(&self.host.storage, &actors)),
            actors,
            now,
            metadata,
            health,
            property,
            lifecycle,
            genes,
        })
    }
}
#[derive(Default)]
struct ReplacementPermits<'a> {
    memory: Option<&'a game_invocation::MemoryReplacement>,
    mission: Option<&'a game_missions::TalkReplacement>,
    agreement: Option<&'a game_agreements::ReportReplacement>,
    realm: Option<&'a game_realm::ReportReplacement>,
}
impl PreparedInterruptions {
    pub(crate) fn projected_activities(&self) -> impl Iterator<Item = (PersonId, &Activity)> {
        self.metadata
            .activities
            .iter()
            .map(|(actor, activity)| (*actor, activity))
    }
    pub(crate) fn event_count(&self) -> Result<u64, Error> {
        self.metadata.steps.iter().try_fold(0u64, |sum, step| {
            sum.checked_add(u64::try_from(step.events.len()).map_err(|_| Error::Overflow)?)
                .ok_or(Error::Overflow)
        })
    }
    pub(crate) fn stage_into(
        self,
        sim: &Simulation,
        batch: &mut Batch,
        final_activities: &BTreeMap<PersonId, Activity>,
    ) -> Result<InterruptionFinalization, Error> {
        self.stage_into_with_memory_replacement(sim, batch, final_activities, None)
    }
    pub(crate) fn stage_into_with_memory_replacement(
        self,
        sim: &Simulation,
        batch: &mut Batch,
        final_activities: &BTreeMap<PersonId, Activity>,
        replacement: Option<&game_invocation::MemoryReplacement>,
    ) -> Result<InterruptionFinalization, Error> {
        self.stage_into_with_replacements(
            sim,
            batch,
            final_activities,
            ReplacementPermits {
                memory: replacement,
                ..Default::default()
            },
        )
    }
    pub(crate) fn stage_into_with_mission_replacement(
        self,
        sim: &Simulation,
        batch: &mut Batch,
        final_activities: &BTreeMap<PersonId, Activity>,
        replacement: &game_missions::TalkReplacement,
    ) -> Result<InterruptionFinalization, Error> {
        self.stage_into_with_replacements(
            sim,
            batch,
            final_activities,
            ReplacementPermits {
                mission: Some(replacement),
                ..Default::default()
            },
        )
    }
    pub(crate) fn stage_into_with_agreement_replacement(
        self,
        sim: &Simulation,
        batch: &mut Batch,
        final_activities: &BTreeMap<PersonId, Activity>,
        replacement: &game_agreements::ReportReplacement,
    ) -> Result<InterruptionFinalization, Error> {
        self.stage_into_with_replacements(
            sim,
            batch,
            final_activities,
            ReplacementPermits {
                agreement: Some(replacement),
                ..Default::default()
            },
        )
    }
    pub(crate) fn stage_into_with_realm_replacement(
        self,
        sim: &Simulation,
        batch: &mut Batch,
        final_activities: &BTreeMap<PersonId, Activity>,
        replacement: &game_realm::ReportReplacement,
    ) -> Result<InterruptionFinalization, Error> {
        self.stage_into_with_replacements(
            sim,
            batch,
            final_activities,
            ReplacementPermits {
                realm: Some(replacement),
                ..Default::default()
            },
        )
    }
    fn stage_into_with_replacements(
        self,
        sim: &Simulation,
        batch: &mut Batch,
        final_activities: &BTreeMap<PersonId, Activity>,
        permits: ReplacementPermits<'_>,
    ) -> Result<InterruptionFinalization, Error> {
        let ReplacementPermits {
            memory: memory_replacement,
            mission: mission_replacement,
            agreement: agreement_replacement,
            realm: realm_replacement,
        } = permits;
        if self.now != sim.host.clock().time {
            return Err(Error::Unavailable);
        }
        let mm = self
            .missions
            .as_ref()
            .map(|p| p.effects().collect())
            .unwrap_or_default();
        let hm = self
            .health
            .as_ref()
            .map(|p| p.effects().collect())
            .unwrap_or_default();
        let pm = self
            .property
            .as_ref()
            .map(|p| p.effects().collect())
            .unwrap_or_default();
        let lm = self
            .lifecycle
            .as_ref()
            .map(|p| p.effects().collect())
            .unwrap_or_default();
        if self.metadata
            != MetadataPlan::collect(
                sim,
                &self.actors,
                &hm,
                &pm,
                &lm,
                &mm,
                self.preserve_transport,
            )
        {
            return Err(Error::Unavailable);
        }
        let store = &sim.host.storage;
        let mut metadata = self.metadata;
        for (actor, activity) in final_activities {
            metadata
                .opening
                .entry(*actor)
                .or_insert_with(|| store.activity(*actor).cloned());
            metadata.activities.insert(*actor, activity.clone());
        }
        for (actor, opening) in &metadata.opening {
            if store.activity(*actor) != opening.as_ref() {
                return Err(Error::Unavailable);
            }
        }
        for (actor, activity) in &metadata.activities {
            batch.insert(StableId(actor.0), activity.clone());
        }
        let mut guards = Vec::new();
        if let Some(enforcement) = self.enforcement {
            guards.push(enforcement.stage(store, batch)?);
        }
        let force = self
            .force
            .map(|change| change.stage_into(store, batch))
            .transpose()?;
        for step in &metadata.steps {
            for removal in &step.removals {
                let guard = match removal {
                    Removal::ArmyTalk(talk) => {
                        Some(game_armies::stage_cancel_talk(store, talk, batch)?)
                    }
                    Removal::ArmyApproach(approach) => {
                        Some(game_armies::stage_cancel_approach(store, approach, batch)?)
                    }
                    Removal::Social(leader, interaction) => {
                        Some(game_characters::social::stage_cancel_interaction(
                            store,
                            *leader,
                            interaction,
                            batch,
                        )?)
                    }
                };
                if let Some(guard) = guard {
                    guards.push(guard);
                }
            }
        }
        let memory = self
            .memory
            .map(|change| {
                change.stage_into_composed(
                    sim.host
                        .state
                        .invocation
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    store,
                    memory_replacement,
                    batch,
                )
            })
            .transpose()?;
        let health = self
            .health
            .map(|change| {
                change.stage_into(
                    sim.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                    store,
                    batch,
                )
            })
            .transpose()?;
        let property = self
            .property
            .map(|change| {
                change.stage_into(
                    sim.host.state.property.as_ref().ok_or(Error::Unavailable)?,
                    store,
                    batch,
                )
            })
            .transpose()?;
        let lifecycle = self
            .lifecycle
            .map(|change| {
                change.stage_into(
                    sim.host
                        .state
                        .development
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    store,
                    batch,
                )
            })
            .transpose()?;
        for change in self.genes {
            guards.push(change.stage_guarded_into(store, batch)?);
        }
        if let Some(change) = self.agriculture {
            guards.push(change.stage_guarded_into(store, batch)?);
        }
        if let Some(change) = self.ecology {
            guards.push(change.stage_guarded_into(store, batch)?);
        }
        let projects = self
            .projects
            .map(|change| {
                change.stage_into(
                    sim.host.state.projects.as_ref().ok_or(Error::Unavailable)?,
                    store,
                    &sim.host.state.workshops,
                    batch,
                )
            })
            .transpose()?;
        let transport = self
            .transport
            .map(|change| {
                change.stage_into(
                    sim.host
                        .state
                        .transport
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    store,
                    &BTreeMap::<PersonId, game_transport::Body>::new(),
                    self.now,
                    batch,
                )
            })
            .transpose()?;
        let agreements = self
            .agreements
            .map(|change| change.stage_into_composed(store, batch, agreement_replacement))
            .transpose()?;
        if let Some(change) = self.missions {
            change.stage_into_composed(store, batch, mission_replacement)?;
        }
        let realm = self
            .realm
            .map(|change| change.stage_into(store, batch, realm_replacement))
            .transpose()?;
        // Schema admission remains shared with all caller-owner writes.
        Ok(InterruptionFinalization {
            guards,
            health,
            lifecycle,
            force,
            memory,
            steps: metadata.steps.into_iter().map(|s| (s.actor, s)).collect(),
            property,
            projects,
            transport,
            agreements,
            realm,
        })
    }
}
pub(crate) struct InterruptionFinalization {
    guards: Vec<game_ecs::ReadGuard>,
    health: Option<game_health::HealthCancellationFinalization>,
    lifecycle: Option<game_lifecycle::LifecycleInterruptionFinalization>,
    force: Option<game_conflict::force::Receipt>,
    memory: Option<game_invocation::MemoryEndingReceipt>,
    steps: BTreeMap<PersonId, MetadataStep>,
    transport: Option<game_transport::TransportFinalization<Vec<game_transport::Lift>>>,
    property: Option<game_property::CancellationFinalization>,
    projects: Option<game_projects::ProjectFinalization>,
    agreements: Option<game_agreements::ReportBatchGuard>,
    realm: Option<game_realm::TalkBatchGuard>,
}
impl InterruptionFinalization {
    pub(crate) fn validate_batch(&self, sim: &Simulation, batch: &Batch) -> Result<(), Error> {
        for guard in &self.guards {
            guard.validate(&sim.host.storage, batch)?;
        }
        if let Some(health) = &self.health {
            health.validate_batch(
                sim.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                &sim.host.storage,
                batch,
            )?;
        }
        if let Some(lifecycle) = &self.lifecycle {
            lifecycle.validate(&sim.host.storage, batch)?;
        }
        if let Some(force) = &self.force {
            force.validate_batch(&sim.host.storage, batch)?;
        }
        if let Some(memory) = &self.memory {
            memory.validate_batch(&sim.host.storage, batch)?;
        }
        if let Some(transport) = &self.transport {
            transport.validate_batch(&sim.host.storage, batch)?;
        }
        if let Some(property) = &self.property {
            property
                .validate_runtime(sim.host.state.property.as_ref().ok_or(Error::Unavailable)?)?;
            property.validate_batch(&sim.host.storage, batch)?;
        }
        if let Some(guard) = &self.projects {
            guard.validate_batch(
                sim.host.state.projects.as_ref().ok_or(Error::Unavailable)?,
                &sim.host.storage,
                &sim.host.state.workshops,
                batch,
            )?;
        }
        if let Some(guard) = &self.realm {
            guard.validate(batch)?;
        }
        if let Some(guard) = &self.agreements {
            guard.validate(batch)?;
        }
        Ok(())
    }

    pub(crate) fn events_for(&self, actors: &[PersonId]) -> Vec<EventKind> {
        let mut seen = BTreeSet::new();
        actors
            .iter()
            .filter(|actor| seen.insert(**actor))
            .filter_map(|actor| self.steps.get(actor))
            .flat_map(|step| step.events.iter().cloned())
            .collect()
    }

    pub(crate) fn finish_actor_metadata(&mut self, sim: &mut Simulation, actor: PersonId) {
        self.finish_actor_inner(sim, actor, false);
    }

    pub(crate) fn finish_actor(&mut self, sim: &mut Simulation, actor: PersonId) {
        self.finish_actor_inner(sim, actor, true);
    }

    fn finish_actor_inner(&mut self, sim: &mut Simulation, actor: PersonId, publish_events: bool) {
        let Some(step) = self.steps.remove(&actor) else {
            return;
        };
        if let Some(transport) = self.transport.take() {
            transport.finish(
                sim.host
                    .state
                    .transport
                    .as_mut()
                    .expect("admitted transport runtime"),
            );
        }
        if let Some(projects) = self.projects.take() {
            projects.finish(
                sim.host
                    .state
                    .projects
                    .as_mut()
                    .expect("admitted project runtime"),
            );
        }
        if let Some(property) = self.property.take() {
            property.finish(
                sim.host
                    .state
                    .property
                    .as_mut()
                    .expect("admitted property runtime"),
            );
        }
        for removal in step.removals {
            match removal {
                Removal::ArmyTalk(_) | Removal::ArmyApproach(_) | Removal::Social(_, _) => {}
            }
        }
        for gestator in step.pregnancies {
            sim.pregnancy_index.remove(&gestator);
        }
        if let Some(child) = step.care {
            sim.care_index.remove(&child);
        }
        for (at, due) in sim.host.state.due.iter_mut() {
            due.retain(|d| match d {
                Due::Physical(a) => {
                    !step.physical_due.contains(a) && !step.report_due.contains(&(*a, *at))
                }
                Due::Birth(id) => !step.pregnancy_due.contains(id),
                _ => true,
            });
        }
        sim.host.state.due.retain(|_, due| !due.is_empty());
        if publish_events {
            for event in step.events {
                sim.emit(event);
            }
        }
    }
}

#[cfg(test)]
mod tests;

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
    RealmTalk(PersonId, realm::Talk),
    Social(PersonId, social::Interaction),
    Enforcement(
        PersonId,
        enforcement::Talk,
        Option<(
            Box<enforcement::Case>,
            game_society::enforcement::ComplaintInterruption,
        )>,
    ),
    Restraint(PersonId, force::Restraint),
    Escape(PersonId, force::Escape),
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
    fn idle_if(
        &mut self,
        sim: &Simulation,
        actor: PersonId,
        predicate: impl FnOnce(&Activity) -> bool,
    ) {
        if self.current(sim, actor).as_ref().is_some_and(predicate) {
            self.activities.insert(actor, Activity::Idle);
        }
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
            if let Some(w) = &sim.host.state.armies {
                for talk in w
                    .talks
                    .values()
                    .filter(|t| t.speaker == actor || t.listener == actor)
                {
                    if !removed.insert((0, talk.speaker.0)) {
                        continue;
                    }
                    plan.idle_if(sim,talk.speaker,|a|matches!(a,Activity::ArmyTalk{completes,..} if *completes==talk.completes));
                    step.removals.push(Removal::ArmyTalk(talk.clone()));
                }
                if let Some(approach) = w.approaches.get(&actor) {
                    plan.idle_if(sim, actor, |a| matches!(a, Activity::ArmyApproach { .. }));
                    step.removals.push(Removal::ArmyApproach(approach.clone()));
                }
            }
            if let Some(w) = &sim.host.state.realm {
                for (speaker, talk) in w
                    .talks
                    .iter()
                    .filter(|(s, t)| **s == actor || t.recipient == actor)
                {
                    if !removed.insert((1, speaker.0)) {
                        continue;
                    }
                    if matches!(
                        plan.current(sim, *speaker),
                        Some(Activity::RealmTalk { .. })
                    ) {
                        plan.activities.insert(*speaker, Activity::Idle);
                        step.physical_due.insert(*speaker);
                    }
                    step.events.push(EventKind::Interrupted { actor: *speaker });
                    step.removals
                        .push(Removal::RealmTalk(*speaker, talk.clone()));
                }
            }
            if !preserve_transport && sim.host.state.transport.is_some() {
                for (_, lift) in game_transport::lifts(&sim.host.storage)
                    .filter(|(_, l)| l.passenger == actor || l.bearers.contains(&actor))
                {
                    if !removed.insert((2, lift.passenger.0)) {
                        continue;
                    }
                    for bearer in &lift.bearers {
                        if matches!(plan.current(sim,*bearer),Some(Activity::Lift{passenger,..}) if passenger==lift.passenger)
                        {
                            plan.activities.insert(*bearer, Activity::Idle);
                            step.physical_due.insert(*bearer);
                        }
                    }
                }
            }
            if let Some(w) = &sim.host.state.social
                && let Some(key) = w.participants.get(&actor)
                && let Some(interaction) = w.interactions.get(key)
                && removed.insert((3, key.0))
            {
                for id in interaction.actors() {
                    plan.idle_if(sim, id, |a| {
                        matches!(
                            a,
                            Activity::Social { .. }
                                | Activity::Instruction { .. }
                                | Activity::Practice { .. }
                        )
                    });
                    step.physical_due.insert(id);
                }
                step.removals
                    .push(Removal::Social(*key, interaction.clone()));
            }
            if let Some(w) = &sim.host.state.enforcement
                && let Some(talk) = w.talks.get(&actor)
            {
                let case = game_society::enforcement::complaint_interruption(
                    sim.host.clock().time,
                    matches!(talk.conversation, enforcement::Conversation::File),
                )
                .map(|change| (Box::new(w.cases[&talk.case].clone()), change));
                step.removals
                    .push(Removal::Enforcement(actor, talk.clone(), case));
            }
            for effect in health
                .get(&actor)
                .into_iter()
                .flat_map(|effects| effects.iter())
            {
                if let game_health::HealthEffect::ClearWork { actor, completes } = effect {
                    plan.idle_if(
                        sim,
                        *actor,
                        |a| matches!(a,Activity::HealthWork{completes:at,..}if at==completes),
                    );
                }
            }
            for talk in missions
                .get(&actor)
                .into_iter()
                .flat_map(|effects| effects.iter())
            {
                plan.idle_if(
                    sim,
                    talk.speaker,
                    |a| matches!(a,Activity::MissionTalk{completes}if *completes==talk.completes),
                );
            }
            if let Some(w) = &sim.host.state.force {
                if let Some(row) = w.restraints.get(&actor) {
                    step.removals.push(Removal::Restraint(actor, row.clone()));
                }
                for (target, attempt) in w.escapes.iter().filter(|(target, attempt)| {
                    (**target == actor || attempt.actor == actor) && !attempt.contested
                }) {
                    if removed.insert((5, target.0)) {
                        step.removals
                            .push(Removal::Escape(*target, attempt.clone()));
                    }
                }
            }
            if let Some(runtime) = &sim.host.state.invocation {
                let ending = runtime.prepare_memory_ending(&sim.host.storage, &[actor]);
                for id in ending.activity_resets(&invocation::MemoryBindings(sim)) {
                    plan.activities.insert(id, Activity::Idle);
                }
            }
            if let Some(Activity::Lesson {
                teacher, student, ..
            }) = plan.current(sim, actor)
            {
                for id in [teacher, student] {
                    plan.idle_if(sim, id, |a| matches!(a, Activity::Lesson { .. }));
                    step.physical_due.insert(id);
                }
            }
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
                        plan.idle_if(sim, *actor, |a| matches!(a, Activity::Take { .. }));
                        step.physical_due.insert(*actor);
                    }
                    game_property::Effect::ClearReport {
                        sender,
                        completes,
                        interrupted,
                    } => {
                        plan.idle_if(sim,*sender,|a|matches!(a,Activity::ReportIncident{completes:at,..}if at==completes));
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
                if let Some(Activity::Care { child, .. } | Activity::Comfort { child, .. }) =
                    plan.current(sim, actor)
                {
                    step.care = Some(child);
                }
                plan.activities.insert(actor, Activity::Idle);
                step.physical_due.insert(actor);
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
}
impl Simulation {
    pub(crate) fn prepare_interruptions(
        &self,
        actors: &[PersonId],
        preserve: ActivityOwner,
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
        let property = self
            .host
            .state
            .property
            .as_ref()
            .filter(|_| preserve != ActivityOwner::Property)
            .map(|w| w.prepare_cancel_many(&self.host.storage, &actors, now));
        let requests: Vec<_> = actors
            .iter()
            .map(|actor| game_lifecycle::InterruptionRequest {
                actor: *actor,
                died: false,
                affected: match self.host.storage.activity(*actor) {
                    Some(Activity::Conceive { pregnancy, .. }) => vec![*pregnancy],
                    _ => Vec::new(),
                },
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
        let metadata = MetadataPlan::collect(
            self,
            &actors,
            &hm,
            &pm,
            &lm,
            &mm,
            preserve == ActivityOwner::Transport,
        );
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
            preserve_transport: preserve == ActivityOwner::Transport,
            transport: self
                .host
                .state
                .transport
                .as_ref()
                .filter(|_| preserve != ActivityOwner::Transport)
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
                .map(|w| w.prepare_cancel_cultivation(&self.host.storage, &actors)),
            ecology: self
                .host
                .state
                .ecology
                .as_ref()
                .map(|w| w.prepare_cancel_gathering(&self.host.storage, &actors)),
            memory: self
                .host
                .state
                .invocation
                .as_ref()
                .map(|runtime| runtime.prepare_memory_ending(&self.host.storage, &actors)),
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
        memory_replacement: Option<&game_invocation::MemoryReplacement>,
    ) -> Result<InterruptionFinalization, Error> {
        self.stage_into_with_replacements(
            sim,
            batch,
            final_activities,
            memory_replacement,
            None,
            None,
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
            None,
            Some(replacement),
            None,
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
            None,
            None,
            Some(replacement),
        )
    }
    fn stage_into_with_replacements(
        self,
        sim: &Simulation,
        batch: &mut Batch,
        final_activities: &BTreeMap<PersonId, Activity>,
        memory_replacement: Option<&game_invocation::MemoryReplacement>,
        mission_replacement: Option<&game_missions::TalkReplacement>,
        agreement_replacement: Option<&game_agreements::ReportReplacement>,
    ) -> Result<InterruptionFinalization, Error> {
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
        if let Some(change) = self.memory {
            change.stage_into_composed(
                sim.host
                    .state
                    .invocation
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
                store,
                memory_replacement,
                batch,
            )?;
        }
        if let Some(change) = self.health {
            change.stage_into(
                sim.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                store,
                batch,
            )?;
        }
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
        if let Some(change) = self.lifecycle {
            change.stage_into(
                sim.host
                    .state
                    .development
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
                store,
                batch,
            )?;
        }
        for change in self.genes {
            change.stage_into(store, batch)?;
        }
        if let Some(change) = self.agriculture {
            change.stage_into(store, batch)?;
        }
        if let Some(change) = self.ecology {
            change.stage_into(store, batch)?;
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
        // Schema admission remains shared with all caller-owner writes.
        Ok(InterruptionFinalization {
            steps: metadata.steps.into_iter().map(|s| (s.actor, s)).collect(),
            property,
            projects,
            transport,
            agreements,
        })
    }
}
pub(crate) struct InterruptionFinalization {
    steps: BTreeMap<PersonId, MetadataStep>,
    transport: Option<game_transport::TransportFinalization<Vec<game_transport::Lift>>>,
    property: Option<game_property::CancellationFinalization>,
    projects: Option<game_projects::ProjectFinalization>,
    agreements: Option<game_agreements::ReportBatchGuard>,
}
impl InterruptionFinalization {
    pub(crate) fn validate_batch(&self, batch: &Batch) -> Result<(), Error> {
        if let Some(guard) = &self.agreements {
            guard.validate(batch)?;
        }
        Ok(())
    }

    pub(crate) fn finish_actor(&mut self, sim: &mut Simulation, actor: PersonId) {
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
                Removal::ArmyTalk(t) => {
                    sim.host
                        .state
                        .armies
                        .as_mut()
                        .unwrap()
                        .talks
                        .remove(&t.speaker);
                }
                Removal::ArmyApproach(a) => {
                    sim.host
                        .state
                        .armies
                        .as_mut()
                        .unwrap()
                        .approaches
                        .remove(&a.actor);
                }
                Removal::RealmTalk(s, _) => {
                    sim.host.state.realm.as_mut().unwrap().talks.remove(&s);
                }
                Removal::Social(key, i) => {
                    let w = sim.host.state.social.as_mut().unwrap();
                    w.interactions.remove(&key);
                    for actor in i.actors() {
                        w.participants.remove(&actor);
                    }
                }
                Removal::Enforcement(actor, talk, case_change) => {
                    let w = sim.host.state.enforcement.as_mut().unwrap();
                    w.talks.remove(&actor);
                    if let Some((_, change)) = case_change {
                        let case = w.cases.get_mut(&talk.case).unwrap();
                        case.status = EnforcementStatus::Withdrawn;
                        case.changed_at = change.changed_at;
                        case.explanation = change.explanation.into();
                    }
                }
                Removal::Restraint(a, _) => {
                    sim.host.state.force.as_mut().unwrap().restraints.remove(&a);
                }
                Removal::Escape(a, _) => {
                    sim.host.state.force.as_mut().unwrap().escapes.remove(&a);
                }
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
        for event in step.events {
            sim.emit(event);
        }
    }
}

#[cfg(test)]
mod tests;

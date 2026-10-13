//! Invocation retains a separate memory source and a person-bound consent ledger.
//! Fractional burden and physiology use opening-state integer arithmetic.
use crate::*;

pub use game_invocation::{Activation, Active, Burden, Consent, Episode, Rules, Warning, severity};

pub(crate) type PreparedInvocationReview = game_invocation::PreparedControllerReview;
pub type World = game_invocation::Runtime;
pub type PastLife = game_invocation::PastLife<game_api::Command>;
pub use game_invocation::{MemoryAction, MemoryWork, Record};

/// Opening controller audience, with exact native membership witnesses.
struct InvocationPreviewContext {
    selected: BTreeSet<game_ecs::StableId>,
    reads: game_ecs::ReadSet,
    contacts: Vec<(PersonId, PersonId, bool)>,
}
impl InvocationPreviewContext {
    fn capture(
        sim: &Simulation,
        mut selected: BTreeSet<game_ecs::StableId>,
    ) -> Result<Self, Error> {
        // Memory interruption can end a paired action owned by either participant.
        loop {
            let before = selected.len();
            for (owner, action) in game_invocation::memory_actions(&sim.host.storage) {
                let recipient = match action.work {
                    MemoryWork::Teach { recipient, .. } => Some(recipient),
                    _ => None,
                };
                if !selected.contains(&game_ecs::StableId(owner.0))
                    && !recipient.is_some_and(|id| selected.contains(&game_ecs::StableId(id.0)))
                {
                    continue;
                }
                selected.insert(game_ecs::StableId(owner.0));
                selected.extend(recipient.map(|id| game_ecs::StableId(id.0)));
                match action.work {
                    MemoryWork::Read { record } | MemoryWork::Move { record, .. } => {
                        selected.insert(game_ecs::StableId(record.0));
                        if let Some(row) = game_invocation::record(&sim.host.storage, record) {
                            selected.insert(game_ecs::StableId(row.inventory.0));
                        }
                    }
                    MemoryWork::Write { material, tool, .. } => {
                        selected
                            .extend([game_ecs::StableId(material.0), game_ecs::StableId(tool.0)]);
                    }
                    _ => {}
                }
            }
            if selected.len() == before {
                break;
            }
        }
        let mut reads = game_ecs::ReadSet::new();
        if let Some(warning) = sim
            .host
            .state
            .invocation
            .as_ref()
            .and_then(|owner| owner.control.pending_consent(&sim.host.storage))
        {
            let id = game_ecs::StableId(warning.id.0);
            selected.insert(id);
            reads.read::<game_invocation::InvocationWarning>(&sim.host.storage, id);
        }
        let mut contacts = Vec::new();
        let actors: BTreeSet<_> = selected
            .iter()
            .map(|id| PersonId(id.0))
            .chain(
                sim.host
                    .state
                    .invocation
                    .as_ref()
                    .and_then(|owner| owner.control.actor(&sim.host.storage)),
            )
            .collect();
        let roster = game_spatial::LocalRoster::build(&sim.host.storage)?;
        for actor in actors {
            let id = game_ecs::StableId(actor.0);
            reads.read::<Location>(&sim.host.storage, id);
            let Some(Location::AtArea(area)) = game_spatial::location(&sim.host.storage, id) else {
                continue;
            };
            let area = *area;
            reads.membership::<Location>(&sim.host.storage, move |_, location| {
                *location == Location::AtArea(area)
            });
            for local in roster.raw_at_area(area) {
                reads.read::<game_physiology::Body>(&sim.host.storage, local);
                if game_physiology::body(&sim.host.storage, PersonId(local.0)).is_some() {
                    selected.insert(local);
                    contacts.push((
                        actor,
                        PersonId(local.0),
                        sim.transport_same_place(actor, PersonId(local.0)),
                    ));
                }
            }
        }
        Ok(Self {
            selected,
            reads,
            contacts,
        })
    }
    fn bind(self, sim: &Simulation, batch: &game_ecs::Batch) -> Result<game_ecs::ReadGuard, Error> {
        for (actor, local, contact) in self.contacts {
            if sim.transport_same_place(actor, local) != contact {
                return Err(Error::Unavailable);
            }
        }
        let mut admitted = game_ecs::ReadFootprint::new();
        for id in batch.edited_ids::<Location>() {
            admitted.protect::<Location>(id);
        }
        for id in batch.edited_ids::<game_physiology::Body>() {
            admitted.protect::<game_physiology::Body>(id);
        }
        for id in batch.edited_ids::<game_invocation::InvocationWarning>() {
            admitted.protect::<game_invocation::InvocationWarning>(id);
        }
        self.reads.bind(&sim.host.storage, batch, &admitted)
    }
}

struct ControllerBindings<'a>(&'a Simulation);
impl game_invocation::ControllerFactsSource for ControllerBindings<'_> {
    fn facts(&self, actor: PersonId) -> Result<game_invocation::ControllerFacts, Error> {
        let sim = self.0;
        let body = game_physiology::body(&sim.host.storage, actor).ok_or(Error::Unavailable)?;
        let rules = &sim
            .host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?
            .rules;
        let area = match game_spatial::location(&sim.host.storage, game_ecs::StableId(actor.0))
            .ok_or(Error::Unavailable)?
        {
            Location::AtArea(area) => Some(*area),
            _ => None,
        };
        let activity = match *sim.host.storage.activity(actor).ok_or(Error::Unavailable)? {
            Activity::Eat { .. } => game_invocation::ControllerActivity::Eat,
            Activity::Load { .. } => game_invocation::ControllerActivity::Load,
            Activity::Care { .. } => game_invocation::ControllerActivity::Care,
            Activity::Tell { .. } => game_invocation::ControllerActivity::Tell,
            Activity::Lesson { .. } => game_invocation::ControllerActivity::Lesson,
            Activity::WritingMemory { .. }
            | Activity::ReadingMemory { .. }
            | Activity::TeachingMemory { .. }
            | Activity::MovingRecord { .. } => game_invocation::ControllerActivity::Memory,
            Activity::Work { site } => game_invocation::ControllerActivity::Work { site },
            _ => game_invocation::ControllerActivity::Other,
        };
        Ok(game_invocation::ControllerFacts {
            in_combat: sim.in_combat(actor),
            independent: sim.independent(actor),
            area,
            activity,
            hunger: body.vitals.hunger,
            meal_relief: rules.meal_relief,
            fatigue: body.vitals.fatigue,
            fatigue_limit: rules.fatigue_limit,
        })
    }
    fn protect(&self, store: &game_ecs::Store, actor: PersonId, reads: &mut game_ecs::ReadSet) {
        let id = game_ecs::StableId(actor.0);
        reads.read::<game_physiology::Body>(store, id);
        reads.read::<Activity>(store, id);
        reads.read::<game_lifecycle::Life>(store, id);
        reads.read::<game_conflict::Participation>(store, id);
        reads.read::<game_characters::Person>(store, id);
        let mut current = actor;
        let mut seen = BTreeSet::new();
        while seen.insert(current) {
            match reads.read::<Location>(store, game_ecs::StableId(current.0)) {
                Some(Location::Carried { carrier }) => current = *carrier,
                _ => break,
            }
        }
    }
}
pub(crate) fn memory_activity(activity: &Activity) -> game_invocation::MemoryActivity {
    use game_invocation::MemoryActivity;
    match activity {
        Activity::WritingMemory { completes } => MemoryActivity::Write(*completes),
        Activity::ReadingMemory { completes } => MemoryActivity::Read(*completes),
        Activity::MovingRecord { completes } => MemoryActivity::Move(*completes),
        Activity::TeachingMemory { completes } => MemoryActivity::Teach(*completes),
        _ => MemoryActivity::Other,
    }
}
fn bound_memory_activity(activity: game_invocation::MemoryActivity) -> Activity {
    use game_invocation::MemoryActivity;
    match activity {
        MemoryActivity::Write(completes) => Activity::WritingMemory { completes },
        MemoryActivity::Read(completes) => Activity::ReadingMemory { completes },
        MemoryActivity::Move(completes) => Activity::MovingRecord { completes },
        MemoryActivity::Teach(completes) => Activity::TeachingMemory { completes },
        MemoryActivity::Other => Activity::Idle,
    }
}
struct MemorySaveFacts<'a, 'b>(&'a CanonicalState<'b>);
impl game_invocation::MemoryReferences for MemorySaveFacts<'_, '_> {
    fn person(&self, id: PersonId) -> Option<game_invocation::MemoryValidationPerson> {
        self.0.person(id)?;
        let body = game_physiology::body(self.0.components, id)?;
        Some(game_invocation::MemoryValidationPerson {
            alive: body.vitals.alive,
            activity: memory_activity(self.0.components.activity(id)?),
        })
    }
    fn death(&self, id: PersonId) -> Option<SimTime> {
        self.0
            .state
            .lineage
            .as_ref()?
            .history(self.0.components)
            .ok()?
            .incarnations
            .iter()
            .find(|life| life.person == id)?
            .died_at
    }
    fn area_exists(&self, id: AreaId) -> bool {
        self.0.state.areas.iter().any(|area| area.id == id)
    }
    fn memory_activities(
        &self,
    ) -> impl Iterator<Item = (PersonId, game_invocation::MemoryActivity)> {
        self.0
            .components
            .activities()
            .map(|(id, activity)| (id, memory_activity(activity)))
    }
}
pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(runtime) = &s.state.invocation else {
        return game_invocation::validate_absent_memory::<game_api::Command>(s.components);
    };
    let fail = || Error::InvalidSave("invalid invocation state".into());
    let lineage = s.state.lineage.as_ref().ok_or_else(fail)?;
    if s.state.physical.is_none() || s.state.decisions.is_none() || s.state.skills.is_none() {
        return Err(fail());
    }
    runtime.validate_memory::<game_api::Command>(
        s.components,
        &MemorySaveFacts(s),
        game_invocation::MemoryValidation {
            now: s.time,
            next_id: s.next_id,
            current: lineage.current(s.components),
            incarnations: &lineage
                .history(s.components)?
                .incarnations
                .iter()
                .map(|life| life.person)
                .collect(),
        },
    )
}
pub(crate) struct MemoryBindings<'a>(pub(crate) &'a Simulation);
impl game_invocation::MemoryFacts for MemoryBindings<'_> {
    fn person(&self, id: PersonId, other: PersonId) -> Option<game_invocation::MemoryPerson> {
        let sim = self.0;
        let body = game_physiology::body(&sim.host.storage, id)?;
        let mind = game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, id)?;
        let activity = sim.host.storage.activity(id)?;
        Some(game_invocation::MemoryPerson {
            location: game_spatial::location(&sim.host.storage, game_ecs::StableId(id.0))?.clone(),
            independent: sim.independent(id),
            in_combat: sim.in_combat(id),
            restricted: sim.combat_restricts(id),
            alive: body.vitals.alive,
            health: body.vitals.health,
            idle_or_resting: matches!(activity, Activity::Idle | Activity::Rest),
            teaching_completes: match activity {
                Activity::TeachingMemory { completes } => Some(*completes),
                _ => None,
            },
            sociability: mind.personality.sociability,
            trust: mind
                .relationships
                .get(&other)
                .map_or(0, |relationship| relationship.trust),
            hunger: body.vitals.hunger,
            meal_relief: sim.host.state.physical.as_ref()?.rules.meal_relief,
        })
    }
    fn stock_area(&self, id: InventoryId) -> Option<AreaId> {
        self.0
            .inventory_area(game_cargo::inventory(&self.0.host.storage, id)?)
    }
    fn has_access(&self, actor: PersonId, id: InventoryId) -> bool {
        game_cargo::inventory(&self.0.host.storage, id)
            .is_some_and(|stock| self.0.has_stock_access(actor, stock))
    }
    fn tool_available(&self, actor: PersonId, id: InventoryId) -> bool {
        self.0.tool_available_for(actor, id)
    }
    fn unreserved(&self, actor: PersonId, id: InventoryId) -> u64 {
        self.0.unreserved_stock(id, actor)
    }
    fn carried_quantity(&self, actor: PersonId) -> u128 {
        self.0
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .filter_map(|id| game_cargo::inventory(&self.0.host.storage, *id))
            .map(|stock| u128::from(stock.quantity.0))
            .sum()
    }
    fn carrying_capacity(&self, _: PersonId) -> u64 {
        self.0
            .host
            .state
            .physical
            .as_ref()
            .unwrap()
            .rules
            .carrying_capacity
            .0
    }
    fn creation_body_capacity(&self, actor: PersonId) -> u64 {
        self.0.body_rules(actor).carrying_capacity.0
    }
    fn creation_transport_capacity(&self, actor: PersonId, quantity: Quantity) -> bool {
        self.0.transport_transfer_capacity(actor, quantity.0)
    }
}
impl game_invocation::MemoryReferences for MemoryBindings<'_> {
    fn person(&self, id: PersonId) -> Option<game_invocation::MemoryValidationPerson> {
        Some(game_invocation::MemoryValidationPerson {
            alive: game_physiology::body(&self.0.host.storage, id)?
                .vitals
                .alive,
            activity: memory_activity(self.0.host.storage.activity(id)?),
        })
    }
    fn death(&self, id: PersonId) -> Option<SimTime> {
        self.0
            .host
            .state
            .lineage
            .as_ref()?
            .history(&self.0.host.storage)
            .ok()?
            .incarnations
            .iter()
            .find(|life| life.person == id)?
            .died_at
    }
    fn area_exists(&self, id: AreaId) -> bool {
        self.0.host.state.areas.iter().any(|area| area.id == id)
    }
    fn memory_activities(
        &self,
    ) -> impl Iterator<Item = (PersonId, game_invocation::MemoryActivity)> {
        self.0
            .host
            .storage
            .activities()
            .map(|(id, activity)| (id, memory_activity(activity)))
    }
}
impl game_invocation::ArchiveFacts<game_api::Command> for MemoryBindings<'_> {
    fn incarnations(&self) -> impl Iterator<Item = (PersonId, Option<SimTime>)> {
        self.0
            .host
            .state
            .lineage
            .as_ref()
            .into_iter()
            .flat_map(|lineage| {
                &lineage
                    .history(&self.0.host.storage)
                    .expect("validated lineage history")
                    .incarnations
            })
            .map(|life| (life.person, life.died_at))
    }
    fn mind(&self, id: PersonId) -> Option<&decisions::Mind> {
        game_characters::decisions::mind::<game_api::Command>(&self.0.host.storage, id)
    }
    fn stocks(&self, id: PersonId) -> Option<&game_characters::decisions::Stocks> {
        game_characters::decisions::stocks(&self.0.host.storage, id)
    }
    fn skills(&self, id: PersonId) -> Vec<SkillView> {
        self.0.own_skill_view(id)
    }
    fn institutions(&self, id: PersonId) -> Vec<InstitutionView> {
        self.0.institution_view(id)
    }
    fn cases(&self, id: PersonId) -> Vec<SocialCaseView> {
        self.0.social_case_view(id)
    }
}

struct ProjectedDeathMemory {
    incarnations: Vec<(PersonId, Option<SimTime>)>,
    minds: BTreeMap<PersonId, decisions::Mind>,
    stocks: BTreeMap<PersonId, game_characters::decisions::Stocks>,
    skills: BTreeMap<PersonId, Vec<SkillView>>,
    institutions: BTreeMap<PersonId, Vec<InstitutionView>>,
    cases: BTreeMap<PersonId, Vec<SocialCaseView>>,
}
impl game_invocation::ArchiveFacts<game_api::Command> for ProjectedDeathMemory {
    fn incarnations(&self) -> impl Iterator<Item = (PersonId, Option<SimTime>)> {
        self.incarnations.iter().copied()
    }
    fn mind(&self, actor: PersonId) -> Option<&decisions::Mind> {
        self.minds.get(&actor)
    }
    fn stocks(&self, actor: PersonId) -> Option<&game_characters::decisions::Stocks> {
        self.stocks.get(&actor)
    }
    fn skills(&self, actor: PersonId) -> Vec<SkillView> {
        self.skills.get(&actor).cloned().unwrap_or_default()
    }
    fn institutions(&self, actor: PersonId) -> Vec<InstitutionView> {
        self.institutions.get(&actor).cloned().unwrap_or_default()
    }
    fn cases(&self, actor: PersonId) -> Vec<SocialCaseView> {
        self.cases.get(&actor).cloned().unwrap_or_default()
    }
}
fn death_projection_read<'a, C: game_ecs::Component>(
    store: &'a game_ecs::Store,
    batch: &'a game_ecs::Batch,
    id: game_ecs::StableId,
    reads: &mut game_ecs::ReadSet,
    admitted: &mut game_ecs::ReadFootprint,
) -> Result<Option<&'a C>, Error> {
    reads.read::<C>(store, id);
    if batch.touches::<C>(id) {
        admitted.protect::<C>(id);
    }
    batch.projected::<C>(store, id)
}
pub(crate) struct StagedDeathInvocation {
    at: SimTime,
    runtime: World,
    lineage: game_lineage::Runtime,
    skill_views: bool,
    institution_views: bool,
    facts: game_ecs::ReadGuard,
    archives: game_invocation::ArchiveFinalization<game_api::Command>,
    control: Option<game_invocation::ControlReceipt>,
}
impl StagedDeathInvocation {
    pub(crate) fn validate(&self, sim: &Simulation, batch: &game_ecs::Batch) -> Result<(), Error> {
        if self.at != sim.host.clock().time
            || sim.host.state.invocation.as_ref() != Some(&self.runtime)
            || sim.host.state.lineage.as_ref() != Some(&self.lineage)
            || sim.host.state.skills.is_some() != self.skill_views
            || sim.host.state.institutions.is_some() != self.institution_views
        {
            return Err(Error::Unavailable);
        }
        self.facts.validate(&sim.host.storage, batch)?;
        self.archives.validate_batch(&sim.host.storage, batch)?;
        if let Some(control) = &self.control {
            control.validate_batch(&self.runtime.control, &sim.host.storage, batch)?;
        }
        Ok(())
    }
    pub(crate) fn guards(&self) -> impl Iterator<Item = game_ecs::ReadGuard> + '_ {
        [self.facts.clone(), self.archives.guard().clone()]
            .into_iter()
            .chain(self.control.as_ref().map(|control| control.guard().clone()))
    }
    pub(crate) fn finish(self, sim: &mut Simulation) {
        self.archives.finish(&mut sim.host.storage);
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingInvocation {
    at: SimTime,
    updates: Option<game_invocation::PreparedAdvance>,
    reached_cost: bool,
    event_demand: u64,
}
impl PendingInvocation {
    pub(crate) fn event_demand(&self) -> u64 {
        self.event_demand
    }
}

impl Simulation {
    /// Mortality receivers are prepared from the final owner projection before
    /// its originating Body/History changes or any records publish.
    pub(crate) fn prepare_death_invocation(
        &self,
        actor: PersonId,
        planned_history: Option<&game_lineage::History>,
        batch: &mut game_ecs::Batch,
    ) -> Result<Option<StagedDeathInvocation>, Error> {
        let Some(runtime) = &self.host.state.invocation else {
            return Ok(None);
        };
        let lineage = self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?;
        let store = &self.host.storage;
        let now = self.host.clock().time;
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        let body = death_projection_read::<game_physiology::Body>(
            store,
            batch,
            game_ecs::StableId(actor.0),
            &mut reads,
            &mut admitted,
        )?
        .ok_or(Error::Unavailable)?;
        game_invocation::validate_death_body(body.vitals.alive, body.vitals.health)?;
        let history = death_projection_read::<game_lineage::History>(
            store,
            batch,
            lineage.history,
            &mut reads,
            &mut admitted,
        )?
        .ok_or(Error::Unavailable)?;
        if planned_history.is_some_and(|expected| expected != history) {
            return Err(Error::Unavailable);
        }
        let incarnations = history
            .incarnations
            .iter()
            .map(|life| (life.person, life.died_at))
            .collect::<Vec<_>>();
        let subjects = runtime.archive_sources_for_death::<game_api::Command>(
            store,
            incarnations.iter().copied(),
            actor,
            now,
        );
        let mut facts = ProjectedDeathMemory {
            incarnations,
            minds: BTreeMap::new(),
            stocks: BTreeMap::new(),
            skills: BTreeMap::new(),
            institutions: BTreeMap::new(),
            cases: BTreeMap::new(),
        };
        for subject in subjects {
            let id = game_ecs::StableId(subject.0);
            let mind = death_projection_read::<decisions::Mind>(
                store,
                batch,
                id,
                &mut reads,
                &mut admitted,
            )?
            .ok_or(Error::Unavailable)?
            .clone();
            let stocks = death_projection_read::<game_characters::decisions::Stocks>(
                store,
                batch,
                id,
                &mut reads,
                &mut admitted,
            )?
            .ok_or(Error::Unavailable)?
            .clone();
            death_projection_read::<game_skills::Skills>(
                store,
                batch,
                id,
                &mut reads,
                &mut admitted,
            )?;
            let skills = self
                .host
                .state
                .skills
                .as_ref()
                .map(|owner| owner.view_projected(store, batch, subject))
                .transpose()?
                .unwrap_or_default();
            let mut institutions = Vec::new();
            let mut cases = Vec::new();
            if let Some(owner) = &self.host.state.institutions {
                death_projection_read::<game_society::Knowledge>(
                    store,
                    batch,
                    id,
                    &mut reads,
                    &mut admitted,
                )?;
                institutions = owner.institution_views_projected(store, batch, subject)?;
                cases = owner.social_case_views_projected(store, batch, subject)?;
                for case in &cases {
                    death_projection_read::<game_society::institutions::SocialCaseView>(
                        store,
                        batch,
                        game_ecs::StableId(case.id.0),
                        &mut reads,
                        &mut admitted,
                    )?;
                }
            }
            facts.minds.insert(subject, mind);
            facts.stocks.insert(subject, stocks);
            facts.skills.insert(subject, skills);
            facts.institutions.insert(subject, institutions);
            facts.cases.insert(subject, cases);
        }
        let archives =
            runtime.prepare_death_archives::<game_api::Command>(store, &facts, actor, now)?;
        let archives = archives.stage_projected_into(store, &facts, now, batch)?;
        let control = runtime
            .control
            .prepare_deaths(store, &BTreeSet::from([actor]), now)?;
        let control = if control.has_updates() {
            Some(control.stage_into(&runtime.control, store, batch)?)
        } else {
            None
        };
        let result = StagedDeathInvocation {
            at: now,
            runtime: runtime.clone(),
            lineage: lineage.clone(),
            skill_views: self.host.state.skills.is_some(),
            institution_views: self.host.state.institutions.is_some(),
            facts: reads.bind(store, batch, &admitted)?,
            archives,
            control,
        };
        result.validate(self, batch)?;
        Ok(Some(result))
    }
    pub fn invocation_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::skills_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "invocation-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.host.state.invocation = Some(World {
            control: game_invocation::ControlState::new(Rules::fixture())?,
        });
        // Separate compressed fixture rates leave room for sustained-use tests.
        let development = sim.host.state.development.as_mut().unwrap();
        development.rules.elderhood = SimDuration(3_600_000);
        development.rules.lifespan = SimDuration(5_400_000);
        for due in sim.host.state.due.values_mut() {
            due.retain(|event| !matches!(event, Due::Age(_)));
        }
        sim.host.state.due.retain(|_, due| !due.is_empty());
        for actor in sim.people().map(|p| p.id).collect::<Vec<_>>() {
            sim.schedule_age(actor);
        }
        // This fixture preserves a life that actually traveled, practiced, and
        // died before an independently conceived child's actual birth.
        let old = sim.host.state.ruler;
        sim.debug_move_actor(old, AreaId(3))?;
        sim.advance(SimDuration(180_000))?;
        let site = game_workshop::sites(&sim.host.storage)
            .map(|(_, site)| site)
            .find(|s| s.area == AreaId(3))
            .unwrap()
            .id;
        sim.debug_actor_command(old, Command::Work { site })?;
        sim.advance(MINUTE)?;
        let damage = sim.host.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(old, damage)?;
        sim.observe_invocation_deaths()?;
        sim.advance(MINUTE)?;
        Ok(sim)
    }
    pub(crate) fn observe_invocation_deaths(&mut self) -> Result<(), Error> {
        let Some(w) = &self.host.state.invocation else {
            return Ok(());
        };
        let Some(lineage) = &self.host.state.lineage else {
            return Ok(());
        };
        let _ = lineage;
        let now = self.host.clock().time;
        let facts = MemoryBindings(self);
        let archives = w.prepare_archives(&self.host.storage, &facts, now)?;
        let mut batch = game_ecs::Batch::new();
        let finish = archives.stage_into(&self.host.storage, &facts, now, &mut batch)?;
        finish.validate_batch(&self.host.storage, &batch)?;
        self.host.storage.apply_batch(batch)?;
        finish.finish(&mut self.host.storage);
        let prepared = self
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .control
            .prepare_observed_deaths(&self.host.storage, self.host.clock().time, |actor| {
                game_physiology::body(&self.host.storage, actor)
                    .is_some_and(|body| body.vitals.alive)
            })?;
        self.apply_invocation_control(prepared)?;
        Ok(())
    }
    fn invocation_control_actor(&self) -> Option<PersonId> {
        self.host
            .state
            .invocation
            .as_ref()?
            .control
            .actor(&self.host.storage)
    }
    fn invocation_admission(&self, actor: PersonId) -> game_invocation::AdmissionFacts {
        let lineage = self.host.state.lineage.as_ref().unwrap();
        let body = game_physiology::body(&self.host.storage, actor).unwrap();
        game_invocation::AdmissionFacts {
            actor,
            now: self.host.clock().time,
            current: lineage.current(&self.host.storage),
            alive: body.vitals.alive,
            health: body.vitals.health,
            independent: self.independent(actor),
            learning: game_lineage::invocation_learning(&self.host.storage, actor),
            required_learning: lineage.rules.invocation_access_threshold,
            fatigue: body.vitals.fatigue,
            fatigue_limit: self
                .host
                .state
                .physical
                .as_ref()
                .unwrap()
                .rules
                .fatigue_limit,
            archived: game_invocation::archives::<game_api::Command>(&self.host.storage)
                .map(|(id, _)| id)
                .collect(),
        }
    }
    pub(crate) fn past_life_controls_actor(&self, actor: PersonId) -> bool {
        self.host.state.invocation.as_ref().is_some_and(|runtime| {
            runtime.control.controls(
                &self.host.storage,
                actor,
                self.host.clock().time,
                game_physiology::body(&self.host.storage, actor)
                    .is_some_and(|body| body.vitals.alive),
            )
        })
    }
    pub(crate) fn required_invocation_input(&self) -> Option<WarningId> {
        self.host
            .state
            .invocation
            .as_ref()?
            .control
            .pending_consent(&self.host.storage)
            .map(|w| w.id)
    }
    fn reconcile_invocation_control(&mut self) -> Result<(), Error> {
        let Some(actor) = self.invocation_control_actor() else {
            return Ok(());
        };
        let facts = self.invocation_admission(actor);
        let prepared = self
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .control
            .prepare_review(
                &self.host.storage,
                &facts,
                Some(WarningId(self.host.next_id())),
            )?;
        self.apply_invocation_control(prepared)
    }
    pub(crate) fn invocation_command(
        &mut self,
        actor: PersonId,
        action: &InvocationAction,
    ) -> Result<(), Error> {
        let mut selected = BTreeSet::from([game_ecs::StableId(actor.0)]);
        match action {
            InvocationAction::Activate { lives } => {
                selected.extend(lives.iter().map(|id| game_ecs::StableId(id.0)))
            }
            InvocationAction::RecordMemory {
                life,
                inventory,
                material,
                tool,
            } => selected.extend([
                game_ecs::StableId(life.0),
                game_ecs::StableId(inventory.0),
                game_ecs::StableId(material.0),
                game_ecs::StableId(tool.0),
            ]),
            InvocationAction::RecordRoute {
                life,
                origin,
                destination,
                material,
                tool,
            } => selected.extend([
                game_ecs::StableId(life.0),
                game_ecs::StableId(origin.0),
                game_ecs::StableId(destination.0),
                game_ecs::StableId(material.0),
                game_ecs::StableId(tool.0),
            ]),
            InvocationAction::TeachMemory {
                life,
                inventory,
                recipient,
            } => selected.extend([
                game_ecs::StableId(life.0),
                game_ecs::StableId(inventory.0),
                game_ecs::StableId(recipient.0),
            ]),
            InvocationAction::ReadRecord { record }
            | InvocationAction::MoveRecord { record, .. } => {
                selected.insert(game_ecs::StableId(record.0));
                if let Some(row) = game_invocation::record(&self.host.storage, *record) {
                    selected.insert(game_ecs::StableId(row.inventory.0));
                }
            }
            _ => {}
        }
        if let InvocationAction::MoveRecord { destination, .. } = action {
            selected.insert(match destination {
                StockPosition::Area(id) => game_ecs::StableId(id.0),
                StockPosition::Carried(id) => game_ecs::StableId(id.0),
            });
        }
        self.publish_invocation_preview(selected, |preview| {
            preview.invocation_command_inner(actor, action)?;
            preview.update_property_events()?;
            preview.reconcile_agreements()?;
            preview.observe_agreement_events();
            preview.host.bump_revision(1)
        })
    }
    fn invocation_command_inner(
        &mut self,
        actor: PersonId,
        action: &InvocationAction,
    ) -> Result<(), Error> {
        let control = action.control_action();
        if let Some(action) = control {
            let w = self
                .host
                .state
                .invocation
                .as_ref()
                .ok_or(Error::Unavailable)?;
            if !game_physiology::body(&self.host.storage, actor).is_some() {
                return Err(Error::Unavailable);
            }
            let facts = self.invocation_admission(actor);
            let prepared = w.control.prepare_action(
                &self.host.storage,
                actor,
                &action,
                &facts,
                Some(WarningId(self.host.next_id())),
            )?;
            self.apply_invocation_control_inner(prepared)
        } else {
            self.start_memory_action(actor, action)
        }
    }
    /// Checks both owner plans before committing cost/control records. Physical
    /// charges, action cleanup, age/death and events remain owner effects here.
    fn apply_invocation_control(
        &mut self,
        prepared: game_invocation::PreparedControl,
    ) -> Result<(), Error> {
        use game_invocation::ControlEffect::*;
        let mut selected: BTreeSet<_> = prepared
            .effects
            .iter()
            .map(|effect| {
                let actor = match effect {
                    WarningRaised { actor, .. }
                    | WarningAccepted { actor, .. }
                    | Started { actor, .. }
                    | Dismissed { actor }
                    | FatigueCharge { actor, .. }
                    | CostApplied { actor, .. }
                    | ControlChanged { actor, .. }
                    | LifespanChanged { actor }
                    | DeathCleanup { actor }
                    | ControllerSelected { actor, .. } => *actor,
                };
                game_ecs::StableId(actor.0)
            })
            .collect();
        for effect in &prepared.effects {
            match effect {
                Started { lives, .. } => {
                    selected.extend(lives.iter().map(|id| game_ecs::StableId(id.0)))
                }
                ControllerSelected { life, .. } => {
                    selected.insert(game_ecs::StableId(life.0));
                }
                _ => {}
            }
        }
        self.publish_invocation_preview(selected, |preview| {
            preview.apply_invocation_control_inner(prepared)
        })
    }
    fn publish_invocation_preview(
        &mut self,
        selected: BTreeSet<game_ecs::StableId>,
        source: impl FnOnce(&mut Simulation) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let context = InvocationPreviewContext::capture(self, selected)?;
        let followups = self.prepare_property_source_followups(
            None,
            &game_ecs::Batch::new(),
            0,
            0,
            context.selected.clone(),
            |_| {},
            source,
        )?;
        let (mut transaction, finish) = followups.into_transaction(self)?;
        transaction
            .guards
            .push(context.bind(self, &transaction.batch)?);
        self.host.commit_transaction(transaction)?;
        finish.finish(self);
        Ok(())
    }
    fn apply_invocation_control_inner(
        &mut self,
        prepared: game_invocation::PreparedControl,
    ) -> Result<(), Error> {
        use game_invocation::ControlEffect;
        if prepared.at() != self.host.clock().time
            || prepared
                .admission()
                .is_some_and(|facts| self.invocation_admission(facts.actor) != *facts)
        {
            return Err(Error::Unavailable);
        }
        prepared.check(
            &self
                .host
                .state
                .invocation
                .as_ref()
                .ok_or(Error::Unavailable)?
                .control,
            &self.host.storage,
        )?;
        let warning = prepared.warning_id();
        if let Some(id) = warning {
            if id.0 != self.host.next_id() {
                return Err(Error::Unavailable);
            }
            self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
        }
        if prepared
            .effects
            .iter()
            .any(|e| matches!(e, ControlEffect::LifespanChanged { .. }))
        {
            self.host.next_id().checked_add(4).ok_or(Error::Overflow)?;
        }
        self.ensure_event_capacity(
            prepared.effects.len()
                + if prepared.effects.iter().any(|e| {
                    matches!(
                        e,
                        ControlEffect::LifespanChanged { .. }
                            | ControlEffect::ControlChanged {
                                controlled: false,
                                ..
                            }
                    )
                }) {
                    8
                } else {
                    0
                },
        )?;
        for effect in &prepared.effects {
            if let ControlEffect::DeathCleanup { actor } = effect {
                let body = game_physiology::body(&self.host.storage, *actor).unwrap();
                game_invocation::validate_death_cleanup(body.vitals.alive)?;
            }
        }
        let charges = prepared
            .effects
            .iter()
            .filter_map(|effect| {
                if let ControlEffect::FatigueCharge { actor, quantity } = effect {
                    Some(
                        game_physiology::prepare_fatigue_charge(
                            &self.host.storage,
                            *actor,
                            *quantity,
                            self.host
                                .state
                                .physical
                                .as_ref()
                                .unwrap()
                                .rules
                                .fatigue_limit,
                        )
                        .map(|charge| (*actor, charge)),
                    )
                } else {
                    None
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut lifespan = BTreeMap::new();
        if let Some(lifecycle) = &self.host.state.development {
            for effect in &prepared.effects {
                if let ControlEffect::LifespanChanged { actor } = effect {
                    let survival = game_physiology::prepare_survival(&self.host.storage, *actor)?;
                    let facts = game_lifecycle::LifespanFacts {
                        actor: *actor,
                        now: self.host.clock().time,
                        alive: survival.alive(),
                        lifespan_loss: prepared.lifespan_loss_after(&self.host.storage, *actor),
                    };
                    let response = lifecycle.prepare_lifespan_change(&self.host.storage, facts)?;
                    lifespan.insert(*actor, (facts, survival, response));
                }
            }
        }
        let effects = prepared.effects.clone();
        let route_reactions = effects
            .iter()
            .filter_map(|effect| match effect {
                ControlEffect::Dismissed { actor } => Some(*actor),
                _ => None,
            })
            .map(|actor| self.prepare_dismissed_route(actor))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let runtime = self.host.state.invocation.as_ref().unwrap();
        let memory_ending = runtime.prepare_memory_control_reactions(&self.host.storage, &effects);
        let canonical = self.canonical_state();
        let memory_resets = memory_ending.activity_resets(&MemorySaveFacts(&canonical));
        let mut batch = game_ecs::Batch::new();
        let control_receipt = prepared.stage_into(
            &self.host.state.invocation.as_ref().unwrap().control,
            &self.host.storage,
            &mut batch,
        )?;
        let mut fatigue_receipts = BTreeMap::new();
        for (actor, charge) in charges {
            fatigue_receipts.insert(
                actor,
                charge.stage_into_with_receipt(&self.host.storage, &mut batch)?,
            );
        }
        let memory_receipt =
            memory_ending.stage_into_with_guard(runtime, &self.host.storage, &mut batch)?;
        for actor in memory_resets {
            batch.insert(game_ecs::StableId(actor.0), Activity::Idle);
        }
        let route_admissions = route_reactions
            .into_iter()
            .map(|(recall, plan)| {
                let actor = plan.actor();
                let admission = plan.stage_into(&self.host.storage, &mut batch)?;
                Ok((actor, recall, admission))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        for (actor, recall, admission) in &route_admissions {
            recall.validate_batch::<game_api::Command>(
                &self.host.storage,
                game_transport::route_steps(&self.host.storage, *actor),
                &batch,
            )?;
            admission.validate_footprint(&self.host.storage, &batch)?;
        }
        control_receipt.validate_batch(
            &self.host.state.invocation.as_ref().unwrap().control,
            &self.host.storage,
            &batch,
        )?;
        memory_receipt.validate_batch(&self.host.storage, &batch)?;
        let lifespan = lifespan
            .into_iter()
            .map(|(actor, (mut facts, survival, response))| {
                facts.now = self.host.clock().time;
                facts.alive =
                    survival.check(&self.host.storage, &batch, fatigue_receipts.get(&actor))?;
                let response = response.check(
                    self.host.state.development.as_ref().unwrap(),
                    &self.host.storage,
                    facts,
                    &batch,
                )?;
                Ok((actor, response))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        let closing = control_receipt.closing_outcome(effects, |actor| {
            let alive = batch
                .projected::<game_physiology::Body>(
                    &self.host.storage,
                    game_ecs::StableId(actor.0),
                )?
                .ok_or(Error::Unavailable)?
                .vitals
                .alive;
            let lifespan = match lifespan.get(&actor) {
                Some(game_lifecycle::LifespanResponse::Died { .. }) => {
                    game_invocation::ClosingLifespan::Died
                }
                Some(game_lifecycle::LifespanResponse::Deadline { .. }) => {
                    game_invocation::ClosingLifespan::Survives
                }
                Some(game_lifecycle::LifespanResponse::Unchanged) | None => {
                    game_invocation::ClosingLifespan::Unchanged
                }
            };
            Ok(game_invocation::ClosingSurvival { alive, lifespan })
        })?;
        self.host.storage.apply_batch(batch)?;
        if warning.is_some() {
            self.host.allocate_ids(1)?;
        }
        for effect in closing.effects {
            match effect {
                ControlEffect::WarningRaised { warning, actor } => {
                    self.emit(EventKind::InvocationWarningRaised { warning, actor })
                }
                ControlEffect::WarningAccepted { warning, actor } => {
                    self.emit(EventKind::InvocationCostAccepted { warning, actor })
                }
                ControlEffect::FatigueCharge { .. } => {}
                ControlEffect::Started { actor, lives } => {
                    self.emit(EventKind::InvocationStarted { actor, lives });
                }
                ControlEffect::Dismissed { actor } => {
                    self.emit(EventKind::InvocationDismissed { actor });
                }
                ControlEffect::CostApplied {
                    actor,
                    cost,
                    severity,
                    permanent,
                } => self.emit(EventKind::InvocationCostApplied {
                    actor,
                    cost,
                    severity,
                    permanent,
                }),
                ControlEffect::ControlChanged {
                    actor,
                    controlled,
                    until,
                } => {
                    self.emit(EventKind::InvocationControlChanged {
                        actor,
                        controlled,
                        until,
                    });
                }
                ControlEffect::LifespanChanged { actor } => {
                    if let Some(response) = lifespan.get(&actor) {
                        self.apply_lifespan_response(*response)?;
                    }
                }
                ControlEffect::DeathCleanup { .. } => {}
                ControlEffect::ControllerSelected { .. } => {}
            }
        }
        if closing.review_controller {
            self.review_invocation()?;
        }
        if closing.observe_deaths {
            self.observe_invocation_deaths()?;
        }
        Ok(())
    }
    fn apply_lifespan_response(
        &mut self,
        response: game_lifecycle::LifespanResponse,
    ) -> Result<(), Error> {
        match response {
            game_lifecycle::LifespanResponse::Unchanged => {}
            game_lifecycle::LifespanResponse::Died { actor } => self.end_life(actor)?,
            game_lifecycle::LifespanResponse::Deadline { actor, at } => {
                for due in self.host.state.due.values_mut() {
                    due.retain(|due| !matches!(due,Due::Age(id)if *id==actor));
                }
                self.host.state.due.retain(|_, due| !due.is_empty());
                if let Some(at) = at {
                    self.host
                        .state
                        .due
                        .entry(at)
                        .or_default()
                        .push(Due::Age(actor));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn invocation_lifespan_loss(&self, actor: PersonId) -> u64 {
        self.host
            .state
            .invocation
            .as_ref()
            .and_then(|_| game_invocation::burden(&self.host.storage, actor))
            .map_or(0, |b| b.lifespan_loss)
    }

    pub(crate) fn invocation_rates(&self, actor: PersonId) -> (u64, u64) {
        let Some(runtime) = &self.host.state.invocation else {
            return (0, 0);
        };
        let rates = runtime
            .body_rates(
                &self.host.storage,
                actor,
                self.host.clock().time,
                game_physiology::body(&self.host.storage, actor)
                    .unwrap()
                    .vitals
                    .alive,
            )
            .expect("validated invocation rates");
        (rates.fatigue_per_mille, rates.health_harm_per_mille)
    }
    pub(crate) fn evaluate_invocation(&self, next: SimTime) -> Result<PendingInvocation, Error> {
        next.0
            .checked_sub(self.host.clock().time.0)
            .ok_or(Error::Overflow)?;
        let Some(w) = &self.host.state.invocation else {
            return Ok(PendingInvocation {
                at: next,
                updates: None,
                reached_cost: false,
                event_demand: 0,
            });
        };
        let updates = Some(w.control.prepare_advance(
            &self.host.storage,
            self.host.clock().time,
            next,
            |actor| {
                game_physiology::body(&self.host.storage, actor)
                    .is_some_and(|body| body.vitals.alive)
            },
        )?);
        self.host
            .next_id()
            .checked_add(self.invocation_allocation_bound(next))
            .ok_or(Error::Overflow)?;
        let event_demand = w.advance_event_demand(&self.host.storage)?;
        self.host
            .next_event()
            .checked_add(event_demand)
            .ok_or(Error::Overflow)?;
        Ok(PendingInvocation {
            at: next,
            updates,
            reached_cost: true,
            event_demand,
        })
    }
    pub(crate) fn invocation_allocation_bound(&self, next: SimTime) -> u64 {
        self.host.state.invocation.as_ref().map_or(0, |runtime| {
            runtime.allocation_bound(&self.host.storage, next)
        })
    }
    pub(crate) fn commit_invocation(&mut self, pending: PendingInvocation) -> Result<(), Error> {
        if self.host.state.invocation.is_none() {
            return Ok(());
        }
        debug_assert_eq!(pending.at, self.host.clock().time);
        if let Some(updates) = pending.updates {
            updates.commit(
                &self.host.state.invocation.as_ref().unwrap().control,
                &mut self.host.storage,
            )?;
        }
        if pending.reached_cost {
            self.reconcile_invocation_control()?;
        }
        self.finish_memory_actions()?;
        self.observe_invocation_deaths()?;
        Ok(())
    }
    pub(crate) fn invocation_boundary(&self, limit: SimTime) -> SimTime {
        let Some(w) = &self.host.state.invocation else {
            return limit;
        };
        let mut next =
            w.control
                .boundary_for(&self.host.storage, self.host.clock().time, limit, |actor| {
                    game_physiology::body(&self.host.storage, actor)
                        .is_some_and(|body| body.vitals.alive)
                });
        next = w.memory_boundary(&self.host.storage, self.host.clock().time, next);
        next
    }

    pub(crate) fn borrowed_skill_level(&self, actor: PersonId, skill: Skill) -> u16 {
        self.host.state.invocation.as_ref().map_or(0, |runtime| {
            runtime.borrowed_skill::<game_api::Command>(&self.host.storage, actor, skill)
        })
    }
    pub(crate) fn invocation_work_rate(
        &self,
        actor: PersonId,
        own: SkillRate,
        body: SkillRate,
    ) -> SkillRate {
        self.invocation_work_rate_for(actor, Skill::Craft, own, body)
    }
    pub(crate) fn invocation_work_rate_for(
        &self,
        actor: PersonId,
        skill: Skill,
        own: SkillRate,
        body: SkillRate,
    ) -> SkillRate {
        let Some(runtime) = &self.host.state.invocation else {
            return own;
        };
        runtime
            .work_rate::<game_api::Command>(
                &self.host.storage,
                actor,
                skill,
                own,
                body,
                game_skills::bonus_rates(self.host.state.skills.as_ref()).work_bonus_per_level,
            )
            .expect("validated frozen body and invocation rate bounds")
    }
    pub(crate) fn invocation_combat_damage(
        &self,
        actor: PersonId,
        base: u64,
    ) -> Result<u64, Error> {
        let Some(runtime) = &self.host.state.invocation else {
            return Ok(base);
        };
        runtime.combat_damage::<game_api::Command>(
            &self.host.storage,
            actor,
            base,
            self.own_skill_level(actor, Skill::Combat),
            game_skills::bonus_rates(self.host.state.skills.as_ref()).damage_bonus_per_level,
        )
    }
    pub(crate) fn invocation_routes(
        &self,
        actor: PersonId,
    ) -> Option<BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>> {
        self.host
            .state
            .invocation
            .as_ref()?
            .routes_for::<game_api::Command>(
                &self.host.storage,
                actor,
                &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)?
                    .routes,
            )
    }
    pub(crate) fn prepare_dismissed_route(
        &self,
        actor: PersonId,
    ) -> Result<
        Option<(
            game_invocation::PreparedRecallLoss,
            game_transport::PreparedRoute,
        )>,
        Error,
    > {
        let Some(recall) = game_invocation::prepare_recall_loss::<game_api::Command>(
            &self.host.storage,
            actor,
            game_transport::route_steps(&self.host.storage, actor),
        )?
        else {
            return Ok(None);
        };
        let route = game_transport::prepare_route_replacement(
            &self.host.storage,
            actor,
            recall.replacement().to_vec(),
        )?;
        Ok(Some((recall, route)))
    }
    pub(crate) fn invocation_personality(&self, actor: PersonId) -> Option<decisions::Personality> {
        self.host
            .state
            .invocation
            .as_ref()?
            .invoked_personality::<game_api::Command>(&self.host.storage, actor)
    }
    pub(crate) fn invocation_social_penalty(&self, actor: PersonId) -> i64 {
        self.host.state.invocation.as_ref().map_or(0, |runtime| {
            runtime.social_penalty(&self.host.storage, actor)
        })
    }
    pub(crate) fn review_invocation(&mut self) -> Result<(), Error> {
        let Some(prepared) = self.prepare_invocation_controller()? else {
            return Ok(());
        };
        let review = {
            let roster = game_spatial::LocalRoster::build(&self.host.storage)?;
            self.prepare_invocation_review(prepared, &roster)
        };
        self.apply_invocation_review(review)
    }
    pub(crate) fn prepare_invocation_controller(
        &self,
    ) -> Result<Option<game_invocation::PreparedControl>, Error> {
        let Some(control) = self
            .host
            .state
            .invocation
            .as_ref()
            .map(|runtime| &runtime.control)
        else {
            return Ok(None);
        };
        let Some(actor) = control.actor(&self.host.storage) else {
            return Ok(None);
        };
        let alive = game_physiology::body(&self.host.storage, actor)
            .unwrap()
            .vitals
            .alive;
        control.prepare_controller(&self.host.storage, self.host.clock().time, alive)
    }
    pub(crate) fn prepare_invocation_review(
        &self,
        prepared: game_invocation::PreparedControl,
        roster: &game_spatial::LocalRoster<'_>,
    ) -> PreparedInvocationReview {
        let subject = prepared
            .controller_subject()
            .expect("prepared controller subject");
        let actor = subject.actor;
        let area = match *game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0))
            .expect("location exists")
        {
            Location::AtArea(area) => Some(area),
            _ => None,
        };
        let local = area
            .into_iter()
            .flat_map(|area| roster.raw_at_area(area))
            .filter_map(|id| {
                let person = PersonId(id.0);
                let body = game_physiology::body(&self.host.storage, person)?;
                Some(game_invocation::ControllerPerson {
                    person,
                    alive: body.vitals.alive,
                    contact: self.transport_same_place(actor, person),
                })
            });
        self.host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .prepare_controller_review::<game_api::Command>(
                &self.host.storage,
                prepared,
                &ControllerBindings(self),
                local,
            )
            .expect("validated controller memories")
    }
    pub(crate) fn apply_invocation_review(
        &mut self,
        review: PreparedInvocationReview,
    ) -> Result<(), Error> {
        let control = &self.host.state.invocation.as_ref().unwrap().control;
        let mut batch = game_ecs::Batch::new();
        let receipt = review.stage_into(
            control,
            &self.host.storage,
            &ControllerBindings(self),
            &mut batch,
        )?;
        receipt.validate_batch(control, &self.host.storage, &batch)?;
        self.host.storage.apply_batch(batch)?;
        let actor = receipt.actor;
        let decision = receipt.decision;
        if let game_invocation::ControllerDecision::Act { action, ready } = decision {
            let result = if ready {
                self.apply_invocation_action(actor, action)
            } else {
                Err(Error::Unavailable)
            };
            if result.is_err()
                && let Some(fallback) = decision.after_failure()
            {
                let _ = self.apply_invocation_action(actor, fallback);
            }
        }
        Ok(())
    }
    fn apply_invocation_action(
        &mut self,
        actor: PersonId,
        action: game_invocation::ControllerAction,
    ) -> Result<(), Error> {
        use game_invocation::ControllerAction;
        match action {
            ControllerAction::Move { destination } => self.start_movement(actor, destination),
            ControllerAction::Engage { target } => self
                .debug_start_encounter(vec![(actor, 0), (target, 1)])
                .map(|_| ()),
            ControllerAction::Rest => self.debug_actor_command(actor, Command::Rest),
            ControllerAction::Cancel => self.debug_actor_command(actor, Command::Cancel),
            ControllerAction::Eat { source } => {
                self.debug_actor_command(actor, Command::Eat { source })
            }
            ControllerAction::Care { child, source } => {
                self.debug_actor_command(actor, Command::Care { child, source })
            }
            ControllerAction::Work { site } => {
                self.debug_actor_command(actor, Command::Work { site })
            }
        }
    }
    pub fn invocation_preview(
        &self,
        lives: BTreeSet<PersonId>,
    ) -> Result<InvocationPreview, Error> {
        self.host
            .state
            .invocation
            .as_ref()
            .ok_or(Error::Unavailable)?
            .preview::<game_api::Command>(
                &self.host.storage,
                &self.invocation_admission(self.host.state.ruler),
                lives,
            )
    }
    pub(crate) fn invocation_view(&self) -> Option<InvocationView> {
        let w = self.host.state.invocation.as_ref()?;
        let actor = self.host.state.ruler;
        let view = w
            .state_view::<game_api::Command>(
                &self.host.storage,
                &self.invocation_admission(actor),
                &MemoryBindings(self),
            )
            .ok()?;
        let b = view.burden;
        Some(InvocationView {
            ready: view.ready,
            available_lives: view.available_lives,
            active_lives: view.active_lives,
            burden: b.burden,
            exposure: SimDuration(b.exposure),
            uninterrupted: SimDuration(b.uninterrupted),
            temporary_identity: b.temporary_identity,
            emotional_disruption: b.emotional_disruption,
            permanent_identity: b.permanent_identity,
            lifespan_loss: SimDuration(b.lifespan_loss),
            player_control: view.player_control,
            loss_ends: view.loss_ends,
            warning: view.warning,
            preview: view.preview,
            memories: view.memories,
            records: view.records,
        })
    }
    pub(crate) fn available_invocation_commands(&self, actor: PersonId) -> Vec<Command> {
        self.host
            .state
            .invocation
            .as_ref()
            .map_or_else(Vec::new, |runtime| {
                runtime
                    .available_invocation_actions(
                        &self.host.storage,
                        &self.invocation_admission(actor),
                        &MemoryBindings(self),
                    )
                    .into_iter()
                    .map(|action| Command::Invocation { action })
                    .collect()
            })
    }

    pub(crate) fn invocation_selected_tool(&self, actor: PersonId) -> Option<InventoryId> {
        self.host
            .state
            .invocation
            .as_ref()?
            .selected_memory_tool(&self.host.storage, actor)
    }
    fn start_memory_action(
        &mut self,
        actor: PersonId,
        request: &InvocationAction,
    ) -> Result<(), Error> {
        let runtime = self
            .host
            .state
            .invocation
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let now = self.host.clock().time;
        let plan = runtime.prepare_memory_start::<game_api::Command>(
            &self.host.storage,
            &MemoryBindings(self),
            actor,
            request,
            now,
        )?;
        self.host.next_id().checked_add(4).ok_or(Error::Overflow)?;
        let action = plan.action().clone();
        let participants = action.starting_participants();
        let actors: Vec<_> = participants.iter().map(|(id, _)| *id).collect();
        let final_activities: BTreeMap<_, _> = participants
            .into_iter()
            .map(|(id, activity)| (id, bound_memory_activity(activity)))
            .collect();
        let interruption =
            self.prepare_interruptions(&actors, crate::commands::ActivityOwner::Invocation)?;
        self.ensure_event_capacity(
            usize::try_from(
                interruption
                    .event_count()?
                    .checked_add(u64::from(action.material_consumption().is_some()))
                    .ok_or(Error::Overflow)?,
            )
            .map_err(|_| Error::Overflow)?
            .max(8),
        )?;
        let mut batch = game_ecs::Batch::new();
        let (_, replacement) = plan.stage_into_with_replacement::<game_api::Command>(
            runtime,
            &self.host.storage,
            &MemoryBindings(self),
            now,
            &mut batch,
        )?;
        let mut interruption = interruption.stage_into_with_memory_replacement(
            self,
            &mut batch,
            &final_activities,
            Some(&replacement),
        )?;
        interruption.validate_batch(self, &batch)?;
        replacement.validate_batch(&self.host.storage, &batch)?;
        self.host.storage.apply_batch(batch)?;
        interruption.finish_actor(self, actor);
        if let Some((source, quantity)) = action.material_consumption() {
            self.emit(EventKind::Consumed {
                actor,
                source,
                quantity,
            });
        }
        for participant in actors.into_iter().filter(|id| *id != actor) {
            interruption.finish_actor(self, participant);
        }
        Ok(())
    }
    fn finish_memory_actions(&mut self) -> Result<(), Error> {
        let now = self.host.clock().time;
        let due = self
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .due_memory_actions(&self.host.storage, now);
        for actor in due {
            let runtime = self.host.state.invocation.as_ref().unwrap();
            let plan = runtime.prepare_memory_completion::<game_api::Command>(
                &self.host.storage,
                &MemoryBindings(self),
                actor,
                now,
                self.host.next_id(),
            )?;
            let allocation_demand = plan.allocation_demand();
            let canonical = self.canonical_state();
            let participants = plan.activity_resets(&MemorySaveFacts(&canonical));
            self.ensure_event_capacity(1)?;
            self.host
                .next_id()
                .checked_add(allocation_demand)
                .ok_or(Error::Overflow)?;
            let mut batch = game_ecs::Batch::new();
            let finish = plan.stage_into::<game_api::Command>(
                runtime,
                &self.host.storage,
                &MemoryBindings(self),
                now,
                &mut batch,
            )?;
            for id in participants {
                batch.insert(game_ecs::StableId(id.0), Activity::Idle);
            }
            finish.validate_batch::<game_api::Command>(&self.host.storage, &batch)?;
            let effect = finish.effect.clone();
            let receiving = match &effect {
                game_invocation::MemoryEffect::Recorded { inventory, .. } => Some(*inventory),
                game_invocation::MemoryEffect::Transferred { destination, .. } => {
                    Some(*destination)
                }
                _ => None,
            };
            let event = match effect {
                game_invocation::MemoryEffect::Recorded {
                    record,
                    actor,
                    inventory,
                } => Some(EventKind::MemoryRecorded {
                    record,
                    actor,
                    inventory,
                }),
                game_invocation::MemoryEffect::Read { record, actor } => {
                    Some(EventKind::MemoryRead { record, actor })
                }
                game_invocation::MemoryEffect::Taught {
                    actor,
                    recipient,
                    life,
                } => Some(EventKind::MemoryTaught {
                    actor,
                    recipient,
                    life,
                }),
                game_invocation::MemoryEffect::Transferred {
                    actor,
                    source,
                    destination,
                    quantity,
                } => Some(EventKind::Transferred {
                    actor,
                    source,
                    destination,
                    quantity,
                }),
                game_invocation::MemoryEffect::Interrupted { actor } => {
                    Some(EventKind::Interrupted { actor })
                }
                game_invocation::MemoryEffect::Cancelled => None,
            };
            let mut transaction =
                game_world::Transaction::new(self.host.transaction_opening(), batch);
            transaction.ids = allocation_demand;
            transaction.events.extend(event);
            self.host.commit_transaction(transaction)?;
            if let Some(id) = receiving {
                let stock = game_cargo::inventory(&self.host.storage, id)
                    .expect("admitted memory inventory");
                self.inventory_index
                    .entry(stock.location.clone())
                    .or_default()
                    .push(id);
            }
        }
        Ok(())
    }
    pub(crate) fn record_quantity_increment(&self, source: InventoryId) -> Quantity {
        game_invocation::record_quantity_increment(&self.host.storage, source)
    }
    pub(crate) fn record_transfer_allowed(&self, source: InventoryId, quantity: Quantity) -> bool {
        game_invocation::record_transfer_allowed(&self.host.storage, source, quantity)
    }
    pub(crate) fn relocate_record_transfer(
        &mut self,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    ) {
        let Some(runtime) = self.host.state.invocation.as_ref() else {
            return;
        };
        if let Some(effect) = runtime
            .prepare_record_receipt(&self.host.storage, source, destination, quantity)
            .expect("admitted record receipt")
        {
            effect
                .commit(&mut self.host.storage)
                .expect("admitted record receipt schema");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invocation_source_and_late_receiver_failure_leave_host_unchanged() {
        let mut sim = Simulation::invocation_fixture(91, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let before = sim.host.snapshot().unwrap();
        let clock = sim.host.clock();
        let result = sim.publish_invocation_preview(
            BTreeSet::from([game_ecs::StableId(actor.0)]),
            |preview| {
                let mut batch = game_ecs::Batch::new();
                batch.insert(game_ecs::StableId(actor.0), Activity::Idle);
                preview.host.storage.apply_batch(batch)?;
                preview.host.allocate_ids(1)?;
                preview
                    .host
                    .publish_many(vec![EventKind::Interrupted { actor }])?;
                Err(Error::Unavailable)
            },
        );
        assert_eq!(result, Err(Error::Unavailable));
        assert_eq!(sim.host.snapshot().unwrap(), before);
        assert_eq!(sim.host.clock(), clock);
    }

    #[test]
    fn invocation_audience_guard_rejects_a_new_local_native_entity() {
        let mut sim = Simulation::invocation_fixture(92, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let id = game_ecs::StableId(actor.0);
        let area = match *game_spatial::location(&sim.host.storage, id).unwrap() {
            Location::AtArea(area) => area,
            _ => panic!("fixture actor must be local"),
        };
        let context = InvocationPreviewContext::capture(&sim, BTreeSet::from([id])).unwrap();
        let newcomer = game_ecs::StableId(sim.host.next_id());
        sim.host
            .storage
            .insert_or_spawn(newcomer, Location::AtArea(area))
            .unwrap();
        assert!(context.bind(&sim, &game_ecs::Batch::new()).is_err());
    }

    fn submit(sim: &mut Simulation, command: Command) -> Result<CommandReceipt, Error> {
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence()?,
            command,
        })
    }
    fn act(sim: &mut Simulation, action: InvocationAction) -> Result<CommandReceipt, Error> {
        submit(sim, Command::Invocation { action })
    }
    pub(super) fn ready(workers: usize) -> Simulation {
        let mut sim = Simulation::invocation_fixture(
            9,
            Execution {
                workers,
                reverse_jobs: true,
            },
        )
        .unwrap();
        let actor = sim.host.state.ruler;
        let child = game_lifecycle::life(&sim.host.storage, actor)
            .expect("life exists")
            .born
            .0
            + sim.host.state.development.as_ref().unwrap().rules.infancy.0;
        sim.advance(SimDuration(child - sim.host.clock().time.0))
            .unwrap();
        let teacher = game_lineage::guardians(&sim.host.storage).next().unwrap().0;
        for _ in 0..2 {
            let mut learned = false;
            for _ in 0..12 {
                if matches!(
                    *game_spatial::location(&sim.host.storage, game_ecs::StableId((teacher).0))
                        .expect("location exists"),
                    Location::AtArea(_)
                ) {
                    sim.debug_actor_command(teacher, Command::Rest).unwrap();
                }
                if sim
                    .start_lesson(actor, teacher, LessonTopic::Invocation)
                    .is_ok()
                    && matches!(
                        *sim.host.storage.activity(actor).unwrap(),
                        Activity::Lesson { .. }
                    )
                {
                    sim.advance(MINUTE).unwrap();
                    learned = true;
                    break;
                }
                sim.advance(MINUTE).unwrap();
            }
            assert!(
                learned,
                "guardian and pupil must actually complete a lesson"
            );
        }
        assert!(sim.lineage_view().unwrap().invocation_ready);
        let adult = game_lifecycle::life(&sim.host.storage, actor)
            .expect("life exists")
            .born
            .0
            + sim
                .host
                .state
                .development
                .as_ref()
                .unwrap()
                .rules
                .adulthood
                .0;
        if sim.host.clock().time.0 < adult {
            sim.advance(SimDuration(adult - sim.host.clock().time.0))
                .unwrap();
        }
        // The fixture's living custodians explicitly permit this person to work;
        // birth itself never supplied these rights or transferred the estate.
        for __stock_id in game_cargo::inventories(&sim.host.storage)
            .filter(|(_, i)| matches!(i.location, InventoryLocation::Area(_)))
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            let inventory = game_cargo::inventory_mut(&mut sim.host.storage, __stock_id)
                .expect("inventory exists");
            inventory.access.insert(actor);
        }
        let sites: Vec<_> = game_workshop::sites(&sim.host.storage)
            .map(|(id, _)| id)
            .collect();
        for id in sites {
            game_workshop::site_mut(&mut sim.host.storage, id)
                .unwrap()
                .access
                .insert(actor);
        }
        let area = match *game_spatial::location(&sim.host.storage, game_ecs::StableId((actor).0))
            .expect("location exists")
        {
            Location::AtArea(area) => area,
            _ => panic!("pupil stayed local"),
        };
        sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(area),
            Quantity(100_000),
            Quantity(100_000),
            BTreeSet::from([actor]),
        )
        .unwrap();
        sim.debug_actor_command(actor, Command::Rest).unwrap();
        sim.advance(MINUTE).unwrap();
        sim
    }
    fn past(sim: &Simulation) -> PersonId {
        game_invocation::archives::<game_api::Command>(&sim.host.storage)
            .next()
            .unwrap()
            .0
    }
    fn activate(sim: &mut Simulation, lives: BTreeSet<PersonId>) {
        act(sim, InvocationAction::Activate { lives }).unwrap();
        for _ in 0..16 {
            let Some(warning) = sim.required_invocation_input() else {
                return;
            };
            let kind = sim
                .host
                .state
                .invocation
                .as_ref()
                .unwrap()
                .control
                .pending_warning(&sim.host.storage)
                .unwrap()
                .cost;
            assert_ne!(kind, InvocationCost::LossOfControl);
            act(sim, InvocationAction::AcceptWarning { warning }).unwrap();
        }
        panic!("activation consent did not settle");
    }
    fn approach_loss(sim: &mut Simulation) -> WarningId {
        for _ in 0..20 {
            sim.advance(SimDuration(1_000_000)).unwrap();
            let warning = sim
                .host
                .state
                .invocation
                .as_ref()
                .unwrap()
                .control
                .pending_warning(&sim.host.storage)
                .expect("new cost must stop time")
                .clone();
            if warning.cost == InvocationCost::LossOfControl {
                return warning.id;
            }
            act(
                sim,
                InvocationAction::AcceptWarning {
                    warning: warning.id,
                },
            )
            .unwrap();
        }
        panic!("loss threshold unreachable");
    }
    #[test]
    fn access_requires_real_guidance_and_activation_costs_precede_benefits() {
        let mut sim = Simulation::invocation_fixture(9, Execution::default()).unwrap();
        let life = past(&sim);
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            act(
                &mut sim,
                InvocationAction::Activate {
                    lives: BTreeSet::from([life])
                }
            ),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        let mut sim = ready(1);
        let actor = sim.host.state.ruler;
        let life = past(&sim);
        let fatigue = game_physiology::body(&sim.host.storage, actor)
            .unwrap()
            .vitals
            .fatigue;
        act(
            &mut sim,
            InvocationAction::Activate {
                lives: BTreeSet::from([life]),
            },
        )
        .unwrap();
        assert!(
            sim.host
                .state
                .invocation
                .as_ref()
                .unwrap()
                .control
                .active(&sim.host.storage)
                .is_none()
        );
        assert_eq!(sim.borrowed_skill_level(actor, Skill::Craft), 0);
        let warning = sim
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .control
            .pending_warning(&sim.host.storage)
            .unwrap()
            .clone();
        assert!(warning.instant_fatigue > 0 && warning.activation_burden > 0);
        act(
            &mut sim,
            InvocationAction::AcceptWarning {
                warning: warning.id,
            },
        )
        .unwrap();
        assert!(sim.borrowed_skill_level(actor, Skill::Craft) > 0);
        let burden = game_invocation::burden(&sim.host.storage, actor)
            .unwrap()
            .burden;
        assert_eq!(
            game_physiology::body(&sim.host.storage, actor)
                .unwrap()
                .vitals
                .fatigue,
            fatigue + warning.instant_fatigue
        );
        act(&mut sim, InvocationAction::Dismiss).unwrap();
        assert_eq!(
            game_invocation::burden(&sim.host.storage, actor)
                .unwrap()
                .burden,
            burden
        );
    }
    #[test]
    fn borrowed_work_is_actual_and_ends_on_dismissal_without_copying_mastery() {
        let mut borrowed = ready(1);
        let actor = borrowed.host.state.ruler;
        let life = past(&borrowed);
        let site = game_workshop::sites(&borrowed.host.storage)
            .map(|(_, site)| site)
            .find(|s| s.area == AreaId(1))
            .unwrap()
            .id;
        // Compare the same free workstation; existing citizens may already be
        // using its single toolkit under the fixture's ordinary autonomy.
        let other_workers: Vec<_> = borrowed
            .host
            .storage
            .activities()
            .filter(|(id, activity)| {
                *id != actor && matches!(**activity,Activity::Work {site:working}if working==site)
            })
            .map(|(id, _)| id)
            .collect();
        for other in other_workers {
            borrowed.debug_actor_command(other, Command::Rest).unwrap();
        }
        let mut baseline =
            Simulation::load(&borrowed.save().unwrap(), Execution::default()).unwrap();
        activate(&mut borrowed, BTreeSet::from([life]));
        for sim in [&mut borrowed, &mut baseline] {
            submit(sim, Command::Work { site }).unwrap();
            sim.advance(SimDuration(30_000)).unwrap();
        }
        let a = game_workshop::job(&borrowed.host.storage, actor)
            .unwrap()
            .remaining;
        let b = game_workshop::job(&baseline.host.storage, actor)
            .unwrap()
            .remaining;
        assert!(a < b);
        assert_eq!(
            borrowed.own_skill_level(actor, Skill::Craft),
            baseline.own_skill_level(actor, Skill::Craft)
        );
        act(&mut borrowed, InvocationAction::Dismiss).unwrap();
        borrowed.advance(SimDuration(10_000)).unwrap();
        assert_eq!(
            game_workshop::job(&borrowed.host.storage, actor)
                .unwrap()
                .remaining,
            a - 10_000
        );
    }
    #[test]
    fn every_new_cost_stops_before_effect_and_rest_chunking_is_exact() {
        let mut whole = ready(1);
        let actor = whole.host.state.ruler;
        let life = past(&whole);
        activate(&mut whole, BTreeSet::from([life]));
        let mut sliced = Simulation::load(
            &whole.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        whole.advance(SimDuration(1_000_000)).unwrap();
        for _ in 0..30 {
            if sliced.required_invocation_input().is_some() {
                break;
            }
            sliced.advance(SimDuration(7_919)).unwrap();
        }
        assert_eq!(whole.canonical_state(), sliced.canonical_state());
        let warning = whole
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .control
            .pending_warning(&whole.host.storage)
            .unwrap()
            .clone();
        assert_eq!(warning.cost, InvocationCost::Strain);
        assert_eq!(
            game_invocation::burden(&whole.host.storage, actor)
                .unwrap()
                .emotional_disruption,
            0
        );
        for sim in [&mut whole, &mut sliced] {
            act(
                sim,
                InvocationAction::AcceptWarning {
                    warning: warning.id,
                },
            )
            .unwrap();
            submit(sim, Command::Rest).unwrap();
        }
        whole.advance(SimDuration(40_013)).unwrap();
        for amount in [3, 11_117, 28_893] {
            sliced.advance(SimDuration(amount)).unwrap();
        }
        assert_eq!(whole.canonical_state(), sliced.canonical_state());
    }
    #[test]
    fn a_written_record_requires_material_time_and_reading_after_dismissal() {
        let mut sim = ready(1);
        let actor = sim.host.state.ruler;
        let life = past(&sim);
        let remote = game_invocation::archive::<game_api::Command>(&sim.host.storage, life)
            .unwrap()
            .stocks
            .known
            .values()
            .find(|k| k.area == AreaId(3) && k.good == Good::Timber)
            .unwrap()
            .inventory;
        assert!(
            !game_characters::decisions::stocks(&sim.host.storage, actor)
                .expect("mind exists")
                .known
                .contains_key(&remote)
        );
        let material = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Carried(actor),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let tool = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(actor),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        activate(&mut sim, BTreeSet::from([life]));
        act(
            &mut sim,
            InvocationAction::RecordMemory {
                life,
                inventory: remote,
                material,
                tool,
            },
        )
        .unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, material)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        sim.advance(SimDuration(59_999)).unwrap();
        assert!(game_invocation::records(&sim.host.storage).next().is_none());
        sim.advance(SimDuration(1)).unwrap();
        let record = game_invocation::records(&sim.host.storage)
            .next()
            .unwrap()
            .0;
        for _ in 0..10 {
            assert!(sim.invocation_view().unwrap().records[0].content.is_none());
        }
        act(&mut sim, InvocationAction::Dismiss).unwrap();
        assert!(sim.invocation_view().unwrap().memories.is_empty());
        assert!(
            !game_characters::decisions::stocks(&sim.host.storage, actor)
                .expect("mind exists")
                .known
                .contains_key(&remote)
        );
        act(&mut sim, InvocationAction::ReadRecord { record }).unwrap();
        sim.advance(MINUTE).unwrap();
        assert!(
            matches!(game_characters::decisions::stocks(&sim.host.storage,actor).expect("mind exists").known[&remote].source,KnowledgeSource::Recorded(id)if id==record)
        );
        assert!(
            !game_characters::decisions::stocks(&sim.host.storage, actor)
                .expect("mind exists")
                .known[&remote]
                .accessible
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn accepted_control_loss_blocks_actions_and_continues_costs_without_more_consent() {
        let mut sim = ready(1);
        let actor = sim.host.state.ruler;
        let life = past(&sim);
        activate(&mut sim, BTreeSet::from([life]));
        let warning = approach_loss(&mut sim);
        act(&mut sim, InvocationAction::AcceptWarning { warning }).unwrap();
        assert!(sim.past_life_controls_actor(actor));
        let ends = sim
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .control
            .active(&sim.host.storage)
            .unwrap()
            .episode
            .as_ref()
            .unwrap()
            .ends;
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(submit(&mut sim, Command::Rest), Err(Error::Unavailable));
        assert_eq!(
            act(&mut sim, InvocationAction::Dismiss),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        let mut restored = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        let before_queries = restored.save().unwrap();
        for _ in 0..8 {
            let _ = restored.invocation_view().unwrap();
            let _ = restored.available_invocation_commands(actor);
            let _ = restored.invocation_preview(BTreeSet::from([life])).unwrap();
            let _ = restored.canonical_snapshot().unwrap();
        }
        assert_eq!(restored.save().unwrap(), before_queries);
        sim.advance(SimDuration(ends.0 - sim.host.clock().time.0))
            .unwrap();
        while restored.host.clock().time < ends {
            restored
                .advance(SimDuration(
                    (ends.0 - restored.host.clock().time.0).min(17_711),
                ))
                .unwrap();
        }
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        assert!(!sim.past_life_controls_actor(actor));
        assert!(
            game_invocation::burden(&sim.host.storage, actor)
                .unwrap()
                .permanent_identity
                > 0
        );
        assert!(
            game_invocation::burden(&sim.host.storage, actor)
                .unwrap()
                .lifespan_loss
                > 0
        );
    }
    #[test]
    fn death_during_loss_is_not_undone_and_direct_penalties_do_not_bind_a_successor() {
        let mut sim = ready(1);
        let actor = sim.host.state.ruler;
        let life = past(&sim);
        activate(&mut sim, BTreeSet::from([life]));
        let warning = approach_loss(&mut sim);
        act(&mut sim, InvocationAction::AcceptWarning { warning }).unwrap();
        let damage = sim.host.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(actor, damage).unwrap();
        sim.observe_invocation_deaths().unwrap();
        assert!(
            sim.host
                .state
                .invocation
                .as_ref()
                .unwrap()
                .control
                .active(&sim.host.storage)
                .is_none()
        );
        assert!(
            !game_physiology::body(&sim.host.storage, actor)
                .unwrap()
                .vitals
                .alive
        );
        let before = sim.host.clock().time;
        for _ in 0..20 {
            sim.advance(MINUTE).unwrap();
            if sim.host.state.ruler != actor {
                break;
            }
        }
        assert!(sim.host.clock().time > before);
        if sim.host.state.ruler != actor {
            assert_eq!(sim.invocation_lifespan_loss(sim.host.state.ruler), 0);
            assert_eq!(
                game_invocation::burden(&sim.host.storage, sim.host.state.ruler)
                    .map_or(0, |b| b.permanent_identity),
                0
            );
            assert!(!sim.past_life_controls_actor(sim.host.state.ruler));
        }
    }
    #[test]
    fn an_accepted_past_controller_uses_its_own_comfort_goal_in_the_actual_body() {
        let mut sim = ready(1);
        let actor = sim.host.state.ruler;
        let life = past(&sim);
        sim.debug_set_personality(
            actor,
            decisions::Personality::industrious(),
            decisions::Aspiration::Independence,
        )
        .unwrap();
        let archived =
            &mut game_invocation::archive_mut::<game_api::Command>(&mut sim.host.storage, life)
                .unwrap()
                .mind;
        archived.dream = decisions::Aspiration::Comfort;
        archived.goal = decisions::Goal::Practice;
        activate(&mut sim, BTreeSet::from([life]));
        let warning = approach_loss(&mut sim);
        let body = game_physiology::body_mut(&mut sim.host.storage, actor).unwrap();
        body.vitals.hunger = 0;
        body.vitals.fatigue = 0;
        sim.host
            .storage
            .set_activity(actor, Activity::Idle)
            .unwrap();
        // AcceptWarning immediately runs the first controller turn. Do not
        // overwrite its action afterward or ask for a second turn before due.
        act(&mut sim, InvocationAction::AcceptWarning { warning }).unwrap();
        assert_eq!(
            sim.host
                .state
                .invocation
                .as_ref()
                .unwrap()
                .control
                .active(&sim.host.storage)
                .unwrap()
                .dominant,
            Some(life)
        );
        assert_eq!(sim.host.storage.activity(actor), Some(&Activity::Rest));
        assert_eq!(
            game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, actor)
                .unwrap()
                .dream,
            decisions::Aspiration::Independence
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn native_invocation_costs_and_warnings_restore_without_entity_map_shadows() {
        let mut sim = ready(1);
        let life = past(&sim);
        activate(&mut sim, BTreeSet::from([life]));
        let actor = sim.host.state.ruler;
        assert!(game_invocation::burden(&sim.host.storage, actor).is_some());
        assert!(
            game_invocation::warnings(&sim.host.storage)
                .next()
                .is_some()
        );
        let saved = sim.save().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        let control = &value["state"]["state"]["invocation"]["control"];
        assert_eq!(control.as_object().unwrap().len(), 1);
        for field in ["people", "warnings", "active", "activation", "warning"] {
            assert!(control.get(field).is_none());
            let mut forged = value.clone();
            forged["state"]["state"]["invocation"]["control"][field] = serde_json::json!({});
            assert!(
                Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default())
                    .is_err()
            );
        }
        for column in [
            "invocation/burden",
            "invocation/warning",
            "invocation/session",
        ] {
            assert!(
                !value["state"]["components"]["columns"][column]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
        let restored = Simulation::load(
            &saved,
            Execution {
                workers: 2,
                reverse_jobs: false,
            },
        )
        .unwrap();
        assert_eq!(restored.canonical_state(), sim.canonical_state());
        assert_eq!(restored.save().unwrap(), saved);
    }

    #[test]
    fn invocation_restore_rejects_native_orphans_warning_identity_and_pending_shadow() {
        let mut sim = ready(1);
        let life = past(&sim);
        act(
            &mut sim,
            InvocationAction::Activate {
                lives: BTreeSet::from([life]),
            },
        )
        .unwrap();
        let saved = sim.save().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        let area = sim.host.state.areas[0].id.0;
        for attack in 0..9 {
            let mut forged = value.clone();
            match attack {
                0 => forged["state"]["state"]["invocation"] = serde_json::Value::Null,
                1 => {
                    forged["state"]["components"]["columns"]["invocation/burden"][0]["entity"] =
                        serde_json::json!(area)
                }
                2 => {
                    forged["state"]["components"]["columns"]["invocation/warning"][0]["value"]["id"] =
                        serde_json::json!(999999)
                }
                3 => {
                    forged["state"]["components"]["columns"]["invocation/session"][0]["value"]["warning"]
                        ["id"] = serde_json::json!(999999)
                }
                4 => {
                    forged["state"]["components"]["columns"]["invocation/session"][0]["value"]["warning"]
                        ["view"] = serde_json::json!({})
                }
                5 => {
                    forged["state"]["components"]["columns"]["invocation/session"][0]["entity"] =
                        serde_json::json!(area)
                }
                6 => {
                    forged["state"]["components"]["columns"]["invocation/session"][0]["value"]["actor"] =
                        serde_json::json!(area)
                }
                7 => {
                    forged["state"]["components"]["columns"]["invocation/session"][0]["value"]["activation"]
                        ["lives"] = serde_json::json!([])
                }
                _ => {
                    let mut second =
                        forged["state"]["components"]["columns"]["invocation/session"][0].clone();
                    second["entity"] = serde_json::json!(area);
                    forged["state"]["components"]["columns"]["invocation/session"]
                        .as_array_mut()
                        .unwrap()
                        .push(second);
                }
            }
            assert!(
                Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default())
                    .is_err(),
                "invocation restore attack {attack}"
            );
            assert_eq!(sim.save().unwrap(), saved);
        }
    }
    #[test]
    fn native_memory_archives_actions_and_records_restore_and_reject_malformed_rows() {
        let mut sim = ready(1);
        let actor = sim.host.state.ruler;
        let life = past(&sim);
        let remote = game_invocation::archive::<game_api::Command>(&sim.host.storage, life)
            .unwrap()
            .stocks
            .known
            .keys()
            .next()
            .copied()
            .unwrap();
        let material = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Carried(actor),
                Quantity(2000),
                Quantity(2000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let tool = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(actor),
                Quantity(1000),
                Quantity(1000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        activate(&mut sim, BTreeSet::from([life]));
        act(
            &mut sim,
            InvocationAction::RecordMemory {
                life,
                inventory: remote,
                material,
                tool,
            },
        )
        .unwrap();
        let saved = sim.save().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            value["state"]["state"]["invocation"]
                .as_object()
                .unwrap()
                .len(),
            1
        );
        for column in ["invocation/archive", "invocation/action"] {
            assert!(
                !value["state"]["components"]["columns"][column]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
        for attack in 0..7 {
            let mut forged = value.clone();
            match attack {
                0 => forged["state"]["state"]["invocation"]["history"] = serde_json::json!({}),
                1 => {
                    forged["state"]["components"]["columns"]["invocation/archive"][0]["value"]["person"] =
                        serde_json::json!(actor.0)
                }
                2 => {
                    forged["state"]["components"]["columns"]["invocation/action"][0]["value"]["actor"] =
                        serde_json::json!(life.0)
                }
                3 => {
                    forged["state"]["components"]["columns"]["invocation/action"][0]["value"]["completes"] =
                        serde_json::json!(sim.host.clock().time.0)
                }
                4 => {
                    forged["state"]["components"]["columns"]["invocation/action"][0]["value"]["area"] =
                        serde_json::json!(u64::MAX)
                }
                5 => {
                    forged["state"]["components"]["columns"]["invocation/action"] =
                        serde_json::json!([])
                }
                _ => forged["state"]["state"]["invocation"] = serde_json::Value::Null,
            }
            assert!(
                Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default())
                    .is_err(),
                "memory restore attack {attack}"
            );
            assert_eq!(sim.save().unwrap(), saved);
        }
        let mut restored = Simulation::load(
            &saved,
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        for _ in 0..12 {
            restored.advance(SimDuration(5000)).unwrap();
        }
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        let saved = sim.save().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert!(
            !value["state"]["components"]["columns"]["invocation/record"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        for attack in 0..4 {
            let mut forged = value.clone();
            match attack {
                0 => {
                    forged["state"]["components"]["columns"]["invocation/record"][0]["value"]["id"] =
                        serde_json::json!(u64::MAX)
                }
                1 => {
                    forged["state"]["components"]["columns"]["invocation/record"][0]["value"]["readers"] =
                        serde_json::json!([u64::MAX])
                }
                2 => {
                    forged["state"]["components"]["columns"]["invocation/record"][0]["value"]["intact"] =
                        serde_json::json!(false)
                }
                _ => {
                    forged["state"]["components"]["columns"]["invocation/record"][0]["value"]["inventory"] =
                        serde_json::json!(material.0)
                }
            }
            assert!(
                Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default())
                    .is_err(),
                "record restore attack {attack}"
            );
            assert_eq!(sim.save().unwrap(), saved);
        }
    }
    #[test]
    fn native_session_restoration_rejects_future_start_and_episode_deadline_mismatch() {
        let mut sim = ready(1);
        let life = past(&sim);
        activate(&mut sim, BTreeSet::from([life]));
        let warning = approach_loss(&mut sim);
        act(&mut sim, InvocationAction::AcceptWarning { warning }).unwrap();
        let saved = sim.save().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        for attack in 0..4 {
            let mut forged = value.clone();
            let active = &mut forged["state"]["components"]["columns"]["invocation/session"][0]["value"]
                ["active"];
            match attack {
                0 => active["started"] = serde_json::json!(sim.host.clock().time.0 + 1),
                1 => active["episode"]["starts"] = serde_json::json!(sim.host.clock().time.0 + 1),
                2 => active["episode"]["ends"] = serde_json::json!(sim.host.clock().time.0),
                _ => active["next_controller"] = serde_json::json!(u64::MAX),
            }
            assert!(
                Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default())
                    .is_err()
            );
            assert_eq!(sim.save().unwrap(), saved);
        }
    }
}

#[cfg(test)]
mod mortality_tests {
    use super::*;

    #[test]
    fn closing_interval_learning_is_captured_exactly_in_the_same_death_transaction() {
        let mut sim = super::tests::ready(1);
        let actor = sim.host.state.ruler;
        let now = sim.host.clock().time;
        let runtime = sim.host.state.skills.as_ref().unwrap().clone();
        let body = game_physiology::body(&sim.host.storage, actor).unwrap();
        let mut evaluation = runtime
            .evaluate_learning(
                &sim.host.storage,
                SimTime(now.0 - MINUTE.0),
                now,
                [game_skills::ActorFacts {
                    actor,
                    alive: body.vitals.alive,
                    health: body.vitals.health,
                    at_area: true,
                    equipped: true,
                    activity: game_skills::ActivityFacts::Construct,
                    workshop_job: false,
                    care_work: None,
                    care_can_progress: false,
                }],
                [],
            )
            .unwrap();
        let learning = runtime
            .prepare_actor_learning(&sim.host.storage, &evaluation, actor)
            .unwrap();
        let death = game_physiology::prepare_death(&sim.host.storage, actor).unwrap();
        let cleanup = sim.prepare_death_cleanup(actor).unwrap();
        let opening = sim.host.transaction_opening();
        let mut batch = game_ecs::Batch::new();
        let body_guard = death.stage_into(&sim.host.storage, &mut batch).unwrap();
        let learning_guard = learning
            .stage_into(&runtime, &sim.host.storage, &mut batch)
            .unwrap();
        let expected = runtime
            .view_projected(&sim.host.storage, &batch, actor)
            .unwrap();
        assert!(
            expected
                .iter()
                .find(|row| row.skill == Skill::Craft)
                .unwrap()
                .practice_ms
                > runtime
                    .view(&sim.host.storage, actor)
                    .iter()
                    .find(|row| row.skill == Skill::Craft)
                    .unwrap()
                    .practice_ms
        );
        let cleanup = cleanup.stage_into(&sim, &mut batch).unwrap();
        let invocation = sim
            .prepare_death_invocation(actor, None, &mut batch)
            .unwrap()
            .unwrap();
        body_guard.validate(&sim.host.storage, &batch).unwrap();
        learning_guard
            .validate(&runtime, &sim.host.storage, &batch)
            .unwrap();
        cleanup.validate(&sim, &batch).unwrap();
        invocation.validate(&sim, &batch).unwrap();
        let acknowledgement = evaluation
            .prepare_acknowledgement(&runtime, &sim.host.storage, &batch, &learning_guard)
            .unwrap();
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.guards.push(learning_guard.row_guard().clone());
        transaction.guards.extend(cleanup.guards());
        transaction.guards.extend(invocation.guards());
        transaction.events = cleanup.events();
        sim.host.commit_transaction(transaction).unwrap();
        acknowledgement.finish();
        assert!(evaluation.deltas().iter().all(|delta| delta.actor != actor));
        cleanup.finish(&mut sim);
        invocation.finish(&mut sim);
        assert!(
            !game_physiology::body(&sim.host.storage, actor)
                .unwrap()
                .vitals
                .alive
        );
        assert_eq!(
            game_invocation::archive::<game_api::Command>(&sim.host.storage, actor)
                .unwrap()
                .skills,
            expected
        );
        assert_eq!(runtime.view(&sim.host.storage, actor), expected);
        runtime
            .prepare_learning(&sim.host.storage, evaluation)
            .unwrap()
            .commit(&runtime, &mut sim.host.storage)
            .unwrap();
        assert_eq!(runtime.view(&sim.host.storage, actor), expected);
        assert_eq!(
            game_invocation::archive::<game_api::Command>(&sim.host.storage, actor)
                .unwrap()
                .skills,
            expected
        );
        Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
    }
}

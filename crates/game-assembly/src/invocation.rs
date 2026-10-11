//! Invocation retains a separate memory source and a person-bound consent ledger.
//! Fractional burden and physiology use opening-state integer arithmetic.
use crate::*;

pub use game_invocation::{Activation, Active, Burden, Consent, Episode, Rules, Warning, severity};

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(w) = &s.state.invocation else {
        return game_invocation::validate_absent(s.components);
    };
    let fail = || Error::InvalidSave("invalid invocation state".into());
    let lineage = s.state.lineage.as_ref().ok_or_else(fail)?;
    s.state.physical.as_ref().ok_or_else(fail)?;
    if s.state.decisions.is_none() || s.state.skills.is_none() {
        return Err(fail());
    }
    let people: BTreeSet<_> = s.state.people.iter().map(|p| p.id).collect();
    let lives: BTreeSet<_> = lineage
        .incarnations
        .iter()
        .map(|life| life.person)
        .collect();
    let areas: BTreeSet<_> = s.state.areas.iter().map(|a| a.id).collect();
    let source_valid = |source: &KnowledgeSource| match source {
        KnowledgeSource::Observed(actor) | KnowledgeSource::Reported(actor) => {
            people.contains(actor)
        }
        KnowledgeSource::Recorded(record) => w.records.contains_key(record),
    };
    let fact_valid = |fact: &StockKnowledge| {
        game_cargo::inventory(s.components, fact.inventory)
            .is_some_and(|inventory| inventory.good == fact.good)
            && areas.contains(&fact.area)
            && fact.observed_at <= fact.learned_at
            && fact.learned_at <= s.time
            && source_valid(&fact.source)
    };
    for (id, past) in &w.history {
        if *id != past.person
            || !lineage
                .incarnations
                .iter()
                .any(|life| life.person == *id && life.died_at == Some(past.died_at))
            || past.died_at > s.time
            || game_physiology::body(s.components, *id)
                .unwrap()
                .vitals
                .alive
            || past
                .mind
                .knowledge
                .values()
                .any(|fact| !fact_valid(fact) || fact.observed_at > past.died_at)
            || past
                .mind
                .relationships
                .keys()
                .any(|id| !people.contains(id))
            || past.mind.routes.iter().any(|(from, links)| {
                !areas.contains(from)
                    || links
                        .iter()
                        .any(|(to, duration)| !areas.contains(to) || duration.0 == 0)
            })
        {
            return Err(fail());
        }
        let mut skills = BTreeSet::new();
        if past
            .skills
            .iter()
            .any(|skill| !skills.insert(skill.skill) || skill.level > 100)
        {
            return Err(fail());
        }
    }
    w.control.validate(
        s.components,
        game_invocation::ControlValidation {
            now: s.time,
            next_id: s.next_id,
            current: lineage.current(),
            incarnations: &lives,
            archived: &w.history.keys().copied().collect(),
        },
    )?;
    let content_valid = |content: &RecordedMemory| match content {
        RecordedMemory::Stock { life, fact } => w.history.contains_key(life) && fact_valid(fact),
        RecordedMemory::Route {
            life,
            origin,
            destination,
            duration,
        } => {
            w.history.contains_key(life)
                && areas.contains(origin)
                && areas.contains(destination)
                && origin != destination
                && duration.0 > 0
        }
    };
    let mut totals: BTreeMap<InventoryId, u64> = BTreeMap::new();
    for (id, record) in &w.records {
        let stock = game_cargo::inventory(s.components, record.inventory).ok_or_else(fail)?;
        if id.0 == 0
            || id.0 >= s.next_id
            || *id != record.id
            || !people.contains(&record.author)
            || !record.readers.is_subset(&people)
            || record.written_at > s.time
            || !content_valid(&record.content)
            || stock.good != Good::Record
        {
            return Err(fail());
        }
        if record.intact {
            *totals.entry(record.inventory).or_default() += 1_000;
        }
    }
    for stock in game_cargo::inventories(s.components)
        .map(|(_, row)| row)
        .filter(|stock| stock.good == Good::Record)
    {
        if stock.quantity.0 != totals.get(&stock.id).copied().unwrap_or(0) {
            return Err(fail());
        }
    }
    for (actor, action) in &w.actions {
        if *actor != action.actor
            || !people.contains(actor)
            || !areas.contains(&action.area)
            || action.completes <= s.time
            || !game_physiology::body(s.components, *actor)
                .unwrap()
                .vitals
                .alive
        {
            return Err(fail());
        }
        let activity = s.components.activity(*actor).unwrap();
        let valid = match &action.work {
            MemoryWork::Write {
                content,
                material,
                tool,
            } => {
                content_valid(content)
                    && game_cargo::inventory(s.components, *(material))
                        .is_some_and(|i| i.good == Good::Timber)
                    && game_cargo::inventory(s.components, *(tool))
                        .is_some_and(|i| i.good == Good::Tool)
                    && matches!(activity,Activity::WritingMemory {completes}if *completes==action.completes)
            }
            MemoryWork::Read { record } => {
                w.records.contains_key(record)
                    && matches!(activity,Activity::ReadingMemory {completes}if *completes==action.completes)
            }
            MemoryWork::Move {
                record,
                destination,
            } => {
                w.records.contains_key(record)
                    && match destination {
                        StockPosition::Area(area) => areas.contains(area),
                        StockPosition::Carried(actor) => people.contains(actor),
                    }
                    && matches!(activity,Activity::MovingRecord {completes}if *completes==action.completes)
            }
            MemoryWork::Teach { content, recipient } => {
                content_valid(content)
                    && people.contains(recipient)
                    && recipient != actor
                    && matches!(activity,Activity::TeachingMemory {completes}if *completes==action.completes)
                    && matches!(*s.components.activity(*recipient).unwrap(),Activity::TeachingMemory {completes}if completes==action.completes)
            }
        };
        if !valid {
            return Err(fail());
        }
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PastLife {
    pub person: PersonId,
    pub died_at: SimTime,
    pub mind: decisions::Mind,
    pub skills: Vec<SkillView>,
    pub institutions: Vec<InstitutionView>,
    pub cases: Vec<SocialCaseView>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub id: RecordId,
    pub inventory: InventoryId,
    pub content: RecordedMemory,
    pub author: PersonId,
    pub written_at: SimTime,
    pub readers: BTreeSet<PersonId>,
    pub intact: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryWork {
    Write {
        content: RecordedMemory,
        material: InventoryId,
        tool: InventoryId,
    },
    Read {
        record: RecordId,
    },
    Move {
        record: RecordId,
        destination: StockPosition,
    },
    Teach {
        content: RecordedMemory,
        recipient: PersonId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryAction {
    pub actor: PersonId,
    pub area: AreaId,
    pub completes: SimTime,
    pub work: MemoryWork,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub control: game_invocation::ControlState,
    pub history: BTreeMap<PersonId, PastLife>,
    pub records: BTreeMap<RecordId, Record>,
    pub actions: BTreeMap<PersonId, MemoryAction>,
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
    pub fn invocation_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::skills_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "invocation-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.host.state.invocation = Some(World {
            control: game_invocation::ControlState::new(Rules::fixture())?,
            history: BTreeMap::new(),
            records: BTreeMap::new(),
            actions: BTreeMap::new(),
        });
        // Separate compressed fixture rates leave room for sustained-use tests.
        let development = sim.host.state.development.as_mut().unwrap();
        development.rules.elderhood = SimDuration(3_600_000);
        development.rules.lifespan = SimDuration(5_400_000);
        for due in sim.host.state.due.values_mut() {
            due.retain(|event| !matches!(event, Due::Age(_)));
        }
        sim.host.state.due.retain(|_, due| !due.is_empty());
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
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
        let ended: Vec<_> = lineage
            .incarnations
            .iter()
            .filter_map(|life| life.died_at.map(|time| (life.person, time)))
            .filter(|(id, _)| !w.history.contains_key(id))
            .collect();
        let fresh: Vec<_> = lineage
            .incarnations
            .iter()
            .filter(|life| {
                life.died_at == Some(self.host.clock().time) && w.history.contains_key(&life.person)
            })
            .map(|life| life.person)
            .collect();
        for person in fresh {
            let skills = self.own_skill_view(person);
            self.host
                .state
                .invocation
                .as_mut()
                .unwrap()
                .history
                .get_mut(&person)
                .unwrap()
                .skills = skills;
        }
        for (person, died_at) in ended {
            let snapshot = PastLife {
                person,
                died_at,
                mind: game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    person,
                )
                .expect("mind exists")
                .clone(),
                skills: self.own_skill_view(person),
                institutions: self.institution_view(person),
                cases: self.social_case_view(person),
            };
            self.host
                .state
                .invocation
                .as_mut()
                .unwrap()
                .history
                .insert(person, snapshot);
        }
        let dead = game_invocation::burdens(&self.host.storage)
            .map(|(actor, _)| actor)
            .chain(self.invocation_control_actor())
            .filter(|p| {
                !game_physiology::body(&self.host.storage, *p)
                    .unwrap()
                    .vitals
                    .alive
            })
            .collect();
        let prepared = self
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .control
            .prepare_deaths(&self.host.storage, &dead, self.host.clock().time);
        self.apply_invocation_control(prepared)?;
        Ok(())
    }
    fn invocation_control_actor(&self) -> Option<PersonId> {
        let w = &self.host.state.invocation.as_ref()?.control;
        w.active
            .as_ref()
            .map(|a| a.actor)
            .or_else(|| w.pending_warning(&self.host.storage).map(|view| view.actor))
            .or_else(|| w.activation.as_ref().map(|p| p.actor))
    }
    fn invocation_admission(&self, actor: PersonId) -> game_invocation::AdmissionFacts {
        let lineage = self.host.state.lineage.as_ref().unwrap();
        let body = game_physiology::body(&self.host.storage, actor).unwrap();
        game_invocation::AdmissionFacts {
            actor,
            now: self.host.clock().time,
            current: lineage.current(),
            alive: body.vitals.alive,
            health: body.vitals.health,
            independent: self.independent(actor),
            learning: lineage
                .invocation_learning
                .get(&actor)
                .copied()
                .unwrap_or(0),
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
            archived: self
                .host
                .state
                .invocation
                .as_ref()
                .unwrap()
                .history
                .keys()
                .copied()
                .collect(),
        }
    }
    fn invocation_ready(&self, actor: PersonId) -> bool {
        self.host
            .state
            .lineage
            .as_ref()
            .is_some_and(|lineage| lineage.current() == Some(actor))
            && self.host.state.physical.is_some()
            && game_physiology::body(&self.host.storage, actor).is_some()
            && self
                .host
                .state
                .invocation
                .as_ref()
                .is_some_and(|w| w.control.ready(actor, &self.invocation_admission(actor)))
    }
    pub(crate) fn past_life_controls_actor(&self, actor: PersonId) -> bool {
        let episode = self
            .host
            .state
            .invocation
            .as_ref()
            .and_then(|w| w.control.active.as_ref())
            .and_then(|a| a.episode.as_ref());
        let alive = self.host.state.physical.is_some()
            && game_physiology::body(&self.host.storage, actor)
                .is_some_and(|body| body.vitals.alive);
        game_invocation::controls_actor(episode, actor, self.host.clock().time, alive)
    }
    pub(crate) fn required_invocation_input(&self) -> Option<WarningId> {
        self.host
            .state
            .invocation
            .as_ref()?
            .control
            .warning
            .as_ref()
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
        use game_invocation::ControlAction;
        let control = match action {
            InvocationAction::Activate { lives } => Some(ControlAction::Activate {
                lives: lives.clone(),
            }),
            InvocationAction::Dismiss => Some(ControlAction::Dismiss),
            InvocationAction::AcceptWarning { warning } => {
                Some(ControlAction::AcceptWarning { warning: *warning })
            }
            _ => None,
        };
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
            self.apply_invocation_control(prepared)
        } else {
            if self.past_life_controls_actor(actor)
                || !game_physiology::body(&self.host.storage, actor)
                    .is_some_and(|b| b.vitals.alive && b.vitals.health > 0)
            {
                return Err(Error::Unavailable);
            }
            self.ensure_event_capacity(8)?;
            self.host.next_id().checked_add(4).ok_or(Error::Overflow)?;
            self.start_memory_action(actor, action)
        }
    }
    /// Checks both owner plans before committing cost/control records. Physical
    /// charges, action cleanup, age/death and events remain owner effects here.
    fn apply_invocation_control(
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
            if let ControlEffect::DeathCleanup { actor } = effect
                && game_physiology::body(&self.host.storage, *actor)
                    .unwrap()
                    .vitals
                    .alive
            {
                return Err(Error::Unavailable);
            }
        }
        let charges = prepared
            .effects
            .iter()
            .filter_map(|effect| {
                if let ControlEffect::FatigueCharge { actor, quantity } = effect {
                    Some(game_physiology::prepare_fatigue_charge(
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
                    ))
                } else {
                    None
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let effects = prepared.effects.clone();
        let mut batch = game_ecs::Batch::new();
        let finish = prepared.stage_into(
            &self.host.state.invocation.as_ref().unwrap().control,
            &self.host.storage,
            &mut batch,
        )?;
        for charge in charges {
            charge.stage_into(&self.host.storage, &mut batch)?;
        }
        self.host.storage.apply_batch(batch)?;
        finish.finish(&mut self.host.state.invocation.as_mut().unwrap().control);
        if warning.is_some() {
            self.host.allocate_ids(1)?;
        }
        let mut lost_control = false;
        for effect in effects {
            match effect {
                ControlEffect::WarningRaised { warning, actor } => {
                    self.emit(EventKind::InvocationWarningRaised { warning, actor })
                }
                ControlEffect::WarningAccepted { warning, actor } => {
                    self.emit(EventKind::InvocationCostAccepted { warning, actor })
                }
                ControlEffect::FatigueCharge { .. } => {}
                ControlEffect::Started { actor, lives } => {
                    if self
                        .host
                        .state
                        .invocation
                        .as_ref()
                        .unwrap()
                        .actions
                        .get(&actor)
                        .is_some_and(|a| match &a.work {
                            MemoryWork::Write { content, .. }
                            | MemoryWork::Teach { content, .. } => {
                                !lives.contains(&Self::memory_life(content))
                            }
                            _ => false,
                        })
                    {
                        self.cancel_recollection_actions(actor);
                    }
                    self.emit(EventKind::InvocationStarted { actor, lives });
                }
                ControlEffect::Dismissed { actor } => {
                    self.cancel_recollection_actions(actor);
                    self.dismiss_recalled_route(actor);
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
                    // The lifespan owner may already have ended this body.
                    // Death preempts a controller handoff, without undoing costs.
                    if game_physiology::body(&self.host.storage, actor)
                        .unwrap()
                        .vitals
                        .alive
                    {
                        self.emit(EventKind::InvocationControlChanged {
                            actor,
                            controlled,
                            until,
                        });
                        lost_control |= !controlled;
                    }
                }
                ControlEffect::LifespanChanged { actor } => self.reschedule_invocation_age(actor),
                ControlEffect::DeathCleanup { actor } => self.cancel_invocation_action(actor),
                ControlEffect::ControllerSelected { .. } => {}
            }
        }
        if lost_control {
            self.review_invocation()?;
        }
        if self.invocation_control_actor().is_some_and(|actor| {
            !game_physiology::body(&self.host.storage, actor)
                .unwrap()
                .vitals
                .alive
        }) {
            self.observe_invocation_deaths()?;
        }
        Ok(())
    }
    fn reschedule_invocation_age(&mut self, actor: PersonId) {
        let Some(development) = &self.host.state.development else {
            return;
        };
        let life = game_lifecycle::life(&self.host.storage, actor).expect("life exists");
        let lifespan = development
            .rules
            .lifespan
            .0
            .saturating_sub(self.invocation_lifespan_loss(actor));
        let age = self.host.clock().time.0 - life.born.0 + life.initial_age.0;
        if age >= lifespan {
            self.end_life(actor);
            return;
        }
        for due in self.host.state.due.values_mut() {
            due.retain(|d| !matches!(d,Due::Age(id)if *id==actor));
        }
        self.host.state.due.retain(|_, due| !due.is_empty());
        self.schedule_age(actor);
    }
    pub(crate) fn invocation_lifespan_loss(&self, actor: PersonId) -> u64 {
        self.host
            .state
            .invocation
            .as_ref()
            .and_then(|_| game_invocation::burden(&self.host.storage, actor))
            .map_or(0, |b| b.lifespan_loss)
    }

    fn cost_input<'a>(active: &'a Active) -> game_invocation::ActiveInput<'a> {
        game_invocation::ActiveInput {
            life_count: active.lives.len(),
            accepted: &active.accepted,
            episode: active.episode.as_ref(),
            next_loss_burden: active.next_loss_burden,
        }
    }
    pub(crate) fn invocation_rates(&self, actor: PersonId) -> (u64, u64) {
        let Some(w) = &self.host.state.invocation else {
            return (0, 0);
        };
        let before = game_invocation::burden(&self.host.storage, actor)
            .cloned()
            .unwrap_or_default();
        let active = w
            .control
            .active
            .as_ref()
            .filter(|a| a.actor == actor)
            .map(Self::cost_input);
        let alive = game_physiology::body(&self.host.storage, actor)
            .unwrap()
            .vitals
            .alive;
        let rates = game_invocation::body_rates(
            &w.control.rules,
            &before,
            active,
            actor,
            self.host.clock().time,
            alive,
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
        // Existing bounded control/cost publications and native continuation
        // reserve, plus up to three publications per memory action. Expose the
        // same checked demand to the composed opening resource envelope.
        let event_demand = (w.actions.len() as u64)
            .checked_mul(3)
            .and_then(|n| n.checked_add(16))
            .ok_or(Error::Overflow)?;
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
        let Some(w) = &self.host.state.invocation else {
            return 0;
        };
        let actions = w
            .actions
            .values()
            .filter(|action| action.completes <= next)
            .map(|action| match action.work {
                MemoryWork::Write { .. } => 2,
                MemoryWork::Move { .. } => 1,
                _ => 0,
            })
            .sum::<u64>();
        let warning = u64::from(w.control.active.is_some() || w.control.activation.is_some());
        let controller = if w
            .control
            .active
            .as_ref()
            .is_some_and(|a| a.episode.is_some() && a.next_controller <= next)
        {
            2
        } else {
            0
        };
        actions + warning + controller
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
    fn next_cost_after(&self, actor: PersonId, lives: &BTreeSet<PersonId>) -> Option<SimDuration> {
        let w = self.host.state.invocation.as_ref()?;
        let before = game_invocation::burden(&self.host.storage, actor)
            .cloned()
            .unwrap_or_default();
        let next_loss = w
            .control
            .active
            .as_ref()
            .filter(|a| a.actor == actor)
            .map_or(w.control.rules.loss_threshold, |a| a.next_loss_burden);
        game_invocation::next_cost_after(&w.control.rules, &before, lives.len(), next_loss)
    }
    pub(crate) fn invocation_boundary(&self, limit: SimTime) -> SimTime {
        let Some(w) = &self.host.state.invocation else {
            return limit;
        };
        let alive = game_invocation::burdens(&self.host.storage)
            .map(|(actor, _)| actor)
            .filter(|p| {
                game_physiology::body(&self.host.storage, *p)
                    .unwrap()
                    .vitals
                    .alive
            })
            .collect();
        let mut next =
            w.control
                .boundary(&self.host.storage, self.host.clock().time, limit, &alive);
        for action in w.actions.values() {
            if action.completes > self.host.clock().time {
                next = next.min(action.completes);
            }
        }
        next
    }

    pub(crate) fn borrowed_skill_level(&self, actor: PersonId, skill: Skill) -> u16 {
        let Some(w) = &self.host.state.invocation else {
            return 0;
        };
        let Some(a) = w.control.active.as_ref().filter(|a| a.actor == actor) else {
            return 0;
        };
        let levels: Vec<_> = a
            .lives
            .iter()
            .map(|id| {
                w.history[id]
                    .skills
                    .iter()
                    .find(|s| s.skill == skill)
                    .map_or(0, |s| s.level)
            })
            .collect();
        let maximum = levels.iter().copied().max().unwrap_or(0);
        (u32::from(maximum)
            + (levels.iter().map(|v| u32::from(*v)).sum::<u32>() - u32::from(maximum)) / 2)
            .min(200) as u16
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
        let Some(w) = &self.host.state.invocation else {
            return own;
        };
        let state = game_invocation::burden(&self.host.storage, actor)
            .cloned()
            .unwrap_or_default();
        let bonus = self
            .host
            .state
            .skills
            .as_ref()
            .map_or(10, |s| s.rules.work_bonus_per_level);
        let borrowing = w
            .control
            .active
            .as_ref()
            .is_some_and(|active| active.actor == actor);
        let borrowed = if borrowing {
            1_000 + u64::from(self.borrowed_skill_level(actor, skill)) * u64::from(bonus)
        } else {
            0
        };
        let penalty = u64::from(state.temporary_identity) + u64::from(state.permanent_identity);
        game_invocation::compose_work_rate(own, body, borrowed, penalty)
            .expect("validated frozen body and invocation rate bounds")
    }
    pub(crate) fn invocation_combat_damage(
        &self,
        actor: PersonId,
        base: u64,
    ) -> Result<u64, Error> {
        let Some(_) = &self.host.state.invocation else {
            return Ok(base);
        };
        let state = game_invocation::burden(&self.host.storage, actor)
            .cloned()
            .unwrap_or_default();
        let own = self.own_skill_level(actor, Skill::Combat);
        let borrowed = self.borrowed_skill_level(actor, Skill::Combat);
        let bonus = self
            .host
            .state
            .skills
            .as_ref()
            .map_or(5, |s| s.rules.damage_bonus_per_level);
        let penalty = u64::from(state.temporary_identity) + u64::from(state.permanent_identity);
        let denominator = 1_000 + u64::from(own) * u64::from(bonus);
        let multiplier = (1_000 + u64::from(borrowed.max(own)) * u64::from(bonus))
            .saturating_sub(penalty * 5)
            .max(1);
        u64::try_from(u128::from(base) * u128::from(multiplier) / u128::from(denominator))
            .map_err(|_| Error::Overflow)
    }
    pub(crate) fn invocation_routes(
        &self,
        actor: PersonId,
    ) -> Option<BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>> {
        let w = self.host.state.invocation.as_ref()?;
        let a = w.control.active.as_ref().filter(|a| a.actor == actor)?;
        let current =
            &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)?
                .routes;
        Some(game_invocation::recalled_routes(
            current,
            a.lives.iter().map(|id| &w.history[id].mind.routes),
        ))
    }
    fn dismiss_recalled_route(&mut self, actor: PersonId) {
        let person = &self.host.state.people[self.person_index[&actor]];
        let Some(destination) = person.route.last().copied() else {
            return;
        };
        let origin =
            match *game_spatial::location(&self.host.storage, game_ecs::StableId((person.id).0))
                .expect("location exists")
            {
                Location::AtArea(area) => area,
                Location::InTransit { to, .. } => to,
                Location::StoppedOnRoad { .. } | Location::Carried { .. } => return,
            };
        let owned =
            &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                .expect("mind exists")
                .routes;
        let route = decisions::known_path(owned, origin, destination)
            .map_or_else(|_| Vec::new(), |(path, _)| path);
        self.host.state.people[self.person_index[&actor]].route = route;
    }
    pub(crate) fn invocation_personality(&self, actor: PersonId) -> Option<decisions::Personality> {
        let w = self.host.state.invocation.as_ref()?;
        let a = w.control.active.as_ref().filter(|a| a.actor == actor)?;
        let life = a.dominant.or_else(|| a.lives.iter().next().copied())?;
        let empty = game_invocation::Burden::default();
        let burden = game_invocation::burden(&self.host.storage, actor).unwrap_or(&empty);
        Some(game_invocation::controller_personality(
            &w.history[&life].mind.personality,
            burden,
        ))
    }
    pub(crate) fn invocation_social_penalty(&self, actor: PersonId) -> i64 {
        self.host
            .state
            .invocation
            .as_ref()
            .and_then(|_| game_invocation::burden(&self.host.storage, actor))
            .map_or(0, |b| {
                i64::from(b.emotional_disruption)
                    + i64::from(b.temporary_identity)
                    + i64::from(b.permanent_identity)
            })
    }
    pub(crate) fn review_invocation(&mut self) -> Result<(), Error> {
        use game_invocation::ControlEffect;
        let Some(actor) = self
            .host
            .state
            .invocation
            .as_ref()
            .and_then(|w| w.control.active.as_ref())
            .map(|a| a.actor)
        else {
            return Ok(());
        };
        let alive = game_physiology::body(&self.host.storage, actor)
            .unwrap()
            .vitals
            .alive;
        let Some(prepared) = self
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .control
            .prepare_controller(&self.host.storage, self.host.clock().time, alive)?
        else {
            return Ok(());
        };
        let (life, changed) = prepared
            .effects
            .iter()
            .find_map(|e| match e {
                ControlEffect::ControllerSelected { life, changed, .. } => Some((*life, *changed)),
                _ => None,
            })
            .unwrap();
        prepared.commit(
            &mut self.host.state.invocation.as_mut().unwrap().control,
            &mut self.host.storage,
        )?;
        let controller_actor = actor;
        let active = self
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .control
            .active
            .as_ref()
            .unwrap();
        let past = &self.host.state.invocation.as_ref().unwrap().history[&life].mind;
        let current = game_characters::decisions::mind::<game_api::Command>(
            &self.host.storage,
            controller_actor,
        )
        .expect("mind exists");
        let body = game_physiology::body(&self.host.storage, controller_actor).unwrap();
        let rules = &self.host.state.physical.as_ref().unwrap().rules;
        let area = match *game_spatial::location(
            &self.host.storage,
            game_ecs::StableId(controller_actor.0),
        )
        .expect("location exists")
        {
            Location::AtArea(area) => Some(area),
            _ => None,
        };
        let local = self
            .host
            .state
            .people
            .iter()
            .filter(|person| {
                area.is_some_and(|area| {
                    game_spatial::location(&self.host.storage, game_ecs::StableId(person.id.0))
                        == Some(&Location::AtArea(area))
                }) && game_physiology::body(&self.host.storage, person.id)
                    .unwrap()
                    .vitals
                    .alive
            })
            .map(|person| game_invocation::ControllerPerson {
                person: person.id,
                contact: self.transport_same_place(controller_actor, person.id),
            });
        let historical_routes: Vec<_> = active
            .lives
            .iter()
            .map(|id| {
                &self.host.state.invocation.as_ref().unwrap().history[id]
                    .mind
                    .routes
            })
            .collect();
        let personality = self.invocation_personality(controller_actor).unwrap();
        let activity = match *self.host.storage.activity(controller_actor).unwrap() {
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
        let decision =
            game_invocation::choose_controller_action(game_invocation::ControllerInput {
                actor: controller_actor,
                changed,
                in_combat: self.in_combat(controller_actor),
                independent: self.independent(controller_actor),
                area,
                activity,
                hunger: body.vitals.hunger,
                meal_relief: rules.meal_relief,
                fatigue: body.vitals.fatigue,
                fatigue_limit: rules.fatigue_limit,
                personality: &personality,
                past: game_invocation::ControllerMemory {
                    goal: past.goal,
                    dream: past.dream,
                    stocks: &past.knowledge,
                    sites: &past.sites,
                    relationships: &past.relationships,
                },
                current_stocks: &current.knowledge,
                current_sites: &current.sites,
                child_signals: &current.child_signals,
                local_people: local,
                current_routes: &current.routes,
                historical_routes: &historical_routes,
            });
        if let game_invocation::ControllerDecision::Act { action, ready } = decision {
            let result = if ready {
                self.apply_invocation_action(controller_actor, action)
            } else {
                Err(Error::Unavailable)
            };
            if result.is_err()
                && let Some(fallback) = decision.after_failure()
            {
                let _ = self.apply_invocation_action(controller_actor, fallback);
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
        let w = self
            .host
            .state
            .invocation
            .as_ref()
            .ok_or(Error::Unavailable)?;
        if !self.invocation_ready(self.host.state.ruler)
            || lives.is_empty()
            || lives.len() > 8
            || lives.iter().any(|id| !w.history.contains_key(id))
        {
            return Err(Error::Unavailable);
        }
        let levels = Skill::ALL
            .into_iter()
            .map(|skill| {
                let values: Vec<_> = lives
                    .iter()
                    .map(|id| {
                        w.history[id]
                            .skills
                            .iter()
                            .find(|s| s.skill == skill)
                            .map_or(0, |s| s.level)
                    })
                    .collect();
                let max = values.iter().copied().max().unwrap_or(0);
                (
                    skill,
                    (u32::from(max)
                        + (values.iter().map(|v| u32::from(*v)).sum::<u32>() - u32::from(max)) / 2)
                        .min(200) as u16,
                )
            })
            .collect();
        Ok(InvocationPreview {
            burden_rate: (lives.len() as u64).pow(2),
            activation_burden: w
                .control
                .rules
                .activation_burden
                .checked_mul((lives.len() as u64).pow(2))
                .ok_or(Error::Overflow)?,
            activation_fatigue: w
                .control
                .rules
                .activation_fatigue
                .checked_mul((lives.len() as u64).pow(2))
                .ok_or(Error::Overflow)?,
            next_cost_in: self.next_cost_after(self.host.state.ruler, &lives),
            lives,
            borrowed_skills: levels,
        })
    }
    pub(crate) fn invocation_view(&self) -> Option<InvocationView> {
        let w = self.host.state.invocation.as_ref()?;
        let actor = self.host.state.ruler;
        let b = game_invocation::burden(&self.host.storage, actor)
            .cloned()
            .unwrap_or_default();
        let active = w.control.active.as_ref().filter(|a| a.actor == actor);
        let lives = active.map_or_else(BTreeSet::new, |a| a.lives.clone());
        let memories = lives
            .iter()
            .map(|id| {
                let life = &w.history[id];
                InvokedLifeView {
                    person: *id,
                    died_at: life.died_at,
                    skills: life.skills.clone(),
                    stocks: life.mind.knowledge.values().take(64).cloned().collect(),
                    routes: life
                        .mind
                        .routes
                        .iter()
                        .flat_map(|(from, links)| {
                            links.iter().map(|(to, duration)| (*from, *to, *duration))
                        })
                        .take(128)
                        .collect(),
                    institutions: life.institutions.clone(),
                    cases: life.cases.clone(),
                }
            })
            .collect();
        let records=w.records.values().filter_map(|record|{let inventory=game_cargo::inventory(&self.host.storage, record.inventory)?;let local=game_spatial::location(&self.host.storage,game_ecs::StableId((actor).0)).expect("location exists");let visible=matches!((&inventory.location,local),(InventoryLocation::Carried(id),_)if *id==actor)||matches!((&inventory.location,local),(InventoryLocation::Area(a),Location::AtArea(b))if a==b);if !visible||!record.intact{return None}Some(MemoryRecordView {id:record.id,inventory:record.inventory,content:record.readers.contains(&actor).then(||record.content.clone()),written_at:record.written_at,author:record.author,position:match inventory.location {InventoryLocation::Area(area)=>StockPosition::Area(area),InventoryLocation::Carried(person)=>StockPosition::Carried(person)},can_read:self.can_read_record(actor,record.id)})}).take(64).collect();
        let preview_lives = w
            .control
            .activation
            .as_ref()
            .map_or_else(|| lives.clone(), |plan| plan.lives.clone());
        let preview = if preview_lives.is_empty() {
            InvocationPreview {
                lives: BTreeSet::new(),
                burden_rate: 0,
                activation_burden: 0,
                activation_fatigue: 0,
                borrowed_skills: Vec::new(),
                next_cost_in: None,
            }
        } else {
            self.invocation_preview(preview_lives).ok()?
        };
        Some(InvocationView {
            ready: self.invocation_ready(actor),
            available_lives: w.history.keys().copied().collect(),
            active_lives: lives,
            burden: b.burden,
            exposure: SimDuration(b.exposure),
            uninterrupted: SimDuration(b.uninterrupted),
            temporary_identity: b.temporary_identity,
            emotional_disruption: b.emotional_disruption,
            permanent_identity: b.permanent_identity,
            lifespan_loss: SimDuration(b.lifespan_loss),
            player_control: !self.past_life_controls_actor(actor)
                && game_physiology::body(&self.host.storage, actor)
                    .unwrap()
                    .vitals
                    .alive,
            loss_ends: active.and_then(|a| a.episode.as_ref()).map(|e| e.ends),
            warning: w.control.pending_warning(&self.host.storage).cloned(),
            preview,
            memories,
            records,
        })
    }
    pub(crate) fn available_invocation_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(w) = &self.host.state.invocation else {
            return Vec::new();
        };
        if self.past_life_controls_actor(actor) {
            return Vec::new();
        }
        let mut actions = Vec::new();
        if let Some(warning) = w.control.pending_warning(&self.host.storage)
            && warning.actor == actor
        {
            actions.push(InvocationAction::AcceptWarning {
                warning: warning.id,
            });
        }
        if w.control.active.as_ref().is_some_and(|a| a.actor == actor)
            || w.control
                .pending_warning(&self.host.storage)
                .is_some_and(|a| a.actor == actor)
        {
            actions.push(InvocationAction::Dismiss);
        }
        if self.invocation_ready(actor) && w.control.warning.is_none() {
            for life in w.history.keys().rev().take(32) {
                let lives = BTreeSet::from([*life]);
                if w.control.active.as_ref().is_none_or(|a| a.lives != lives) {
                    actions.push(InvocationAction::Activate { lives });
                }
            }
        }
        for record in w
            .records
            .values()
            .filter(|record| self.can_read_record(actor, record.id))
            .take(32)
        {
            actions.push(InvocationAction::ReadRecord { record: record.id });
        }
        actions
            .into_iter()
            .map(|action| Command::Invocation { action })
            .collect()
    }

    fn memory_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        if !self.independent(actor) || self.in_combat(actor) || self.combat_restricts(actor) {
            return Err(Error::Unavailable);
        }
        match *game_spatial::location(&self.host.storage, game_ecs::StableId((actor).0))
            .expect("location exists")
        {
            Location::AtArea(area) => Ok(area),
            _ => Err(Error::Unavailable),
        }
    }
    fn local_memory_input(
        &self,
        actor: PersonId,
        inventory: InventoryId,
        good: Good,
        quantity: u64,
    ) -> bool {
        game_cargo::inventory(&self.host.storage, inventory).is_some_and(|stock| {
            stock.good == good
                && stock.quantity.0 >= quantity
                && (good != Good::Timber || self.unreserved_stock(inventory, actor) >= quantity)
                && (good != Good::Tool || self.tool_available_for(actor, inventory))
                && self.has_stock_access(actor, stock)
                && self.inventory_area(stock) == self.memory_area(actor).ok()
        })
    }
    pub(crate) fn invocation_selected_tool(&self, actor: PersonId) -> Option<InventoryId> {
        match &self
            .host
            .state
            .invocation
            .as_ref()?
            .actions
            .get(&actor)?
            .work
        {
            MemoryWork::Write { tool, .. } => Some(*tool),
            _ => None,
        }
    }
    fn recalled_stock(
        &self,
        actor: PersonId,
        life: PersonId,
        inventory: InventoryId,
    ) -> Result<RecordedMemory, Error> {
        let w = self
            .host
            .state
            .invocation
            .as_ref()
            .ok_or(Error::Unavailable)?;
        if !w
            .control
            .active
            .as_ref()
            .is_some_and(|a| a.actor == actor && a.lives.contains(&life))
        {
            return Err(Error::Unavailable);
        }
        let fact = w.history[&life]
            .mind
            .knowledge
            .get(&inventory)
            .cloned()
            .ok_or(Error::Unavailable)?;
        Ok(RecordedMemory::Stock { life, fact })
    }
    fn can_read_record(&self, actor: PersonId, record: RecordId) -> bool {
        let Some(w) = &self.host.state.invocation else {
            return false;
        };
        let Some(record) = w.records.get(&record).filter(|r| r.intact) else {
            return false;
        };
        let Some(stock) = game_cargo::inventory(&self.host.storage, record.inventory) else {
            return false;
        };
        game_physiology::body(&self.host.storage, actor)
            .is_some_and(|a| a.vitals.alive && a.vitals.health > 0)
            && self
                .memory_area(actor)
                .ok()
                .is_some_and(|area| self.inventory_area(stock) == Some(area))
            && stock.good == Good::Record
            && stock.quantity.0 >= 1_000
            && self.has_stock_access(actor, stock)
    }
    fn start_memory_action(
        &mut self,
        actor: PersonId,
        action: &InvocationAction,
    ) -> Result<(), Error> {
        let area = self.memory_area(actor)?;
        let r = self
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .control
            .rules
            .clone();
        let (work, duration) = match action {
            InvocationAction::RecordMemory {
                life,
                inventory,
                material,
                tool,
            } => {
                let content = self.recalled_stock(actor, *life, *inventory)?;
                if !self.local_memory_input(actor, *material, Good::Timber, 1_000)
                    || !self.local_memory_input(actor, *tool, Good::Tool, 1_000)
                {
                    return Err(Error::Unavailable);
                }
                (
                    MemoryWork::Write {
                        content,
                        material: *material,
                        tool: *tool,
                    },
                    r.writing_time,
                )
            }
            InvocationAction::RecordRoute {
                life,
                origin,
                destination,
                material,
                tool,
            } => {
                let w = self.host.state.invocation.as_ref().unwrap();
                if !w
                    .control
                    .active
                    .as_ref()
                    .is_some_and(|a| a.actor == actor && a.lives.contains(life))
                {
                    return Err(Error::Unavailable);
                }
                let duration = w.history[life]
                    .mind
                    .routes
                    .get(origin)
                    .and_then(|links| links.get(destination))
                    .copied()
                    .ok_or(Error::Unavailable)?;
                if !self.local_memory_input(actor, *material, Good::Timber, 1_000)
                    || !self.local_memory_input(actor, *tool, Good::Tool, 1_000)
                {
                    return Err(Error::Unavailable);
                }
                (
                    MemoryWork::Write {
                        content: RecordedMemory::Route {
                            life: *life,
                            origin: *origin,
                            destination: *destination,
                            duration,
                        },
                        material: *material,
                        tool: *tool,
                    },
                    r.writing_time,
                )
            }
            InvocationAction::ReadRecord { record } => {
                if !self.can_read_record(actor, *record) {
                    return Err(Error::Unavailable);
                }
                (MemoryWork::Read { record: *record }, r.reading_time)
            }
            InvocationAction::MoveRecord {
                record,
                destination,
            } => {
                if !self.can_read_record(actor, *record) {
                    return Err(Error::Unavailable);
                }
                match destination {
                    StockPosition::Area(target) if *target != area => {
                        return Err(Error::Unavailable);
                    }
                    StockPosition::Carried(person)
                        if self.memory_area(*person).ok() != Some(area) =>
                    {
                        return Err(Error::Unavailable);
                    }
                    _ => {}
                }
                let record_data = &self.host.state.invocation.as_ref().unwrap().records[record];
                let source = game_cargo::inventory(&self.host.storage, record_data.inventory)
                    .expect("inventory exists");
                let desired = match destination {
                    StockPosition::Area(area) => InventoryLocation::Area(*area),
                    StockPosition::Carried(actor) => InventoryLocation::Carried(*actor),
                };
                if source.location == desired
                    || !matches!(source.location,InventoryLocation::Carried(holder)if holder==actor)
                        && source.location != InventoryLocation::Area(area)
                {
                    return Err(Error::Unavailable);
                }
                if let InventoryLocation::Carried(carrier) = desired {
                    let held: u128 = self
                        .inventory_index
                        .get(&InventoryLocation::Carried(carrier))
                        .into_iter()
                        .flatten()
                        .map(|id| {
                            u128::from(
                                game_cargo::inventory(&self.host.storage, *(id))
                                    .expect("inventory exists")
                                    .quantity
                                    .0,
                            )
                        })
                        .sum();
                    if held + 1_000
                        > u128::from(
                            self.host
                                .state
                                .physical
                                .as_ref()
                                .unwrap()
                                .rules
                                .carrying_capacity
                                .0,
                        )
                    {
                        return Err(Error::Unavailable);
                    }
                }
                (
                    MemoryWork::Move {
                        record: *record,
                        destination: destination.clone(),
                    },
                    r.moving_time,
                )
            }
            InvocationAction::TeachMemory {
                life,
                inventory,
                recipient,
            } => {
                let content = self.recalled_stock(actor, *life, *inventory)?;
                if *recipient == actor
                    || self.memory_area(*recipient).ok() != Some(area)
                    || !game_physiology::body(&self.host.storage, *recipient)
                        .unwrap()
                        .vitals
                        .alive
                    || !matches!(
                        *self.host.storage.activity(*recipient).unwrap(),
                        Activity::Idle | Activity::Rest
                    )
                {
                    return Err(Error::Unavailable);
                }
                let mind = &game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    *recipient,
                )
                .expect("mind exists");
                let trust = mind.relationships.get(&actor).map_or(0, |r| r.trust);
                if i64::from(mind.personality.sociability) + i64::from(trust) < 200
                    || game_physiology::body(&self.host.storage, *recipient)
                        .unwrap()
                        .vitals
                        .hunger
                        >= self.host.state.physical.as_ref().unwrap().rules.meal_relief
                {
                    return Err(Error::Unavailable);
                }
                (
                    MemoryWork::Teach {
                        content,
                        recipient: *recipient,
                    },
                    r.teaching_time,
                )
            }
            _ => return Err(Error::Unavailable),
        };
        let completes = SimTime(
            self.host
                .clock()
                .time
                .0
                .checked_add(duration.0)
                .ok_or(Error::Overflow)?,
        );
        self.interrupt_actor_activity(actor);
        if let MemoryWork::Write { material, .. } = work {
            game_cargo::inventory_mut(&mut self.host.storage, material)
                .unwrap()
                .quantity
                .0 -= 1_000;
            self.emit(EventKind::Consumed {
                actor,
                source: material,
                quantity: Quantity(1_000),
            });
        }
        if let MemoryWork::Teach { recipient, .. } = work {
            self.interrupt_actor_activity(recipient);
            *self.host.storage.activity_mut(recipient).unwrap() =
                Activity::TeachingMemory { completes };
        }
        let activity = match work {
            MemoryWork::Write { .. } => Activity::WritingMemory { completes },
            MemoryWork::Read { .. } => Activity::ReadingMemory { completes },
            MemoryWork::Move { .. } => Activity::MovingRecord { completes },
            MemoryWork::Teach { .. } => Activity::TeachingMemory { completes },
        };
        *self.host.storage.activity_mut(actor).unwrap() = activity;
        self.host.state.invocation.as_mut().unwrap().actions.insert(
            actor,
            MemoryAction {
                actor,
                area,
                completes,
                work,
            },
        );
        Ok(())
    }
    pub(crate) fn cancel_invocation_action(&mut self, actor: PersonId) {
        let Some(w) = &self.host.state.invocation else {
            return;
        };
        let affected: Vec<_> = w
            .actions
            .values()
            .filter(|action| {
                action.actor == actor
                    || matches!(action.work,MemoryWork::Teach {recipient,..}if recipient==actor)
            })
            .cloned()
            .collect();
        for action in affected {
            self.host
                .state
                .invocation
                .as_mut()
                .unwrap()
                .actions
                .remove(&action.actor);
            let mut actors = vec![action.actor];
            if let MemoryWork::Teach { recipient, .. } = action.work {
                actors.push(recipient);
            }
            for id in actors {
                if let Some(activity) = self.host.storage.activity_mut(id)
                    && matches!(
                        *activity,
                        Activity::WritingMemory { .. }
                            | Activity::ReadingMemory { .. }
                            | Activity::TeachingMemory { .. }
                            | Activity::MovingRecord { .. }
                    )
                {
                    *activity = Activity::Idle;
                }
            }
        }
    }
    fn cancel_recollection_actions(&mut self, actor: PersonId) {
        if self
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .actions
            .get(&actor)
            .is_some_and(|a| matches!(a.work, MemoryWork::Write { .. } | MemoryWork::Teach { .. }))
        {
            self.cancel_invocation_action(actor);
        }
    }
    fn install_recorded_memory(
        &mut self,
        actor: PersonId,
        content: &RecordedMemory,
        source: KnowledgeSource,
    ) {
        let owner_now = self.host.clock().time;
        let mind = game_characters::decisions::mind_mut::<game_api::Command>(
            &mut self.host.storage,
            actor,
        )
        .unwrap();
        match content {
            RecordedMemory::Stock { fact, .. } => {
                if mind
                    .knowledge
                    .get(&fact.inventory)
                    .is_some_and(|old| old.observed_at > fact.observed_at)
                {
                    return;
                }
                let mut learned = fact.clone();
                learned.accessible = mind
                    .knowledge
                    .get(&fact.inventory)
                    .is_some_and(|old| old.accessible);
                learned.learned_at = owner_now;
                learned.source = source;
                mind.knowledge.insert(fact.inventory, learned);
            }
            RecordedMemory::Route {
                origin,
                destination,
                duration,
                ..
            } => {
                mind.routes
                    .entry(*origin)
                    .or_default()
                    .entry(*destination)
                    .or_insert(*duration);
            }
        }
    }
    fn memory_life(content: &RecordedMemory) -> PersonId {
        match content {
            RecordedMemory::Stock { life, .. } | RecordedMemory::Route { life, .. } => *life,
        }
    }
    fn finish_memory_actions(&mut self) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        let actions: Vec<_> = self
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .actions
            .values()
            .filter(|action| action.completes <= owner_now)
            .cloned()
            .collect();
        for action in actions {
            let actor = action.actor;
            if self.memory_area(actor).ok() != Some(action.area)
                || !game_physiology::body(&self.host.storage, actor)
                    .unwrap()
                    .vitals
                    .alive
            {
                self.cancel_invocation_action(actor);
                continue;
            }
            let mut success = false;
            match &action.work {
                MemoryWork::Write { content, tool, .. } => {
                    let life = Self::memory_life(content);
                    let held: u128 = self
                        .inventory_index
                        .get(&InventoryLocation::Carried(actor))
                        .into_iter()
                        .flatten()
                        .map(|id| {
                            u128::from(
                                game_cargo::inventory(&self.host.storage, *(id))
                                    .expect("inventory exists")
                                    .quantity
                                    .0,
                            )
                        })
                        .sum();
                    if self
                        .host
                        .state
                        .invocation
                        .as_ref()
                        .unwrap()
                        .control
                        .active
                        .as_ref()
                        .is_some_and(|a| a.actor == actor && a.lives.contains(&life))
                        && self.local_memory_input(actor, *tool, Good::Tool, 1_000)
                        && held + 1_000
                            <= u128::from(
                                self.host
                                    .state
                                    .physical
                                    .as_ref()
                                    .unwrap()
                                    .rules
                                    .carrying_capacity
                                    .0,
                            )
                    {
                        let record = RecordId(self.host.allocate_id()?.0);
                        let inventory = self.debug_add_inventory(
                            Good::Record,
                            InventoryLocation::Carried(actor),
                            Quantity(1_000),
                            Quantity(10_000),
                            BTreeSet::from([actor]),
                        )?;
                        self.host.state.invocation.as_mut().unwrap().records.insert(
                            record,
                            Record {
                                id: record,
                                inventory,
                                content: content.clone(),
                                author: actor,
                                written_at: owner_now,
                                readers: BTreeSet::new(),
                                intact: true,
                            },
                        );
                        self.emit(EventKind::MemoryRecorded {
                            record,
                            actor,
                            inventory,
                        });
                        success = true;
                    }
                }
                MemoryWork::Read { record } => {
                    if self.can_read_record(actor, *record) {
                        let content = self.host.state.invocation.as_ref().unwrap().records[record]
                            .content
                            .clone();
                        self.install_recorded_memory(
                            actor,
                            &content,
                            KnowledgeSource::Recorded(*record),
                        );
                        self.host
                            .state
                            .invocation
                            .as_mut()
                            .unwrap()
                            .records
                            .get_mut(record)
                            .unwrap()
                            .readers
                            .insert(actor);
                        self.emit(EventKind::MemoryRead {
                            record: *record,
                            actor,
                        });
                        success = true;
                    }
                }
                MemoryWork::Teach { content, recipient } => {
                    let life = Self::memory_life(content);
                    if self
                        .host
                        .state
                        .invocation
                        .as_ref()
                        .unwrap()
                        .control
                        .active
                        .as_ref()
                        .is_some_and(|a| a.actor == actor && a.lives.contains(&life))
                        && self.memory_area(*recipient).ok() == Some(action.area)
                        && matches!(*self.host.storage.activity(*recipient).unwrap(),Activity::TeachingMemory {completes}if completes==action.completes)
                    {
                        self.install_recorded_memory(
                            *recipient,
                            content,
                            KnowledgeSource::Reported(actor),
                        );
                        self.emit(EventKind::MemoryTaught {
                            actor,
                            recipient: *recipient,
                            life,
                        });
                        success = true;
                    }
                }
                MemoryWork::Move {
                    record,
                    destination,
                } => {
                    if self.can_read_record(actor, *record) {
                        let source =
                            self.host.state.invocation.as_ref().unwrap().records[record].inventory;
                        let location = match destination {
                            StockPosition::Area(area) => InventoryLocation::Area(*area),
                            StockPosition::Carried(carrier) => InventoryLocation::Carried(*carrier),
                        };
                        let valid = match destination {
                            StockPosition::Area(area) => *area == action.area,
                            StockPosition::Carried(carrier) => {
                                self.memory_area(*carrier).ok() == Some(action.area)
                                    && self
                                        .inventory_index
                                        .get(&location)
                                        .into_iter()
                                        .flatten()
                                        .map(|id| {
                                            u128::from(
                                                game_cargo::inventory(&self.host.storage, *(id))
                                                    .expect("inventory exists")
                                                    .quantity
                                                    .0,
                                            )
                                        })
                                        .sum::<u128>()
                                        + 1_000
                                        <= u128::from(
                                            self.host
                                                .state
                                                .physical
                                                .as_ref()
                                                .unwrap()
                                                .rules
                                                .carrying_capacity
                                                .0,
                                        )
                            }
                        };
                        if valid {
                            let mut access = game_cargo::inventory(&self.host.storage, source)
                                .expect("inventory exists")
                                .access
                                .clone();
                            if let StockPosition::Carried(carrier) = destination {
                                access.insert(*carrier);
                            }
                            let target = self.debug_add_inventory(
                                Good::Record,
                                location,
                                Quantity(0),
                                Quantity(10_000),
                                access,
                            )?;
                            game_cargo::inventory_mut(&mut self.host.storage, source)
                                .unwrap()
                                .quantity
                                .0 -= 1_000;
                            game_cargo::inventory_mut(&mut self.host.storage, target)
                                .unwrap()
                                .quantity
                                .0 += 1_000;
                            let original: Vec<_> = self
                                .host
                                .state
                                .invocation
                                .as_ref()
                                .unwrap()
                                .records
                                .values()
                                .filter(|r| r.intact && r.inventory == source)
                                .map(|r| r.id)
                                .collect();
                            self.emit(EventKind::Transferred {
                                actor,
                                source,
                                destination: target,
                                quantity: Quantity(1_000),
                            });
                            for id in original {
                                self.host
                                    .state
                                    .invocation
                                    .as_mut()
                                    .unwrap()
                                    .records
                                    .get_mut(&id)
                                    .unwrap()
                                    .inventory = if id == *record { target } else { source };
                            }
                            success = true;
                        }
                    }
                }
            }
            self.cancel_invocation_action(actor);
            if !success {
                self.emit(EventKind::Interrupted { actor });
            }
        }
        Ok(())
    }
    pub(crate) fn record_quantity_increment(&self, source: InventoryId) -> Quantity {
        Quantity(
            if game_cargo::inventory(&self.host.storage, source)
                .is_some_and(|stock| stock.good == Good::Record)
            {
                1_000
            } else {
                1
            },
        )
    }
    pub(crate) fn record_transfer_allowed(&self, source: InventoryId, quantity: Quantity) -> bool {
        let Some(stock) = game_cargo::inventory(&self.host.storage, source) else {
            return false;
        };
        if stock.good != Good::Record {
            return true;
        }
        let Some(w) = &self.host.state.invocation else {
            return false;
        };
        let increment = self.record_quantity_increment(source).0;
        quantity.0 > 0
            && quantity.0.is_multiple_of(increment)
            && w.records
                .values()
                .filter(|record| record.intact && record.inventory == source)
                .count() as u64
                >= quantity.0 / increment
    }
    pub(crate) fn relocate_record_transfer(
        &mut self,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    ) {
        if game_cargo::inventory(&self.host.storage, source).is_none_or(|s| s.good != Good::Record)
        {
            return;
        }
        let Some(w) = self.host.state.invocation.as_mut() else {
            return;
        };
        for record in w
            .records
            .values_mut()
            .filter(|record| record.intact && record.inventory == source)
            .take((quantity.0 / 1_000) as usize)
        {
            record.inventory = destination;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn submit(sim: &mut Simulation, command: Command) -> Result<CommandReceipt, Error> {
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence()?,
            command,
        })
    }
    fn act(sim: &mut Simulation, action: InvocationAction) -> Result<CommandReceipt, Error> {
        submit(sim, Command::Invocation { action })
    }
    fn ready(workers: usize) -> Simulation {
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
        let teacher = *sim
            .host
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .keys()
            .next()
            .unwrap();
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
        assert!(sim.invocation_ready(actor));
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
        *sim.host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .history
            .keys()
            .next()
            .unwrap()
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
                .active
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
        let remote = sim.host.state.invocation.as_ref().unwrap().history[&life]
            .mind
            .knowledge
            .values()
            .find(|k| k.area == AreaId(3) && k.good == Good::Timber)
            .unwrap()
            .inventory;
        assert!(
            !game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, actor)
                .expect("mind exists")
                .knowledge
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
        assert!(
            sim.host
                .state
                .invocation
                .as_ref()
                .unwrap()
                .records
                .is_empty()
        );
        sim.advance(SimDuration(1)).unwrap();
        let record = *sim
            .host
            .state
            .invocation
            .as_ref()
            .unwrap()
            .records
            .keys()
            .next()
            .unwrap();
        for _ in 0..10 {
            assert!(sim.invocation_view().unwrap().records[0].content.is_none());
        }
        act(&mut sim, InvocationAction::Dismiss).unwrap();
        assert!(sim.invocation_view().unwrap().memories.is_empty());
        assert!(
            !game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, actor)
                .expect("mind exists")
                .knowledge
                .contains_key(&remote)
        );
        act(&mut sim, InvocationAction::ReadRecord { record }).unwrap();
        sim.advance(MINUTE).unwrap();
        assert!(
            matches!(game_characters::decisions::mind::<game_api::Command>(&sim.host.storage,actor).expect("mind exists").knowledge[&remote].source,KnowledgeSource::Recorded(id)if id==record)
        );
        assert!(
            !game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, actor)
                .expect("mind exists")
                .knowledge[&remote]
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
            .active
            .as_ref()
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
                .active
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
        let archived = &mut sim
            .host
            .state
            .invocation
            .as_mut()
            .unwrap()
            .history
            .get_mut(&life)
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
                .active
                .as_ref()
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
        assert_eq!(control.as_object().unwrap().len(), 4);
        for field in ["people", "warnings"] {
            assert!(control.get(field).is_none());
            let mut forged = value.clone();
            forged["state"]["state"]["invocation"]["control"][field] = serde_json::json!({});
            assert!(
                Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default())
                    .is_err()
            );
        }
        for column in ["invocation/burden", "invocation/warning"] {
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
        for attack in 0..5 {
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
                    forged["state"]["state"]["invocation"]["control"]["warning"]["id"] =
                        serde_json::json!(999999)
                }
                _ => {
                    forged["state"]["state"]["invocation"]["control"]["warning"]["view"] =
                        serde_json::json!({})
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
}

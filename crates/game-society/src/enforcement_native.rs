//! Native enforcement bindings around the existing chosen-policy system.
//! Transient typed projections contain only the rows its policy reads. Canonical
//! writes, peer commands, leases, events and cursors stay in the shared journal.
use crate::{
    enforcement::{self, *},
    institutions::{self, *},
    services,
};
use game_agreements::{AgreementAction, AgreementView};
use game_characters::{
    decisions::{Activity, Mind, Personality},
    services::{ReadActivity, ReadPlacement, ReadStage},
};
use game_core::*;
use game_ecs::{Batch, Component, ReadGuard, Schema, StableId, Store};
use game_missions::{MissionAction, MissionProposal, MissionView};
use game_world::{
    OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot,
    actions::{AcquireAction, ActionSlot, InterruptActor, ReleaseAction},
};
use std::{cell::RefCell, collections::BTreeSet, marker::PhantomData};
#[cfg(test)]
#[path = "enforcement_native_tests.rs"]
mod tests;

pub struct ReadPastLifeControl(pub PersonId);
impl Request for ReadPastLifeControl {
    type Response = bool;
}
pub struct ExecuteEnforcement {
    pub actor: PersonId,
    pub action: EnforcementAction,
}
impl Request for ExecuteEnforcement {
    type Response = ();
}
pub struct CancelEnforcement(pub PersonId);
impl Request for CancelEnforcement {
    type Response = ();
}
pub struct ReadEnforcementView(pub PersonId);
impl Request for ReadEnforcementView {
    type Response = EnforcementView;
}
pub struct AvailableEnforcement(pub PersonId);
impl Request for AvailableEnforcement {
    type Response = Vec<EnforcementAction>;
}
pub struct ValidateSave;
impl Request for ValidateSave {
    type Response = ();
}
pub struct ValidateEnforcementEvent(pub Event);
impl Request for ValidateEnforcementEvent {
    type Response = ();
}
struct ReleaseEnforcementLease(PersonId);
impl Request for ReleaseEnforcementLease {
    type Response = ();
}
pub enum Input {
    Initialize,
    Action(ExecuteEnforcement),
    Observe,
    Review,
    Complete(PersonId),
    Cancel(PersonId),
    ValidateSave,
}
pub struct Systems<C, E> {
    strike: fn(&E) -> Option<Strike>,
    marker: PhantomData<C>,
}
impl<C, E> Systems<C, E> {
    pub fn new(strike: fn(&E) -> Option<Strike>) -> Self {
        Self {
            strike,
            marker: PhantomData,
        }
    }
}
impl<C: Component, E: From<Event> + 'static> OwnerSystem<E> for Systems<C, E> {
    type Runtime = Option<World>;
    type Input = Input;
    type Output = ();
    fn execute(&self, tx: &mut OwnerTxn<'_, '_, Self, E>, input: Input) -> Result<(), Error> {
        if matches!(input, Input::Initialize) {
            if tx.runtime().is_some() {
                return Err(Error::Unavailable);
            }
            let last_event = tx.next_event() - 1;
            *tx.runtime_mut() = Some(World { last_event });
            return Ok(());
        }
        if tx.runtime().is_none() {
            if matches!(input, Input::Action(_)) {
                return Err(Error::Unavailable);
            }
            if matches!(input, Input::ValidateSave)
                && (!tx.iter::<Case>().is_empty()
                    || !tx.iter::<Talk>().is_empty()
                    || !tx.iter::<Policies>().is_empty()
                    || !tx.iter::<Memory>().is_empty())
            {
                return Err(Error::InvalidSave(
                    "inactive enforcement has canonical rows".into(),
                ));
            }
            return Ok(());
        }
        let port = NativePort::new(Access::Write(tx), self.strike)?;
        let mut system = match &input {
            Input::Action(q) => System::for_action(port, q.actor, &q.action),
            Input::Complete(actor) | Input::Cancel(actor) => System::for_actor(port, *actor),
            Input::ValidateSave => System::for_validation(port),
            _ => System::new(port),
        };
        match input {
            Input::Action(q) => system.enforcement_command(q.actor, &q.action)?,
            Input::Observe => system.observe_enforcement(),
            Input::Review => system.review_enforcement()?,
            Input::Complete(actor) => system.complete_enforcement_talk(actor),
            Input::Cancel(actor) => system.interrupt_enforcement(actor),
            Input::ValidateSave => return system.validate_enforcement(),
            Input::Initialize => unreachable!(),
        }
        system.publish()
    }
}
pub fn register_systems<S: 'static, C: Component, E: From<Event> + 'static>(
    registry: &mut Registry<S, E>,
    slot: RuntimeSlot<S, Option<World>>,
    strike: fn(&E) -> Option<Strike>,
) -> Result<(), Error> {
    registry.register(Systems::<C, E>::new(strike), slot)?;
    registry.owns::<Systems<C, E>, Case>()?;
    registry.owns::<Systems<C, E>, Talk>()?;
    registry.owns::<Systems<C, E>, Policies>()?;
    registry.owns::<Systems<C, E>, Memory>()?;
    registry.register_request::<Systems<C, E>, ExecuteEnforcement>(execute::<C, E>)?;
    registry.register_request::<Systems<C, E>, CancelEnforcement>(cancel::<C, E>)?;
    registry.register_request::<Systems<C, E>, ReleaseEnforcementLease>(release::<C, E>)?;
    registry.register_request::<Systems<C, E>, services::ReceiveEnforcementOutcome>(
        receive_outcome::<C, E>,
    )?;
    registry.register_read_request::<Systems<C, E>, services::ValidateEnforcementChange>(
        validate_change::<C, E>,
    )?;
    registry.register_read_request::<Systems<C, E>, services::ScoreEnforcementChange>(
        score_change::<C, E>,
    )?;
    registry.register_read_request::<Systems<C, E>, ReadEnforcementView>(view::<C, E>)?;
    registry.register_read_request::<Systems<C, E>, AvailableEnforcement>(available::<C, E>)?;
    registry.register_read_request::<Systems<C, E>, ValidateSave>(validate_save::<C, E>)?;
    registry
        .register_read_request::<Systems<C, E>, ValidateEnforcementEvent>(validate_event::<C, E>)
}
fn execute<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C, E>,
    tx: &mut OwnerTxn<'_, '_, Systems<C, E>, E>,
    q: ExecuteEnforcement,
) -> Result<(), Error> {
    owner.execute(tx, Input::Action(q))
}
fn cancel<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C, E>,
    tx: &mut OwnerTxn<'_, '_, Systems<C, E>, E>,
    q: CancelEnforcement,
) -> Result<(), Error> {
    owner.execute(tx, Input::Cancel(q.0))
}
fn release<C: Component, E: From<Event> + 'static>(
    _: &Systems<C, E>,
    tx: &mut OwnerTxn<'_, '_, Systems<C, E>, E>,
    q: ReleaseEnforcementLease,
) -> Result<(), Error> {
    if tx.read::<Talk>(StableId(q.0.0)).is_none() {
        tx.request(ReleaseAction {
            actor: StableId(q.0.0),
            owner: "enforcement".into(),
        })?;
    }
    Ok(())
}
pub fn cancel_action<C: Component, E: From<Event> + 'static>(
    tx: &mut OwnerTxn<'_, '_, game_world::actions::Actions<E>, E>,
    actor: StableId,
) -> Result<(), Error> {
    tx.request(CancelEnforcement(PersonId(actor.0)))
}
fn view<C: Component, E: From<Event> + 'static>(
    _: &Systems<C, E>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C, E>, E>,
    q: ReadEnforcementView,
) -> Result<EnforcementView, Error> {
    let row = tx
        .runtime()
        .is_some()
        .then(|| tx.read::<Memory>(StableId(q.0.0)).cloned())
        .flatten()
        .unwrap_or_default();
    Ok(EnforcementView {
        policies: row
            .known_policies
            .values()
            .flat_map(|offices| offices.values())
            .take(32)
            .cloned()
            .collect(),
        cases: row.known.values().rev().take(64).cloned().collect(),
        assaults: row.assaults.values().rev().take(32).cloned().collect(),
    })
}
fn available<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C, E>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C, E>, E>,
    q: AvailableEnforcement,
) -> Result<Vec<EnforcementAction>, Error> {
    if tx.runtime().is_none() {
        return Ok(Vec::new());
    }
    let port = NativePort::new(Access::Read(tx), owner.strike)?;
    Ok(System::for_actor(port, q.0).available_enforcement_commands(q.0))
}
fn validate_change<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C, E>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C, E>, E>,
    q: services::ValidateEnforcementChange,
) -> Result<bool, Error> {
    let port = NativePort::new(Access::Read(tx), owner.strike)?;
    let system = System::for_change(port, &q.change);
    match system.validate_enforcement_change(&q.institution, &q.change) {
        Ok(()) => Ok(system.enforcement_change_actor_allowed(
            q.actor,
            &q.institution,
            &q.change,
            q.proposing,
        )),
        Err(Error::Unavailable) => Ok(false),
        Err(error) => Err(error),
    }
}
fn score_change<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C, E>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C, E>, E>,
    q: services::ScoreEnforcementChange,
) -> Result<i64, Error> {
    let port = NativePort::new(Access::Read(tx), owner.strike)?;
    Ok(System::for_change(port, &q.change).enforcement_vote_score(
        q.actor,
        &q.institution,
        &q.change,
    ))
}
fn receive_outcome<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C, E>,
    tx: &mut OwnerTxn<'_, '_, Systems<C, E>, E>,
    q: services::ReceiveEnforcementOutcome,
) -> Result<(), Error> {
    if q.status != RuleProposalStatus::Adopted {
        return Ok(());
    }
    if q.at != tx.now() || q.sequence == 0 || q.sequence >= tx.next_event() {
        return Err(Error::SequenceConflict);
    }
    if tx
        .read::<RuleProposal>(StableId(q.proposal.0))
        .is_none_or(|proposal| {
            proposal.institution != q.institution
                || proposal.status != q.status
                || proposal.change != q.change
        })
    {
        return Err(Error::Unavailable);
    }
    let port = NativePort::new(Access::Write(tx), owner.strike)?;
    let mut system = System::for_adoption(port, q.institution, q.proposal);
    system.preflight_enforcement_adoption(q.institution, q.proposal)?;
    system.adopt_enforcement_change(q.institution, q.proposal)?;
    system.publish()
}
enum Access<'a, 'tx, 's, C: Component, E: From<Event> + 'static> {
    Write(&'a mut OwnerTxn<'tx, 's, Systems<C, E>, E>),
    Read(&'a mut OwnerReadTxn<'tx, 's, Systems<C, E>, E>),
}
impl<C: Component, E: From<Event> + 'static> Access<'_, '_, '_, C, E> {
    fn request<Q: Request>(&mut self, q: Q) -> Result<Q::Response, Error> {
        match self {
            Self::Write(tx) => tx.request(q),
            Self::Read(tx) => tx.request(q),
        }
    }
    fn read<R: Component>(&mut self, id: StableId) -> Option<R> {
        match self {
            Self::Write(tx) => tx.read::<R>(id).cloned(),
            Self::Read(tx) => tx.read::<R>(id).cloned(),
        }
    }
    fn iter<R: Component>(&mut self) -> Vec<(StableId, R)> {
        match self {
            Self::Write(tx) => tx.iter::<R>(),
            Self::Read(tx) => tx.iter::<R>(),
        }
    }
    fn runtime(&mut self) -> Option<World> {
        match self {
            Self::Write(tx) => tx.runtime().clone(),
            Self::Read(tx) => tx.runtime().clone(),
        }
    }
    fn now(&self) -> SimTime {
        match self {
            Self::Write(tx) => tx.now(),
            Self::Read(tx) => tx.now(),
        }
    }
    fn next_id(&self) -> u64 {
        match self {
            Self::Write(tx) => tx.next_id(),
            Self::Read(tx) => tx.next_id(),
        }
    }
    fn next_event(&self) -> u64 {
        match self {
            Self::Write(tx) => tx.next_event(),
            Self::Read(tx) => tx.next_event(),
        }
    }
    fn write<R: Component>(&mut self, id: StableId, row: R) -> Result<(), Error> {
        match self {
            Self::Write(tx) => tx.write(id, row),
            Self::Read(_) => Err(Error::Unavailable),
        }
    }
    fn remove<R: Component>(&mut self, id: StableId) -> Result<(), Error> {
        match self {
            Self::Write(tx) => tx.remove::<R>(id),
            Self::Read(_) => Err(Error::Unavailable),
        }
    }
    fn defer<Q: Request<Response = ()>>(&mut self, q: Q) -> Result<(), Error> {
        match self {
            Self::Write(tx) => tx.defer(q),
            Self::Read(_) => Err(Error::Unavailable),
        }
    }
    fn reserve_ids(&mut self, n: u64) -> Result<StableId, Error> {
        match self {
            Self::Write(tx) => tx.reserve_ids(n),
            Self::Read(_) => Err(Error::Unavailable),
        }
    }
    fn emit(&mut self, event: Event) -> Result<(), Error> {
        match self {
            Self::Write(tx) => tx.emit(event.into()).map(|_| ()),
            Self::Read(_) => Err(Error::Unavailable),
        }
    }
    fn revise(&mut self, n: u64) -> Result<(), Error> {
        match self {
            Self::Write(tx) => tx.revise(n),
            Self::Read(_) => Err(Error::Unavailable),
        }
    }
}
struct NativePort<'a, 'tx, 's, C: Component, E: From<Event> + 'static> {
    access: RefCell<Access<'a, 'tx, 's, C, E>>,
    store: Store,
    runtime: Option<World>,
    strike: fn(&E) -> Option<Strike>,
}
impl<'a, 'tx, 's, C: Component, E: From<Event> + 'static> NativePort<'a, 'tx, 's, C, E> {
    fn new(
        mut access: Access<'a, 'tx, 's, C, E>,
        strike: fn(&E) -> Option<Strike>,
    ) -> Result<Self, Error> {
        for (id, _) in access.iter::<game_characters::Person>() {
            if access.read::<Mind<C>>(id).is_none() {
                return Err(Error::InvalidConfiguration(
                    "enforcement requires current character mind facts".into(),
                ));
            }
        }
        let runtime = access.runtime();
        let store = projection(&mut access)?;
        Ok(Self {
            access: RefCell::new(access),
            store,
            runtime,
            strike,
        })
    }
    fn query<Q: Request>(&self, q: Q, fallback: Q::Response) -> Q::Response {
        // Request failures poison the enclosing journal. A local fallback only
        // lets this legacy pure interface finish its evaluation; it cannot publish.
        self.access.borrow_mut().request(q).unwrap_or(fallback)
    }
    fn refresh(&mut self) -> Result<(), Error> {
        self.store = projection(self.access.get_mut())?;
        self.runtime = self.access.get_mut().runtime();
        Ok(())
    }
}
fn projection<C: Component, E: From<Event> + 'static>(
    access: &mut Access<'_, '_, '_, C, E>,
) -> Result<Store, Error> {
    let mut schema = Schema::new();
    crate::register_components(&mut schema)?;
    enforcement::register_components(&mut schema)?;
    game_cargo::register_components(&mut schema)?;
    game_agreements::register_components(&mut schema)?;
    game_property::register_components(&mut schema)?;
    game_missions::register_components(&mut schema)?;
    let mut store = Store::with_schema(schema);
    macro_rules! rows{($($ty:ty),+)=>{$(for(id,row)in access.iter::<$ty>(){store.insert_or_spawn(id,row)?;})+};}
    rows!(
        Institution,
        RuleProposal,
        crate::Concept,
        crate::Knowledge,
        crate::Requests,
        crate::Exclusions,
        crate::Hearing,
        SocialCaseView,
        Case,
        Talk,
        Policies,
        Memory,
        game_cargo::Inventory,
        game_agreements::Agreement,
        game_agreements::Obligation,
        game_property::Incident,
        game_property::InventoryCustody,
        game_missions::Mission
    );
    Ok(store)
}
impl<C: Component, E: From<Event> + 'static> Port for NativePort<'_, '_, '_, C, E> {
    fn store(&self) -> &Store {
        &self.store
    }
    fn runtime(&self) -> Option<&World> {
        self.runtime.as_ref()
    }
    fn set_runtime(&mut self, world: World) {
        self.runtime = Some(world.clone());
        if let Access::Write(tx) = self.access.get_mut() {
            *tx.runtime_mut() = Some(world);
        }
    }
    fn now(&self) -> SimTime {
        self.access.borrow().now()
    }
    fn next_id(&self) -> u64 {
        self.access.borrow().next_id()
    }
    fn next_event(&self) -> u64 {
        self.access.borrow().next_event()
    }
    fn ruler(&self) -> PersonId {
        self.access
            .borrow_mut()
            .iter::<game_characters::Person>()
            .into_iter()
            .find(|(_, person)| person.role == Role::Ruler)
            .map_or(PersonId(0), |(_, person)| person.id)
    }
    fn people(&self) -> Vec<PersonFacts> {
        let actors = self.access.borrow_mut().iter::<game_characters::Person>();
        actors
            .into_iter()
            .filter_map(|(_, person)| self.person(person.id))
            .collect()
    }
    fn local_people(&self, area: AreaId) -> Vec<PersonFacts> {
        self.people()
            .into_iter()
            .filter(|person| person.location == Location::AtArea(area))
            .collect()
    }
    fn person(&self, actor: PersonId) -> Option<PersonFacts> {
        let person = self
            .access
            .borrow_mut()
            .read::<game_characters::Person>(StableId(actor.0))?;
        let placement = self.query(ReadPlacement(actor), None)?;
        let body = self.query(game_physiology::services::ReadBody(actor), None)?;
        let stage = self.query(
            ReadStage(actor),
            game_characters::services::StageFacts {
                stage: None,
                independent: false,
            },
        );
        let combat = self.query(
            game_conflict::services::ReadCombatStatus(actor),
            game_conflict::services::CombatStatus {
                in_combat: false,
                restricted: true,
                can_fight: None,
            },
        );
        Some(PersonFacts {
            id: actor,
            inventory: person.inventory,
            location: placement.location,
            vitals: body.vitals,
            independent: stage.independent,
            combat_restricted: combat.restricted,
        })
    }
    fn areas(&self) -> Vec<AreaId> {
        self.query(game_transport::ReadTopology, Vec::new())
            .into_iter()
            .map(|area| area.id)
            .collect()
    }
    fn authority(&self, people: &BTreeSet<PersonId>) -> AuthorityFacts {
        AuthorityFacts {
            alive: people
                .iter()
                .copied()
                .filter(|actor| {
                    self.query(game_physiology::services::ReadBody(*actor), None)
                        .is_some_and(|body| body.vitals.alive)
                })
                .collect(),
        }
    }
    fn mind(&self, actor: PersonId) -> MindFacts {
        self.access
            .borrow_mut()
            .read::<Mind<C>>(StableId(actor.0))
            .map_or(
                MindFacts {
                    personality: Personality::restful(),
                    relationships: Default::default(),
                },
                |row| MindFacts {
                    personality: row.personality,
                    relationships: row.relationships,
                },
            )
    }
    fn physical_rules(&self) -> Option<PhysicalFacts> {
        self.query(game_physiology::services::ReadBody(self.ruler()), None)
            .map(|body| PhysicalFacts {
                meal_size: body.rules.meal_size,
            })
    }
    fn in_combat(&self, actor: PersonId) -> bool {
        self.query(
            game_conflict::services::ReadCombatStatus(actor),
            game_conflict::services::CombatStatus {
                in_combat: true,
                restricted: true,
                can_fight: None,
            },
        )
        .in_combat
    }
    fn past_life_controls_actor(&self, actor: PersonId) -> bool {
        self.query(ReadPastLifeControl(actor), true)
    }
    fn idle_or_resting(&self, actor: PersonId) -> bool {
        matches!(
            self.query(ReadActivity(actor), Activity::Committed),
            Activity::Idle | Activity::Rest
        )
    }
    fn talk_activity(&self, actor: PersonId) -> Option<(EnforcementCaseId, PersonId, SimTime)> {
        let row = self.access.borrow_mut().read::<Talk>(StableId(actor.0))?;
        let lease = self.query(
            game_world::actions::ReadAction {
                actor: StableId(actor.0),
            },
            None,
        )?;
        (lease.owner == "enforcement" && lease.completes == Some(row.completes)).then_some((
            row.case,
            row.recipient,
            row.completes,
        ))
    }
    fn completion_scheduled(&self, actor: PersonId, at: SimTime) -> bool {
        self.talk_activity(actor)
            .is_some_and(|(_, _, completes)| completes == at)
    }
    fn agreements_available(&self) -> bool {
        self.query(game_agreements::native::ReadAgreementAvailable, false)
    }
    fn missions_available(&self) -> bool {
        self.query(game_missions::native::ReadMissionAvailable, false)
    }
    fn institutions_available(&self) -> bool {
        self.query(services::ReadInstitutionsAvailable, false)
    }
    fn combat_available(&self) -> bool {
        self.query(game_conflict::services::CombatAvailable, false)
    }
    fn property_available(&self) -> bool {
        self.query(game_property::services::ReadPropertyAvailable, false)
    }
    fn events(&self) -> Vec<StrikeRecord> {
        let access = self.access.borrow();
        match &*access {
            Access::Write(tx) => tx
                .events_since(self.runtime.as_ref().map_or(0, |world| world.last_event))
                .into_iter()
                .filter_map(|(sequence, at, event)| {
                    (self.strike)(event).map(|event| StrikeRecord {
                        sequence,
                        at,
                        event,
                    })
                })
                .collect(),
            Access::Read(_) => Vec::new(),
        }
    }
    fn encounter_area(&self, id: EncounterId) -> Option<AreaId> {
        self.access
            .borrow_mut()
            .read::<game_conflict::Encounter>(StableId(id.0))
            .map(|encounter| encounter.area)
    }
    fn incidents(&self, actor: PersonId) -> Vec<game_property::IncidentView> {
        self.query(
            game_property::services::ReadPropertyIncidents(actor),
            Vec::new(),
        )
    }
    fn institution_view(&self, actor: PersonId) -> Vec<InstitutionView> {
        self.query(services::ReadInstitutionViews(actor), Vec::new())
    }
    fn agreement_view(&self, actor: PersonId) -> Vec<AgreementView> {
        self.query(
            game_agreements::native::ReadAgreementView(actor),
            Vec::new(),
        )
    }
    fn social_case_view(&self, actor: PersonId) -> Vec<SocialCaseView> {
        self.query(services::ReadSocialCaseViews(actor), Vec::new())
    }
    fn mission_view(&self, actor: PersonId) -> Vec<MissionView> {
        self.query(game_missions::native::ReadMissionView(actor), Vec::new())
    }
    fn preflight(&self, _: u64) -> Result<(), Error> {
        Ok(())
    }
    fn bump_revision(&mut self, n: u64) -> Result<(), Error> {
        self.access.get_mut().revise(n)
    }
    fn ensure_event_capacity(&self, n: usize) -> Result<(), Error> {
        self.next_event()
            .checked_add(u64::try_from(n).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)
            .map(|_| ())
    }
    fn agreement_command(
        &mut self,
        actor: PersonId,
        action: &AgreementAction,
    ) -> Result<(), Error> {
        self.access
            .get_mut()
            .request(game_agreements::native::ExecuteAgreement {
                actor,
                action: action.clone(),
            })?;
        self.refresh()
    }
    fn institution_command(
        &mut self,
        actor: PersonId,
        action: &InstitutionAction,
    ) -> Result<(), Error> {
        let reactions =
            self.access
                .get_mut()
                .request(services::ExecuteInstitutionForEnforcement {
                    actor,
                    action: action.clone(),
                })?;
        for reaction in reactions {
            let outcome = reaction.outcome();
            let institution = self
                .access
                .get_mut()
                .request(services::ReadInstitution(outcome.institution))?
                .ok_or(Error::Unavailable)?;
            let proposal = self
                .access
                .get_mut()
                .request(services::ReadRuleProposal(outcome.proposal))?
                .ok_or(Error::Unavailable)?;
            let outcome = reaction.into_outcome(&institution, &proposal)?;
            let Access::Write(tx) = self.access.get_mut() else {
                return Err(Error::Unavailable);
            };
            // The source has returned and released its Services frame. This is
            // an inline operation of the already-active Enforcement owner,
            // rather than recursive entry through its receiving request.
            receive_outcome(&Systems::<C, E>::new(self.strike), tx, outcome)?;
        }
        self.refresh()
    }
    fn mission_command(&mut self, actor: PersonId, action: &MissionAction) -> Result<(), Error> {
        self.access
            .get_mut()
            .request(game_missions::native::ExecuteMission {
                actor,
                action: action.clone(),
            })?;
        self.refresh()
    }
    fn preflight_mission_target_report(
        &self,
        proposal: &MissionProposal,
        sender: PersonId,
        target: PersonId,
        area: AreaId,
        at: SimTime,
    ) -> Result<(), Error> {
        self.access
            .borrow_mut()
            .request(game_missions::native::PreflightTargetReport {
                proposal: proposal.clone(),
                sender,
                target,
                area,
                at,
            })
    }
    fn seed_mission_target_report(
        &mut self,
        mission: MissionId,
        sender: PersonId,
        target: PersonId,
        area: AreaId,
        at: SimTime,
    ) -> Result<(), Error> {
        self.access
            .get_mut()
            .request(game_missions::native::SeedTargetReport {
                mission,
                sender,
                target,
                area,
                at,
            })?;
        self.refresh()
    }
    fn publish(
        &mut self,
        batch: Batch,
        guard: ReadGuard,
        activities: Vec<ActivityEffect>,
        events: Vec<Event>,
        allocations: u64,
        interruptions: Vec<PersonId>,
    ) -> Result<(), Error> {
        guard.validate(&self.store, &batch)?;
        let access = self.access.get_mut();
        if allocations > 0 {
            access.reserve_ids(allocations)?;
        }
        for actor in interruptions {
            access.request(InterruptActor {
                actor: StableId(actor.0),
                preserve: "enforcement".into(),
            })?;
        }
        // Services delivers enforcement outcomes after releasing its own frame.
        // Its memory receiver can therefore see and merge successive journals
        // immediately, including when one policy invocation publishes twice.
        for id in batch.edited_ids::<crate::Knowledge>() {
            let after = batch
                .projected::<crate::Knowledge>(&self.store, id)?
                .cloned()
                .ok_or(Error::Unavailable)?;
            let before = access.read::<crate::Knowledge>(id);
            access.request(services::ReceiveInstitutionMemory {
                actor: PersonId(id.0),
                before,
                after,
            })?;
        }
        macro_rules! rows{($($ty:ty),+)=>{$(for id in batch.edited_ids::<$ty>(){match batch.projected::<$ty>(&self.store,id)?.cloned(){Some(row)=>access.write(id,row)?,None=>access.remove::<$ty>(id)?,}})+};}
        rows!(Case, Talk, Policies, Memory);
        for id in batch.edited_ids::<Talk>() {
            if batch.projected::<Talk>(&self.store, id)?.is_none() {
                access.defer(ReleaseEnforcementLease(PersonId(id.0)))?;
            }
        }
        for activity in activities {
            match activity {
                ActivityEffect::Talk {
                    actor, completes, ..
                } => {
                    access.request(AcquireAction {
                        actor: StableId(actor.0),
                        owner: "enforcement".into(),
                        completes: Some(completes),
                    })?;
                }
                ActivityEffect::Idle { actor } => access.defer(ReleaseEnforcementLease(actor))?,
            }
        }
        for event in events {
            access.emit(event)?;
        }
        self.store.apply_batch(batch)
    }
}
pub fn boundary<C: Component, E: From<Event> + 'static>(
    _: &Systems<C, E>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C, E>, E>,
    span: game_world::scheduling::ClockSpan,
) -> Result<Option<SimTime>, Error> {
    if tx.runtime().is_none() {
        return Ok(None);
    }
    let review = SimTime(
        (span.from.0 / MINUTE.0)
            .checked_add(1)
            .and_then(|tick| tick.checked_mul(MINUTE.0))
            .ok_or(Error::Overflow)?,
    );
    Ok(tx
        .iter::<Talk>()
        .into_iter()
        .map(|(_, talk)| talk.completes)
        .chain([review])
        .filter(|at| *at > span.from && *at <= span.to)
        .min())
}
pub fn close<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C, E>,
    tx: &mut OwnerTxn<'_, '_, Systems<C, E>, E>,
    _: game_world::scheduling::ClockSpan,
) -> Result<(), Error> {
    if tx.runtime().is_none() {
        return Ok(());
    }
    for (id, talk) in tx.iter::<Talk>() {
        if talk.completes <= tx.now() {
            owner.execute(tx, Input::Complete(PersonId(id.0)))?;
        }
    }
    let input = if game_characters::orchestration::review_due(tx.now()) {
        Input::Review
    } else {
        Input::Observe
    };
    owner.execute(tx, input)
}
pub fn register_schedule<S: 'static, C: Component, E: From<Event> + 'static>(
    registry: &mut Registry<S, E>,
    scheduler: &mut game_world::scheduling::Scheduler<E>,
    order: game_world::scheduling::Order,
) -> Result<(), Error> {
    scheduler.register_owner_phased::<Systems<C, E>, S>(
        registry,
        order,
        Some(boundary::<C, E>),
        None,
        None,
        Some(close::<C, E>),
    )
}
fn validate_save<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C, E>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C, E>, E>,
    _: ValidateSave,
) -> Result<(), Error> {
    if tx.runtime().is_none() {
        return if tx.iter::<Case>().is_empty()
            && tx.iter::<Talk>().is_empty()
            && tx.iter::<Policies>().is_empty()
            && tx.iter::<Memory>().is_empty()
        {
            Ok(())
        } else {
            Err(Error::InvalidSave(
                "inactive enforcement has canonical rows".into(),
            ))
        };
    }
    let port = NativePort::new(Access::Read(tx), owner.strike)?;
    System::for_validation(port).validate_enforcement()
}
fn validate_event<C: Component, E: From<Event> + 'static>(
    _: &Systems<C, E>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C, E>, E>,
    request: ValidateEnforcementEvent,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent enforcement event references".into());
    if tx.runtime().is_none() {
        return Err(fail());
    }
    let person = |tx: &mut OwnerReadTxn<'_, '_, Systems<C, E>, E>, actor: PersonId| {
        tx.read::<game_characters::Person>(StableId(actor.0))
            .is_some_and(|person| person.id == actor)
    };
    let case_exists = |tx: &mut OwnerReadTxn<'_, '_, Systems<C, E>, E>, id: EnforcementCaseId| {
        tx.read::<Case>(StableId(id.0))
            .is_some_and(|case| case.id == id)
    };
    let valid = match request.0 {
        Event::EnforcementPolicyChanged { institution, .. } => {
            tx.request(services::ReadInstitutionsAvailable)?
                && tx
                    .request(services::ReadInstitution(institution))?
                    .is_some()
        }
        Event::EnforcementFiling {
            case,
            actor,
            recipient,
        }
        | Event::EnforcementReported {
            case,
            sender: actor,
            recipient,
        } => person(tx, actor) && person(tx, recipient) && case_exists(tx, case),
        Event::EnforcementFiled {
            case,
            actor,
            institution,
        } => {
            person(tx, actor)
                && tx
                    .read::<Case>(StableId(case.0))
                    .is_some_and(|row| row.institution == institution)
                && tx.request(services::ReadInstitutionsAvailable)?
                && tx
                    .request(services::ReadInstitution(institution))?
                    .is_some()
        }
        Event::EnforcementDecided {
            case,
            judgment,
            proposer,
        } => {
            person(tx, proposer)
                && tx
                    .read::<Case>(StableId(case.0))
                    .is_some_and(|row| row.judgment.as_ref().is_some_and(|row| row.id == judgment))
        }
        Event::EnforcementResponded { case, actor, .. } => {
            person(tx, actor) && case_exists(tx, case)
        }
        Event::EnforcementDispatched {
            case,
            actor,
            mission,
        } => {
            person(tx, actor)
                && case_exists(tx, case)
                && tx.request(game_missions::native::ReadMissionAvailable)?
                && tx
                    .read::<game_missions::Mission>(StableId(mission.0))
                    .is_some_and(|row| row.id == mission)
        }
        Event::EnforcementOutcome { case, .. } => case_exists(tx, case),
    };
    if valid { Ok(()) } else { Err(fail()) }
}

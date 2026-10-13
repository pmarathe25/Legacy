//! Enforcement policy operates on prepared owner projections of canonical rows.
//! The boundary supplies facts or invokes an already-owned external operation.
use crate::enforcement::*;
use crate::institutions::{self, *};
use game_agreements::{AgreementAction, AgreementStatus, AgreementTerm, AgreementView};
use game_characters::decisions::{Personality, Relationship};
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use game_missions::{
    CompanionStatus, MissionAction, MissionObjective, MissionProposal, MissionStatus,
    MissionSupply, MissionView,
};
use game_property::IncidentOutcome;
use std::cell::{Ref, RefCell};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonFacts {
    pub id: PersonId,
    pub inventory: InventoryId,
    pub location: Location,
    pub vitals: Vitals,
    pub independent: bool,
    pub combat_restricted: bool,
}
#[derive(Clone, Debug)]
pub struct MindFacts {
    pub personality: Personality,
    pub relationships: BTreeMap<PersonId, Relationship>,
}
#[derive(Clone, Copy, Debug)]
pub struct PhysicalFacts {
    pub meal_size: Quantity,
}
#[derive(Clone, Debug)]
pub struct StrikeRecord {
    pub sequence: u64,
    pub at: SimTime,
    pub event: Strike,
}
#[derive(Clone, Debug)]
pub enum Strike {
    Struck {
        encounter: EncounterId,
        actor: PersonId,
        target: PersonId,
        damage: u64,
    },
}
#[derive(Clone, Debug)]
pub enum Event {
    EnforcementPolicyChanged {
        institution: InstitutionId,
        office: u16,
    },
    EnforcementDecided {
        case: EnforcementCaseId,
        judgment: JudgmentId,
        proposer: PersonId,
    },
    EnforcementFiling {
        case: EnforcementCaseId,
        actor: PersonId,
        recipient: PersonId,
    },
    EnforcementResponded {
        case: EnforcementCaseId,
        actor: PersonId,
        comply: bool,
    },
    EnforcementFiled {
        case: EnforcementCaseId,
        actor: PersonId,
        institution: InstitutionId,
    },
    EnforcementReported {
        case: EnforcementCaseId,
        sender: PersonId,
        recipient: PersonId,
    },
    EnforcementDispatched {
        case: EnforcementCaseId,
        actor: PersonId,
        mission: MissionId,
    },
    EnforcementOutcome {
        case: EnforcementCaseId,
        status: EnforcementStatus,
    },
}
#[derive(Clone, Debug)]
pub enum ActivityEffect {
    Talk {
        actor: PersonId,
        case: EnforcementCaseId,
        recipient: PersonId,
        completes: SimTime,
    },
    Idle {
        actor: PersonId,
    },
}
pub trait Port {
    fn store(&self) -> &Store;
    fn runtime(&self) -> Option<&World>;
    fn set_runtime(&mut self, world: World);
    fn now(&self) -> SimTime;
    fn next_id(&self) -> u64;
    fn next_event(&self) -> u64;
    fn ruler(&self) -> PersonId;
    fn people(&self) -> Vec<PersonFacts>;
    fn local_people(&self, area: AreaId) -> Vec<PersonFacts>;
    fn person(&self, actor: PersonId) -> Option<PersonFacts>;
    fn areas(&self) -> Vec<AreaId>;
    fn authority(&self, people: &BTreeSet<PersonId>) -> AuthorityFacts;
    fn mind(&self, actor: PersonId) -> MindFacts;
    fn physical_rules(&self) -> Option<PhysicalFacts>;
    fn in_combat(&self, actor: PersonId) -> bool;
    fn past_life_controls_actor(&self, actor: PersonId) -> bool;
    fn idle_or_resting(&self, actor: PersonId) -> bool;
    fn talk_activity(&self, actor: PersonId) -> Option<(EnforcementCaseId, PersonId, SimTime)>;
    fn completion_scheduled(&self, actor: PersonId, at: SimTime) -> bool;
    fn agreements_available(&self) -> bool;
    fn missions_available(&self) -> bool;
    fn institutions_available(&self) -> bool;
    fn combat_available(&self) -> bool;
    fn property_available(&self) -> bool;
    fn events(&self) -> Vec<StrikeRecord>;
    fn encounter_area(&self, id: EncounterId) -> Option<AreaId>;
    fn incidents(&self, actor: PersonId) -> Vec<game_property::IncidentView>;
    fn institution_view(&self, actor: PersonId) -> Vec<InstitutionView>;
    fn agreement_view(&self, actor: PersonId) -> Vec<AgreementView>;
    fn social_case_view(&self, actor: PersonId) -> Vec<SocialCaseView>;
    fn mission_view(&self, actor: PersonId) -> Vec<MissionView>;
    fn preflight(&self, revisions: u64) -> Result<(), Error>;
    fn bump_revision(&mut self, revisions: u64) -> Result<(), Error>;
    fn ensure_event_capacity(&self, count: usize) -> Result<(), Error>;
    fn agreement_command(&mut self, actor: PersonId, action: &AgreementAction)
    -> Result<(), Error>;
    fn institution_command(
        &mut self,
        actor: PersonId,
        action: &InstitutionAction,
    ) -> Result<(), Error>;
    fn mission_command(&mut self, actor: PersonId, action: &MissionAction) -> Result<(), Error>;
    fn preflight_mission_target_report(
        &self,
        proposal: &MissionProposal,
        sender: PersonId,
        target: PersonId,
        area: AreaId,
        at: SimTime,
    ) -> Result<(), Error>;
    fn seed_mission_target_report(
        &mut self,
        mission: MissionId,
        sender: PersonId,
        target: PersonId,
        area: AreaId,
        at: SimTime,
    ) -> Result<(), Error>;
    /// Generic publication: validation has completed; bind public events/activities.
    fn publish(
        &mut self,
        batch: Batch,
        guard: ReadGuard,
        activities: Vec<ActivityEffect>,
        events: Vec<Event>,
        allocations: u64,
        interruptions: Vec<PersonId>,
    ) -> Result<(), Error>;
}
#[derive(Clone, Debug, Default)]
struct Projection {
    policies: BTreeMap<InstitutionId, BTreeMap<u16, EnforcementPolicyView>>,
    cases: BTreeMap<EnforcementCaseId, Case>,
    talks: BTreeMap<PersonId, Talk>,
    known: BTreeMap<PersonId, BTreeMap<EnforcementCaseId, EnforcementCaseView>>,
    known_policies:
        BTreeMap<PersonId, BTreeMap<InstitutionId, BTreeMap<u16, EnforcementPolicyView>>>,
    contacts: BTreeMap<PersonId, BTreeMap<PersonId, ReportedContact>>,
    assaults: BTreeMap<PersonId, BTreeMap<EventId, WitnessedAssault>>,
    reconsider_after: BTreeMap<PersonId, SimTime>,
    last_event: u64,
    present_memories: BTreeSet<PersonId>,
    institutions: BTreeMap<InstitutionId, Institution>,
    institution_memories: BTreeMap<PersonId, (crate::Knowledge, crate::Knowledge)>,
}
impl Projection {
    fn capture(store: &Store, runtime: Option<&World>, selection: &Selection) -> Self {
        let mut rows = Self {
            last_event: runtime.map_or(0, |world| world.last_event),
            ..Self::default()
        };
        if selection.global {
            rows.cases = cases(store).map(|(id, row)| (id, row.clone())).collect();
            rows.talks = talks(store).map(|(id, row)| (id, row.clone())).collect();
            rows.policies = policies(store)
                .map(|(id, row)| (id, row.offices.clone()))
                .collect();
        } else {
            let mut case_ids = selection.cases.clone();
            let mut institutions = selection.institutions.clone();
            for actor in &selection.actors {
                rows.capture_memory(store, *actor);
                if let Some(row) = talk(store, *actor) {
                    case_ids.insert(row.case);
                    rows.talks.insert(*actor, row.clone());
                }
                if let Some(row) = memory(store, *actor) {
                    institutions.extend(
                        row.known
                            .values()
                            .rev()
                            .take(64)
                            .map(|case| case.institution),
                    );
                    institutions.extend(row.known_policies.keys().copied());
                }
            }
            for id in case_ids {
                if let Some(row) = case(store, id) {
                    institutions.insert(row.institution);
                    rows.cases.insert(id, row.clone());
                }
            }
            for institution in institutions {
                if let Some(row) = store.get::<Policies>(StableId(institution.0)) {
                    rows.policies.insert(institution, row.offices.clone());
                }
            }
        }
        let institutions = rows
            .policies
            .keys()
            .copied()
            .chain(rows.cases.values().map(|case| case.institution))
            .collect::<BTreeSet<_>>();
        for institution in institutions {
            if let Some(row) = crate::institution(store, institution) {
                rows.institutions.insert(institution, row.clone());
            }
        }
        rows
    }
    fn capture_memory(&mut self, store: &Store, actor: PersonId) {
        let original = memory(store, actor);
        if original.is_some() {
            self.present_memories.insert(actor);
        }
        let row = original.cloned().unwrap_or_default();
        self.known.insert(actor, row.known);
        self.known_policies.insert(actor, row.known_policies);
        self.contacts.insert(actor, row.contacts);
        self.assaults.insert(actor, row.assaults);
        if let Some(at) = row.reconsider_after {
            self.reconsider_after.insert(actor, at);
        }
    }
    fn memory(&self, actor: PersonId) -> Memory {
        Memory {
            known: self.known.get(&actor).cloned().unwrap_or_default(),
            known_policies: self.known_policies.get(&actor).cloned().unwrap_or_default(),
            contacts: self.contacts.get(&actor).cloned().unwrap_or_default(),
            assaults: self.assaults.get(&actor).cloned().unwrap_or_default(),
            reconsider_after: self.reconsider_after.get(&actor).copied(),
        }
    }
    fn memory_ids(&self) -> BTreeSet<PersonId> {
        self.known
            .keys()
            .chain(self.known_policies.keys())
            .chain(self.contacts.keys())
            .chain(self.assaults.keys())
            .chain(self.reconsider_after.keys())
            .copied()
            .collect()
    }
}
#[derive(Default)]
struct Selection {
    global: bool,
    actors: BTreeSet<PersonId>,
    cases: BTreeSet<EnforcementCaseId>,
    institutions: BTreeSet<InstitutionId>,
    areas: BTreeSet<AreaId>,
}
/// One system invocation owns a transient prepared projection, never serialized
/// or retained by a runtime. Canonical owner rows are captured, guarded and staged.
pub struct System<P: Port> {
    port: P,
    state: Projection,
    opening: Projection,
    now: SimTime,
    person_index: BTreeMap<PersonId, usize>,
    activities: Vec<ActivityEffect>,
    events: Vec<Event>,
    selection: Selection,
    reviewing: bool,
    revisions: u64,
    opening_next_id: u64,
    allocations: u64,
    interruptions: Vec<PersonId>,
    reads: ReadSet,
    person_reads: RefCell<BTreeMap<PersonId, Option<PersonFacts>>>,
    local_rosters: RefCell<BTreeMap<AreaId, Vec<PersonFacts>>>,
}
impl<P: Port> System<P> {
    /// Global systems capture cases/work/policy once; personal rows load on contact.
    pub fn new(port: P) -> Self {
        Self::selected(
            port,
            Selection {
                global: true,
                ..Selection::default()
            },
        )
    }
    pub fn for_actor(port: P, actor: PersonId) -> Self {
        let mut selection = Selection::default();
        selection.actors.insert(actor);
        if let Some(facts) = port.person(actor)
            && let Location::AtArea(area) = facts.location
        {
            selection.areas.insert(area);
        }
        Self::selected(port, selection)
    }
    pub fn for_action(port: P, actor: PersonId, action: &EnforcementAction) -> Self {
        let mut system = Self::for_actor(port, actor);
        match action {
            EnforcementAction::File { institution, .. } => {
                system.selection.institutions.insert(*institution);
            }
            EnforcementAction::Report { case, recipient } => {
                system.selection.cases.insert(*case);
                system.selection.actors.insert(*recipient);
            }
            EnforcementAction::Respond { case, .. } | EnforcementAction::Dispatch { case, .. } => {
                system.selection.cases.insert(*case);
            }
        }
        system.reload();
        system
    }
    pub fn for_change(port: P, change: &InstitutionChange) -> Self {
        let mut selection = Selection::default();
        if let InstitutionChange::Adjudicate { case, .. } = change {
            selection.cases.insert(*case);
        }
        Self::selected(port, selection)
    }
    pub fn for_adoption(port: P, institution: InstitutionId, proposal: RuleProposalId) -> Self {
        let mut selection = Selection::default();
        selection.institutions.insert(institution);
        if let Some(row) = crate::proposal(port.store(), proposal)
            && let InstitutionChange::Adjudicate { case, .. } = row.change
        {
            selection.cases.insert(case);
        }
        if let Some(row) = crate::institution(port.store(), institution) {
            selection.areas.insert(row.charter.jurisdiction);
        }
        Self::selected(port, selection)
    }
    pub fn for_validation(port: P) -> Self {
        let mut system = Self::new(port);
        let actors = memories(system.port.store())
            .map(|(actor, _)| actor)
            .collect::<Vec<_>>();
        for actor in actors {
            system.ensure_memory(actor);
        }
        system
    }
    fn selected(port: P, selection: Selection) -> Self {
        let now = port.now();
        let opening_next_id = port.next_id();
        let state = Projection::capture(port.store(), port.runtime(), &selection);
        let reads = Self::membership_reads(port.store(), selection.global);
        let person_index = if selection.global {
            port.people()
                .iter()
                .enumerate()
                .map(|(index, person)| (person.id, index))
                .collect()
        } else {
            BTreeMap::new()
        };
        Self {
            port,
            state: state.clone(),
            opening: state,
            now,
            person_index,
            activities: Vec::new(),
            events: Vec::new(),
            selection,
            reviewing: false,
            revisions: 0,
            opening_next_id,
            allocations: 0,
            interruptions: Vec::new(),
            reads,
            person_reads: RefCell::new(BTreeMap::new()),
            local_rosters: RefCell::new(BTreeMap::new()),
        }
    }
    fn membership_reads(store: &Store, global: bool) -> ReadSet {
        let mut reads = ReadSet::new();
        if global {
            reads.membership::<Case>(store, |_, _| true);
            reads.membership::<Talk>(store, |_, _| true);
            reads.membership::<Policies>(store, |_, _| true);
        }
        reads
    }
    fn person(&self, actor: PersonId) -> Option<PersonFacts> {
        let facts = self.port.person(actor);
        self.person_reads
            .borrow_mut()
            .entry(actor)
            .or_insert_with(|| facts.clone());
        facts
    }
    fn local_people(&self, area: AreaId) -> Ref<'_, Vec<PersonFacts>> {
        if !self.local_rosters.borrow().contains_key(&area) {
            let mut people = self.port.local_people(area);
            people.sort_by_key(|person| person.id);
            self.local_rosters.borrow_mut().insert(area, people);
        }
        Ref::map(self.local_rosters.borrow(), |rosters| &rosters[&area])
    }
    fn reload(&mut self) {
        self.state = Projection::capture(self.port.store(), self.port.runtime(), &self.selection);
        self.opening = self.state.clone();
        self.opening_next_id = self.port.next_id();
        self.reads = Self::membership_reads(self.port.store(), self.selection.global);
        self.person_reads.get_mut().clear();
        self.local_rosters.get_mut().clear();
    }
    fn ensure_memory(&mut self, actor: PersonId) {
        if !self.opening.known.contains_key(&actor) {
            self.opening.capture_memory(self.port.store(), actor);
            self.state.capture_memory(self.port.store(), actor);
        }
    }
    pub fn publish(&mut self) -> Result<(), Error> {
        if self
            .person_reads
            .borrow()
            .iter()
            .any(|(actor, before)| self.port.person(*actor) != *before)
        {
            return Err(Error::Unavailable);
        }
        for (area, before) in self.local_rosters.borrow().iter() {
            let mut after = self.port.local_people(*area);
            after.sort_by_key(|person| person.id);
            if before
                .iter()
                .map(|person| person.id)
                .ne(after.iter().map(|person| person.id))
            {
                return Err(Error::Unavailable);
            }
        }
        let mut batch = Batch::new();
        let mut reads = std::mem::take(&mut self.reads);
        let mut writes = ReadFootprint::new();
        macro_rules! rows {
            ($field:ident,$ty:ty) => {
                let ids = self
                    .opening
                    .$field
                    .keys()
                    .chain(self.state.$field.keys())
                    .copied()
                    .collect::<BTreeSet<_>>();
                for id in ids {
                    let entity = StableId(id.0);
                    let before = self.opening.$field.get(&id);
                    let after = self.state.$field.get(&id);
                    reads.expect_row::<$ty>(entity, before.cloned());
                    if before != after {
                        writes.protect::<$ty>(entity);
                        if let Some(row) = after {
                            batch.insert(entity, row.clone());
                        } else {
                            batch.remove::<$ty>(entity);
                        }
                    }
                }
            };
        }
        for (id, row) in &self.opening.institutions {
            reads.expect_row::<Institution>(StableId(id.0), Some(row.clone()));
        }
        for (actor, (before, after)) in &self.state.institution_memories {
            let id = StableId(actor.0);
            reads.expect_row::<crate::Knowledge>(id, Some(before.clone()));
            if before != after {
                writes.protect::<crate::Knowledge>(id);
                batch.insert(id, after.clone());
            }
        }
        rows!(cases, Case);
        rows!(talks, Talk);
        for id in self
            .opening
            .policies
            .keys()
            .chain(self.state.policies.keys())
            .copied()
            .collect::<BTreeSet<_>>()
        {
            let entity = StableId(id.0);
            let before = self.opening.policies.get(&id).map(|offices| Policies {
                offices: offices.clone(),
            });
            let after = self.state.policies.get(&id).map(|offices| Policies {
                offices: offices.clone(),
            });
            reads.expect_row::<Policies>(entity, before.clone());
            if before != after {
                writes.protect::<Policies>(entity);
                if let Some(row) = after {
                    batch.insert(entity, row);
                } else {
                    batch.remove::<Policies>(entity);
                }
            }
        }
        for actor in self
            .opening
            .memory_ids()
            .union(&self.state.memory_ids())
            .copied()
            .collect::<BTreeSet<_>>()
        {
            let entity = StableId(actor.0);
            let before = self
                .opening
                .present_memories
                .contains(&actor)
                .then(|| self.opening.memory(actor));
            reads.expect_row::<Memory>(entity, before.clone());
            let after = self.state.memory(actor);
            if before.as_ref() != Some(&after) {
                writes.protect::<Memory>(entity);
                batch.insert(entity, after);
                self.state.present_memories.insert(actor);
            }
        }
        if self.port.runtime().map_or(0, |world| world.last_event) != self.opening.last_event {
            return Err(Error::Unavailable);
        }
        let guard = reads.bind(self.port.store(), &batch, &writes)?;
        guard.validate(self.port.store(), &batch)?;
        self.port.store().validate_batch(&batch)?;
        self.port.preflight(self.revisions)?;
        self.port.ensure_event_capacity(self.events.len())?;
        if self.port.next_id() != self.opening_next_id {
            return Err(Error::Unavailable);
        }
        self.opening_next_id
            .checked_add(self.allocations)
            .ok_or(Error::Overflow)?;
        self.port.publish(
            batch,
            guard,
            std::mem::take(&mut self.activities),
            std::mem::take(&mut self.events),
            std::mem::take(&mut self.allocations),
            std::mem::take(&mut self.interruptions),
        )?;
        self.port
            .bump_revision(std::mem::take(&mut self.revisions))?;
        self.opening_next_id = self.port.next_id();
        self.port.set_runtime(World {
            last_event: self.state.last_event,
        });
        self.state.institution_memories.clear();
        self.person_reads.get_mut().clear();
        self.local_rosters.get_mut().clear();
        self.opening = self.state.clone();
        self.reads = Self::membership_reads(self.port.store(), self.selection.global);
        Ok(())
    }
    fn emit(&mut self, event: Event) {
        self.events.push(event);
    }
    fn allocate_id(&mut self) -> Result<StableId, Error> {
        let id = self
            .opening_next_id
            .checked_add(self.allocations)
            .ok_or(Error::Overflow)?;
        self.allocations = self.allocations.checked_add(1).ok_or(Error::Overflow)?;
        self.opening_next_id
            .checked_add(self.allocations)
            .ok_or(Error::Overflow)?;
        Ok(StableId(id))
    }
    fn interrupt_actor_activity(&mut self, actor: PersonId) {
        self.interrupt_enforcement(actor);
        if !self.interruptions.contains(&actor) {
            self.interruptions.push(actor);
        }
    }
    fn agreement_command(
        &mut self,
        actor: PersonId,
        action: &AgreementAction,
    ) -> Result<(), Error> {
        self.publish()?;
        self.port.agreement_command(actor, action)?;
        self.reload();
        Ok(())
    }
    fn institution_command(
        &mut self,
        actor: PersonId,
        action: &InstitutionAction,
    ) -> Result<(), Error> {
        self.publish()?;
        self.port.institution_command(actor, action)?;
        self.reload();
        Ok(())
    }
    fn mission_command(&mut self, actor: PersonId, action: &MissionAction) -> Result<(), Error> {
        self.publish()?;
        self.port.mission_command(actor, action)?;
        self.reload();
        Ok(())
    }
    fn office_effective(&self, institution: &Institution, office: &OfficeMandate) -> bool {
        institution.office_effective(office, &self.port.authority(&institution.members))
    }
    fn subjects(&self, institution: &Institution, subjects: &Subjects) -> BTreeSet<PersonId> {
        institution.subjects(subjects, &self.port.authority(&institution.members))
    }
    fn validate_subjects(&self, subjects: &Subjects, offices: &[OfficeMandate]) -> bool {
        subjects
            .named
            .iter()
            .all(|actor| self.person(*actor).is_some())
            && subjects
                .offices
                .iter()
                .all(|slot| offices.iter().any(|office| office.slot == *slot))
    }
    fn enforcement_world(&mut self) {
        if self.port.runtime().is_none() {
            self.state.last_event = self.port.next_event() - 1;
        }
    }
    fn enforcement_area(&self, actor: PersonId) -> Option<AreaId> {
        let facts = self.person(actor)?;
        if !facts.vitals.alive
            || facts.vitals.health == 0
            || !facts.independent
            || facts.combat_restricted
        {
            return None;
        }
        match facts.location {
            Location::AtArea(area) => Some(area),
            _ => None,
        }
    }
    fn enforcement_office(
        &self,
        institution: InstitutionId,
        office: u16,
    ) -> Option<&OfficeMandate> {
        let i = crate::institution(self.port.store(), institution)?;
        let mandate = i.charter.offices.iter().find(|o| o.slot == office)?;
        self.office_effective(i, mandate).then_some(mandate)
    }
    pub fn validate_enforcement_policy(
        &self,
        i: &institutions::Institution,
        office: u16,
        policy: &EnforcementPolicy,
    ) -> Result<(), Error> {
        let judge = i
            .charter
            .offices
            .iter()
            .find(|o| o.slot == office)
            .ok_or(Error::Unavailable)?;
        let enforcer = i
            .charter
            .offices
            .iter()
            .find(|o| o.slot == policy.enforcer_office)
            .ok_or(Error::Unavailable)?;
        if !judge.powers.contains(&OfficePower::Adjudicate)
            || !enforcer.powers.contains(&OfficePower::Enforce)
            || policy.topics.is_empty()
            || policy.remedies.is_empty()
            || policy.areas.is_empty()
            || policy.areas.len() > 16
            || !policy.areas.contains(&i.charter.jurisdiction)
            || policy
                .areas
                .iter()
                .any(|id| !self.port.areas().contains(id))
            || !self.validate_subjects(&policy.subjects, &i.charter.offices)
            || policy.spending.len() > 16
            || policy.supplies.len() > 16
            || policy.remedies.contains(&RemedyKind::Confinement)
                && policy.maximum_confinement.0 == 0
            || policy.spending.iter().any(|(source, q)| {
                q.0 == 0
                    || !judge.inventories.contains(source)
                    || game_cargo::inventory(self.port.store(), *(source))
                        .is_none_or(|i| q > &i.capacity)
            })
            || policy.supplies.iter().any(|(source, q)| {
                q.0 == 0
                    || !enforcer.inventories.contains(source)
                    || game_cargo::inventory(self.port.store(), *(source))
                        .is_none_or(|i| q > &i.capacity)
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn validate_enforcement_change(
        &self,
        i: &institutions::Institution,
        change: &InstitutionChange,
    ) -> Result<(), Error> {
        match change {
            InstitutionChange::EnforcementPolicy {
                office,
                policy: Some(policy),
            } => self.validate_enforcement_policy(i, *office, policy),
            InstitutionChange::EnforcementPolicy {
                office,
                policy: None,
            } => i
                .charter
                .offices
                .iter()
                .any(|o| o.slot == *office)
                .then_some(())
                .ok_or(Error::Unavailable),
            InstitutionChange::Adjudicate { case, remedy } => {
                let case = self.state.cases.get(case).ok_or(Error::Unavailable)?;
                if case.institution != i.id
                    || case.status != EnforcementStatus::Pending
                    || case.judgment.is_some()
                    || !case.policy.remedies.contains(&remedy.kind())
                    || self
                        .enforcement_office(i.id, case.office)
                        .is_none_or(|m| !m.powers.contains(&OfficePower::Adjudicate))
                    || self
                        .state
                        .policies
                        .get(&i.id)
                        .and_then(|w| w.get(&case.office))
                        .is_none()
                {
                    return Err(Error::Unavailable);
                }
                match remedy {
                    EnforcementRemedy::Dismiss => {}
                    EnforcementRemedy::Confine { until } => {
                        if *until <= self.now
                            || until.0 - self.now.0 > case.policy.maximum_confinement.0
                            || !self.port.combat_available()
                        {
                            return Err(Error::Unavailable);
                        }
                    }
                    EnforcementRemedy::Depose {
                        office,
                        replacement,
                    } => {
                        if !i.members.contains(replacement)
                            || *replacement == case.target
                            || i.charter
                                .offices
                                .iter()
                                .find(|m| m.slot == *office)
                                .is_none_or(|m| m.holder != case.target)
                        {
                            return Err(Error::Unavailable);
                        }
                    }
                    EnforcementRemedy::Settlement { terms } => {
                        if terms.is_empty() || terms.len()>8 || terms.iter().any(|term| !matches!(term,AgreementTerm::Delivery {debtor,quantity,deadline,..} if *debtor==case.target && quantity.0>0 && *deadline>self.now)) {return Err(Error::Unavailable);}
                        for term in terms {
                            if let AgreementTerm::Delivery {
                                source,
                                destination,
                                quantity,
                                ..
                            } = term
                                && game_cargo::inventory(self.port.store(), *(source))
                                    .zip(game_cargo::inventory(self.port.store(), *(destination)))
                                    .is_none_or(|(a, b)| {
                                        a.good != b.good
                                            || a.id == b.id
                                            || quantity.0 > b.capacity.0
                                    })
                            {
                                return Err(Error::Unavailable);
                            }
                        }
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub fn enforcement_change_actor_allowed(
        &self,
        actor: PersonId,
        i: &institutions::Institution,
        change: &InstitutionChange,
        proposing: bool,
    ) -> bool {
        if let InstitutionChange::Adjudicate { case, .. } = change {
            let Some(case) = self
                .state
                .cases
                .get(case)
                .or_else(|| crate::enforcement::case(self.port.store(), *case))
            else {
                return false;
            };
            self.state
                .known
                .get(&actor)
                .is_some_and(|known| known.contains_key(&case.id))
                && (!proposing
                    || self
                        .enforcement_office(i.id, case.office)
                        .is_some_and(|o| o.holder == actor))
        } else {
            true
        }
    }
    pub fn enforcement_change_ids(change: &InstitutionChange) -> u64 {
        u64::from(matches!(change, InstitutionChange::Adjudicate { .. }))
    }
    pub fn preflight_enforcement_adoption(
        &self,
        institution: InstitutionId,
        proposal: RuleProposalId,
    ) -> Result<(), Error> {
        let record = self
            .port
            .institutions_available()
            .then_some(())
            .and_then(|_| crate::proposal(self.port.store(), proposal))
            .ok_or(Error::Unavailable)?;
        let community =
            crate::institution(self.port.store(), institution).ok_or(Error::Unavailable)?;
        if record.institution != institution {
            return Err(Error::Unavailable);
        }
        self.validate_enforcement_change(community, &record.change)?;
        if matches!(record.change, InstitutionChange::Adjudicate { .. }) {
            self.port.next_id().checked_add(1).ok_or(Error::Overflow)?;
        }
        self.port.ensure_event_capacity(2)
    }
    pub fn adopt_enforcement_change(
        &mut self,
        institution: InstitutionId,
        proposal: RuleProposalId,
    ) -> Result<(), Error> {
        let owner_now = self.now;
        let change = crate::proposal(self.port.store(), proposal)
            .expect("validated Society reference")
            .clone();
        match change.change.clone() {
            InstitutionChange::EnforcementPolicy { office, policy } => {
                self.enforcement_world();
                let policies = self.state.policies.entry(institution).or_default();
                super::adoption::apply_policy_change(
                    policies,
                    institution,
                    office,
                    policy.as_ref(),
                    owner_now,
                );
                self.emit(Event::EnforcementPolicyChanged {
                    institution,
                    office,
                });
            }
            InstitutionChange::Adjudicate { case, remedy } => {
                let judgment = JudgmentId(self.allocate_id()?.0);
                let jurisdiction = crate::institution(self.port.store(), institution)
                    .expect("validated Society reference")
                    .charter
                    .jurisdiction;
                let record = self.state.cases.get_mut(&case).unwrap();
                *record = super::adoption::adopted_case(
                    record,
                    &change,
                    &remedy,
                    judgment,
                    jurisdiction,
                    owner_now,
                );
                self.emit(Event::EnforcementDecided {
                    case,
                    judgment,
                    proposer: change.proposer,
                });
            }
            _ => {}
        }
        self.observe_enforcement();
        Ok(())
    }
    fn evidence_for(
        &self,
        actor: PersonId,
        evidence: EnforcementEvidenceRef,
    ) -> Option<(
        EnforcementTopic,
        PersonId,
        EnforcementEvidence,
        Option<ReportedContact>,
    )> {
        let (topic, target, basis, contact) = match evidence {
            EnforcementEvidenceRef::Incident(id) => {
                let incident = self
                    .port
                    .incidents(actor)
                    .into_iter()
                    .find(|i| i.id == id)?;
                (
                    EnforcementTopic::Taking,
                    incident.actor,
                    EnforcementEvidence::Incident(incident.clone()),
                    Some(ReportedContact {
                        person: incident.actor,
                        area: incident.area,
                        observed_at: incident.changed_at,
                    }),
                )
            }
            EnforcementEvidenceRef::Assault(id) => {
                let assault = self.state.assaults.get(&actor)?.get(&id)?.clone();
                (
                    EnforcementTopic::Assault,
                    assault.attacker,
                    EnforcementEvidence::Assault(assault.clone()),
                    Some(ReportedContact {
                        person: assault.attacker,
                        area: assault.area,
                        observed_at: assault.observed_at,
                    }),
                )
            }
            EnforcementEvidenceRef::Default(id) => {
                let obligation = self
                    .port
                    .agreement_view(actor)
                    .into_iter()
                    .flat_map(|a| a.obligations)
                    .find(|o| o.id == id && o.defaulted && o.performed < o.amount())?;
                let target = obligation.term.parties()[0];
                (
                    EnforcementTopic::Default,
                    target,
                    EnforcementEvidence::Default(obligation),
                    None,
                )
            }
            EnforcementEvidenceRef::Social(id) => {
                let case = self
                    .port
                    .social_case_view(actor)
                    .into_iter()
                    .find(|c| c.id == id && c.kind == SocialCaseKind::Exclusion)?;
                (
                    EnforcementTopic::Exclusion,
                    case.counterpart,
                    EnforcementEvidence::Social(case.clone()),
                    Some(ReportedContact {
                        person: case.counterpart,
                        area: case.area,
                        observed_at: case.observed_at,
                    }),
                )
            }
        };
        let newer = self
            .state
            .contacts
            .get(&actor)
            .and_then(|k| k.get(&target))
            .cloned();
        let contact = match (contact, newer) {
            (Some(old), Some(new)) => Some(if new.observed_at > old.observed_at {
                new
            } else {
                old
            }),
            (old, new) => old.or(new),
        };
        Some((topic, target, basis, contact))
    }
    fn case_view(
        &self,
        case: &Case,
        actor: PersonId,
        source: KnowledgeSource,
    ) -> EnforcementCaseView {
        super::view_of_case(case, actor, self.now, source)
    }
    pub fn enforcement_view(&self, actor: PersonId) -> EnforcementView {
        let world = &self.state;
        EnforcementView {
            policies: world
                .known_policies
                .get(&actor)
                .into_iter()
                .flat_map(|institutions| institutions.values())
                .flat_map(|offices| offices.values())
                .take(32)
                .cloned()
                .collect(),
            cases: world
                .known
                .get(&actor)
                .into_iter()
                .flat_map(|cases| cases.values().rev())
                .take(64)
                .cloned()
                .collect(),
            assaults: world
                .assaults
                .get(&actor)
                .into_iter()
                .flat_map(|cases| cases.values().rev())
                .take(32)
                .cloned()
                .collect(),
        }
    }
    pub fn debug_enforcement_action(
        &mut self,
        actor: PersonId,
        action: EnforcementAction,
    ) -> Result<(), Error> {
        self.enforcement_command(actor, &action)
    }
    pub fn enforcement_command(
        &mut self,
        actor: PersonId,
        action: &EnforcementAction,
    ) -> Result<(), Error> {
        let owner_now = self.now;
        self.port.preflight(1)?;
        let area = self.enforcement_area(actor).ok_or(Error::Unavailable)?;
        if !self.selection.global {
            self.selection.areas.insert(area);
        }
        if self.port.in_combat(actor) {
            return Err(Error::Unavailable);
        }
        match action {
            EnforcementAction::File {
                institution,
                office,
                evidence,
            } => {
                let known = self
                    .enforcement_view(actor)
                    .policies
                    .into_iter()
                    .find(|p| p.institution == *institution && p.office == *office)
                    .ok_or(Error::Unavailable)?;
                let actual = self
                    .state
                    .policies
                    .get(institution)
                    .and_then(|w| w.get(office))
                    .ok_or(Error::Unavailable)?;
                if known.policy != actual.policy {
                    return Err(Error::Unavailable);
                }
                let (topic, target, basis, contact) = self
                    .evidence_for(actor, *evidence)
                    .ok_or(Error::Unavailable)?;
                let i =
                    crate::institution(self.port.store(), institution).ok_or(Error::Unavailable)?;
                let officer = self
                    .enforcement_office(*institution, *office)
                    .ok_or(Error::Unavailable)?;
                let recipient = officer.holder;
                if i.charter.jurisdiction != area
                    || self.enforcement_area(recipient) != Some(area)
                    || !officer.powers.contains(&OfficePower::Adjudicate)
                    || !known.policy.topics.contains(&topic)
                    || !self.subjects(i, &known.policy.subjects).contains(&target)
                    || contact
                        .as_ref()
                        .is_some_and(|c| !known.policy.areas.contains(&c.area))
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(owner_now.0.checked_add(MINUTE.0).ok_or(Error::Overflow)?);
                self.port.ensure_event_capacity(2)?;
                let id = EnforcementCaseId(self.allocate_id()?.0);
                self.interrupt_actor_activity(actor);
                let case = Case {
                    id,
                    institution: *institution,
                    office: *office,
                    filer: actor,
                    target,
                    topic,
                    evidence: basis,
                    contact,
                    policy: known.policy,
                    judgment: None,
                    mission: None,
                    settlement: None,
                    status: EnforcementStatus::Filing,
                    explanation: "Preparing a complaint from remembered observations or reports"
                        .into(),
                    changed_at: owner_now,
                    notice_area: None,
                    witnesses: BTreeSet::from([actor]),
                };
                let view = self.case_view(&case, actor, KnowledgeSource::Observed(actor));
                let world = &mut self.state;
                world.cases.insert(id, case);
                world.known.entry(actor).or_default().insert(id, view);
                self.start_enforcement_talk(
                    actor,
                    Talk {
                        case: id,
                        recipient,
                        area,
                        completes,
                        conversation: Conversation::File,
                    },
                );
                self.emit(Event::EnforcementFiling {
                    case: id,
                    actor,
                    recipient,
                });
            }
            EnforcementAction::Report { case, recipient } => {
                let snapshot = self
                    .state
                    .known
                    .get(&actor)
                    .and_then(|k| k.get(case))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                let institution = self
                    .port
                    .institution_view(actor)
                    .into_iter()
                    .find(|i| i.id == snapshot.institution)
                    .ok_or(Error::Unavailable)?;
                if *recipient == actor
                    || self.enforcement_area(*recipient) != Some(area)
                    || matches!(snapshot.status, EnforcementStatus::Filing)
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(owner_now.0.checked_add(MINUTE.0).ok_or(Error::Overflow)?);
                self.port.ensure_event_capacity(1)?;
                self.interrupt_actor_activity(actor);
                self.start_enforcement_talk(
                    actor,
                    Talk {
                        case: *case,
                        recipient: *recipient,
                        area,
                        completes,
                        conversation: Conversation::Report {
                            snapshot: Box::new(snapshot),
                            institution: Box::new(institution),
                        },
                    },
                );
            }
            EnforcementAction::Respond { case, comply } => {
                let known = self
                    .state
                    .known
                    .get(&actor)
                    .and_then(|k| k.get(case))
                    .ok_or(Error::Unavailable)?;
                let actual = self
                    .state
                    .cases
                    .get(case)
                    .ok_or(Error::Unavailable)?
                    .clone();
                let judgment = actual.judgment.as_ref().ok_or(Error::Unavailable)?;
                if actual.target != actor
                    || known.judgment.as_ref() != Some(judgment)
                    || !matches!(
                        actual.status,
                        EnforcementStatus::Decided | EnforcementStatus::Refused
                    )
                    || crate::institution(self.port.store(), actual.institution)
                        .unwrap()
                        .charter
                        .jurisdiction
                        != area
                {
                    return Err(Error::Unavailable);
                }
                self.port.ensure_event_capacity(2)?;
                let mut settlement = None;
                if *comply {
                    if let EnforcementRemedy::Settlement { terms } = &judgment.remedy {
                        if let Some(old) = self.existing_settlement_debt(&actual) {
                            let debt = game_agreements::obligation(self.port.store(), old)
                                .ok_or(Error::Unavailable)?;
                            settlement = Some(debt.agreement);
                        } else {
                            let parties = terms
                                .iter()
                                .flat_map(|t| t.parties())
                                .collect::<BTreeSet<_>>();
                            let next = self.port.next_id();
                            self.agreement_command(
                                actor,
                                &AgreementAction::Propose {
                                    parties,
                                    terms: terms.clone(),
                                    procedure: None,
                                },
                            )?;
                            settlement = game_agreements::agreements(self.port.store())
                                .map(|(_, row)| row)
                                .find(|a| a.id.0 >= next && a.proposer == actor)
                                .map(|a| a.id);
                        }
                    } else {
                        return Err(Error::Unavailable);
                    }
                }
                self.port.ensure_event_capacity(1)?;
                let record = self.state.cases.get_mut(case).unwrap();
                record.settlement = settlement;
                record.status = if *comply {
                    EnforcementStatus::Offered
                } else {
                    EnforcementStatus::Refused
                };
                record.changed_at = owner_now;
                record.notice_area = Some(area);
                record.witnesses = BTreeSet::from([actor]);
                record.explanation = if *comply {
                    "The person offered physical settlement; goods are still owed"
                } else {
                    "The person refused the court's request"
                }
                .into();
                self.emit(Event::EnforcementResponded {
                    case: *case,
                    actor,
                    comply: *comply,
                });
            }
            EnforcementAction::Dispatch { case, proposal } => {
                self.dispatch_enforcement(actor, *case, proposal)?
            }
        }
        if !self.reviewing {
            self.observe_enforcement();
        }
        self.revisions = self.revisions.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    fn start_enforcement_talk(&mut self, actor: PersonId, talk: Talk) {
        self.activities.push(ActivityEffect::Talk {
            actor,
            case: talk.case,
            recipient: talk.recipient,
            completes: talk.completes,
        });
        self.state.talks.insert(actor, talk);
    }
    pub fn interrupt_enforcement(&mut self, actor: PersonId) {
        let owner_now = self.now;
        let Some(talk) = self.state.talks.remove(&actor) else {
            return;
        };
        if let Some(change) = crate::enforcement::complaint_interruption(
            owner_now,
            matches!(talk.conversation, Conversation::File),
        ) {
            let case = self.state.cases.get_mut(&talk.case).unwrap();
            case.status = EnforcementStatus::Withdrawn;
            case.changed_at = change.changed_at;
            case.explanation = change.explanation.into();
        }
    }
    pub fn complete_enforcement_talk(&mut self, actor: PersonId) {
        if self
            .state
            .talks
            .get(&actor)
            .is_some_and(|talk| talk.completes > self.now)
        {
            return;
        }
        let owner_now = self.now;
        let Some(talk) = self.state.talks.remove(&actor) else {
            return;
        };
        if self.enforcement_area(actor) == Some(talk.area)
            && self.enforcement_area(talk.recipient) == Some(talk.area)
        {
            match talk.conversation {
                Conversation::File => {
                    let record = self.state.cases.get_mut(&talk.case).unwrap();
                    record.status = EnforcementStatus::Pending;
                    record.changed_at = owner_now;
                    record.notice_area = Some(talk.area);
                    record.witnesses = BTreeSet::from([actor, talk.recipient]);
                    record.explanation =
                        "A complaint reached the officer; the accusation has not been decided"
                            .into();
                    let case = record.clone();
                    let listeners = self
                        .local_enforcement_listeners(talk.area)
                        .into_iter()
                        .chain([actor, talk.recipient])
                        .collect::<BTreeSet<_>>();
                    for listener in listeners {
                        self.ensure_memory(listener);
                        let snapshot =
                            self.case_view(&case, listener, KnowledgeSource::Reported(actor));
                        self.state
                            .known
                            .entry(listener)
                            .or_default()
                            .insert(case.id, snapshot);
                    }
                    self.emit(Event::EnforcementFiled {
                        case: case.id,
                        actor,
                        institution: case.institution,
                    });
                }
                Conversation::Report {
                    mut snapshot,
                    institution,
                } => {
                    self.ensure_memory(talk.recipient);
                    snapshot.learned_at = owner_now;
                    snapshot.source = KnowledgeSource::Reported(actor);
                    let known = self.state.known.entry(talk.recipient).or_default();
                    if let Some(old) = known.get(&snapshot.id).cloned() {
                        let contact = match (old.contact.clone(), snapshot.contact.clone()) {
                            (Some(a), Some(b)) => {
                                Some(if a.observed_at > b.observed_at { a } else { b })
                            }
                            (a, b) => a.or(b),
                        };
                        if old.observed_at > snapshot.observed_at {
                            *snapshot = old;
                        }
                        snapshot.contact = contact;
                    }
                    known.insert(snapshot.id, *snapshot);
                    if let Some(opening) = crate::knowledge(self.port.store(), talk.recipient) {
                        let (_, memory) = self
                            .state
                            .institution_memories
                            .entry(talk.recipient)
                            .or_insert_with(|| (opening.clone(), opening.clone()));
                        if memory
                            .institutions
                            .get(&institution.id)
                            .is_none_or(|old| old.observed_at <= institution.observed_at)
                        {
                            memory.institutions.insert(institution.id, *institution);
                        }
                    }
                    self.emit(Event::EnforcementReported {
                        case: talk.case,
                        sender: actor,
                        recipient: talk.recipient,
                    });
                }
            }
        } else if matches!(talk.conversation, Conversation::File) {
            let case = self.state.cases.get_mut(&talk.case).unwrap();
            case.status = EnforcementStatus::Withdrawn;
            case.changed_at = owner_now;
            case.explanation = "The officer could no longer hear the complaint".into();
        }
        self.activities.push(ActivityEffect::Idle { actor });
    }

    fn local_enforcement_listeners(&self, area: AreaId) -> Vec<PersonId> {
        self.local_people(area)
            .iter()
            .filter_map(|person| {
                let facts = self.person(person.id)?;
                (facts.location == Location::AtArea(area)
                    && facts.vitals.alive
                    && facts.vitals.health > 0)
                    .then_some(person.id)
            })
            .take(64)
            .collect()
    }
    fn dispatch_enforcement(
        &mut self,
        actor: PersonId,
        id: EnforcementCaseId,
        proposal: &MissionProposal,
    ) -> Result<(), Error> {
        let owner_now = self.now;
        let area = self.enforcement_area(actor).ok_or(Error::Unavailable)?;
        let known = self
            .state
            .known
            .get(&actor)
            .and_then(|k| k.get(&id))
            .cloned()
            .ok_or(Error::Unavailable)?;
        let case = self
            .state
            .cases
            .get(&id)
            .cloned()
            .ok_or(Error::Unavailable)?;
        let judgment = case.judgment.as_ref().ok_or(Error::Unavailable)?;
        let EnforcementRemedy::Confine { until } = judgment.remedy else {
            return Err(Error::Unavailable);
        };
        let judge = self
            .enforcement_office(case.institution, case.office)
            .ok_or(Error::Unavailable)?;
        let officer = self
            .enforcement_office(case.institution, case.policy.enforcer_office)
            .ok_or(Error::Unavailable)?;
        let contact = known.contact.as_ref().ok_or(Error::Unavailable)?;
        if judge.holder != actor
            || !judge.powers.contains(&OfficePower::Adjudicate)
            || !officer.powers.contains(&OfficePower::Enforce)
            || known.judgment.as_ref() != Some(judgment)
            || !matches!(
                case.status,
                EnforcementStatus::Decided | EnforcementStatus::Refused
            )
            || case.mission.is_some()
            || contact.person != case.target
            || contact.observed_at > owner_now
            || !case.policy.areas.contains(&contact.area)
            || until <= owner_now
            || proposal.leader != officer.holder
            || proposal.home != area
            || proposal.objective
                != (MissionObjective::Confine {
                    person: case.target,
                    until,
                })
            || proposal.deadline < until
            || crate::institution(self.port.store(), case.institution)
                .expect("validated Society reference")
                .charter
                .jurisdiction
                != area
            || self
                .state
                .policies
                .get(&case.institution)
                .and_then(|p| p.get(&case.office))
                .is_none_or(|p| p.policy != case.policy)
        {
            return Err(Error::Unavailable);
        }
        let mut budget = crate::enforcement::DispatchBudget::default();
        for supply in &proposal.supplies {
            *budget.supplies.entry(supply.source).or_default() += u128::from(supply.quantity.0);
        }
        for terms in proposal.compensation.values() {
            for term in terms {
                match *term {
                    AgreementTerm::Delivery {
                        debtor,
                        source,
                        quantity,
                        ..
                    } if debtor == actor => {
                        *budget.spending.entry(source).or_default() += u128::from(quantity.0)
                    }
                    _ => return Err(Error::Unavailable),
                }
            }
        }
        budget.validate(&case.policy)?;
        self.port.preflight_mission_target_report(
            proposal,
            actor,
            contact.person,
            contact.area,
            contact.observed_at,
        )?;
        self.port.ensure_event_capacity(128)?;
        let next = self.port.next_id();
        self.mission_command(
            actor,
            &MissionAction::Propose {
                proposal: proposal.clone(),
            },
        )?;
        let mission = game_missions::missions(self.port.store())
            .map(|(_, row)| row)
            .find(|m| m.id.0 >= next && m.sponsor == actor)
            .unwrap()
            .id;
        self.port.seed_mission_target_report(
            mission,
            actor,
            contact.person,
            contact.area,
            contact.observed_at,
        )?;
        let participants = proposal
            .recruits
            .iter()
            .copied()
            .chain([proposal.leader])
            .collect::<BTreeSet<_>>();
        let record = self.state.cases.get_mut(&id).unwrap();
        record.mission = Some(mission);
        record.status = EnforcementStatus::Offered;
        record.changed_at = owner_now;
        record.notice_area = Some(area);
        record.witnesses = BTreeSet::from([actor]);
        record.explanation="The officer received a funded expedition offer; volunteers and supplies are still required".into();
        let snapshot = self.case_view(
            &self.state.cases[&id],
            actor,
            KnowledgeSource::Reported(actor),
        );
        for person in participants {
            self.ensure_memory(person);
            self.state
                .known
                .entry(person)
                .or_default()
                .insert(id, snapshot.clone());
        }
        self.emit(Event::EnforcementDispatched {
            case: id,
            actor,
            mission,
        });
        Ok(())
    }
    pub fn observe_enforcement(&mut self) {
        let world = &self.state;
        let cursor = world.last_event;
        let policies = world
            .policies
            .values()
            .flat_map(|p| p.values())
            .cloned()
            .collect::<Vec<_>>();
        let cases = world.cases.values().cloned().collect::<Vec<_>>();
        let events = self
            .port
            .events()
            .iter()
            .filter(|e| e.sequence > cursor && e.at == self.now)
            .cloned()
            .collect::<Vec<_>>();
        for event in events {
            if let Strike::Struck {
                encounter,
                actor,
                target,
                damage,
            } = event.event
                && let Some(area) = self.port.encounter_area(encounter)
            {
                for observer in self.local_enforcement_listeners(area) {
                    self.ensure_memory(observer);
                    self.state.assaults.entry(observer).or_default().insert(
                        EventId(event.sequence),
                        WitnessedAssault {
                            event: EventId(event.sequence),
                            attacker: actor,
                            target,
                            area,
                            damage,
                            observed_at: event.at,
                        },
                    );
                }
            }
        }
        self.state.last_event = self.port.next_event() - 1;
        let mut by_area = BTreeMap::<AreaId, Vec<PersonId>>::new();
        let people = if self.selection.global {
            self.port.people()
        } else {
            self.selection
                .areas
                .iter()
                .flat_map(|area| self.local_people(*area).clone())
                .collect()
        };
        for person in people {
            let Some(facts) = self.person(person.id) else {
                continue;
            };
            if let Location::AtArea(area) = facts.location
                && facts.vitals.alive
                && facts.vitals.health > 0
                && (self.selection.global || self.selection.areas.contains(&area))
            {
                by_area.entry(area).or_default().push(person.id);
            }
        }
        for (area, mut people) in by_area {
            people.sort_unstable();
            for actor in people.iter().copied() {
                self.ensure_memory(actor);
                let observations = people
                    .iter()
                    .copied()
                    .take(64)
                    .map(|person| {
                        (
                            person,
                            ReportedContact {
                                person,
                                area,
                                observed_at: self.now,
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                for (person, contact) in observations {
                    self.state
                        .contacts
                        .entry(actor)
                        .or_default()
                        .insert(person, contact);
                }
                for policy in policies.iter().filter(|p| {
                    crate::institution(self.port.store(), p.institution)
                        .expect("validated Society reference")
                        .charter
                        .jurisdiction
                        == area
                }) {
                    let mut observed = policy.clone();
                    observed.observed_at = self.now;
                    self.state
                        .known_policies
                        .entry(actor)
                        .or_default()
                        .entry(policy.institution)
                        .or_default()
                        .insert(policy.office, observed);
                }
                // Announced formal decisions are local. Internal expedition outcomes
                // are never copied into a remote court's memory by this observer.
                for case in &cases {
                    if case.changed_at == self.now
                        && (case.witnesses.contains(&actor) || case.notice_area == Some(area))
                        && case.status != EnforcementStatus::Filing
                    {
                        let view = self.case_view(case, actor, KnowledgeSource::Observed(actor));
                        self.state
                            .known
                            .entry(actor)
                            .or_default()
                            .insert(case.id, view);
                    }
                }
                let contacts = self.state.contacts.get(&actor).cloned().unwrap_or_default();
                if let Some(known) = self.state.known.get_mut(&actor) {
                    for view in known.values_mut() {
                        if let Some(contact) = contacts.get(&view.target)
                            && view
                                .contact
                                .as_ref()
                                .is_none_or(|old| old.observed_at < contact.observed_at)
                        {
                            view.contact = Some(contact.clone());
                        }
                    }
                }
                let obsolete = self
                    .state
                    .known_policies
                    .get(&actor)
                    .into_iter()
                    .flat_map(|institutions| institutions.iter())
                    .filter(|(id, _)| {
                        crate::institution(self.port.store(), *id)
                            .expect("validated Society reference")
                            .charter
                            .jurisdiction
                            == area
                    })
                    .map(|(id, known)| {
                        (
                            *id,
                            known
                                .keys()
                                .copied()
                                .filter(|office| {
                                    !self
                                        .state
                                        .policies
                                        .get(id)
                                        .is_some_and(|p| p.contains_key(office))
                                })
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>();
                for (id, offices) in obsolete {
                    for office in offices {
                        self.state
                            .known_policies
                            .get_mut(&actor)
                            .unwrap()
                            .get_mut(&id)
                            .unwrap()
                            .remove(&office);
                    }
                }
            }
        }
    }
    fn reconcile_enforcement(&mut self) {
        let owner_now = self.now;
        let world = &self.state;
        let cases = world.cases.values().cloned().collect::<Vec<_>>();
        for case in cases {
            let outcome = if let Some(mission) = case.mission {
                self.port
                    .missions_available()
                    .then_some(())
                    .and_then(|_| game_missions::mission(self.port.store(), mission))
                    .and_then(|m| match m.status {
                        MissionStatus::Completed => Some(EnforcementStatus::Completed),
                        MissionStatus::Failed | MissionStatus::Cancelled => {
                            Some(EnforcementStatus::Failed)
                        }
                        MissionStatus::Traveling
                        | MissionStatus::Operating
                        | MissionStatus::Returning => Some(EnforcementStatus::Enforcing),
                        MissionStatus::Preparing
                            if m.members.values().all(|p| {
                                matches!(
                                    p.status,
                                    CompanionStatus::Refused | CompanionStatus::Withdrawn
                                )
                            }) =>
                        {
                            Some(EnforcementStatus::Refused)
                        }
                        _ => None,
                    })
            } else if let Some(agreement) = case.settlement {
                if let Some(old) = self.existing_settlement_debt(&case) {
                    self.port
                        .agreements_available()
                        .then_some(())
                        .and_then(|_| game_agreements::obligation(self.port.store(), old))
                        .filter(|o| o.performed >= o.amount())
                        .map(|_| EnforcementStatus::Completed)
                } else {
                    self.port
                        .agreements_available()
                        .then_some(())
                        .and_then(|_| game_agreements::agreement(self.port.store(), agreement))
                        .and_then(|a| match a.status {
                            AgreementStatus::Fulfilled => Some(EnforcementStatus::Completed),
                            AgreementStatus::Rejected | AgreementStatus::Withdrawn => {
                                Some(EnforcementStatus::Refused)
                            }
                            _ => None,
                        })
                }
            } else {
                None
            };
            let Some(outcome) = outcome else {
                continue;
            };
            let changed = outcome != case.status;
            if changed {
                let record = self.state.cases.get_mut(&case.id).unwrap();
                record.status = outcome;
                record.changed_at = owner_now;
                record.notice_area = None;
                record.witnesses.clear();
                record.explanation=match outcome {EnforcementStatus::Completed=>"Actual agents or delivery fulfilled the requested task; assent to the court is separate",EnforcementStatus::Refused=>"Named people refused the proposed service or settlement",EnforcementStatus::Failed=>"The physical expedition returned without fulfilling the chosen task",_=>"Consenting officers are undertaking the physical expedition"}.into();
            }
            let actors = memories(self.port.store())
                .filter(|(_, memory)| memory.known.contains_key(&case.id))
                .map(|(actor, _)| actor)
                .collect::<Vec<_>>();
            for actor in actors {
                self.ensure_memory(actor);
                let informed = if let Some(mission) = case.mission {
                    self.port
                        .mission_view(actor)
                        .into_iter()
                        .find(|m| m.id == mission)
                        .is_some_and(|m| match outcome {
                            EnforcementStatus::Completed => m.status == MissionStatus::Completed,
                            EnforcementStatus::Failed => {
                                matches!(m.status, MissionStatus::Failed | MissionStatus::Cancelled)
                            }
                            EnforcementStatus::Enforcing => matches!(
                                m.status,
                                MissionStatus::Traveling
                                    | MissionStatus::Operating
                                    | MissionStatus::Returning
                            ),
                            EnforcementStatus::Refused => m.recruitments.iter().all(|r| {
                                matches!(
                                    r.status,
                                    CompanionStatus::Refused | CompanionStatus::Withdrawn
                                )
                            }),
                            _ => false,
                        })
                } else if let Some(agreement) = case.settlement {
                    self.port
                        .agreement_view(actor)
                        .into_iter()
                        .find(|a| a.id == agreement)
                        .is_some_and(|a| {
                            if let Some(id) = self.existing_settlement_debt(&case) {
                                a.obligations
                                    .iter()
                                    .any(|o| o.id == id && o.performed >= o.amount())
                            } else {
                                a.status == AgreementStatus::Fulfilled
                            }
                        })
                } else {
                    false
                };
                if informed
                    && self.state.known[&actor]
                        .get(&case.id)
                        .is_some_and(|known| known.status != outcome)
                {
                    let actual = self.state.cases[&case.id].clone();
                    let source = case
                        .mission
                        .and_then(|id| {
                            self.port
                                .mission_view(actor)
                                .into_iter()
                                .find(|m| m.id == id)
                                .and_then(|m| m.reported_by)
                        })
                        .map_or(KnowledgeSource::Observed(actor), KnowledgeSource::Reported);
                    let mut snapshot = self.case_view(&actual, actor, source);
                    if let Some(old) = self.state.known.get(&actor).and_then(|k| k.get(&case.id)) {
                        snapshot.contact = old.contact.clone();
                    }
                    self.state
                        .known
                        .entry(actor)
                        .or_default()
                        .insert(case.id, snapshot);
                }
            }
            if changed {
                self.emit(Event::EnforcementOutcome {
                    case: case.id,
                    status: outcome,
                });
            }
        }
    }
    fn existing_settlement_debt(&self, case: &Case) -> Option<ObligationId> {
        let EnforcementEvidence::Default(old) = &case.evidence else {
            return None;
        };
        let EnforcementRemedy::Settlement { terms } = &case.judgment.as_ref()?.remedy else {
            return None;
        };
        if terms.len() != 1 {
            return None;
        }
        match (&old.term, &terms[0]) {
            (
                AgreementTerm::Delivery {
                    debtor: a,
                    creditor: b,
                    source: s,
                    destination: d,
                    ..
                },
                AgreementTerm::Delivery {
                    debtor: x,
                    creditor: y,
                    source: u,
                    destination: v,
                    quantity,
                    ..
                },
            ) if a == x
                && b == y
                && s == u
                && d == v
                && quantity.0 == old.amount() - old.performed =>
            {
                Some(old.id)
            }
            _ => None,
        }
    }
    pub fn review_enforcement(&mut self) -> Result<(), Error> {
        let owner_now = self.now;
        self.reconcile_enforcement();
        self.observe_enforcement();
        self.reviewing = true;
        let actors = self
            .person_index
            .keys()
            .copied()
            .filter(|p| *p != self.port.ruler())
            .collect::<Vec<_>>();
        for actor in actors {
            self.ensure_memory(actor);
            if self.enforcement_area(actor).is_none()
                || self.port.past_life_controls_actor(actor)
                || self.port.in_combat(actor)
                || ({
                    let w = &self.state;
                    w.talks.contains_key(&actor)
                        || w.reconsider_after
                            .get(&actor)
                            .is_some_and(|t| *t > owner_now)
                })
            {
                continue;
            }
            if !self.port.idle_or_resting(actor) {
                continue;
            }
            let commands = self.available_enforcement_commands(actor);
            let personality = &self.port.mind(actor).personality;
            let chosen = commands.into_iter().find(|command| match command {
                EnforcementAction::File { .. } => personality.honesty >= 700,
                EnforcementAction::Respond { comply, .. } => {
                    *comply == (personality.honesty >= 500)
                }
                EnforcementAction::Dispatch { .. } => personality.discipline >= 600,
                _ => false,
            });
            if let Some(action) = chosen {
                let _ = self.enforcement_command(actor, &action);
                self.state
                    .reconsider_after
                    .insert(actor, SimTime(owner_now.0.saturating_add(180_000)));
            }
        }
        self.reviewing = false;
        self.propose_known_judgments()?;
        self.review_enforcement_support();
        Ok(())
    }
    fn default_dispatch_proposal(
        &self,
        actor: PersonId,
        case: &EnforcementCaseView,
    ) -> Option<MissionProposal> {
        let EnforcementRemedy::Confine { until } = case.judgment.as_ref()?.remedy else {
            return None;
        };
        let policy = self
            .enforcement_view(actor)
            .policies
            .into_iter()
            .find(|p| p.institution == case.institution && p.office == case.office)?
            .policy;
        let institution = self
            .port
            .institution_view(actor)
            .into_iter()
            .find(|i| i.id == case.institution)?;
        let judge = institution
            .offices
            .iter()
            .find(|o| o.mandate.slot == case.office && o.effective && o.mandate.holder == actor)?;
        let _ = judge;
        let leader = institution
            .offices
            .iter()
            .find(|o| o.mandate.slot == policy.enforcer_office && o.effective)?
            .mandate
            .holder;
        let area = self.enforcement_area(actor)?;
        if self.enforcement_area(leader) != Some(area) || !self.port.missions_available() {
            return None;
        }
        let rules = self.port.physical_rules()?;
        let supplies = policy
            .supplies
            .iter()
            .filter_map(|(source, cap)| {
                let good = game_cargo::inventory(self.port.store(), *(source))?.good;
                let desired = match good {
                    Good::Food => rules.meal_size.0.saturating_mul(2),
                    Good::Tool | Good::Binding => 1_000,
                    _ => return None,
                };
                let quantity = Quantity(desired.min(cap.0));
                (quantity.0 > 0).then_some(MissionSupply {
                    carrier: leader,
                    source: *source,
                    quantity,
                })
            })
            .collect();
        let payment_destination = self.person(leader)?.inventory;
        let payment = policy
            .spending
            .iter()
            .find(|(source, _)| {
                game_cargo::inventory(self.port.store(), **source)
                    .is_some_and(|i| i.good == Good::Food)
            })
            .map(|(source, cap)| AgreementTerm::Delivery {
                debtor: actor,
                creditor: leader,
                source: *source,
                destination: payment_destination,
                quantity: Quantity(cap.0.min(rules.meal_size.0)),
                deadline: SimTime(self.now.0.saturating_add(300_000)),
            });
        let compensation = payment
            .map(|term| BTreeMap::from([(leader, vec![term])]))
            .unwrap_or_default();
        Some(MissionProposal {
            leader,
            recruits: BTreeSet::new(),
            objective: MissionObjective::Confine {
                person: case.target,
                until,
            },
            home: area,
            supplies,
            compensation,
            deadline: SimTime(until.0.checked_add(600_000)?),
        })
    }
    pub fn available_enforcement_commands(&self, actor: PersonId) -> Vec<EnforcementAction> {
        let Some(area) = self.enforcement_area(actor) else {
            return Vec::new();
        };
        if self.port.in_combat(actor) {
            return Vec::new();
        }
        let view = self.enforcement_view(actor);
        let mut actions = Vec::new();
        let mut evidence = self
            .port
            .incidents(actor)
            .iter()
            .filter(|i| i.actor != actor)
            .take(8)
            .map(|i| EnforcementEvidenceRef::Incident(i.id))
            .collect::<Vec<_>>();
        evidence.extend(
            view.assaults
                .iter()
                .filter(|a| a.attacker != actor)
                .take(8)
                .map(|a| EnforcementEvidenceRef::Assault(a.event)),
        );
        evidence.extend(
            self.port
                .agreement_view(actor)
                .iter()
                .flat_map(|a| a.obligations.iter())
                .filter(|o| o.defaulted && o.performed < o.amount())
                .take(8)
                .map(|o| EnforcementEvidenceRef::Default(o.id)),
        );
        evidence.extend(
            self.port
                .social_case_view(actor)
                .iter()
                .filter(|c| c.kind == SocialCaseKind::Exclusion)
                .take(8)
                .map(|c| EnforcementEvidenceRef::Social(c.id)),
        );
        for policy in &view.policies {
            let Some(officer) = self.enforcement_office(policy.institution, policy.office) else {
                continue;
            };
            if self.enforcement_area(officer.holder) != Some(area) {
                continue;
            }
            for source in &evidence {
                let Some((topic, target, _, contact)) = self.evidence_for(actor, *source) else {
                    continue;
                };
                let duplicate = view.cases.iter().any(|c| {
                    c.filer == actor
                        && match (&c.evidence, source) {
                            (
                                EnforcementEvidence::Incident(i),
                                EnforcementEvidenceRef::Incident(id),
                            ) => i.id == *id,
                            (
                                EnforcementEvidence::Assault(a),
                                EnforcementEvidenceRef::Assault(id),
                            ) => a.event == *id,
                            (
                                EnforcementEvidence::Default(o),
                                EnforcementEvidenceRef::Default(id),
                            ) => o.id == *id,
                            (
                                EnforcementEvidence::Social(c),
                                EnforcementEvidenceRef::Social(id),
                            ) => c.id == *id,
                            _ => false,
                        }
                });
                let institution =
                    &crate::institution(self.port.store(), policy.institution).unwrap();
                if !duplicate
                    && policy.policy.topics.contains(&topic)
                    && self
                        .subjects(institution, &policy.policy.subjects)
                        .contains(&target)
                    && contact.is_none_or(|c| policy.policy.areas.contains(&c.area))
                {
                    actions.push(EnforcementAction::File {
                        institution: policy.institution,
                        office: policy.office,
                        evidence: *source,
                    });
                }
            }
        }
        for case in &view.cases {
            if case.target == actor
                && self
                    .port
                    .institution_view(actor)
                    .iter()
                    .any(|i| i.id == case.institution && i.charter.jurisdiction == area)
                && let Some(judgment) = &case.judgment
                && matches!(
                    case.status,
                    EnforcementStatus::Decided | EnforcementStatus::Refused
                )
            {
                actions.push(EnforcementAction::Respond {
                    case: case.id,
                    comply: false,
                });
                if matches!(judgment.remedy, EnforcementRemedy::Settlement { .. }) {
                    actions.push(EnforcementAction::Respond {
                        case: case.id,
                        comply: true,
                    });
                }
            }
            if case.mission.is_none()
                && case.contact.is_some()
                && matches!(
                    case.status,
                    EnforcementStatus::Decided | EnforcementStatus::Refused
                )
                && let Some(proposal) = self.default_dispatch_proposal(actor, case)
            {
                actions.push(EnforcementAction::Dispatch {
                    case: case.id,
                    proposal,
                });
            }
            if case.status != EnforcementStatus::Filing {
                for recipient in self
                    .local_enforcement_listeners(area)
                    .into_iter()
                    .filter(|p| *p != actor)
                    .take(4)
                {
                    actions.push(EnforcementAction::Report {
                        case: case.id,
                        recipient,
                    });
                }
            }
        }
        actions.into_iter().take(64).collect()
    }
    fn propose_known_judgments(&mut self) -> Result<(), Error> {
        let actors = self
            .person_index
            .keys()
            .copied()
            .filter(|p| *p != self.port.ruler())
            .collect::<Vec<_>>();
        for actor in actors {
            self.ensure_memory(actor);
            if self.port.past_life_controls_actor(actor) {
                continue;
            }
            let Some(area) = self.enforcement_area(actor) else {
                continue;
            };
            if self.port.in_combat(actor) {
                continue;
            }
            for case in self
                .enforcement_view(actor)
                .cases
                .into_iter()
                .filter(|c| c.status == EnforcementStatus::Pending)
                .take(8)
            {
                self.ensure_memory(actor);
                if self
                    .enforcement_office(case.institution, case.office)
                    .is_none_or(|o| o.holder != actor)
                {
                    continue;
                }
                let Some(institution) = self
                    .port
                    .institution_view(actor)
                    .into_iter()
                    .find(|i| i.id == case.institution)
                else {
                    continue;
                };
                if institution.charter.jurisdiction!=area || institution.proposals.iter().any(|p|p.status==RuleProposalStatus::Proposed && matches!(p.change,InstitutionChange::Adjudicate {case:id,..} if id==case.id)) {continue;}
                let Some(policy) = self
                    .enforcement_view(actor)
                    .policies
                    .into_iter()
                    .find(|p| p.institution == case.institution && p.office == case.office)
                    .map(|p| p.policy)
                else {
                    continue;
                };
                let mind = &self.port.mind(actor);
                let remedy = match &case.evidence {
                    EnforcementEvidence::Default(old)
                        if policy.remedies.contains(&RemedyKind::Settlement) =>
                    {
                        if let AgreementTerm::Delivery {
                            debtor,
                            creditor,
                            source,
                            destination,
                            ..
                        } = old.term
                        {
                            Some(EnforcementRemedy::Settlement {
                                terms: vec![AgreementTerm::Delivery {
                                    debtor,
                                    creditor,
                                    source,
                                    destination,
                                    quantity: Quantity(old.amount() - old.performed),
                                    deadline: SimTime(self.now.0.saturating_add(600_000)),
                                }],
                            })
                        } else {
                            None
                        }
                    }
                    EnforcementEvidence::Incident(incident)
                        if incident.outcome == IncidentOutcome::Taken
                            && policy.remedies.contains(&RemedyKind::Settlement)
                            && self.port.agreements_available().then_some(()).is_some_and(
                                |_| {
                                    game_property::inventory_custodian(
                                        self.port.store(),
                                        incident.source,
                                    ) == Some(case.filer)
                                },
                            ) =>
                    {
                        Some(EnforcementRemedy::Settlement {
                            terms: vec![AgreementTerm::Delivery {
                                debtor: case.target,
                                creditor: case.filer,
                                source: incident.destination,
                                destination: incident.source,
                                quantity: incident.quantity,
                                deadline: SimTime(self.now.0.saturating_add(600_000)),
                            }],
                        })
                    }
                    _ if policy.remedies.contains(&RemedyKind::Confinement)
                        && mind.personality.aggression >= 600
                        && mind.personality.empathy < 700 =>
                    {
                        Some(EnforcementRemedy::Confine {
                            until: SimTime(self.now.0.saturating_add(
                                (u128::from(policy.maximum_confinement.0)
                                    * u128::from(mind.personality.aggression)
                                    / 1_000) as u64,
                            )),
                        })
                    }
                    _ if policy.remedies.contains(&RemedyKind::Dismiss) => {
                        Some(EnforcementRemedy::Dismiss)
                    }
                    _ => None,
                };
                if let Some(remedy) = remedy {
                    let _ = self.institution_command(
                        actor,
                        &InstitutionAction::Propose {
                            institution: case.institution,
                            change: InstitutionChange::Adjudicate {
                                case: case.id,
                                remedy,
                            },
                        },
                    );
                }
            }
        }
        Ok(())
    }
    fn review_enforcement_support(&mut self) {
        let actors = self
            .person_index
            .keys()
            .copied()
            .filter(|p| *p != self.port.ruler())
            .collect::<Vec<_>>();
        for actor in actors {
            if self.port.past_life_controls_actor(actor) || self.enforcement_area(actor).is_none() {
                continue;
            }
            let unpaid = self
                .port
                .agreement_view(actor)
                .iter()
                .flat_map(|a| a.obligations.iter())
                .filter(|o| o.defaulted && o.term.parties()[1] == actor && o.performed < o.amount())
                .map(|o| o.term.parties()[0])
                .collect::<BTreeSet<_>>();
            if unpaid.is_empty() || self.port.mind(actor).personality.honesty < 500 {
                continue;
            }
            for institution in self.port.institution_view(actor) {
                for office in institution.offices {
                    if unpaid.contains(&office.mandate.holder) && office.supporters.contains(&actor)
                    {
                        let _ = self.institution_command(
                            actor,
                            &InstitutionAction::SupportOffice {
                                institution: institution.id,
                                office: office.mandate.slot,
                                support: false,
                            },
                        );
                    }
                }
            }
        }
    }
    pub fn validate_enforcement(&self) -> Result<(), Error> {
        let world = &self.state;
        let bad = || {
            Error::InvalidSave(
                "invalid enforcement procedures, evidence, work or personal knowledge".into(),
            )
        };
        self.port
            .institutions_available()
            .then_some(())
            .ok_or_else(bad)?;
        self.port.physical_rules().ok_or_else(bad)?;
        if world.last_event >= self.port.next_event() {
            return Err(bad());
        }
        let people = self.person_index.keys().copied().collect::<BTreeSet<_>>();
        let area = |id| self.port.areas().contains(&id);
        let evidence_valid = |e: &EnforcementEvidence| {
            match e {
            EnforcementEvidence::Incident(i)=>self.port.property_available().then_some(()).is_some_and(|_|game_property::incident(self.port.store(),i.id).is_some()) && people.contains(&i.actor) && game_cargo::inventory(self.port.store(), i.source).is_some() && game_cargo::inventory(self.port.store(), i.destination).is_some() && area(i.area) && i.started_at<=i.changed_at && i.changed_at<=i.learned_at && i.learned_at<=self.now && i.quantity.0>0,
            EnforcementEvidence::Assault(a)=>people.contains(&a.attacker) && people.contains(&a.target) && area(a.area) && a.observed_at<=self.now && self.port.events().iter().any(|e|EventId(e.sequence)==a.event && e.at==a.observed_at && matches!(e.event,Strike::Struck {actor,target,damage,..} if actor==a.attacker && target==a.target && damage==a.damage)),
            EnforcementEvidence::Default(o)=>game_agreements::obligation(self.port.store(),o.id).is_some() && o.defaulted && o.performed<o.amount() && o.observed_at<=o.learned_at && o.learned_at<=self.now && o.term.parties().iter().all(|p|people.contains(p)),
            EnforcementEvidence::Social(c)=>crate::case(self.port.store(),c.id)==Some(c),
        }
        };
        for (id, offices) in &world.policies {
            let institution = crate::institution(self.port.store(), id).ok_or_else(bad)?;
            for (office, policy) in offices {
                if policy.institution != *id
                    || policy.office != *office
                    || policy.adopted_at > self.now
                    || self
                        .validate_enforcement_policy(institution, *office, &policy.policy)
                        .is_err()
                {
                    return Err(bad());
                }
            }
        }
        for (id, case) in &world.cases {
            let institution =
                crate::institution(self.port.store(), case.institution).ok_or_else(bad)?;
            if *id != case.id
                || id.0 >= self.port.next_id()
                || !people.contains(&case.filer)
                || !people.contains(&case.target)
                || !evidence_valid(&case.evidence)
                || case.changed_at > self.now
                || case.notice_area.is_some_and(|a| !area(a))
                || !case.witnesses.is_subset(&people)
                || self
                    .validate_enforcement_policy(institution, case.office, &case.policy)
                    .is_err()
                || case.contact.as_ref().is_some_and(|c| {
                    c.person != case.target || !area(c.area) || c.observed_at > self.now
                })
            {
                return Err(bad());
            }
            let (topic, target) = match &case.evidence {
                EnforcementEvidence::Incident(i) => (EnforcementTopic::Taking, i.actor),
                EnforcementEvidence::Assault(a) => (EnforcementTopic::Assault, a.attacker),
                EnforcementEvidence::Default(o) => (EnforcementTopic::Default, o.term.parties()[0]),
                EnforcementEvidence::Social(c) => (EnforcementTopic::Exclusion, c.counterpart),
            };
            if topic != case.topic || target != case.target {
                return Err(bad());
            }
            if let Some(judgment) = &case.judgment {
                let proposal =
                    crate::proposal(self.port.store(), judgment.proposal).ok_or_else(bad)?;
                if judgment.id.0 >= self.port.next_id()
                    || judgment.adopted_at > self.now
                    || !people.contains(&judgment.proposer)
                    || proposal.status != RuleProposalStatus::Adopted
                    || proposal.proposer != judgment.proposer
                    || !matches!(&proposal.change,InstitutionChange::Adjudicate {case:id,remedy} if *id==case.id && remedy==&judgment.remedy)
                {
                    return Err(bad());
                }
            } else if !matches!(
                case.status,
                EnforcementStatus::Filing
                    | EnforcementStatus::Pending
                    | EnforcementStatus::Withdrawn
            ) {
                return Err(bad());
            }
            if let Some(id) = case.mission {
                let mission = self
                    .port
                    .missions_available()
                    .then_some(())
                    .and_then(|_| game_missions::mission(self.port.store(), id))
                    .ok_or_else(bad)?;
                if !matches!(case.judgment.as_ref().map(|j|&j.remedy),Some(EnforcementRemedy::Confine {until}) if mission.proposal.objective==MissionObjective::Confine {person:case.target,until:*until})
                {
                    return Err(bad());
                }
            }
            if case.settlement.is_some_and(|id| {
                self.port
                    .agreements_available()
                    .then_some(())
                    .is_none_or(|_| game_agreements::agreement(self.port.store(), id).is_none())
            }) {
                return Err(bad());
            }
        }
        for (actor, talk) in &world.talks {
            if !people.contains(actor)
                || !people.contains(&talk.recipient)
                || !world.cases.contains_key(&talk.case)
                || !area(talk.area)
                || talk.completes <= self.now
                || self.port.talk_activity(*actor)
                    != Some((talk.case, talk.recipient, talk.completes))
                || !self.port.completion_scheduled(*actor, talk.completes)
            {
                return Err(bad());
            }
            if let Conversation::Report {
                snapshot,
                institution,
            } = &talk.conversation
                && (snapshot.id != talk.case
                    || snapshot.observed_at > snapshot.learned_at
                    || snapshot.learned_at > self.now
                    || !evidence_valid(&snapshot.evidence)
                    || institution.id != snapshot.institution
                    || institution.observed_at > self.now)
            {
                return Err(bad());
            }
        }
        for (actor, known) in &world.known {
            if !people.contains(actor) {
                return Err(bad());
            }
            for (id, view) in known {
                let actual = world.cases.get(id).ok_or_else(bad)?;
                if *id != view.id
                    || view.institution != actual.institution
                    || view.office != actual.office
                    || view.filer != actual.filer
                    || view.target != actual.target
                    || view.topic != actual.topic
                    || view.evidence != actual.evidence
                    || view
                        .judgment
                        .as_ref()
                        .is_some_and(|j| actual.judgment.as_ref() != Some(j))
                    || view.mission.is_some_and(|id| actual.mission != Some(id))
                    || view
                        .settlement
                        .is_some_and(|id| actual.settlement != Some(id))
                    || match view.source {
                        KnowledgeSource::Observed(observer) => observer != *actor,
                        KnowledgeSource::Reported(speaker) => !people.contains(&speaker),
                        KnowledgeSource::Recorded(_) => true,
                    }
                    || !evidence_valid(&view.evidence)
                    || view.observed_at > view.learned_at
                    || view.learned_at > self.now
                    || view.contact.as_ref().is_some_and(|c| {
                        c.person != view.target || !area(c.area) || c.observed_at > self.now
                    })
                {
                    return Err(bad());
                }
            }
        }
        if world.contacts.iter().any(|(actor, known)| {
            !people.contains(actor)
                || known.iter().any(|(person, c)| {
                    *person != c.person
                        || !people.contains(person)
                        || !area(c.area)
                        || c.observed_at > self.now
                })
        }) || world.assaults.iter().any(|(actor, known)| {
            !people.contains(actor)
                || known.iter().any(|(id, a)| {
                    *id != a.event || !evidence_valid(&EnforcementEvidence::Assault(a.clone()))
                })
        }) || world.reconsider_after.keys().any(|p| !people.contains(p))
        {
            return Err(bad());
        }
        for (actor, known) in &world.known_policies {
            if !people.contains(actor) {
                return Err(bad());
            }
            for (id, offices) in known {
                if !crate::institution(self.port.store(), id).is_some()
                    || offices.iter().any(|(slot, p)| {
                        p.institution != *id
                            || p.office != *slot
                            || p.adopted_at > p.observed_at
                            || p.observed_at > self.now
                            || self
                                .validate_enforcement_policy(
                                    crate::institution(self.port.store(), id)
                                        .expect("validated Society reference"),
                                    *slot,
                                    &p.policy,
                                )
                                .is_err()
                    })
                {
                    return Err(bad());
                }
            }
        }
        Ok(())
    }
    pub fn enforcement_allocation_bound(&self) -> u64 {
        self.port.people().len() as u64 * 128
    }
    pub fn enforcement_vote_score(
        &self,
        actor: PersonId,
        _i: &institutions::Institution,
        change: &InstitutionChange,
    ) -> i64 {
        let mind = &self.port.mind(actor);
        match change {
            InstitutionChange::EnforcementPolicy {
                policy: Some(policy),
                ..
            } => {
                i64::from(mind.personality.honesty) - 500
                    + if policy.remedies.contains(&RemedyKind::Confinement) {
                        i64::from(mind.personality.aggression) - i64::from(mind.personality.empathy)
                    } else {
                        0
                    }
            }
            InstitutionChange::EnforcementPolicy { policy: None, .. } => {
                500 - i64::from(mind.personality.honesty)
            }
            InstitutionChange::Adjudicate { case, remedy } => {
                let Some(known) = self.state.known.get(&actor).and_then(|k| k.get(case)) else {
                    return -10_000;
                };
                let trust = if known.filer == actor {
                    1_000
                } else {
                    mind.relationships.get(&known.filer).map_or(0, |r| r.trust)
                };
                let confidence = match known.evidence {
                    EnforcementEvidence::Default(ref o) if o.defaulted => 500,
                    EnforcementEvidence::Incident(ref i) if i.outcome == IncidentOutcome::Taken => {
                        300
                    }
                    EnforcementEvidence::Assault(_) => 300,
                    _ => -200,
                } + i64::from(trust) / 2;
                match remedy {
                    EnforcementRemedy::Dismiss => -confidence,
                    EnforcementRemedy::Settlement { .. } => {
                        confidence + i64::from(mind.personality.honesty) / 2
                    }
                    EnforcementRemedy::Confine { .. } => {
                        confidence + i64::from(mind.personality.aggression)
                            - i64::from(mind.personality.empathy)
                    }
                    EnforcementRemedy::Depose { replacement, .. } => {
                        confidence
                            + i64::from(mind.relationships.get(replacement).map_or(0, |r| r.trust))
                            - i64::from(
                                mind.relationships.get(&known.target).map_or(0, |r| r.trust),
                            )
                    }
                }
            }
            _ => 0,
        }
    }
}

#[cfg(test)]
#[path = "enforcement_systems_tests.rs"]
mod tests;

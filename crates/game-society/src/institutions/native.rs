//! Native institutional commands, knowledge, and local review in one owner journal.
use super::*;
use crate::services::Services;
use game_ecs::{Component, Schema};
use game_world::{OwnerReadTxn, OwnerTxn, Registry, Request};

pub struct ExecuteInstitution {
    pub actor: PersonId,
    pub action: InstitutionAction,
}
impl Request for ExecuteInstitution {
    type Response = ();
}
/// An active Enforcement workflow applies its own reactions after this source
/// returns, before considering the next actor in the same review.
pub struct ExecuteInstitutionForEnforcement {
    pub actor: PersonId,
    pub action: InstitutionAction,
}
impl Request for ExecuteInstitutionForEnforcement {
    type Response = Vec<EnforcementReaction>;
}
pub struct EnforcementReaction {
    outcome: ReceiveEnforcementOutcome,
    institution: Institution,
    proposal: RuleProposal,
}
impl EnforcementReaction {
    pub fn outcome(&self) -> &ReceiveEnforcementOutcome {
        &self.outcome
    }
    /// Exact source postimages bind this reaction to the institutional effect.
    pub fn into_outcome(
        self,
        institution: &Institution,
        proposal: &RuleProposal,
    ) -> Result<ReceiveEnforcementOutcome, Error> {
        if institution != &self.institution || proposal != &self.proposal {
            return Err(Error::Unavailable);
        }
        Ok(self.outcome)
    }
}
pub struct ValidateInstitutionEvent(pub Event);
impl Request for ValidateInstitutionEvent {
    type Response = ();
}
pub struct ReadInstitution(pub InstitutionId);
impl Request for ReadInstitution {
    type Response = Option<Institution>;
}
pub struct ReadRuleProposal(pub RuleProposalId);
impl Request for ReadRuleProposal {
    type Response = Option<RuleProposal>;
}
pub fn read_proposal<E: 'static>(
    _: &Services,
    tx: &mut ReadTx<'_, '_, E>,
    request: ReadRuleProposal,
) -> Result<Option<RuleProposal>, Error> {
    Ok(tx.read::<RuleProposal>(StableId(request.0.0)).cloned())
}
pub struct ReadInstitutionMemory(pub PersonId);
impl Request for ReadInstitutionMemory {
    type Response = Option<crate::Knowledge>;
}
pub fn read_memory<E: 'static>(
    _: &Services,
    tx: &mut ReadTx<'_, '_, E>,
    request: ReadInstitutionMemory,
) -> Result<Option<crate::Knowledge>, Error> {
    Ok(tx.read::<crate::Knowledge>(StableId(request.0.0)).cloned())
}
pub struct ReadAuthority(pub InstitutionId);
impl Request for ReadAuthority {
    type Response = AuthorityFacts;
}
pub struct ReadInstitutionViews(pub PersonId);
impl Request for ReadInstitutionViews {
    type Response = Vec<InstitutionView>;
}
pub struct ReadSocialCaseViews(pub PersonId);
impl Request for ReadSocialCaseViews {
    type Response = Vec<SocialCaseView>;
}
pub struct ReadInstitutionsAvailable;
impl Request for ReadInstitutionsAvailable {
    type Response = bool;
}
pub struct ReadInstitutionSite(pub SiteId);
impl Request for ReadInstitutionSite {
    type Response = Option<AreaId>;
}
pub struct InstitutionAreaExists(pub AreaId);
impl Request for InstitutionAreaExists {
    type Response = bool;
}
pub struct ReadSocialPenalty(pub PersonId);
impl Request for ReadSocialPenalty {
    type Response = i64;
}
pub struct StartInstitutionWork {
    pub actor: PersonId,
    pub site: SiteId,
}
impl Request for StartInstitutionWork {
    type Response = ();
}
pub struct ReviewInstitutions;
impl Request for ReviewInstitutions {
    type Response = ();
}
pub struct ValidateInstitutions;
impl Request for ValidateInstitutions {
    type Response = ();
}
pub struct ReadInstitutionCommands(pub PersonId);
impl Request for ReadInstitutionCommands {
    type Response = Vec<InstitutionAction>;
}
pub struct ClearSocialLearning {
    pub actor: PersonId,
    pub values: bool,
    pub precedents: bool,
}
impl Request for ClearSocialLearning {
    type Response = ();
}
#[derive(Clone, Debug)]
pub struct ReceiveSocialReceipts(pub Vec<crate::reactions::SocialReceipt>);
impl Request for ReceiveSocialReceipts {
    type Response = ();
}
pub struct SubmitRegionalPassage {
    pub actor: PersonId,
    pub institution: InstitutionId,
    pub change: InstitutionChange,
}
impl Request for SubmitRegionalPassage {
    type Response = RuleProposalId;
}
pub struct ReceiveInstitutionMemory {
    pub actor: PersonId,
    pub before: Option<crate::Knowledge>,
    pub after: crate::Knowledge,
}
impl Request for ReceiveInstitutionMemory {
    type Response = ();
}

pub struct ValidateRegionalChange {
    pub actor: PersonId,
    pub institution: Institution,
    pub change: InstitutionChange,
    pub proposing: bool,
}
impl Request for ValidateRegionalChange {
    type Response = bool;
}
pub struct ReceiveRegionalOutcome {
    pub institution: InstitutionId,
    pub proposal: RuleProposalId,
    pub status: RuleProposalStatus,
    pub change: InstitutionChange,
    pub recipients: Vec<PersonId>,
    pub at: SimTime,
    pub sequence: u64,
}
impl Request for ReceiveRegionalOutcome {
    type Response = ();
}
pub struct ReceiveRegionalPublications {
    pub institutions: Vec<InstitutionId>,
    pub recipients: Vec<PersonId>,
    pub at: SimTime,
    pub sequence: u64,
    pub observe: bool,
}
impl Request for ReceiveRegionalPublications {
    type Response = ();
}
pub struct ValidateEnforcementChange {
    pub actor: PersonId,
    pub institution: Institution,
    pub change: InstitutionChange,
    pub proposing: bool,
}
impl Request for ValidateEnforcementChange {
    type Response = bool;
}
pub struct ReceiveEnforcementOutcome {
    pub institution: InstitutionId,
    pub proposal: RuleProposalId,
    pub status: RuleProposalStatus,
    pub change: InstitutionChange,
    pub at: SimTime,
    pub sequence: u64,
}
impl Request for ReceiveEnforcementOutcome {
    type Response = ();
}
pub struct ScoreEnforcementChange {
    pub actor: PersonId,
    pub institution: Institution,
    pub change: InstitutionChange,
}
impl Request for ScoreEnforcementChange {
    type Response = i64;
}
pub struct ScoreRegionalChange {
    pub actor: PersonId,
    pub change: InstitutionChange,
}
impl Request for ScoreRegionalChange {
    type Response = i64;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Proposed {
        institution: InstitutionId,
        proposer: PersonId,
    },
    Founded {
        institution: InstitutionId,
    },
    Left {
        institution: InstitutionId,
        actor: PersonId,
    },
    OfficeSupportChanged {
        institution: InstitutionId,
        office: u16,
        actor: PersonId,
        support: bool,
    },
    Changed {
        institution: InstitutionId,
        proposal: RuleProposalId,
    },
    SocialCaseRecorded {
        case: CaseId,
        actor: PersonId,
    },
    AccessApplied {
        institution: InstitutionId,
        index: u16,
        actor: PersonId,
    },
    DutyRecognized {
        institution: InstitutionId,
        index: u16,
        actor: PersonId,
        agreement: AgreementId,
    },
}
/// Structural source records retained in the application's public event envelope.
/// Source identity fields are decoded here; all factual resolution remains native.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryEvent {
    Ate {
        actor: PersonId,
        source: InventoryId,
    },
    Transferred {
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
    },
    Produced {
        actor: PersonId,
        site: SiteId,
        output: InventoryId,
    },
    Performed {
        obligation: ObligationId,
        actor: PersonId,
    },
    Defaulted {
        obligation: ObligationId,
    },
}

type Tx<'a, 's, E> = OwnerTxn<'a, 's, Services, E>;
type ReadTx<'a, 's, E> = OwnerReadTxn<'a, 's, Services, E>;

pub fn read_institution<E: 'static>(
    _: &Services,
    tx: &mut ReadTx<'_, '_, E>,
    request: ReadInstitution,
) -> Result<Option<Institution>, Error> {
    Ok(tx.read::<Institution>(StableId(request.0.0)).cloned())
}
pub fn read_authority<E: 'static>(
    _: &Services,
    tx: &mut ReadTx<'_, '_, E>,
    request: ReadAuthority,
) -> Result<AuthorityFacts, Error> {
    let people = tx
        .read::<Institution>(StableId(request.0.0))
        .map(crate::review::authority_people)
        .unwrap_or_default();
    let mut facts = AuthorityFacts::default();
    for actor in people {
        if tx
            .request(game_physiology::services::ReadBody(actor))?
            .is_some_and(|body| body.vitals.alive)
        {
            facts.alive.insert(actor);
        }
    }
    Ok(facts)
}
pub fn read_available<E: 'static>(
    _: &Services,
    tx: &mut ReadTx<'_, '_, E>,
    _: ReadInstitutionsAvailable,
) -> Result<bool, Error> {
    Ok(tx.runtime().is_some())
}
pub fn read_views<E: 'static>(
    _: &Services,
    tx: &mut ReadTx<'_, '_, E>,
    request: ReadInstitutionViews,
) -> Result<Vec<InstitutionView>, Error> {
    if tx.runtime().is_none() {
        return Ok(Vec::new());
    }
    Ok(tx
        .read::<crate::Knowledge>(StableId(request.0.0))
        .map(|memory| {
            memory
                .institutions
                .values()
                .rev()
                .take(64)
                .cloned()
                .collect()
        })
        .unwrap_or_default())
}
pub fn read_cases<E: 'static>(
    _: &Services,
    tx: &mut ReadTx<'_, '_, E>,
    request: ReadSocialCaseViews,
) -> Result<Vec<SocialCaseView>, Error> {
    if tx.runtime().is_none() {
        return Ok(Vec::new());
    }
    let ids = tx
        .read::<crate::Knowledge>(StableId(request.0.0))
        .map(|memory| memory.cases.clone())
        .unwrap_or_default();
    Ok(ids
        .into_iter()
        .rev()
        .take(64)
        .filter_map(|id| tx.read::<SocialCaseView>(StableId(id.0)).cloned())
        .collect())
}
pub fn receive_memory<E: 'static>(
    _: &Services,
    tx: &mut Tx<'_, '_, E>,
    request: ReceiveInstitutionMemory,
) -> Result<(), Error> {
    let id = StableId(request.actor.0);
    if tx.read::<crate::Knowledge>(id) != request.before.as_ref() {
        return Err(Error::Unavailable);
    }
    tx.write(id, request.after)
}

fn schema() -> Result<Schema, Error> {
    let mut schema = Schema::new();
    crate::register_components(&mut schema)?;
    game_agreements::register_components(&mut schema)?;
    game_property::register_components(&mut schema)?;
    Ok(schema)
}
fn insert_rows<C: Component>(store: &mut Store, rows: Vec<(StableId, C)>) -> Result<(), Error> {
    for (id, row) in rows {
        store.insert_or_spawn(id, row)?;
    }
    Ok(())
}
/// This projection contains institutional rows only. Foreign rows required by
/// existing pure kernels are fetched through factual contracts and never staged.
fn projection<E: 'static>(tx: &mut Tx<'_, '_, E>) -> Result<Store, Error> {
    let mut store = Store::with_schema(schema()?);
    insert_rows(&mut store, tx.iter::<Institution>())?;
    insert_rows(&mut store, tx.iter::<RuleProposal>())?;
    insert_rows(&mut store, tx.iter::<crate::Concept>())?;
    insert_rows(&mut store, tx.iter::<SocialCaseView>())?;
    insert_rows(&mut store, tx.iter::<crate::Knowledge>())?;
    insert_rows(&mut store, tx.iter::<crate::Requests>())?;
    insert_rows(&mut store, tx.iter::<crate::Exclusions>())?;
    insert_rows(&mut store, tx.iter::<crate::Hearing>())?;
    Ok(store)
}
fn read_projection<E: 'static>(tx: &mut ReadTx<'_, '_, E>) -> Result<Store, Error> {
    let mut store = Store::with_schema(schema()?);
    insert_rows(&mut store, tx.iter::<Institution>())?;
    insert_rows(&mut store, tx.iter::<RuleProposal>())?;
    insert_rows(&mut store, tx.iter::<crate::Concept>())?;
    insert_rows(&mut store, tx.iter::<SocialCaseView>())?;
    insert_rows(&mut store, tx.iter::<crate::Knowledge>())?;
    insert_rows(&mut store, tx.iter::<crate::Requests>())?;
    insert_rows(&mut store, tx.iter::<crate::Exclusions>())?;
    insert_rows(&mut store, tx.iter::<crate::Hearing>())?;
    Ok(store)
}
fn stage_column<C: Component, E: 'static>(
    tx: &mut Tx<'_, '_, E>,
    before: &Store,
    after: &Store,
) -> Result<(), Error> {
    let ids: BTreeSet<_> = before
        .iter::<C>()
        .chain(after.iter::<C>())
        .map(|(id, _)| id)
        .collect();
    for id in ids {
        if before.get::<C>(id) == after.get::<C>(id) {
            continue;
        }
        match after.get::<C>(id) {
            Some(row) => tx.write(id, row.clone())?,
            None => tx.remove::<C>(id)?,
        }
    }
    Ok(())
}
fn stage<E: 'static>(tx: &mut Tx<'_, '_, E>, before: &Store, after: &Store) -> Result<(), Error> {
    stage_column::<Institution, E>(tx, before, after)?;
    stage_column::<RuleProposal, E>(tx, before, after)?;
    stage_column::<crate::Concept, E>(tx, before, after)?;
    stage_column::<SocialCaseView, E>(tx, before, after)?;
    stage_column::<crate::Knowledge, E>(tx, before, after)?;
    stage_column::<crate::Requests, E>(tx, before, after)?;
    stage_column::<crate::Exclusions, E>(tx, before, after)?;
    stage_column::<crate::Hearing, E>(tx, before, after)
}
fn social_area<E: 'static>(
    tx: &mut Tx<'_, '_, E>,
    actor: PersonId,
) -> Result<Option<AreaId>, Error> {
    use game_characters::services::{ReadPhysical, ReadPlacement, ReadStage};
    let physical = tx.request(ReadPhysical(actor))?;
    let place = tx.request(ReadPlacement(actor))?;
    let life = tx.request(ReadStage(actor))?;
    let combat = tx.request(game_conflict::services::ReadCombatStatus(actor))?;
    Ok(crate::review::social_area(crate::review::SocialAreaFacts {
        area: place.and_then(|place| match place.location {
            Location::AtArea(area) => Some(area),
            _ => None,
        }),
        independent: life.independent,
        combat_restricted: combat.restricted,
        alive: physical.is_some_and(|body| body.vitals.alive),
    })
    .ok())
}
fn references<E: 'static>(
    tx: &mut Tx<'_, '_, E>,
    request: &ReferenceRequest,
) -> Result<ReferenceFacts, Error> {
    let mut facts = ReferenceFacts::default();
    for actor in &request.people {
        if tx
            .request(game_characters::services::ReadPerson(*actor))?
            .is_some()
        {
            facts.people.insert(*actor);
            if let Some(area) = social_area(tx, *actor)? {
                facts.eligible_locations.insert(*actor, area);
            }
        }
    }
    for inventory in &request.stocks {
        if let Some(stock) = tx.request(game_cargo::facts::ReadInventory(*inventory))? {
            let area = tx.request(game_cargo::facts::ReadInventoryArea {
                inventory: *inventory,
            })?;
            let custodian = tx.request(game_property::services::ReadCustodian(*inventory))?;
            facts.stocks.insert(
                *inventory,
                StockReference {
                    area,
                    custodian,
                    good: stock.good,
                    capacity: stock.capacity,
                },
            );
        }
    }
    for site in &request.sites {
        if let Some(area) = tx.request(ReadInstitutionSite(*site))? {
            facts.sites.insert(*site, area);
        }
    }
    Ok(facts)
}
fn authority<E: 'static>(
    tx: &mut Tx<'_, '_, E>,
    people: BTreeSet<PersonId>,
) -> Result<AuthorityFacts, Error> {
    let mut facts = AuthorityFacts::default();
    for actor in people {
        if tx
            .request(game_physiology::services::ReadBody(actor))?
            .is_some_and(|body| body.vitals.alive)
        {
            facts.alive.insert(actor);
        }
    }
    Ok(facts)
}
fn roster<C: Component, E: 'static>(
    tx: &mut Tx<'_, '_, E>,
) -> Result<Vec<crate::perception::ObserverFacts>, Error> {
    let mut actors = Vec::new();
    for actor in tx.request(game_characters::services::ReadPeople)? {
        let body = tx.request(game_physiology::services::ReadBody(actor))?;
        let life = tx.request(game_characters::services::ReadStage(actor))?;
        let place = tx.request(game_characters::services::ReadPlacement(actor))?;
        let combat = tx.request(game_conflict::services::ReadCombatStatus(actor))?;
        let mind = tx.request(game_characters::services::ReadMind::<C>(
            actor,
            std::marker::PhantomData,
        ))?;
        actors.push(crate::perception::ObserverFacts {
            actor,
            area: place.and_then(|place| match place.location {
                Location::AtArea(area) => Some(area),
                _ => None,
            }),
            alive: body.is_some_and(|body| body.vitals.alive),
            independent: life.independent,
            conversation_allowed: !combat.restricted,
            empathy: mind.map_or(0, |mind| mind.personality.empathy),
        });
    }
    Ok(actors)
}
fn external(change: &InstitutionChange) -> bool {
    matches!(
        change,
        InstitutionChange::RatifyRegional { .. }
            | InstitutionChange::AuthorizeRegionalPassage { .. }
            | InstitutionChange::EnforcementPolicy { .. }
            | InstitutionChange::Adjudicate { .. }
    )
}
fn regional(change: &InstitutionChange) -> bool {
    matches!(
        change,
        InstitutionChange::RatifyRegional { .. }
            | InstitutionChange::AuthorizeRegionalPassage { .. }
    )
}
fn permit<E: 'static>(
    tx: &mut Tx<'_, '_, E>,
    store: &Store,
    actor: PersonId,
    action: &InstitutionAction,
) -> Result<Option<ExternalChangePermit>, Error> {
    let (institution, change, proposing) = match action {
        InstitutionAction::Propose {
            institution,
            change,
        } => (
            crate::institution(store, *institution).ok_or(Error::Unavailable)?,
            change,
            true,
        ),
        InstitutionAction::Vote { proposal, .. } => {
            let proposal = crate::proposal(store, *proposal).ok_or(Error::Unavailable)?;
            (
                crate::institution(store, proposal.institution).ok_or(Error::Unavailable)?,
                &proposal.change,
                false,
            )
        }
        _ => return Ok(None),
    };
    if !external(change) {
        return Ok(None);
    }
    let allowed = if regional(change) {
        tx.request(ValidateRegionalChange {
            actor,
            institution: institution.clone(),
            change: change.clone(),
            proposing,
        })?
    } else {
        tx.request(ValidateEnforcementChange {
            actor,
            institution: institution.clone(),
            change: change.clone(),
            proposing,
        })?
    };
    if !allowed {
        return Err(Error::Unavailable);
    }
    Ok(Some(ExternalChangePermit {
        actor,
        institution: institution.id,
        institution_revision: institution.revision,
        change: change.clone(),
    }))
}

pub fn execute<C: Component, E: From<Event> + 'static>(
    owner: &Services,
    tx: &mut Tx<'_, '_, E>,
    request: ExecuteInstitution,
) -> Result<(), Error> {
    execute_action::<C, E>(tx, request.actor, &request.action)?;
    reconcile_history::<C, E>(owner, tx)?;
    tx.revise(1)
}
fn execute_action<C: Component, E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    actor: PersonId,
    action: &InstitutionAction,
) -> Result<(), Error> {
    execute_action_with_reactions::<C, E>(tx, actor, action, false).map(|_| ())
}
pub fn execute_for_enforcement<C: Component, E: From<Event> + 'static>(
    owner: &Services,
    tx: &mut Tx<'_, '_, E>,
    request: ExecuteInstitutionForEnforcement,
) -> Result<Vec<EnforcementReaction>, Error> {
    let mut reactions =
        execute_action_with_reactions::<C, E>(tx, request.actor, &request.action, true)?;
    reconcile_history::<C, E>(owner, tx)?;
    for reaction in &mut reactions {
        reaction.institution = tx
            .read::<Institution>(StableId(reaction.outcome.institution.0))
            .cloned()
            .ok_or(Error::Unavailable)?;
        reaction.proposal = tx
            .read::<RuleProposal>(StableId(reaction.outcome.proposal.0))
            .cloned()
            .ok_or(Error::Unavailable)?;
    }
    tx.revise(1)?;
    Ok(reactions)
}
fn execute_action_with_reactions<C: Component, E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    actor: PersonId,
    action: &InstitutionAction,
    collect_enforcement: bool,
) -> Result<Vec<EnforcementReaction>, Error> {
    let runtime = tx.runtime().clone().ok_or(Error::Unavailable)?;
    let area = social_area(tx, actor)?.ok_or(Error::Unavailable)?;
    let mut store = projection(tx)?;
    runtime.preflight_rule_deadlines(&store, tx.now())?;
    let before = store.clone();
    let observers = roster::<C, E>(tx)?;
    let index = crate::perception::ObservationIndex::new(&observers)?;
    let people = crate::institutions(&store)
        .flat_map(|(_, row)| crate::review::authority_people(row))
        .collect();
    let all_authority = authority(tx, people)?;
    if matches!(
        action,
        InstitutionAction::HearCase { .. } | InstitutionAction::TellCase { .. }
    ) {
        let recipient_area = if let InstitutionAction::TellCase { recipient, .. } = action {
            social_area(tx, *recipient)?
        } else {
            None
        };
        let refusal = runtime.case_command_record(&store, action, tx.now());
        let refusal_observers = refusal
            .as_ref()
            .map(|case| index.case_observers(case, false))
            .transpose()?
            .unwrap_or_default();
        let refusal_id = refusal.as_ref().map(|_| CaseId(tx.next_id()));
        let plan = runtime.prepare_case_command(
            &store,
            action,
            crate::case_commands::CaseCommandFacts {
                actor,
                area,
                now: tx.now(),
                recipient_area,
                authority: &all_authority,
                refusal: refusal_id,
                refusal_observers: &refusal_observers,
            },
        )?;
        let publications =
            plan.prepare_publications(&runtime, &store, &all_authority, tx.now(), &index)?;
        let institutions = publications.iter().map(|p| p.institution()).collect();
        let recipients = publications
            .iter()
            .flat_map(|p| p.recipients())
            .collect::<BTreeSet<_>>();
        let mut batch = Batch::new();
        let publication =
            plan.stage_with_publications(&runtime, &store, tx.now(), publications, &mut batch)?;
        publication.validate(&runtime, &store, tx.now(), &batch)?;
        let recorded = publication.effects.recorded;
        store.apply_batch(batch)?;
        if refusal_id.is_some() {
            tx.reserve_ids(1)?;
        }
        stage(tx, &before, &store)?;
        if let Some((case, actor)) = recorded {
            tx.emit(Event::SocialCaseRecorded { case, actor }.into())?;
        }
        tx.defer(ReceiveRegionalPublications {
            institutions,
            recipients: recipients.into_iter().collect(),
            at: tx.now(),
            sequence: tx.next_event() - 1,
            observe: false,
        })?;
        return Ok(Vec::new());
    }
    let needed = runtime.command_references(&store, actor, action)?;
    let facts = references(tx, &needed)?;
    let command_authority = authority(tx, needed.people.clone())?;
    let permit = permit(tx, &store, actor, action)?;
    let plan = runtime.prepare_command(
        &store,
        action,
        &CommandFacts {
            actor,
            area,
            now: tx.now(),
            references: &facts,
            authority: &command_authority,
            external_permit: permit.as_ref(),
        },
        CommandIds {
            first: tx.next_id(),
        },
    )?;
    let count = plan
        .next_id
        .checked_sub(tx.next_id())
        .ok_or(Error::Overflow)?;
    let effects = plan.effects.clone();
    let publication_people = effects
        .iter()
        .filter_map(|effect| match effect {
            CommandEffect::Publish { institution } => Some(*institution),
            _ => None,
        })
        .flat_map(|id| {
            plan.publication_authority_people(&store, id)
                .unwrap_or_default()
        })
        .collect();
    let publication_authority = authority(tx, publication_people)?;
    let publications =
        plan.prepare_publications(&runtime, &store, &publication_authority, tx.now(), &index)?;
    let institutions = publications.iter().map(|p| p.institution()).collect();
    let recipients = publications
        .iter()
        .flat_map(|p| p.recipients())
        .collect::<BTreeSet<_>>();
    let observe = effects
        .iter()
        .any(|effect| matches!(effect, CommandEffect::ObserveRegional));
    let mut batch = Batch::new();
    let guard = plan.stage_with_publications(&runtime, &store, publications, &mut batch)?;
    guard.validate(&batch)?;
    store.apply_batch(batch)?;
    tx.reserve_ids(count)?;
    stage(tx, &before, &store)?;
    let mut reactions = Vec::new();
    for effect in effects {
        let event = match effect {
            CommandEffect::Funding {
                institution,
                agreement,
                obligation,
                term,
            } => {
                tx.request(game_agreements::native::CreateInstitutionDuty(
                    game_agreements::InstitutionDuty {
                        institution,
                        agreement,
                        obligation,
                        term,
                    },
                ))?;
                None
            }
            CommandEffect::Proposed {
                institution,
                proposer,
            } => Some(Event::Proposed {
                institution,
                proposer,
            }),
            CommandEffect::Founded { institution } => Some(Event::Founded { institution }),
            CommandEffect::Left { institution, actor } => Some(Event::Left { institution, actor }),
            CommandEffect::OfficeSupportChanged {
                institution,
                office,
                actor,
                support,
            } => Some(Event::OfficeSupportChanged {
                institution,
                office,
                actor,
                support,
            }),
            CommandEffect::Changed {
                institution,
                proposal,
            } => Some(Event::Changed {
                institution,
                proposal,
            }),
            CommandEffect::ExternalOutcome {
                institution,
                proposal,
                outcome,
                change,
            } => {
                if regional(&change) {
                    tx.defer(ReceiveRegionalOutcome {
                        institution,
                        proposal,
                        status: outcome,
                        change,
                        recipients: observers.iter().map(|row| row.actor).collect(),
                        at: tx.now(),
                        sequence: tx.next_event() - 1,
                    })?;
                } else {
                    let outcome = ReceiveEnforcementOutcome {
                        institution,
                        proposal,
                        status: outcome,
                        change,
                        at: tx.now(),
                        sequence: tx.next_event() - 1,
                    };
                    if collect_enforcement {
                        reactions.push(EnforcementReaction {
                            institution: crate::institution(&store, institution)
                                .cloned()
                                .ok_or(Error::Unavailable)?,
                            proposal: crate::proposal(&store, proposal)
                                .cloned()
                                .ok_or(Error::Unavailable)?,
                            outcome,
                        });
                    } else {
                        tx.defer(outcome)?;
                    }
                }
                None
            }
            CommandEffect::Publish { .. } | CommandEffect::ObserveRegional => None,
        };
        if let Some(event) = event {
            tx.emit(event.into())?;
        }
    }
    tx.defer(ReceiveRegionalPublications {
        institutions,
        recipients: recipients.into_iter().collect(),
        at: tx.now(),
        sequence: tx.next_event() - 1,
        observe,
    })?;
    Ok(reactions)
}

fn publish_case<C: Component, E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    reaction: crate::reactions::CaseReaction,
    observers: &crate::perception::ObservationIndex<'_>,
) -> Result<(), Error> {
    let runtime = tx.runtime().clone().ok_or(Error::Unavailable)?;
    let mut store = projection(tx)?;
    if let Some(inventory) = reaction.case.inventory {
        if let Some(custodian) = tx.request(game_property::services::ReadCustodian(inventory))? {
            store.insert_or_spawn(
                StableId(inventory.0),
                game_property::InventoryCustody { custodian },
            )?;
        }
    }
    let before = store.clone();
    let witnesses = observers.case_observers(&reaction.case, reaction.public)?;
    let id = CaseId(tx.next_id());
    let plan = runtime.prepare_case_reaction(&store, reaction, id, &witnesses)?;
    let mut batch = Batch::new();
    let publication = plan.stage_into(&runtime, &store, tx.now(), &mut batch)?;
    publication.validate(&runtime, &store, tx.now(), &batch)?;
    let actor = publication.actor;
    store.apply_batch(batch)?;
    tx.reserve_ids(1)?;
    stage(tx, &before, &store)?;
    tx.emit(Event::SocialCaseRecorded { case: id, actor }.into())?;
    Ok(())
}
pub fn receive_receipts<C: Component, E: From<Event> + 'static>(
    _: &Services,
    tx: &mut Tx<'_, '_, E>,
    request: ReceiveSocialReceipts,
) -> Result<(), Error> {
    if tx.runtime().is_none() {
        return Ok(());
    }
    for actor in tx.request(game_characters::services::ReadPeople)? {
        let id = StableId(actor.0);
        if tx.read::<crate::Knowledge>(id).is_none() {
            tx.write(id, crate::Knowledge::default())?;
        }
    }
    let through = tx.next_event() - 1;
    let actors = roster::<C, E>(tx)?;
    let witnesses = crate::perception::ObservationIndex::new(&actors)?;
    let index = crate::reactions::ReceiptIndex::new(&request.0);
    for receipt in request.0 {
        let actor = crate::review::receipt_actor(&receipt);
        let body = tx
            .request(game_physiology::services::ReadBody(actor))?
            .ok_or(Error::Unavailable)?;
        let place = tx.request(game_characters::services::ReadPlacement(actor))?;
        let subject = crate::reactions::SubjectFacts {
            actor,
            hunger: body.vitals.hunger,
            area: place.and_then(|place| match place.location {
                Location::AtArea(area) => Some(area),
                _ => None,
            }),
        };
        let store = projection(tx)?;
        let now = tx.now();
        let reaction = tx
            .runtime()
            .as_ref()
            .unwrap()
            .receipt_reaction(&store, &receipt, subject, now, &index);
        if let Some(reaction) = reaction {
            publish_case::<C, E>(tx, reaction, &witnesses)?;
        }
    }
    tx.runtime_mut().as_mut().unwrap().last_event = through;
    Ok(())
}
fn apply_rules<C: Component, E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
) -> Result<(), Error> {
    let runtime = tx.runtime().clone().ok_or(Error::Unavailable)?;
    let store = projection(tx)?;
    let holders = crate::review::office_holders(&store, true);
    let mut spoken = Vec::new();
    for actor in holders {
        let area = social_area(tx, actor)?;
        let body = tx
            .request(game_physiology::services::ReadBody(actor))?
            .ok_or(Error::Unavailable)?;
        let known = tx
            .request(game_characters::services::ReadKnownStocks(actor))?
            .into_iter()
            .map(|row| (row.inventory, row))
            .collect();
        spoken.push((actor, area, body.vitals.hunger, known));
    }
    let speakers = spoken
        .iter()
        .map(
            |(actor, area, hunger, stocks)| crate::deliberation::RuleSpeaker {
                actor: *actor,
                area: *area,
                hunger: *hunger,
                known_stocks: stocks,
            },
        )
        .collect::<Vec<_>>();
    let mut locations = BTreeMap::new();
    for actor in tx.request(game_characters::services::ReadPeople)? {
        if let Some(area) = social_area(tx, actor)? {
            locations.insert(actor, area);
        }
    }
    let all_authority = authority(
        tx,
        crate::institutions(&store)
            .flat_map(|(_, row)| crate::review::authority_people(row))
            .collect(),
    )?;
    let applications =
        runtime.rule_applications(&store, &all_authority, &speakers, &locations, tx.now())?;
    let actors = roster::<C, E>(tx)?;
    let observations = crate::perception::ObservationIndex::new(&actors)?;
    for application in applications {
        let mut store = projection(tx)?;
        let before = store.clone();
        let plan =
            runtime.prepare_rule_application(&store, &application, tx.next_id(), tx.now())?;
        let effects = plan.effects.clone();
        let duties = plan.duties().to_vec();
        let count = plan
            .next_id
            .checked_sub(tx.next_id())
            .ok_or(Error::Overflow)?;
        let institution = plan.institution().id;
        let active_authority = authority(tx, crate::review::authority_people(plan.institution()))?;
        let publications = plan.prepare_publications(
            &runtime,
            &store,
            &active_authority,
            tx.now(),
            &observations,
        )?;
        let recipients = publications
            .iter()
            .flat_map(|p| p.recipients())
            .collect::<BTreeSet<_>>();
        let mut batch = Batch::new();
        let publication =
            plan.stage_with_publications(&runtime, &store, tx.now(), publications, &mut batch)?;
        publication.validate(&runtime, &store, tx.now(), &batch)?;
        store.apply_batch(batch)?;
        tx.reserve_ids(count)?;
        stage(tx, &before, &store)?;
        for effect in effects {
            let event = match effect {
                crate::rule_publication::RuleApplicationEffect::Access {
                    institution,
                    index,
                    actor,
                } => Event::AccessApplied {
                    institution,
                    index,
                    actor,
                },
                crate::rule_publication::RuleApplicationEffect::Duty {
                    institution,
                    index,
                    actor,
                    agreement,
                } => {
                    let duty = duties
                        .iter()
                        .find(|duty| duty.agreement == agreement)
                        .ok_or(Error::Unavailable)?
                        .clone();
                    tx.request(game_agreements::native::CreateInstitutionDuty(duty))?;
                    Event::DutyRecognized {
                        institution,
                        index,
                        actor,
                        agreement,
                    }
                }
            };
            tx.emit(event.into())?;
        }
        tx.defer(ReceiveRegionalPublications {
            institutions: vec![institution],
            recipients: recipients.into_iter().collect(),
            at: tx.now(),
            sequence: tx.next_event() - 1,
            observe: false,
        })?;
    }
    Ok(())
}

struct VoterKnowledge {
    stocks: BTreeMap<InventoryId, StockKnowledge>,
    custodians: BTreeMap<InventoryId, PersonId>,
    relationships: BTreeMap<PersonId, game_characters::decisions::Relationship>,
}
impl crate::voting::VotingKnowledge for VoterKnowledge {
    fn stock(&self, id: InventoryId) -> Option<&StockKnowledge> {
        self.stocks.get(&id)
    }
    fn custodian(&self, id: InventoryId) -> Option<PersonId> {
        self.custodians.get(&id).copied()
    }
    fn trust(&self, id: PersonId) -> i32 {
        self.relationships.get(&id).map_or(0, |row| row.trust)
    }
}
struct Voter {
    actor: PersonId,
    area: Option<AreaId>,
    autonomous: bool,
    citizen: bool,
    empathy: u16,
    discipline: u16,
    supports_others: bool,
    sites: Vec<crate::deliberation::LivelihoodSite>,
    knowledge: VoterKnowledge,
}
impl Voter {
    fn facts(&self) -> crate::voting::VoterFacts<'_> {
        crate::voting::VoterFacts {
            actor: self.actor,
            local_area: self.area,
            autonomous: self.autonomous,
            empathy: self.empathy,
            supports_others: self.supports_others,
            knowledge: &self.knowledge,
        }
    }
}
fn voters<C: Component, E: 'static>(tx: &mut Tx<'_, '_, E>) -> Result<Vec<Voter>, Error> {
    let mut voters = Vec::new();
    let mut policy = ReferenceRequest::default();
    for (_, row) in tx.iter::<Institution>() {
        policy.charter(&row.charter);
    }
    for (_, row) in tx.iter::<RuleProposal>() {
        policy.change(&row.change);
    }
    for actor in tx.request(game_characters::services::ReadPeople)? {
        let person = tx
            .request(game_characters::services::ReadPerson(actor))?
            .ok_or(Error::Unavailable)?;
        let Some(mind) = tx.request(game_characters::services::ReadMind::<C>(
            actor,
            std::marker::PhantomData,
        ))?
        else {
            continue;
        };
        let stocks: BTreeMap<_, _> = tx
            .request(game_characters::services::ReadKnownStocks(actor))?
            .into_iter()
            .map(|row| (row.inventory, row))
            .collect();
        let mut custodians = BTreeMap::new();
        for inventory in stocks
            .keys()
            .copied()
            .chain(policy.stocks.iter().copied())
            .collect::<BTreeSet<_>>()
        {
            if let Some(actor) = tx.request(game_property::services::ReadCustodian(inventory))? {
                custodians.insert(inventory, actor);
            }
        }
        voters.push(Voter {
            actor,
            area: social_area(tx, actor)?,
            autonomous: person.role != Role::Ruler,
            citizen: person.role == Role::Citizen,
            empathy: mind.personality.empathy,
            discipline: mind.personality.discipline,
            supports_others: mind.dream == game_characters::decisions::Aspiration::SupportOthers,
            sites: mind
                .sites
                .values()
                .map(|site| crate::deliberation::LivelihoodSite {
                    site: site.site,
                    area: site.area,
                    accessible: site.access,
                })
                .collect(),
            knowledge: VoterKnowledge {
                stocks,
                custodians,
                relationships: mind.relationships,
            },
        });
    }
    Ok(voters)
}

fn execute_review_action<C: Component, E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    actor: PersonId,
    action: &InstitutionAction,
) -> Result<bool, Error> {
    // Candidate admission is checked before IDs, canonical writes or notifications.
    // Once an admitted candidate starts receiving effects, its failure aborts the journal.
    let runtime = tx.runtime().clone().ok_or(Error::Unavailable)?;
    let Some(area) = social_area(tx, actor)? else {
        return Ok(false);
    };
    let store = projection(tx)?;
    if !matches!(
        action,
        InstitutionAction::HearCase { .. } | InstitutionAction::TellCase { .. }
    ) {
        let needed = match runtime.command_references(&store, actor, action) {
            Ok(needed) => needed,
            Err(Error::Unavailable) => return Ok(false),
            Err(error) => return Err(error),
        };
        let references = references(tx, &needed)?;
        let authority = authority(tx, needed.people)?;
        let permit = match permit(tx, &store, actor, action) {
            Ok(permit) => permit,
            Err(Error::Unavailable) => return Ok(false),
            Err(error) => return Err(error),
        };
        match runtime.prepare_command(
            &store,
            action,
            &CommandFacts {
                actor,
                area,
                now: tx.now(),
                references: &references,
                authority: &authority,
                external_permit: permit.as_ref(),
            },
            CommandIds {
                first: tx.next_id(),
            },
        ) {
            Ok(_) => {}
            Err(Error::Unavailable) => return Ok(false),
            Err(error) => return Err(error),
        }
    } else {
        let actors = roster::<C, E>(tx)?;
        let index = crate::perception::ObservationIndex::new(&actors)?;
        let authority = authority(
            tx,
            crate::institutions(&store)
                .flat_map(|(_, row)| crate::review::authority_people(row))
                .collect(),
        )?;
        let recipient_area = if let InstitutionAction::TellCase { recipient, .. } = action {
            social_area(tx, *recipient)?
        } else {
            None
        };
        let refusal = runtime.case_command_record(&store, action, tx.now());
        let refusal_observers = refusal
            .as_ref()
            .map(|case| index.case_observers(case, false))
            .transpose()?
            .unwrap_or_default();
        match runtime.prepare_case_command(
            &store,
            action,
            crate::case_commands::CaseCommandFacts {
                actor,
                area,
                now: tx.now(),
                recipient_area,
                authority: &authority,
                refusal: refusal.map(|_| CaseId(tx.next_id())),
                refusal_observers: &refusal_observers,
            },
        ) {
            Ok(_) => {}
            Err(Error::Unavailable) => return Ok(false),
            Err(error) => return Err(error),
        }
    }
    execute_action::<C, E>(tx, actor, action)?;
    Ok(true)
}

pub fn review<C: Component, E: From<Event> + 'static>(
    owner: &Services,
    tx: &mut Tx<'_, '_, E>,
    _: ReviewInstitutions,
) -> Result<(), Error> {
    reconcile_history::<C, E>(owner, tx)?;
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(());
    };
    for actor in tx.request(game_characters::services::ReadPeople)? {
        let id = StableId(actor.0);
        if tx.read::<crate::Knowledge>(id).is_none() {
            tx.write(id, crate::Knowledge::default())?;
        }
    }
    let actors = voters::<C, E>(tx)?;
    let observer_roster = roster::<C, E>(tx)?;
    let observation = crate::perception::ObservationIndex::new(&observer_roster)?;
    let store = projection(tx)?;
    let mut exclusions = Vec::new();
    for actor in &actors {
        let Some(body) = tx.request(game_physiology::services::ReadBody(actor.actor))? else {
            continue;
        };
        let area = tx
            .request(game_characters::services::ReadPlacement(actor.actor))?
            .and_then(|place| match place.location {
                Location::AtArea(area) => Some(area),
                _ => None,
            });
        exclusions.push(crate::review::ExclusionFacts {
            subject: crate::reactions::SubjectFacts {
                actor: actor.actor,
                area,
                hunger: body.vitals.hunger,
            },
            available_area: actor.area,
            known_stocks: &actor.knowledge.stocks,
        });
    }
    for reaction in runtime.observed_exclusions(&store, &exclusions, tx.now()) {
        publish_case::<C, E>(tx, reaction, &observation)?;
    }
    let store = projection(tx)?;
    let founder_ids = crate::review::formation_voters(&store);
    let founders = actors
        .iter()
        .filter(|actor| founder_ids.contains(&actor.actor))
        .map(Voter::facts)
        .collect::<Vec<_>>();
    let voters = actors.iter().map(Voter::facts).collect::<Vec<_>>();
    let active_authority = authority(
        tx,
        crate::institutions(&store)
            .flat_map(|(_, row)| crate::review::authority_people(row))
            .collect(),
    )?;
    let mut replies = runtime.formation_replies(&store, &founders)?;
    let mut proposers = Vec::new();
    for actor in crate::review::proposal_proposers(&store) {
        proposers.push(crate::proposal_voting::ProposerFacts {
            actor,
            area: social_area(tx, actor)?,
            social_penalty: tx.request(ReadSocialPenalty(actor))?,
        });
    }
    let mut foreign = Vec::new();
    for (proposal, actor) in runtime.proposal_vote_requests(&store, &active_authority, &voters)? {
        let proposal_row = crate::proposal(&store, proposal).ok_or(Error::Unavailable)?;
        let institution =
            crate::institution(&store, proposal_row.institution).ok_or(Error::Unavailable)?;
        let score = if regional(&proposal_row.change) {
            Some(tx.request(ScoreRegionalChange {
                actor,
                change: proposal_row.change.clone(),
            })?)
        } else if external(&proposal_row.change) {
            Some(tx.request(ScoreEnforcementChange {
                actor,
                institution: institution.clone(),
                change: proposal_row.change.clone(),
            })?)
        } else {
            None
        };
        if let Some(score) = score {
            foreign.push(crate::proposal_voting::ForeignVoteJudgment {
                proposal,
                actor,
                score,
            });
        }
    }
    replies.extend(runtime.proposal_replies(
        &store,
        &active_authority,
        &voters,
        &proposers,
        &foreign,
    )?);
    for (actor, action) in replies {
        execute_review_action::<C, E>(tx, actor, &action)?;
    }

    let recruits = actors
        .iter()
        .map(|actor| crate::deliberation::Recruit {
            actor: actor.actor,
            area: actor.area,
            autonomous: actor.autonomous,
            citizen: actor.citizen,
        })
        .collect::<Vec<_>>();
    let discovery = crate::review::DiscoveryIndex::new(&recruits);
    for (actor, area) in discovery.actors() {
        let Some(voter) = actors.iter().find(|voter| voter.actor == actor) else {
            continue;
        };
        let store = projection(tx)?;
        let needed = runtime.discovery_references(
            &store,
            actor,
            area,
            &voter.sites,
            discovery.recruits(area),
        );
        let references = references(tx, &needed)?;
        let authority = authority(tx, needed.people)?;
        let voter_facts = voter.facts();
        let Some(decision) = runtime.discovery_decision(
            &store,
            crate::deliberation::DiscoveryFacts {
                voter: &voter_facts,
                now: tx.now(),
                authority: &authority,
                references: &references,
                sites: &voter.sites,
                recruits: discovery.recruits(area),
            },
        ) else {
            continue;
        };
        let accepted = execute_review_action::<C, E>(tx, actor, &decision.action)?;
        if !accepted && decision.failure == crate::deliberation::FailureHandling::Propagate {
            return Err(Error::Unavailable);
        }
        tx.write(
            StableId(actor.0),
            crate::Requests {
                search_after: decision.search_after,
            },
        )?;
    }
    apply_rules::<C, E>(tx)?;

    let mut store = projection(tx)?;
    let (agreements, obligations) = tx.request(game_agreements::native::ReadDutyLedger)?;
    insert_rows(
        &mut store,
        agreements
            .into_iter()
            .map(|row| (StableId(row.id.0), row))
            .collect(),
    )?;
    insert_rows(
        &mut store,
        obligations
            .into_iter()
            .map(|row| (StableId(row.id.0), row))
            .collect(),
    )?;
    let needed_workers = runtime.duty_workers(&store)?;
    let mut work_facts = Vec::new();
    for worker in actors
        .iter()
        .filter(|actor| needed_workers.contains(&actor.actor))
    {
        let body = tx
            .request(game_physiology::services::ReadBody(worker.actor))?
            .ok_or(Error::Unavailable)?;
        let activity = tx.request(game_characters::services::ReadActivity(worker.actor))?;
        work_facts.push(crate::deliberation::DutyWorker {
            actor: worker.actor,
            autonomous: worker.autonomous,
            area: worker.area,
            idle: matches!(
                activity,
                game_characters::decisions::Activity::Idle
                    | game_characters::decisions::Activity::Rest
            ),
            work_budget: game_physiology::sustainable_work(
                &body.vitals,
                body.rules.fatigue_limit,
                body.rules.hunger_harm,
                body.rules.eating_time,
            ),
            discipline: worker.discipline,
            empathy: worker.empathy,
            sites: &worker.sites,
        });
    }
    for intention in runtime.duty_work_intentions(&store, &work_facts)? {
        tx.defer(StartInstitutionWork {
            actor: intention.actor,
            site: intention.site,
        })?;
    }
    let holders = crate::review::office_holders(&store, false);
    let hearings = actors
        .iter()
        .filter(|actor| holders.contains(&actor.actor))
        .map(|actor| crate::deliberation::HearingActor {
            actor: actor.actor,
            area: actor.area,
            autonomous: actor.autonomous,
            empathy: actor.empathy,
        })
        .collect::<Vec<_>>();
    let active_authority = authority(
        tx,
        crate::institutions(&store)
            .flat_map(|(_, row)| crate::review::authority_people(row))
            .collect(),
    )?;
    for (actor, action) in runtime.hearing_intentions(&store, &active_authority, &hearings)? {
        execute_review_action::<C, E>(tx, actor, &action)?;
    }
    Ok(())
}
pub fn clear_learning<E: 'static>(
    _: &Services,
    tx: &mut Tx<'_, '_, E>,
    request: ClearSocialLearning,
) -> Result<(), Error> {
    if tx.runtime().is_none() {
        return Err(Error::Unavailable);
    }
    let id = StableId(request.actor.0);
    let mut memory = tx
        .read::<crate::Knowledge>(id)
        .cloned()
        .ok_or(Error::Unavailable)?;
    if request.values {
        memory.values.clear();
    }
    if request.precedents {
        memory.cases.clear();
    }
    tx.write(id, memory)
}
pub fn read_commands<E: 'static>(
    _: &Services,
    tx: &mut ReadTx<'_, '_, E>,
    request: ReadInstitutionCommands,
) -> Result<Vec<InstitutionAction>, Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(Vec::new());
    };
    let body = tx.request(game_physiology::services::ReadBody(request.0))?;
    let place = tx.request(game_characters::services::ReadPlacement(request.0))?;
    let stage = tx.request(game_characters::services::ReadStage(request.0))?;
    let combat = tx.request(game_conflict::services::ReadCombatStatus(request.0))?;
    let Ok(area) = crate::review::social_area(crate::review::SocialAreaFacts {
        area: place.and_then(|place| match place.location {
            Location::AtArea(area) => Some(area),
            _ => None,
        }),
        independent: stage.independent,
        combat_restricted: combat.restricted,
        alive: body.is_some_and(|body| body.vitals.alive),
    }) else {
        return Ok(Vec::new());
    };
    let store = read_projection(tx)?;
    let mut authority = AuthorityFacts::default();
    for actor in crate::institutions(&store)
        .flat_map(|(_, row)| crate::review::authority_people(row))
        .collect::<BTreeSet<_>>()
    {
        if tx
            .request(game_physiology::services::ReadBody(actor))?
            .is_some_and(|body| body.vitals.alive)
        {
            authority.alive.insert(actor);
        }
    }
    Ok(runtime.institution_opportunities(&store, request.0, area, &authority))
}
pub fn validate<C: Component, E: 'static>(
    _: &Services,
    tx: &mut Tx<'_, '_, E>,
    _: ValidateInstitutions,
) -> Result<(), Error> {
    let mut store = projection(tx)?;
    let Some(runtime) = tx.runtime().clone() else {
        if store.len() == 0 {
            return Ok(());
        }
        return Err(Error::InvalidSave(
            "society components require their runtime".into(),
        ));
    };
    if !tx.request(game_physiology::services::PhysicalAvailable)?
        || !tx.request(game_characters::services::ReadPlanningAvailable)?
        || !tx.request(game_agreements::native::ReadAgreementAvailable)?
    {
        return Err(Error::InvalidSave("invalid institution state".into()));
    }
    let request = runtime.validation_references(&store);
    let facts = references(tx, &request)?;
    for actor in &request.people {
        if tx
            .request(game_characters::services::ReadMind::<C>(
                *actor,
                std::marker::PhantomData,
            ))?
            .is_none()
        {
            return Err(Error::InvalidSave("invalid institutional person".into()));
        }
    }
    let mut areas = BTreeSet::new();
    let needed_areas = crate::institutions(&store)
        .map(|(_, row)| row.charter.jurisdiction)
        .chain(crate::cases(&store).map(|(_, row)| row.area))
        .collect::<BTreeSet<_>>();
    for area in needed_areas {
        if tx.request(InstitutionAreaExists(area))? {
            areas.insert(area);
        }
    }
    let needed_agreements = crate::institutions(&store)
        .flat_map(|(_, row)| {
            row.funding.iter().copied().chain(
                row.applied
                    .values()
                    .flat_map(|people| people.values().copied()),
            )
        })
        .collect::<BTreeSet<_>>();
    for id in needed_agreements {
        if let Some(row) = tx.request(game_agreements::native::ReadAgreement(id))? {
            store.insert_or_spawn(StableId(id.0), row)?;
        }
    }
    runtime.validate(
        &store,
        &crate::save_validation::ValidationFacts {
            at: tx.now(),
            next_id: tx.next_id(),
            next_event: tx.next_event(),
            areas: &areas,
            references: &facts,
        },
    )
}
pub fn validate_event<E: 'static>(
    _: &Services,
    tx: &mut ReadTx<'_, '_, E>,
    query: ValidateInstitutionEvent,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent committed state".into());
    if tx.runtime().is_none() {
        return Err(fail());
    }
    let institution = match query.0 {
        Event::Proposed {
            institution,
            proposer,
        } => {
            if tx
                .request(game_characters::services::ReadPerson(proposer))?
                .is_none()
            {
                return Err(fail());
            }
            institution
        }
        Event::Founded { institution }
        | Event::Left { institution, .. }
        | Event::OfficeSupportChanged { institution, .. }
        | Event::AccessApplied { institution, .. }
        | Event::DutyRecognized { institution, .. } => institution,
        Event::Changed {
            institution,
            proposal,
        } => {
            if tx
                .read::<RuleProposal>(StableId(proposal.0))
                .is_none_or(|row| row.institution != institution)
            {
                return Err(fail());
            }
            institution
        }
        Event::SocialCaseRecorded { case, actor } => {
            if tx
                .request(game_characters::services::ReadPerson(actor))?
                .is_none()
                || tx.read::<SocialCaseView>(StableId(case.0)).is_none()
            {
                return Err(fail());
            }
            return Ok(());
        }
    };
    if tx.read::<Institution>(StableId(institution.0)).is_none() {
        return Err(fail());
    }
    Ok(())
}

pub fn register_native<S: 'static, C: Component, E: From<Event> + 'static>(
    registry: &mut Registry<S, E>,
) -> Result<(), Error> {
    registry.owns::<Services, Institution>()?;
    registry.owns::<Services, RuleProposal>()?;
    registry.owns::<Services, crate::Concept>()?;
    registry.owns::<Services, SocialCaseView>()?;
    registry.owns::<Services, crate::Knowledge>()?;
    registry.owns::<Services, crate::Requests>()?;
    registry.owns::<Services, crate::Exclusions>()?;
    registry.owns::<Services, crate::Hearing>()?;
    registry.register_read_request::<Services, ReadInstitution>(read_institution::<E>)?;
    registry.register_read_request::<Services, ReadRuleProposal>(read_proposal::<E>)?;
    registry.register_read_request::<Services, ReadInstitutionMemory>(read_memory::<E>)?;
    registry.register_read_request::<Services, ReadAuthority>(read_authority::<E>)?;
    registry.register_read_request::<Services, ReadInstitutionViews>(read_views::<E>)?;
    registry.register_read_request::<Services, ReadSocialCaseViews>(read_cases::<E>)?;
    registry.register_read_request::<Services, ReadInstitutionsAvailable>(read_available::<E>)?;
    registry.register_read_request::<Services, ReadInstitutionCommands>(read_commands::<E>)?;
    registry.register_request::<Services, ReceiveInstitutionMemory>(receive_memory::<E>)?;
    registry.register_request::<Services, ExecuteInstitution>(execute::<C, E>)?;
    registry.register_request::<Services, ExecuteInstitutionForEnforcement>(
        execute_for_enforcement::<C, E>,
    )?;
    registry.register_read_request::<Services, ValidateInstitutionEvent>(validate_event::<E>)?;
    registry.register_request::<Services, ReviewInstitutions>(review::<C, E>)?;
    registry.register_request::<Services, ValidateInstitutions>(validate::<C, E>)?;
    registry.register_request::<Services, ClearSocialLearning>(clear_learning::<E>)?;
    registry.register_request::<Services, ReceiveSocialReceipts>(receive_receipts::<C, E>)?;
    registry.register_request::<Services, SubmitRegionalPassage>(submit_regional::<C, E>)?;
    registry.register_request::<Services, ProcessSocialInbox>(process_inbox::<C, E>)?;
    registry.register_subscriber::<Services, ReceiveSocialReceipts>(receive_topic::<E>)?;
    registry
        .register_subscriber::<Services, game_cargo::receiving::CargoEvent>(receive_cargo::<E>)?;
    registry
        .register_subscriber::<Services, game_physiology::systems::Event>(receive_physical::<E>)?;
    registry.register_subscriber::<Services, game_agreements::native::Event>(receive_agreement::<E>)
}

pub fn submit_regional<C: Component, E: From<Event> + 'static>(
    owner: &Services,
    tx: &mut Tx<'_, '_, E>,
    request: SubmitRegionalPassage,
) -> Result<RuleProposalId, Error> {
    if !regional(&request.change) {
        return Err(Error::Unavailable);
    }
    let first = tx.next_id();
    execute_action::<C, E>(
        tx,
        request.actor,
        &InstitutionAction::Propose {
            institution: request.institution,
            change: request.change,
        },
    )?;
    reconcile_history::<C, E>(owner, tx)?;
    let id = RuleProposalId(first);
    if tx.read::<RuleProposal>(StableId(first)).is_none() {
        return Err(Error::Unavailable);
    }
    Ok(id)
}
struct SocialInbox {
    receipts: Vec<crate::reactions::SocialReceipt>,
    queued: bool,
}
struct ProcessSocialInbox;
impl Request for ProcessSocialInbox {
    type Response = ();
}
fn enqueue<E: 'static>(
    tx: &mut Tx<'_, '_, E>,
    receipts: Vec<crate::reactions::SocialReceipt>,
) -> Result<(), Error> {
    if tx.runtime().is_none() || receipts.is_empty() {
        return Ok(());
    }
    if tx.fact::<SocialInbox>().is_none() {
        tx.share(SocialInbox {
            receipts: Vec::new(),
            queued: false,
        })?;
    }
    let inbox = tx.fact_mut::<SocialInbox>()?.ok_or(Error::Unavailable)?;
    inbox.receipts.extend(receipts);
    let queue = !inbox.queued;
    inbox.queued = true;
    if queue {
        tx.defer(ProcessSocialInbox)?;
    }
    Ok(())
}
fn process_inbox<C: Component, E: From<Event> + 'static>(
    owner: &Services,
    tx: &mut Tx<'_, '_, E>,
    _: ProcessSocialInbox,
) -> Result<(), Error> {
    let inbox = tx.fact_mut::<SocialInbox>()?.ok_or(Error::Unavailable)?;
    inbox.queued = false;
    let receipts = std::mem::take(&mut inbox.receipts);
    if owner.has_history() {
        return reconcile_history::<C, E>(owner, tx);
    }
    receive_receipts::<C, E>(owner, tx, ReceiveSocialReceipts(receipts))
}
fn receive_topic<E: 'static>(
    _: &Services,
    tx: &mut Tx<'_, '_, E>,
    request: ReceiveSocialReceipts,
) -> Result<(), Error> {
    enqueue(tx, request.0)
}
fn receive_physical<E: 'static>(
    _: &Services,
    tx: &mut Tx<'_, '_, E>,
    event: game_physiology::systems::Event,
) -> Result<(), Error> {
    match event {
        game_physiology::systems::Event::Ate { actor, source, .. } => enqueue(
            tx,
            vec![crate::reactions::SocialReceipt::Ate { actor, source }],
        ),
        _ => Ok(()),
    }
}
fn receive_cargo<E: 'static>(
    _: &Services,
    tx: &mut Tx<'_, '_, E>,
    event: game_cargo::receiving::CargoEvent,
) -> Result<(), Error> {
    match event {
        game_cargo::receiving::CargoEvent::Transferred { receipt, after, .. } => enqueue(
            tx,
            vec![crate::reactions::SocialReceipt::Transferred {
                actor: receipt.actor,
                source: receipt.source,
                good: receipt.good,
                destination_holder: match after[1].location {
                    game_cargo::InventoryLocation::Carried(actor) => Some(actor),
                    _ => None,
                },
            }],
        ),
        _ => Ok(()),
    }
}
fn receipt_asset<E: 'static>(
    tx: &mut Tx<'_, '_, E>,
    inventory: InventoryId,
    area: Option<AreaId>,
) -> Result<crate::reactions::CaseAsset, Error> {
    let row = tx
        .request(game_cargo::facts::ReadInventory(inventory))?
        .ok_or(Error::Unavailable)?;
    Ok(crate::reactions::CaseAsset {
        inventory,
        area,
        good: row.good,
    })
}
fn receive_agreement<E: 'static>(
    _: &Services,
    tx: &mut Tx<'_, '_, E>,
    event: game_agreements::native::Event,
) -> Result<(), Error> {
    if tx.runtime().is_none() {
        return Ok(());
    }
    let decoded = match event {
        game_agreements::native::Event::Reconciliation(
            game_agreements::ReconciliationEvent::Performed {
                obligation, actor, ..
            },
        ) => HistoryEvent::Performed { obligation, actor },
        game_agreements::native::Event::Reconciliation(
            game_agreements::ReconciliationEvent::Defaulted { obligation },
        ) => HistoryEvent::Defaulted { obligation },
        _ => return Ok(()),
    };
    if let Some(receipt) = history_receipt(tx, decoded)? {
        enqueue(tx, vec![receipt])?;
    }
    Ok(())
}
fn history_receipt<E: 'static>(
    tx: &mut Tx<'_, '_, E>,
    event: HistoryEvent,
) -> Result<Option<crate::reactions::SocialReceipt>, Error> {
    use crate::reactions::SocialReceipt;
    let (id, performed, attributed) = match event {
        HistoryEvent::Ate { actor, source } => {
            return Ok(Some(SocialReceipt::Ate { actor, source }));
        }
        HistoryEvent::Transferred {
            actor,
            source,
            destination,
        } => {
            let row = tx
                .request(game_cargo::facts::ReadInventory(source))?
                .ok_or(Error::Unavailable)?;
            let destination = tx
                .request(game_cargo::facts::ReadInventory(destination))?
                .ok_or(Error::Unavailable)?;
            return Ok(Some(SocialReceipt::Transferred {
                actor,
                source,
                good: row.good,
                destination_holder: match destination.location {
                    game_cargo::InventoryLocation::Carried(actor) => Some(actor),
                    _ => None,
                },
            }));
        }
        HistoryEvent::Produced {
            actor,
            site,
            output,
        } => {
            let area = tx.request(ReadInstitutionSite(site))?;
            let asset = receipt_asset(tx, output, area)?;
            let custodian = tx.request(game_property::services::ReadCustodian(output))?;
            return Ok(Some(SocialReceipt::Produced {
                actor,
                site,
                asset,
                custodian,
            }));
        }
        HistoryEvent::Performed { obligation, actor } => (obligation, true, Some(actor)),
        HistoryEvent::Defaulted { obligation } => (obligation, false, None),
    };
    let row = tx
        .request(game_agreements::native::ReadObligation(id))?
        .ok_or(Error::Unavailable)?;
    let target = match row.term {
        game_agreements::AgreementTerm::Labor { site, .. } => {
            let terms = tx.request(game_agreements::native::ReadWorkshopTerms(site))?;
            let Some(inventory) = terms.outputs.values().next().copied() else {
                return Err(Error::Unavailable);
            };
            let area = tx.request(ReadInstitutionSite(site))?;
            Some(crate::reactions::CooperationTarget::Labor {
                site,
                asset: receipt_asset(tx, inventory, area)?,
            })
        }
        game_agreements::AgreementTerm::Delivery { destination, .. } => {
            let area = tx.request(game_cargo::facts::ReadInventoryArea {
                inventory: destination,
            })?;
            Some(crate::reactions::CooperationTarget::Delivery {
                asset: receipt_asset(tx, destination, area)?,
            })
        }
        _ => None,
    };
    let Some(target) = target else {
        return Ok(None);
    };
    let [debtor, counterpart] = row.term.parties();
    let actor = attributed.unwrap_or(debtor);
    let receipt = if performed {
        crate::reactions::SocialReceipt::Performed {
            actor,
            counterpart,
            agreement: row.agreement,
            target,
        }
    } else {
        crate::reactions::SocialReceipt::Defaulted {
            actor,
            counterpart,
            agreement: row.agreement,
            asset: match target {
                crate::reactions::CooperationTarget::Labor { asset, .. }
                | crate::reactions::CooperationTarget::Delivery { asset } => asset,
            },
        }
    };
    Ok(Some(receipt))
}
fn reconcile_history<C: Component, E: From<Event> + 'static>(
    owner: &Services,
    tx: &mut Tx<'_, '_, E>,
) -> Result<(), Error> {
    if !owner.has_history() {
        return Ok(());
    }
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(());
    };
    let decoded = tx
        .events_since(runtime.last_event)
        .into_iter()
        .filter_map(|(_, _, event)| owner.history_event(event))
        .collect::<Vec<_>>();
    let mut receipts = Vec::new();
    for event in decoded {
        if let Some(receipt) = history_receipt(tx, event)? {
            receipts.push(receipt);
        }
    }
    receive_receipts::<C, E>(owner, tx, ReceiveSocialReceipts(receipts))
}

pub fn boundary<E: 'static>(
    _: &Services,
    tx: &mut ReadTx<'_, '_, E>,
    span: game_world::scheduling::ClockSpan,
) -> Result<Option<SimTime>, Error> {
    if tx.runtime().is_none() {
        return Ok(None);
    }
    let next = (span.from.0 / 60_000)
        .checked_add(1)
        .and_then(|tick| tick.checked_mul(60_000))
        .ok_or(Error::Overflow)?;
    Ok(Some(SimTime(next)).filter(|next| *next <= span.to))
}
pub fn close<C: Component, E: From<Event> + 'static>(
    owner: &Services,
    tx: &mut Tx<'_, '_, E>,
    span: game_world::scheduling::ClockSpan,
) -> Result<(), Error> {
    if span.to.0 % 60_000 == 0 {
        review::<C, E>(owner, tx, ReviewInstitutions)?;
    }
    Ok(())
}
pub fn register_schedule<S: 'static, C: Component, E: From<Event> + 'static>(
    registry: &mut Registry<S, E>,
    scheduler: &mut game_world::scheduling::Scheduler<E>,
    order: game_world::scheduling::Order,
) -> Result<(), Error> {
    scheduler.register_owner_phased::<Services, S>(
        registry,
        order,
        Some(boundary::<E>),
        None,
        None,
        Some(close::<C, E>),
    )
}

#[cfg(test)]
mod tests;

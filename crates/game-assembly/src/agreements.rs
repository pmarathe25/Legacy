//! Voluntary local bargaining, chosen amendment procedures, and physical performance.
use crate::*;
pub use game_agreements::{Agreement, Amendment, Obligation};
use game_agreements::{accrue_share, same_duty, stream_ended};

struct OfferKnowledgeFacts<'a> {
    stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
    sites: &'a BTreeMap<SiteId, decisions::SiteKnowledge>,
    projects: BTreeSet<ProjectId>,
    shares: BTreeMap<(SiteId, u64, u64), game_agreements::ShareProspect>,
    speakers: BTreeMap<PersonId, game_agreements::SpeakerFacts>,
}
impl game_agreements::OfferKnowledge for OfferKnowledgeFacts<'_> {
    fn stock(&self, id: InventoryId) -> Option<&StockKnowledge> {
        self.stocks.get(&id)
    }
    fn work_site_access(&self, id: SiteId) -> bool {
        self.sites.get(&id).is_some_and(|site| site.access)
    }
    fn project_known(&self, id: ProjectId) -> bool {
        self.projects.contains(&id)
    }
    fn share_prospect(
        &self,
        id: SiteId,
        numerator: u64,
        denominator: u64,
    ) -> game_agreements::ShareProspect {
        self.shares
            .get(&(id, numerator, denominator))
            .copied()
            .unwrap_or(game_agreements::ShareProspect::Unknown)
    }
    fn speaker(&self, id: PersonId) -> Option<game_agreements::SpeakerFacts> {
        self.speakers.get(&id).copied()
    }
}

fn site_exists(state: &CanonicalState, site: SiteId) -> bool {
    state
        .state
        .physical
        .as_ref()
        .is_some_and(|_| game_workshop::site(state.components, site).is_some())
        || state
            .state
            .agriculture
            .as_ref()
            .is_some_and(|w| w.field(state.components, site).is_some())
        || state
            .state
            .ecology
            .as_ref()
            .is_some_and(|w| w.plot(state.components, site).is_some())
}
fn site_custodian(state: &CanonicalState, site: SiteId) -> Option<PersonId> {
    state
        .state
        .agriculture
        .as_ref()
        .and_then(|w| w.field(state.components, site))
        .and_then(|f| f.custodian)
        .or_else(|| {
            state
                .state
                .ecology
                .as_ref()
                .and_then(|w| w.plot(state.components, site))
                .and_then(|p| p.custodian)
        })
}
fn output_source(state: &CanonicalState, site: SiteId, good: Good) -> Option<InventoryId> {
    state
        .state
        .agriculture
        .as_ref()
        .and_then(|w| w.field(state.components, site))
        .and_then(|f| match good {
            Good::Food => Some(f.food),
            Good::Seed => Some(f.seed),
            _ => None,
        })
        .or_else(|| {
            state
                .state
                .projects
                .as_ref()
                .and_then(|_| {
                    game_projects::projects(state.components)
                        .map(|(_, row)| row)
                        .find(|p| matches!(p.kind,ProjectKind::Farm {plot,..} if plot==site))
                })
                .and_then(|p| match good {
                    Good::Food => Some(p.output),
                    Good::Seed => Some(p.input),
                    _ => None,
                })
        })
        .or_else(|| {
            state
                .state
                .physical
                .as_ref()
                .and_then(|_| game_workshop::site(state.components, site))
                .and_then(|s| {
                    (game_cargo::inventory(state.components, s.output)
                        .expect("inventory exists")
                        .good
                        == good)
                        .then_some(s.output)
                })
        })
}
fn output_share_valid(state: &CanonicalState, term: &AgreementTerm) -> bool {
    let AgreementTerm::OutputShare {
        site,
        good,
        numerator,
        denominator,
        destination,
        pay_by,
        ends_at,
        ..
    } = term
    else {
        return false;
    };
    *numerator > 0
        && *denominator > 0
        && numerator <= denominator
        && pay_by >= ends_at
        && output_source(state, *site, *good).is_some_and(|source| source != *destination)
        && game_cargo::inventory(state.components, *(destination)).is_some_and(|i| i.good == *good)
}

fn refresh_known_status(view: &mut AgreementView) {
    if matches!(
        view.status,
        AgreementStatus::Active | AgreementStatus::Fulfilled | AgreementStatus::Breached
    ) {
        view.status = if !view.obligations.is_empty()
            && view
                .obligations
                .iter()
                .all(|o| o.performed == o.amount() && stream_ended(&o.term, view.observed_at))
        {
            AgreementStatus::Fulfilled
        } else if view.obligations.iter().any(|o| o.defaulted) {
            AgreementStatus::Breached
        } else {
            AgreementStatus::Active
        };
    }
}

fn merge_agreement_knowledge(old: AgreementView, incoming: &mut AgreementView) {
    if old.revision > incoming.revision {
        *incoming = old;
        return;
    }
    if old.revision == incoming.revision {
        incoming.accepted.extend(old.accepted.iter().copied());
        if matches!(
            old.status,
            AgreementStatus::Withdrawn | AgreementStatus::Rejected
        ) || incoming.status == AgreementStatus::Proposed
            && old.status != AgreementStatus::Proposed
        {
            incoming.status = old.status;
        }
        for prior in old.obligations {
            if let Some(new) = incoming.obligations.iter_mut().find(|o| o.id == prior.id) {
                if prior.observed_at > new.observed_at {
                    *new = prior;
                } else if prior.term == new.term {
                    new.performed = new.performed.max(prior.performed);
                    new.defaulted |= prior.defaulted;
                }
            } else {
                incoming.obligations.push(prior);
            }
        }
        for prior in old.amendments {
            if let Some(new) = incoming.amendments.iter_mut().find(|a| a.id == prior.id) {
                for (actor, support) in prior.votes {
                    new.votes.entry(actor).or_insert(support);
                }
                if prior.status != AmendmentStatus::Proposed {
                    new.status = prior.status;
                }
            } else {
                incoming.amendments.push(prior);
            }
        }
    }
    incoming.obligations.sort_by_key(|o| o.id);
    incoming.amendments.sort_by_key(|a| a.id);
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(w) = &s.state.agreements else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid agreement state".into());
    let people: BTreeSet<_> = s.state.people.iter().map(|p| p.id).collect();
    s.state.physical.as_ref().ok_or_else(fail)?;
    if s.state.decisions.is_none()
        || w.last_event >= s.next_event
        || w.last_observation_event >= s.next_event
        || w.custodians.iter().any(|(inventory, actor)| {
            !game_cargo::inventory(s.components, *(inventory)).is_some() || !people.contains(actor)
        })
        || w.next_search.keys().any(|id| !people.contains(id))
    {
        return Err(fail());
    }
    let valid_term = |term: &AgreementTerm, parties: &BTreeSet<PersonId>| {
        let [a, b] = term.parties();
        if a == b || !parties.contains(&a) || !parties.contains(&b) {
            return false;
        }
        match term {
            AgreementTerm::Delivery {
                source,
                destination,
                quantity,
                ..
            } => game_cargo::inventory(s.components, *(source))
                .zip(game_cargo::inventory(s.components, *(destination)))
                .is_some_and(|(a, b)| {
                    a.id != b.id && a.good == b.good && quantity.0 <= b.capacity.0
                }),
            AgreementTerm::Labor { site, .. } => site_exists(s, *site),
            AgreementTerm::SiteAccess { grantor, site, .. } => {
                site_custodian(s, *site) == Some(*grantor)
            }
            AgreementTerm::OutputShare { .. } => output_share_valid(s, term),
            AgreementTerm::ConstructionLabor { project, .. } => s
                .state
                .projects
                .as_ref()
                .is_some_and(|_| game_projects::project(s.components, *project).is_some()),
            AgreementTerm::Access {
                grantor, inventory, ..
            } => {
                w.custodians.get(inventory) == Some(grantor)
                    || game_cargo::inventory(s.components, *(inventory))
                        .is_some_and(|i| i.location == InventoryLocation::Carried(*grantor))
            }
        }
    };
    let mut referenced = BTreeSet::new();
    let mut cargo_totals: BTreeMap<InventoryId, u64> = BTreeMap::new();
    for (id, a) in &w.agreements {
        if *id != a.id
            || id.0 >= s.next_id
            || !(2..=8).contains(&a.parties.len())
            || !a.parties.is_subset(&people)
            || !a.parties.contains(&a.proposer)
            || !a.accepted.is_subset(&a.parties)
            || !a.accepted.contains(&a.proposer)
            || a.proposed_at > s.time
            || a.terms.is_empty()
            || a.terms.len() > 16
            || a.terms.iter().any(|t| !valid_term(t, &a.parties))
        {
            return Err(fail());
        }
        if let Some(p) = &a.procedure
            && (p.eligible_proposers.is_empty()
                || !p.eligible_proposers.is_subset(&a.parties)
                || !p.approvers.is_subset(&a.parties)
                || p.threshold == 0
                || p.threshold > p.approvers.len()
                || p.term_approvers.iter().any(|(index, ids)| {
                    *index as usize >= a.terms.len() || ids.is_empty() || !ids.is_subset(&a.parties)
                }))
        {
            return Err(fail());
        }
        let active = matches!(
            a.status,
            AgreementStatus::Active | AgreementStatus::Fulfilled | AgreementStatus::Breached
        );
        if a.terms.iter().enumerate().any(|(index, term)| {
            term.amount() == 0
                && !matches!(term, AgreementTerm::OutputShare { .. })
                && (!active
                    || !w.amendments.values().any(|amendment| {
                        amendment.agreement == a.id
                            && amendment.status == AmendmentStatus::Adopted
                            && amendment.replacements.get(&(index as u16)) == Some(term)
                    }))
        }) {
            return Err(fail());
        }
        if (active && (a.obligations.len() != a.terms.len() || a.accepted != a.parties))
            || (!active && !a.obligations.is_empty())
        {
            return Err(fail());
        }
        for (index, obligation) in a.obligations.iter().enumerate() {
            let o = w.obligations.get(obligation).ok_or_else(fail)?;
            if !referenced.insert(*obligation) || o.agreement != *id || o.term != a.terms[index] {
                return Err(fail());
            }
        }
        if active {
            let fulfilled = a.obligations.iter().all(|id| {
                w.obligations[id].performed == w.obligations[id].amount()
                    && stream_ended(&w.obligations[id].term, s.time)
            });
            let breached = a.obligations.iter().any(|id| w.obligations[id].defaulted);
            if (a.status == AgreementStatus::Fulfilled) != fulfilled
                || (!fulfilled && (a.status == AgreementStatus::Breached) != breached)
            {
                return Err(fail());
            }
        }
    }
    if referenced.len() != w.obligations.len() {
        return Err(fail());
    }
    if w.pending_work.iter().any(|(id, quantity)| {
        w.obligations.get(id).is_none_or(|o| {
            !matches!(
                o.term,
                AgreementTerm::Labor { .. } | AgreementTerm::ConstructionLabor { .. }
            ) || *quantity == 0
                || *quantity > o.performed
        })
    }) {
        return Err(fail());
    }
    for (id, o) in &w.obligations {
        if *id != o.id
            || id.0 >= s.next_id
            || o.performed > o.amount()
            || o.amount() == 0 && o.defaulted
            || match o.term {
                AgreementTerm::OutputShare { denominator, .. } => {
                    o.share_remainder
                        >= if o.share_scale == 0 {
                            denominator
                        } else {
                            o.share_scale
                        }
                }
                _ => o.accrued != 0 || o.share_remainder != 0 || o.share_scale != 0,
            }
            || (o.defaulted && !o.term.deadline().is_some_and(|d| d <= s.time))
        {
            return Err(fail());
        }
        let mut total = 0_u64;
        for (inventory, quantity) in &o.cargo {
            let (debtor, source) = match o.term {
                AgreementTerm::Delivery { debtor, source, .. } => (debtor, source),
                AgreementTerm::OutputShare {
                    debtor, site, good, ..
                } => (debtor, output_source(s, site, good).ok_or_else(fail)?),
                _ => return Err(fail()),
            };
            let stock = game_cargo::inventory(s.components, *(inventory)).ok_or_else(fail)?;
            if *quantity == 0
                || stock.location != InventoryLocation::Carried(debtor)
                || stock.good
                    != game_cargo::inventory(s.components, source)
                        .expect("inventory exists")
                        .good
            {
                return Err(fail());
            }
            total = total.checked_add(*quantity).ok_or_else(fail)?;
            let sum = cargo_totals.entry(*inventory).or_default();
            *sum = sum.checked_add(*quantity).ok_or_else(fail)?;
        }
        if total > o.amount() - o.performed {
            return Err(fail());
        }
    }
    if cargo_totals.iter().any(|(id, quantity)| {
        *quantity
            > game_cargo::inventory(s.components, *(id))
                .expect("inventory exists")
                .quantity
                .0
    }) {
        return Err(fail());
    }
    for (id, m) in &w.amendments {
        let a = w.agreements.get(&m.agreement).ok_or_else(fail)?;
        let p = a.procedure.as_ref().ok_or_else(fail)?;
        if *id != m.id
            || id.0 >= s.next_id
            || !p.eligible_proposers.contains(&m.proposer)
            || m.base_revision > a.revision
            || (m.status == AmendmentStatus::Proposed && m.base_revision != a.revision)
            || m.replacements.is_empty()
            || m.replacements.len() > 16
            || m.replacements.iter().any(|(index, term)| {
                *index as usize >= a.terms.len() || !valid_term(term, &a.parties)
            })
            || m.votes.keys().any(|actor| {
                !p.approvers.contains(actor)
                    && !p.term_approvers.values().any(|ids| ids.contains(actor))
            })
        {
            return Err(fail());
        }
    }
    for (actor, known) in &w.knowledge {
        if !people.contains(actor) {
            return Err(fail());
        }
        for (id, view) in known {
            if *id != view.id || !valid_knowledge(s, view) {
                return Err(fail());
            }
        }
    }
    for (_, activity) in s.components.activities() {
        if let Activity::ReportAgreement { snapshot, .. } = activity
            && !valid_knowledge(s, snapshot)
        {
            return Err(fail());
        }
    }
    Ok(())
}

fn valid_knowledge(state: &CanonicalState, view: &AgreementView) -> bool {
    let Some(world) = &state.state.agreements else {
        return false;
    };
    let Some(actual) = world.agreements.get(&view.id) else {
        return false;
    };
    let exists = |actor| state.state.people.iter().any(|person| person.id == actor);
    let source_valid = |source: &Option<KnowledgeSource>| {
        source.as_ref().is_none_or(|source| match source {
            KnowledgeSource::Observed(actor) | KnowledgeSource::Reported(actor) => exists(*actor),
            KnowledgeSource::Recorded(_) => false,
        })
    };
    if view.parties != actual.parties
        || view.proposer != actual.proposer
        || !view.accepted.is_subset(&view.parties)
        || view.terms.is_empty()
        || view.terms.len() > 16
        || view.revision > actual.revision
        || view.observed_at > view.learned_at
        || view.learned_at > state.time
        || !source_valid(&view.source)
    {
        return false;
    }
    let valid_term = |term: &AgreementTerm| {
        let [a, b] = term.parties();
        if a == b || !view.parties.contains(&a) || !view.parties.contains(&b) {
            return false;
        }
        match term {
            AgreementTerm::Delivery {
                source,
                destination,
                ..
            } => {
                source != destination
                    && game_cargo::inventory(state.components, *(source)).is_some()
                    && game_cargo::inventory(state.components, *(destination)).is_some()
            }
            AgreementTerm::Labor { site, .. } => site_exists(state, *site),
            AgreementTerm::SiteAccess { site, .. } => site_exists(state, *site),
            AgreementTerm::OutputShare { .. } => output_share_valid(state, term),
            AgreementTerm::ConstructionLabor { project, .. } => state
                .state
                .projects
                .as_ref()
                .is_some_and(|_| game_projects::project(state.components, *project).is_some()),
            AgreementTerm::Access { inventory, .. } => {
                game_cargo::inventory(state.components, *(inventory)).is_some()
            }
        }
    };
    if view.terms.iter().any(|term| !valid_term(term)) {
        return false;
    }
    let mut obligations = BTreeSet::new();
    for obligation in &view.obligations {
        if !obligations.insert(obligation.id)
            || !world
                .obligations
                .get(&obligation.id)
                .is_some_and(|o| o.agreement == view.id)
            || !valid_term(&obligation.term)
            || obligation.performed > obligation.amount()
            || match obligation.term {
                AgreementTerm::OutputShare { denominator, .. } => {
                    obligation.share_remainder
                        >= if obligation.share_scale == 0 {
                            denominator
                        } else {
                            obligation.share_scale
                        }
                }
                _ => {
                    obligation.accrued != 0
                        || obligation.share_remainder != 0
                        || obligation.share_scale != 0
                }
            }
            || obligation.observed_at > obligation.learned_at
            || obligation.learned_at > state.time
            || !source_valid(&obligation.source)
            || obligation.defaulted
                && !obligation
                    .term
                    .deadline()
                    .is_some_and(|at| at <= obligation.observed_at)
            || obligation.cargo.iter().any(|(id, qty)| {
                !game_cargo::inventory(state.components, *(id)).is_some() || *qty == 0
            })
            || obligation
                .cargo
                .values()
                .try_fold(0_u64, |sum, qty| sum.checked_add(*qty))
                .is_none_or(|sum| sum > obligation.amount() - obligation.performed)
        {
            return false;
        }
    }
    let mut amendments = BTreeSet::new();
    for proposal in &view.amendments {
        if !amendments.insert(proposal.id)
            || !world
                .amendments
                .get(&proposal.id)
                .is_some_and(|a| a.agreement == view.id)
            || proposal.base_revision > view.revision
            || proposal.replacements.is_empty()
            || proposal
                .replacements
                .iter()
                .any(|(index, term)| *index as usize >= view.terms.len() || !valid_term(term))
            || proposal
                .votes
                .keys()
                .any(|actor| !view.parties.contains(actor))
        {
            return false;
        }
    }
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub ledger: game_agreements::State,
    /// Physical asset control remains outside the economic ledger.
    pub custodians: BTreeMap<InventoryId, PersonId>,
}
impl std::ops::Deref for World {
    type Target = game_agreements::State;
    fn deref(&self) -> &Self::Target {
        &self.ledger
    }
}
impl std::ops::DerefMut for World {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ledger
    }
}

impl Simulation {
    pub fn agreement_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::autonomy_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "agreement-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let local = sim.host.state.areas[0].id;
        let ruler = sim.host.state.ruler;
        let citizen = sim
            .host
            .state
            .people
            .iter()
            .find(|p| {
                p.role == Role::Citizen
                    && *game_spatial::location(&sim.host.storage, game_ecs::StableId((p.id).0))
                        .expect("location exists")
                        == Location::AtArea(local)
            })
            .unwrap()
            .id;
        let custodians = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .filter(|i| i.location == InventoryLocation::Area(local))
            .map(|i| (i.id, if i.good == Good::Food { ruler } else { citizen }))
            .collect();
        // This fixture authors one controlled store; sharing is negotiated below.
        for __stock_id in game_cargo::inventories(&sim.host.storage)
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            let inventory = game_cargo::inventory_mut(&mut sim.host.storage, __stock_id)
                .expect("inventory exists");
            if inventory.location == InventoryLocation::Area(local) && inventory.good == Good::Food
            {
                inventory.access = BTreeSet::from([ruler]);
            }
        }
        sim.host.state.agreements = Some(World {
            ledger: game_agreements::State {
                agreements: BTreeMap::new(),
                obligations: BTreeMap::new(),
                amendments: BTreeMap::new(),
                last_event: sim.host.next_event() - 1,
                next_search: BTreeMap::new(),
                pending_work: BTreeMap::new(),
                knowledge: BTreeMap::new(),
                last_observation_event: sim.host.next_event() - 1,
            },
            custodians,
        });
        Ok(sim)
    }

    pub fn agreement_view(&self, actor: PersonId) -> Vec<AgreementView> {
        let Some(world) = &self.host.state.agreements else {
            return Vec::new();
        };
        world
            .knowledge
            .get(&actor)
            .into_iter()
            .flat_map(|known| known.values())
            .take(64)
            .cloned()
            .collect()
    }

    fn actual_agreement_snapshot(
        &self,
        actor: PersonId,
        agreement: AgreementId,
    ) -> Option<AgreementView> {
        let world = self.host.state.agreements.as_ref()?;
        let a = world.agreements.get(&agreement)?;
        Some(AgreementView {
            id: a.id,
            proposer: a.proposer,
            parties: a.parties.clone(),
            accepted: a.accepted.clone(),
            status: a.status,
            terms: a.terms.clone(),
            obligations: a
                .obligations
                .iter()
                .map(|id| {
                    let o = &world.obligations[id];
                    ObligationView {
                        accrued: o.accrued,
                        share_remainder: o.share_remainder,
                        share_scale: o.share_scale,
                        id: *id,
                        term: o.term.clone(),
                        performed: o.performed,
                        defaulted: o.defaulted,
                        cargo: if o.term.parties()[0] == actor {
                            o.cargo.clone()
                        } else {
                            BTreeMap::new()
                        },
                        observed_at: self.host.clock().time,
                        learned_at: self.host.clock().time,
                        source: Some(KnowledgeSource::Observed(actor)),
                    }
                })
                .collect(),
            procedure: a.procedure.clone(),
            amendments: world
                .amendments
                .values()
                .filter(|m| m.agreement == a.id)
                .map(|m| AmendmentView {
                    id: m.id,
                    proposer: m.proposer,
                    replacements: m.replacements.clone(),
                    votes: m.votes.clone(),
                    status: m.status,
                    base_revision: m.base_revision,
                })
                .collect(),
            revision: a.revision,
            observed_at: self.host.clock().time,
            learned_at: self.host.clock().time,
            source: Some(KnowledgeSource::Observed(actor)),
        })
    }

    /// A genuine notice discloses declared terms, not remote warehouse facts.
    pub(crate) fn notice_agreement(&mut self, actor: PersonId, agreement: AgreementId) {
        let Some(mut view) = self.actual_agreement_snapshot(actor, agreement) else {
            return;
        };
        let old = self
            .host
            .state
            .agreements
            .as_ref()
            .unwrap()
            .knowledge
            .get(&actor)
            .and_then(|known| known.get(&agreement))
            .cloned();
        view.amendments = old
            .as_ref()
            .map_or_else(Vec::new, |old| old.amendments.clone());
        for amendment in &mut view.amendments {
            if amendment.status == AmendmentStatus::Proposed
                && amendment.base_revision < view.revision
            {
                amendment.status = AmendmentStatus::Rejected;
            }
        }
        for obligation in &mut view.obligations {
            let prior = old
                .as_ref()
                .and_then(|old| old.obligations.iter().find(|o| o.id == obligation.id));
            if let Some(prior) = prior {
                obligation.accrued = prior.accrued;
                obligation.share_remainder = prior.share_remainder;
                obligation.share_scale = prior.share_scale;
                obligation.performed = if same_duty(&prior.term, &obligation.term) {
                    prior.performed.min(obligation.amount())
                } else if matches!(
                    obligation.term,
                    AgreementTerm::Access { .. } | AgreementTerm::SiteAccess { .. }
                ) {
                    1
                } else {
                    0
                };
                obligation.defaulted = if prior.term == obligation.term {
                    prior.defaulted
                } else {
                    obligation.performed < obligation.amount()
                        && obligation
                            .term
                            .deadline()
                            .is_some_and(|d| d <= self.host.clock().time)
                        && obligation.term.parties()[0] == actor
                };
                obligation.cargo = if same_duty(&prior.term, &obligation.term) {
                    prior.cargo.clone()
                } else {
                    BTreeMap::new()
                };
                let mut remaining = obligation.amount() - obligation.performed;
                for quantity in obligation.cargo.values_mut() {
                    *quantity = (*quantity).min(remaining);
                    remaining -= *quantity;
                }
                obligation.cargo.retain(|_, quantity| *quantity > 0);
                obligation.observed_at = prior.observed_at;
                obligation.learned_at = prior.learned_at;
                obligation.source = prior.source.clone();
                if obligation.defaulted && prior.term != obligation.term {
                    obligation.observed_at = self.host.clock().time;
                    obligation.learned_at = self.host.clock().time;
                    obligation.source = Some(KnowledgeSource::Observed(actor));
                }
            } else {
                obligation.accrued = 0;
                obligation.share_remainder = 0;
                obligation.share_scale = 0;
                obligation.performed = if matches!(
                    obligation.term,
                    AgreementTerm::Access { .. } | AgreementTerm::SiteAccess { .. }
                ) {
                    obligation.performed
                } else {
                    0
                };
                obligation.defaulted = false;
                obligation.cargo.clear();
            }
        }
        if matches!(
            view.status,
            AgreementStatus::Active | AgreementStatus::Fulfilled | AgreementStatus::Breached
        ) {
            view.status = AgreementStatus::Active;
        }
        refresh_known_status(&mut view);
        self.store_agreement_knowledge(actor, view);
    }

    fn store_agreement_knowledge(&mut self, actor: PersonId, view: AgreementView) {
        let old = self
            .host
            .state
            .agreements
            .as_ref()
            .unwrap()
            .knowledge
            .get(&actor)
            .and_then(|known| known.get(&view.id));
        let failures: Vec<_> = view
            .obligations
            .iter()
            .filter(|o| {
                o.defaulted
                    && o.term.parties()[1] == actor
                    && !old.is_some_and(|known| {
                        known
                            .obligations
                            .iter()
                            .any(|prior| prior.id == o.id && prior.defaulted)
                    })
            })
            .map(|o| o.term.parties()[0])
            .collect();
        self.host
            .state
            .agreements
            .as_mut()
            .unwrap()
            .knowledge
            .entry(actor)
            .or_default()
            .insert(view.id, view);
        if let Some(mind) = self.host.state.decisions.as_mut().and_then(|_| {
            game_characters::decisions::mind_mut::<game_api::Command>(&mut self.host.storage, actor)
        }) {
            for debtor in failures {
                let loss = i32::from(mind.personality.honesty) / 100;
                let relationship =
                    mind.relationships
                        .entry(debtor)
                        .or_insert(decisions::Relationship {
                            trust: 0,
                            affection: 0,
                        });
                relationship.trust = (relationship.trust - loss).max(-1_000);
            }
        }
    }

    fn local_agreement_notice(
        &mut self,
        agreement: AgreementId,
        speaker: PersonId,
        amendment: Option<AmendmentId>,
    ) {
        let Some(area) = self.agreement_actor_area(speaker).ok() else {
            return;
        };
        let parties = self.host.state.agreements.as_ref().unwrap().agreements[&agreement]
            .parties
            .clone();
        for actor in parties
            .into_iter()
            .filter(|actor| self.agreement_actor_area(*actor).ok() == Some(area))
            .collect::<Vec<_>>()
        {
            self.notice_agreement(actor, agreement);
            if let Some(amendment) = amendment {
                let Some(proposal) = self
                    .host
                    .state
                    .agreements
                    .as_ref()
                    .unwrap()
                    .amendments
                    .get(&amendment)
                else {
                    continue;
                };
                let learned = AmendmentView {
                    id: proposal.id,
                    proposer: proposal.proposer,
                    replacements: proposal.replacements.clone(),
                    votes: proposal.votes.clone(),
                    status: proposal.status,
                    base_revision: proposal.base_revision,
                };
                let known = self
                    .host
                    .state
                    .agreements
                    .as_mut()
                    .unwrap()
                    .knowledge
                    .get_mut(&actor)
                    .unwrap()
                    .get_mut(&agreement)
                    .unwrap();
                if let Some(old) = known.amendments.iter_mut().find(|old| old.id == amendment) {
                    *old = learned;
                } else {
                    known.amendments.push(learned);
                    known.amendments.sort_by_key(|a| a.id);
                }
            }
        }
    }

    pub(crate) fn observe_agreement_events(&mut self) {
        let Some(world) = &self.host.state.agreements else {
            return;
        };
        let cursor = world.last_observation_event;
        let events: Vec<_> = self
            .host
            .events()
            .iter()
            .skip(cursor as usize)
            .cloned()
            .collect();
        for event in events {
            match event.event {
                EventKind::AgreementProposed {
                    agreement,
                    proposer,
                } if event.at == self.host.clock().time => {
                    self.local_agreement_notice(agreement, proposer, None);
                }
                EventKind::AgreementActivated { agreement }
                    if event.at == self.host.clock().time =>
                {
                    let proposer = self.host.state.agreements.as_ref().unwrap().agreements
                        [&agreement]
                        .proposer;
                    self.local_agreement_notice(agreement, proposer, None);
                }
                EventKind::AgreementRejected { agreement, actor }
                    if event.at == self.host.clock().time =>
                {
                    self.local_agreement_notice(agreement, actor, None);
                }
                EventKind::ObligationAccrued {
                    obligation, actor, ..
                }
                | EventKind::ObligationPerformed {
                    obligation, actor, ..
                } => {
                    self.observe_obligation_fact(obligation, actor, event.at);
                }
                EventKind::ObligationDefaulted { obligation } => {
                    let actor = self.host.state.agreements.as_ref().unwrap().obligations
                        [&obligation]
                        .term
                        .parties()[0];
                    self.observe_obligation_fact(obligation, actor, event.at);
                }
                _ => {}
            }
        }
        self.host
            .state
            .agreements
            .as_mut()
            .unwrap()
            .last_observation_event = self.host.next_event() - 1;
        // A person knows the cargo and work they themselves actually performed.
        // This does not reveal amended duties they have not yet been told about.
        let actors: Vec<_> = self
            .host
            .state
            .agreements
            .as_ref()
            .unwrap()
            .knowledge
            .iter()
            .filter(|(_, known)| !known.is_empty())
            .map(|(actor, _)| *actor)
            .collect();
        for actor in actors {
            if !game_physiology::body(&self.host.storage, actor)
                .expect("body exists")
                .vitals
                .alive
                || game_physiology::body(&self.host.storage, actor)
                    .expect("body exists")
                    .vitals
                    .health
                    == 0
            {
                continue;
            }
            let mut known = self.agreement_view_unbounded(actor);
            for view in &mut known {
                for fact in &mut view.obligations {
                    if fact.term.parties()[0] != actor {
                        continue;
                    }
                    if let Some(actual) = self
                        .host
                        .state
                        .agreements
                        .as_ref()
                        .unwrap()
                        .obligations
                        .get(&fact.id)
                        && actual.term == fact.term
                        && (fact.performed != actual.performed
                            || fact.defaulted != actual.defaulted
                            || fact.cargo != actual.cargo)
                    {
                        fact.performed = actual.performed;
                        fact.defaulted = actual.defaulted;
                        fact.cargo = actual.cargo.clone();
                        fact.observed_at = self.host.clock().time;
                        fact.learned_at = self.host.clock().time;
                        fact.source = Some(KnowledgeSource::Observed(actor));
                    }
                }
                refresh_known_status(view);
                self.store_agreement_knowledge(actor, view.clone());
            }
        }
    }

    fn agreement_view_unbounded(&self, actor: PersonId) -> Vec<AgreementView> {
        self.host
            .state
            .agreements
            .as_ref()
            .and_then(|world| world.knowledge.get(&actor))
            .map_or_else(Vec::new, |known| known.values().cloned().collect())
    }

    fn observe_obligation_fact(
        &mut self,
        obligation: ObligationId,
        performer: PersonId,
        at: SimTime,
    ) {
        let Some(actual) = self
            .host
            .state
            .agreements
            .as_ref()
            .unwrap()
            .obligations
            .get(&obligation)
            .cloned()
        else {
            return;
        };
        let area = self.agreement_actor_area(performer).ok();
        let observers: Vec<_> = self
            .host
            .state
            .agreements
            .as_ref()
            .unwrap()
            .knowledge
            .iter()
            .filter_map(|(actor, known)| {
                known
                    .get(&actual.agreement)
                    .is_some_and(|view| view.obligations.iter().any(|fact| fact.id == obligation))
                    .then_some(*actor)
            })
            .filter(|actor| {
                game_physiology::body(&self.host.storage, *actor)
                    .expect("body exists")
                    .vitals
                    .alive
                    && game_physiology::body(&self.host.storage, *actor)
                        .expect("body exists")
                        .vitals
                        .health
                        > 0
                    && (*actor == performer
                        || at == self.host.clock().time
                            && area.is_some()
                            && self.agreement_actor_area(*actor).ok() == area)
            })
            .collect();
        for actor in observers {
            let mut view = self.host.state.agreements.as_ref().unwrap().knowledge[&actor]
                [&actual.agreement]
                .clone();
            let fact = view
                .obligations
                .iter_mut()
                .find(|fact| fact.id == obligation)
                .unwrap();
            if fact.term != actual.term {
                continue;
            }
            fact.accrued = actual.accrued;
            fact.share_remainder = actual.share_remainder;
            fact.share_scale = actual.share_scale;
            fact.performed = actual.performed;
            fact.defaulted = actual.defaulted;
            fact.cargo = if actual.term.parties()[0] == actor {
                actual.cargo.clone()
            } else {
                fact.cargo.clone()
            };
            fact.observed_at = at;
            fact.learned_at = self.host.clock().time;
            fact.source = Some(KnowledgeSource::Observed(actor));
            view.observed_at = at;
            view.learned_at = self.host.clock().time;
            refresh_known_status(&mut view);
            self.store_agreement_knowledge(actor, view);
        }
    }

    pub(crate) fn start_agreement_report(
        &mut self,
        sender: PersonId,
        recipient: PersonId,
        agreement: AgreementId,
    ) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        let area = self.agreement_actor_area(sender)?;
        if sender == recipient
            || self.agreement_actor_area(recipient)? != area
            || self.care_index.contains_key(&sender)
            || !matches!(
                *self.host.storage.activity(recipient).expect("body exists"),
                Activity::Idle | Activity::Rest
            )
        {
            return Err(Error::Unavailable);
        }
        let snapshot = self
            .host
            .state
            .agreements
            .as_ref()
            .and_then(|world| world.knowledge.get(&sender))
            .and_then(|known| known.get(&agreement))
            .cloned()
            .ok_or(Error::Unavailable)?;
        let completes = SimTime(
            self.host
                .clock()
                .time
                .0
                .checked_add(MINUTE.0)
                .ok_or(Error::Overflow)?,
        );
        self.ensure_event_capacity(1)?;
        self.interrupt_actor_activity(sender);
        *self.host.storage.activity_mut(sender).unwrap() = Activity::ReportAgreement {
            recipient,
            snapshot: Box::new(snapshot),
            completes,
        };
        self.host
            .state
            .due
            .entry(completes)
            .or_default()
            .push(Due::Physical(sender));
        self.host.bump_revision(1)?;
        Ok(())
    }

    pub(crate) fn complete_agreement_report(&mut self, sender: PersonId) {
        let Activity::ReportAgreement {
            recipient,
            snapshot,
            ..
        } = self
            .host
            .storage
            .activity(sender)
            .expect("body exists")
            .clone()
        else {
            return;
        };
        if self.agreement_actor_area(sender).ok() == self.agreement_actor_area(recipient).ok()
            && self.agreement_actor_area(sender).is_ok()
            && matches!(
                *self.host.storage.activity(recipient).expect("body exists"),
                Activity::Idle | Activity::Rest
            )
        {
            let agreement = snapshot.id;
            self.receive_agreement_report(sender, recipient, *snapshot);
            self.emit(EventKind::AgreementReported {
                sender,
                recipient,
                agreement,
            });
        } else {
            self.emit(EventKind::Interrupted { actor: sender });
        }
        *self.host.storage.activity_mut(sender).unwrap() = Activity::Idle;
    }

    fn receive_agreement_report(
        &mut self,
        sender: PersonId,
        recipient: PersonId,
        mut incoming: AgreementView,
    ) {
        incoming.learned_at = self.host.clock().time;
        incoming.source = Some(KnowledgeSource::Reported(sender));
        for obligation in &mut incoming.obligations {
            obligation.learned_at = self.host.clock().time;
            obligation.source = Some(KnowledgeSource::Reported(sender));
        }
        if let Some(old) = self
            .host
            .state
            .agreements
            .as_ref()
            .unwrap()
            .knowledge
            .get(&recipient)
            .and_then(|known| known.get(&incoming.id))
            .cloned()
        {
            merge_agreement_knowledge(old, &mut incoming);
        }
        refresh_known_status(&mut incoming);
        self.store_agreement_knowledge(recipient, incoming);
    }

    pub(crate) fn agreement_accepts_delivery(
        &self,
        actor: PersonId,
        destination: InventoryId,
        quantity: Quantity,
    ) -> bool {
        let Some(w) = &self.host.state.agreements else {
            return false;
        };
        let Some(inventory) = game_cargo::inventory(&self.host.storage, destination) else {
            return false;
        };
        let Ok(area) = self.agreement_actor_area(actor) else {
            return false;
        };
        if self.inventory_area(inventory) != Some(area) {
            return false;
        }
        w.obligations.values().any(|o| {
            let (debtor, creditor, goal, promised) = match o.term {
                AgreementTerm::Delivery {
                    debtor,
                    creditor,
                    destination,
                    quantity,
                    ..
                } => (debtor, creditor, destination, quantity.0),
                AgreementTerm::OutputShare {
                    debtor,
                    creditor,
                    destination,
                    ..
                } => (debtor, creditor, destination, o.accrued),
                _ => return false,
            };
            debtor == actor
                && goal == destination
                && matches!(
                    w.agreements[&o.agreement].status,
                    AgreementStatus::Active | AgreementStatus::Breached
                )
                && quantity.0 <= promised - o.performed
                && (inventory.location == InventoryLocation::Carried(creditor)
                    || w.custodians.get(&destination) == Some(&creditor))
        })
    }

    pub(crate) fn available_agreement_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(_world) = &self.host.state.agreements else {
            return Vec::new();
        };
        let Ok(area) = self.agreement_actor_area(actor) else {
            return Vec::new();
        };
        let mut commands = Vec::new();
        let known = self.agreement_view(actor);
        for a in known
            .iter()
            .filter(|a| a.parties.contains(&actor) && a.status == AgreementStatus::Proposed)
            .take(32)
        {
            if self.agreement_actor_area(a.proposer).ok() != Some(area) {
                continue;
            }
            if a.proposer == actor {
                commands.push(Command::Agreement {
                    action: AgreementAction::Withdraw { agreement: a.id },
                });
            } else {
                commands.push(Command::Agreement {
                    action: AgreementAction::Reject { agreement: a.id },
                });
            }
            if !a.accepted.contains(&actor)
                && a.terms.iter().all(|term| {
                    !term
                        .deadline()
                        .is_some_and(|deadline| deadline <= self.host.clock().time)
                })
            {
                commands.push(Command::Agreement {
                    action: AgreementAction::Accept { agreement: a.id },
                });
            }
        }
        for (a, m) in known
            .iter()
            .filter(|a| a.parties.contains(&actor))
            .flat_map(|a| a.amendments.iter().map(move |m| (a, m)))
            .filter(|(_, m)| m.status == AmendmentStatus::Proposed)
            .take(32)
        {
            let Some(p) = &a.procedure else { continue };
            if m.base_revision != a.revision
                || m.votes.contains_key(&actor)
                || self.agreement_actor_area(m.proposer).ok() != Some(area)
                || (!p.approvers.contains(&actor)
                    && !p.term_approvers.values().any(|ids| ids.contains(&actor)))
            {
                continue;
            }
            commands.push(Command::Agreement {
                action: AgreementAction::Vote {
                    amendment: m.id,
                    support: true,
                },
            });
            commands.push(Command::Agreement {
                action: AgreementAction::Vote {
                    amendment: m.id,
                    support: false,
                },
            });
        }
        commands
    }

    /// Test controls cannot substitute for an NPC's ordinary willingness evaluation.
    pub fn debug_agreement_action(
        &mut self,
        actor: PersonId,
        action: AgreementAction,
    ) -> Result<(), Error> {
        self.agreement_command(actor, &action)
    }

    /// Projects only actual contact and referenced assets. The agreement kernel
    /// owns consent, chosen procedures, revision and obligation transitions;
    /// this adapter owns global IDs, physical permissions and observations.
    pub(crate) fn agreement_command(
        &mut self,
        actor: PersonId,
        action: &AgreementAction,
    ) -> Result<(), Error> {
        use game_agreements::{TransitionFacts, TransitionIds};
        let area = self.agreement_actor_area(actor)?;
        self.reserve_agreement_ids(0)?;
        let world = self
            .host
            .state
            .agreements
            .as_ref()
            .ok_or(Error::Unavailable)?;
        if let AgreementAction::Report {
            agreement,
            recipient,
        } = action
        {
            return self.start_agreement_report(actor, *recipient, *agreement);
        }
        let requirements = world.ledger.transition_requirements(action)?;
        let mut relevant = requirements.contact_people;
        relevant.insert(actor);
        let facts = TransitionFacts {
            actor_available: true,
            present: relevant
                .into_iter()
                .filter(|id| self.agreement_actor_area(*id).ok() == Some(area))
                .collect(),
            assets: self.economic_terms_context(&requirements.terms),
        };
        let needs = requirements.allocation;
        let count = u64::try_from(needs.obligations).map_err(|_| Error::Overflow)?
            + u64::from(needs.agreement)
            + u64::from(needs.amendment);
        self.reserve_agreement_ids(count)?;
        let mut next = self.host.next_id();
        let mut ids = TransitionIds::default();
        if needs.agreement {
            ids.agreement = Some(AgreementId(next));
            next += 1;
        }
        if needs.amendment {
            ids.amendment = Some(AmendmentId(next));
            next += 1;
        }
        ids.obligations = (next..next + needs.obligations as u64)
            .map(ObligationId)
            .collect();
        next += needs.obligations as u64;
        let prepared = world
            .ledger
            .prepare_transition(actor, action, &facts, &ids)?;
        self.apply_prepared_agreement(actor, prepared, next)
    }

    pub(crate) fn prepare_service_agreement(
        &self,
        consent: &game_missions::ServiceConsent,
    ) -> Result<(game_agreements::PreparedTransition, u64), Error> {
        let area = self.agreement_actor_area(consent.recruit)?;
        if self.agreement_actor_area(consent.sponsor)? != area {
            return Err(Error::Unavailable);
        }
        let offer = self
            .host
            .state
            .missions
            .as_ref()
            .and_then(|w| w.recruitments.get(&consent.recruitment))
            .ok_or(Error::Unavailable)?;
        let mission = &self.host.state.missions.as_ref().unwrap().missions[&offer.mission];
        if consent.agreement.0 != self.host.next_id()
            || offer.person != consent.recruit
            || mission.sponsor != consent.sponsor
        {
            return Err(Error::Unavailable);
        }
        let count = 1 + consent.terms.len() as u64;
        self.reserve_agreement_ids(count)?;
        let next = self.host.next_id() + count;
        let facts = game_agreements::TransitionFacts {
            actor_available: true,
            present: BTreeSet::from([consent.sponsor, consent.recruit]),
            assets: self.economic_terms_context(&consent.terms),
        };
        let ids = game_agreements::TransitionIds {
            agreement: Some(consent.agreement),
            obligations: (self.host.next_id() + 1..next).map(ObligationId).collect(),
            ..Default::default()
        };
        let accepted = game_agreements::HeardServiceConsent {
            recruitment: consent.recruitment,
            sponsor: consent.sponsor,
            recruit: consent.recruit,
            offered_terms: offer.compensation.clone(),
            accepted_terms: consent.terms.clone(),
            unanswered_counteroffer: offer.countered,
        };
        let prepared = self
            .host
            .state
            .agreements
            .as_ref()
            .ok_or(Error::Unavailable)?
            .ledger
            .prepare_service_commitment(&accepted, &facts, &ids)?;
        Ok((prepared, next))
    }

    pub(crate) fn apply_prepared_agreement(
        &mut self,
        actor: PersonId,
        prepared: game_agreements::PreparedTransition,
        next: u64,
    ) -> Result<(), Error> {
        use game_agreements::TransitionEvent;
        prepared.check(
            &self
                .host
                .state
                .agreements
                .as_ref()
                .ok_or(Error::Unavailable)?
                .ledger,
        )?;
        self.ensure_event_capacity(prepared.events.len())?;
        let permissions = prepared.permissions.clone();
        let events = prepared.events.clone();
        let notices = prepared.notices.clone();
        // All fallible work is complete. Asset owners were checked in the same
        // opening snapshot and these effects only alter existing access sets.
        prepared.commit(&mut self.host.state.agreements.as_mut().unwrap().ledger)?;
        for permission in permissions {
            self.apply_permission_effect(permission);
        }
        self.host.allocate_ids(
            (next)
                .checked_sub(self.host.next_id())
                .ok_or(Error::Overflow)?,
        )?;
        for event in events {
            self.emit(match event {
                TransitionEvent::Proposed {
                    agreement,
                    proposer,
                } => EventKind::AgreementProposed {
                    agreement,
                    proposer,
                },
                TransitionEvent::Activated { agreement } => {
                    EventKind::AgreementActivated { agreement }
                }
                TransitionEvent::Rejected { agreement, actor } => {
                    EventKind::AgreementRejected { agreement, actor }
                }
                TransitionEvent::Amended {
                    agreement,
                    amendment,
                } => EventKind::AgreementAmended {
                    agreement,
                    amendment,
                },
                TransitionEvent::Performed {
                    obligation,
                    actor,
                    quantity,
                } => EventKind::ObligationPerformed {
                    obligation,
                    actor,
                    quantity,
                },
            });
        }
        for notice in notices {
            self.local_agreement_notice(notice.agreement, actor, notice.amendment);
        }
        self.observe_agreement_events();
        Ok(())
    }

    fn reserve_agreement_ids(&self, count: u64) -> Result<(), Error> {
        self.host
            .next_id()
            .checked_add(count)
            .ok_or(Error::Overflow)?;
        let pending = self
            .host
            .state
            .agreements
            .as_ref()
            .map_or(0, |w| w.pending_work.len() as u64);
        self.host
            .next_event()
            .checked_add(
                count
                    .checked_add(pending)
                    .and_then(|v| v.checked_add(2))
                    .ok_or(Error::Overflow)?,
            )
            .ok_or(Error::Overflow)?;
        Ok(())
    }

    pub(crate) fn preflight_output_shares(
        &self,
        production: &[(SiteId, Good, Quantity)],
    ) -> Result<(), Error> {
        let Some(world) = &self.host.state.agreements else {
            return Ok(());
        };
        for obligation in world.obligations.values() {
            if let AgreementTerm::OutputShare {
                site,
                good,
                numerator,
                denominator,
                ..
            } = obligation.term
            {
                let total = production
                    .iter()
                    .filter(|(s, g, _)| *s == site && *g == good)
                    .try_fold(0_u128, |total, (_, _, q)| {
                        total.checked_add(u128::from(q.0))
                    })
                    .ok_or(Error::Overflow)?;
                accrue_share(
                    obligation.accrued,
                    obligation.share_remainder,
                    obligation.share_scale,
                    numerator,
                    denominator,
                    total,
                )?;
            }
        }
        Ok(())
    }
    pub(crate) fn closing_agreement_demand(
        &self,
        at: SimTime,
        due: &[Due],
        outputs: &[(SiteId, Good, Quantity)],
    ) -> Result<(game_agreements::ClosingReceiptDemand, u64), Error> {
        let Some(world) = &self.host.state.agreements else {
            return Ok((Default::default(), 0));
        };
        let mut receipts = Vec::new();
        let mut direct = outputs.len() as u64;
        for event in self.host.events().iter().skip(world.last_event as usize) {
            match event.event {
                EventKind::Transferred {
                    actor,
                    source,
                    destination,
                    ..
                } => receipts.push(game_agreements::ClosingReceipt::Transfer {
                    actor,
                    source,
                    destination,
                }),
                EventKind::Produced {
                    actor,
                    site,
                    output,
                    ..
                } => receipts.push(game_agreements::ClosingReceipt::Production {
                    actor: Some(actor),
                    site,
                    good: game_cargo::inventory(&self.host.storage, output)
                        .expect("inventory exists")
                        .good,
                }),
                EventKind::CropHarvested { actor, field, .. } => {
                    for good in [Good::Food, Good::Seed] {
                        receipts.push(game_agreements::ClosingReceipt::Production {
                            actor: Some(actor),
                            site: field,
                            good,
                        });
                    }
                }
                _ => {}
            }
        }
        for (site, good, _) in outputs {
            receipts.push(game_agreements::ClosingReceipt::Production {
                actor: None,
                site: *site,
                good: *good,
            });
        }
        for event in due {
            let transfers = match event {
                Due::Transfers(transfers) => transfers.clone(),
                Due::Physical(actor) => {
                    match *self.host.storage.activity(*actor).expect("body exists") {
                        Activity::Load {
                            source,
                            destination,
                            quantity,
                            ..
                        } => vec![TransferIntent {
                            actor: *actor,
                            source,
                            destination,
                            quantity,
                        }],
                        Activity::Eat { .. } => {
                            direct = direct.checked_add(1).ok_or(Error::Overflow)?;
                            Vec::new()
                        }
                        _ => Vec::new(),
                    }
                }
                _ => Vec::new(),
            };
            for transfer in transfers {
                direct = direct.checked_add(1).ok_or(Error::Overflow)?;
                receipts.push(game_agreements::ClosingReceipt::Transfer {
                    actor: transfer.actor,
                    source: transfer.source,
                    destination: transfer.destination,
                });
            }
        }
        let work = self
            .host
            .state
            .physical
            .as_ref()
            .into_iter()
            .flat_map(|_| game_physiology::bodies(&self.host.storage))
            .filter_map(|(actor, body)| {
                let actor = &actor;
                let activity = self.host.storage.activity(*actor).expect("activity exists");
                if at <= self.host.clock().time || !body.vitals.alive || body.vitals.health == 0 {
                    return None;
                }
                let target = match *activity {
                    Activity::Work { site } => game_agreements::WorkTarget::Site(site),
                    Activity::Cultivate { field, .. } => game_agreements::WorkTarget::Site(field),
                    Activity::Construct { project, .. } => {
                        game_agreements::WorkTarget::Project(project)
                    }
                    _ => return None,
                };
                Some(game_agreements::WorkAssignment {
                    actor: *actor,
                    target,
                })
            })
            .collect::<Vec<_>>();
        Ok((
            world.ledger.closing_receipt_demand(at, &receipts, &work)?,
            direct,
        ))
    }

    pub(crate) fn preflight_agreements(&self) -> Result<(), Error> {
        let events = if self.host.state.agreements.as_ref().is_some_and(|world| {
            !world.ledger.obligations.is_empty() || !world.ledger.pending_work.is_empty()
        }) {
            self.closing_agreement_demand(self.host.clock().time, &[], &[])?
                .0
                .events()?
        } else {
            0
        };
        self.host
            .next_event()
            .checked_add(events.checked_add(2).ok_or(Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        Ok(())
    }

    fn agreement_actor_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        let p = self
            .person_index
            .get(&actor)
            .map(|i| &self.host.state.people[*i])
            .ok_or(Error::Unavailable)?;
        let alive = self.host.state.physical.as_ref().is_some_and(|_| {
            game_physiology::body(&self.host.storage, actor)
                .expect("body exists")
                .vitals
                .alive
        });
        if !alive
            || !self.independent(actor)
            || self.in_combat(actor)
            || game_physiology::body(&self.host.storage, actor)
                .expect("body exists")
                .vitals
                .health
                == 0
        {
            return Err(Error::Unavailable);
        }
        match *game_spatial::location(&self.host.storage, game_ecs::StableId((p.id).0))
            .ok_or(Error::Unavailable)?
        {
            Location::AtArea(area) => Ok(area),
            _ => Err(Error::Unavailable),
        }
    }

    pub(crate) fn economic_terms_context(
        &self,
        terms: &[AgreementTerm],
    ) -> game_agreements::TermsContext {
        use game_agreements::{InventoryTerms, PermissionAsset, TermsContext};
        let mut context = TermsContext {
            now: self.host.clock().time,
            ..TermsContext::default()
        };
        for term in terms {
            let inventories = match *term {
                AgreementTerm::Delivery {
                    source,
                    destination,
                    ..
                } => vec![source, destination],
                AgreementTerm::Access { inventory, .. } => vec![inventory],
                AgreementTerm::OutputShare { destination, .. } => vec![destination],
                _ => Vec::new(),
            };
            for id in inventories {
                if let Some(stock) = game_cargo::inventory(&self.host.storage, id) {
                    context.inventories.insert(
                        id,
                        InventoryTerms {
                            good: stock.good,
                            capacity: stock.capacity,
                        },
                    );
                }
            }
            match *term {
                AgreementTerm::Labor { site, .. } => {
                    if site_exists(&self.canonical_state(), site) {
                        context.sites.insert(site);
                    }
                }
                AgreementTerm::ConstructionLabor { project, .. } => {
                    if self.host.state.projects.as_ref().is_some_and(|_| {
                        game_projects::project(&self.host.storage, project).is_some()
                    }) {
                        context.projects.insert(project);
                    }
                }
                AgreementTerm::Access {
                    grantor, inventory, ..
                } => {
                    if game_cargo::inventory(&self.host.storage, inventory).is_some_and(|i| {
                        self.host
                            .state
                            .agreements
                            .as_ref()
                            .unwrap()
                            .custodians
                            .get(&inventory)
                            == Some(&grantor)
                            || i.location == InventoryLocation::Carried(grantor)
                    }) {
                        context
                            .grants
                            .insert((PermissionAsset::Inventory(inventory), grantor));
                    }
                }
                AgreementTerm::SiteAccess { grantor, site, .. } => {
                    if site_custodian(&self.canonical_state(), site) == Some(grantor) {
                        context
                            .grants
                            .insert((PermissionAsset::Site(site), grantor));
                    }
                }
                AgreementTerm::OutputShare { site, good, .. } => {
                    if let Some(source) = output_source(&self.canonical_state(), site, good) {
                        context.outputs.insert((site, good), source);
                    }
                }
                _ => {}
            }
        }
        context
    }
    fn apply_permission_effect(&mut self, effect: game_agreements::PermissionEffect) {
        use game_agreements::PermissionAsset;
        let access = match effect.asset {
            PermissionAsset::Inventory(inventory) => Some(
                &mut game_cargo::inventory_mut(&mut self.host.storage, inventory)
                    .unwrap()
                    .access,
            ),
            PermissionAsset::Site(site) => {
                let cultivated = self
                    .host
                    .state
                    .agriculture
                    .as_ref()
                    .is_some_and(|world| world.field(&self.host.storage, site).is_some());
                if cultivated {
                    self.host
                        .state
                        .agriculture
                        .as_ref()
                        .and_then(|world| world.field_mut(&mut self.host.storage, site))
                        .map(|field| &mut field.access)
                } else {
                    self.host
                        .state
                        .ecology
                        .as_ref()
                        .and_then(|world| world.plot_mut(&mut self.host.storage, site))
                        .map(|plot| &mut plot.access)
                }
            }
        };
        if let Some(access) = access {
            if effect.allowed {
                access.insert(effect.grantee);
            } else {
                access.remove(&effect.grantee);
            }
        }
    }
    fn refresh_agreement_status(&mut self, id: AgreementId) {
        let owner_now = self.host.clock().time;
        self.host
            .state
            .agreements
            .as_mut()
            .unwrap()
            .ledger
            .refresh_status(id, owner_now)
            .expect("validated agreement ledger");
    }

    pub(crate) fn next_agreement_deadline(&self) -> Option<SimTime> {
        self.host
            .state
            .agreements
            .as_ref()?
            .obligations
            .values()
            .flat_map(|o| {
                [
                    (!o.defaulted && o.performed < o.amount())
                        .then(|| o.term.deadline())
                        .flatten(),
                    match o.term {
                        AgreementTerm::OutputShare { ends_at, .. } => Some(ends_at),
                        _ => None,
                    },
                ]
                .into_iter()
                .flatten()
            })
            .filter(|d| *d > self.host.clock().time)
            .min()
    }

    /// Physical eligibility comes from the coordinator; economic allocation is
    /// performed by the independent ledger against this exact clock interval.
    pub(crate) fn record_agreement_labor_batch(
        &mut self,
        from: SimTime,
        to: SimTime,
        assignments: &[game_agreements::WorkAssignment],
    ) -> Result<(), Error> {
        let Some(world) = self.host.state.agreements.as_mut() else {
            return Ok(());
        };
        let prepared = world.ledger.prepare_labor(from, to, assignments)?;
        prepared.commit(&mut world.ledger)
    }
    pub(crate) fn record_agreement_work(&mut self, actor: PersonId, site: SiteId, elapsed: u64) {
        let now = self.host.clock().time;
        self.record_agreement_labor_batch(
            SimTime(now.0 - elapsed),
            now,
            &[game_agreements::WorkAssignment {
                actor,
                target: game_agreements::WorkTarget::Site(site),
            }],
        )
        .expect("preflight eligible site work");
    }
    pub(crate) fn record_agreement_construction(
        &mut self,
        actor: PersonId,
        project: ProjectId,
        elapsed: u64,
    ) {
        let now = self.host.clock().time;
        self.record_agreement_labor_batch(
            SimTime(now.0 - elapsed),
            now,
            &[game_agreements::WorkAssignment {
                actor,
                target: game_agreements::WorkTarget::Project(project),
            }],
        )
        .expect("preflight eligible construction work");
    }

    fn flush_agreement_work(&mut self) {
        let pending =
            std::mem::take(&mut self.host.state.agreements.as_mut().unwrap().pending_work);
        for (obligation, quantity) in pending {
            let actor = self.host.state.agreements.as_ref().unwrap().obligations[&obligation]
                .term
                .parties()[0];
            self.emit(EventKind::ObligationPerformed {
                obligation,
                actor,
                quantity: Quantity(quantity),
            });
        }
    }

    /// Observes conserved physical events; signatures alone never discharge deliveries.
    pub(crate) fn reconcile_agreements(&mut self) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        let Some(world) = &self.host.state.agreements else {
            return Ok(());
        };
        let cursor = world.last_event;
        let events: Vec<_> = self
            .host
            .events()
            .iter()
            .skip(cursor as usize)
            .cloned()
            .collect();
        let flush_work = owner_now.0.is_multiple_of(60_000)
            || events.iter().any(|e| {
                e.at == owner_now
                    && matches!(
                        e.event,
                        EventKind::Produced { .. } | EventKind::ProjectCompleted { .. }
                    )
            })
            || world
                .obligations
                .values()
                .any(|o| o.term.deadline() == Some(owner_now));
        let last_event = self.host.next_event() - 1;
        let mut credits = Vec::new();
        let mut accruals = Vec::new();
        let share_origins = self
            .host
            .state
            .agreements
            .as_ref()
            .unwrap()
            .obligations
            .values()
            .filter_map(|o| match o.term {
                AgreementTerm::OutputShare { site, good, .. } => {
                    output_source(&self.canonical_state(), site, good).map(|i| (o.id, i))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        for e in events {
            let productions = match e.event {
                EventKind::Produced {
                    actor,
                    site,
                    output,
                    quantity,
                } => vec![(
                    actor,
                    site,
                    game_cargo::inventory(&self.host.storage, output)
                        .expect("inventory exists")
                        .good,
                    quantity,
                )],
                EventKind::CropHarvested {
                    actor,
                    field,
                    food,
                    seed,
                } => vec![
                    (actor, field, Good::Food, food),
                    (actor, field, Good::Seed, seed),
                ],
                _ => Vec::new(),
            };
            for (producer, site, good, quantity) in productions {
                let fact = game_agreements::ProductionFact {
                    actor: producer,
                    site,
                    good,
                    quantity,
                    time: e.at,
                };
                let plan = self
                    .host
                    .state
                    .agreements
                    .as_ref()
                    .unwrap()
                    .ledger
                    .prepare_production(fact)?;
                accruals.extend(
                    plan.credits
                        .iter()
                        .map(|c| (c.obligation, c.actor, c.quantity.0)),
                );
                plan.commit(&mut self.host.state.agreements.as_mut().unwrap().ledger)?;
            }
            match e.event {
                EventKind::Transferred {
                    actor,
                    source,
                    destination,
                    quantity,
                } => {
                    let carried_to_actor = game_cargo::inventory(&self.host.storage, destination)
                        .expect("inventory exists")
                        .location
                        == InventoryLocation::Carried(actor);
                    let mut unallocated = quantity.0;
                    let world = self.host.state.agreements.as_mut().unwrap();
                    for o in world.obligations.values_mut() {
                        let (debtor, origin, goal, promised) = match o.term {
                            AgreementTerm::Delivery {
                                debtor,
                                source,
                                destination,
                                quantity,
                                ..
                            } => (debtor, source, destination, quantity.0),
                            AgreementTerm::OutputShare {
                                debtor,
                                destination,
                                ..
                            } => (debtor, share_origins[&o.id], destination, o.accrued),
                            _ => continue,
                        };
                        let source_cargo = o.cargo.remove(&source).unwrap_or(0);
                        let from_cargo = source_cargo.min(unallocated);
                        if source_cargo > from_cargo {
                            o.cargo.insert(source, source_cargo - from_cargo);
                        }
                        unallocated -= from_cargo;
                        let direct = source == origin && actor == debtor;
                        let eligible = if direct { unallocated } else { from_cargo };
                        let outstanding = promised - o.performed;
                        if actor == debtor && destination == goal {
                            let credit = eligible.min(outstanding);
                            o.performed += credit;
                            if direct {
                                unallocated -= credit;
                            }
                            if credit > 0 {
                                credits.push((o.id, o.agreement, actor, credit));
                            }
                        } else if actor == debtor && carried_to_actor {
                            let already: u64 = o.cargo.values().sum();
                            let allocated = eligible.min(outstanding.saturating_sub(already));
                            if allocated > 0 {
                                *o.cargo.entry(destination).or_default() += allocated;
                                if direct {
                                    unallocated -= allocated;
                                }
                            }
                        }
                    }
                }
                EventKind::Cared {
                    source, quantity, ..
                }
                | EventKind::Ate {
                    source, quantity, ..
                }
                | EventKind::Consumed {
                    source, quantity, ..
                } => self.lose_agreement_cargo(source, quantity.0),
                EventKind::Spoiled {
                    inventory,
                    quantity,
                } => self.lose_agreement_cargo(inventory, quantity.0),
                _ => {}
            }
        }
        // Force and other physical actions can move a unique input without a
        // fungible consumption event. Cargo attribution cannot outlive the goods.
        let mut totals: BTreeMap<InventoryId, u64> = BTreeMap::new();
        for o in self
            .host
            .state
            .agreements
            .as_ref()
            .unwrap()
            .obligations
            .values()
        {
            for (inventory, quantity) in &o.cargo {
                *totals.entry(*inventory).or_default() += *quantity;
            }
        }
        for (inventory, attributed) in totals {
            let physical = game_cargo::inventory(&self.host.storage, inventory)
                .expect("inventory exists")
                .quantity
                .0;
            if attributed > physical {
                self.lose_agreement_cargo(inventory, attributed - physical);
            }
        }
        self.host.state.agreements.as_mut().unwrap().last_event = last_event;
        for (obligation, actor, quantity) in accruals {
            self.emit(EventKind::ObligationAccrued {
                obligation,
                actor,
                quantity: Quantity(quantity),
            });
        }
        if flush_work {
            self.flush_agreement_work();
        }
        for (obligation, agreement, actor, quantity) in credits {
            self.emit(EventKind::ObligationPerformed {
                obligation,
                actor,
                quantity: Quantity(quantity),
            });
            self.refresh_agreement_status(agreement);
        }
        let streams = self
            .host
            .state
            .agreements
            .as_ref()
            .unwrap()
            .agreements
            .values()
            .filter(|a| {
                a.terms
                    .iter()
                    .any(|t| matches!(t, AgreementTerm::OutputShare { .. }))
                    && !a.obligations.is_empty()
            })
            .map(|a| a.id)
            .collect::<Vec<_>>();
        for id in streams {
            self.refresh_agreement_status(id);
        }
        let defaults = self
            .host
            .state
            .agreements
            .as_ref()
            .unwrap()
            .ledger
            .defaults_due(owner_now)
            .into_iter()
            .map(|id| {
                (
                    id,
                    self.host.state.agreements.as_ref().unwrap().obligations[&id].agreement,
                )
            })
            .collect::<Vec<_>>();
        for (obligation, agreement) in defaults {
            self.host
                .state
                .agreements
                .as_mut()
                .unwrap()
                .ledger
                .mark_default(obligation, owner_now)?;
            self.emit(EventKind::ObligationDefaulted { obligation });
            self.refresh_agreement_status(agreement);
        }
        Ok(())
    }

    fn lose_agreement_cargo(&mut self, source: InventoryId, mut quantity: u64) {
        for o in self
            .host
            .state
            .agreements
            .as_mut()
            .unwrap()
            .obligations
            .values_mut()
        {
            if let Some(cargo) = o.cargo.get_mut(&source) {
                let lost = quantity.min(*cargo);
                *cargo -= lost;
                quantity -= lost;
            }
            o.cargo.retain(|_, quantity| *quantity > 0);
        }
    }

    pub(crate) fn review_agreements(&mut self) -> Result<(), Error> {
        let Some(world) = &self.host.state.agreements else {
            return Ok(());
        };
        let mut answers = Vec::new();
        for (actor, known) in &world.knowledge {
            if *actor == self.host.state.ruler {
                continue;
            }
            for a in known.values() {
                if !game_agreements::needs_proposal_response(*actor, a) {
                    continue;
                }
                if self.agreement_actor_area(*actor).ok()
                    != self.agreement_actor_area(a.proposer).ok()
                {
                    continue;
                }
                let score = self.agreement_offer_score(*actor, &a.terms)?;
                if let Some(answer) = game_agreements::proposal_answer(*actor, a, score, true) {
                    answers.push((*actor, answer));
                }
            }
        }
        for (actor, answer) in answers {
            let _ = self.agreement_command(actor, &answer);
        }
        let mut votes = Vec::new();
        for (actor, known) in &self.host.state.agreements.as_ref().unwrap().knowledge {
            if *actor == self.host.state.ruler {
                continue;
            }
            for a in known.values() {
                for m in game_agreements::pending_amendments(*actor, a) {
                    if self.agreement_actor_area(*actor).ok()
                        != self.agreement_actor_area(m.proposer).ok()
                    {
                        continue;
                    }
                    let revised = game_agreements::amended_terms(a, m)?;
                    let mind = &game_characters::decisions::mind::<game_api::Command>(
                        &self.host.storage,
                        *actor,
                    )
                    .unwrap();
                    let support = game_agreements::amendment_support(
                        self.agreement_offer_score(*actor, &a.terms)?,
                        self.agreement_offer_score(*actor, &revised)?,
                        mind.personality.empathy,
                    )?;
                    votes.push((
                        *actor,
                        AgreementAction::Vote {
                            amendment: m.id,
                            support,
                        },
                    ));
                }
            }
        }
        for (actor, vote) in votes {
            let _ = self.agreement_command(actor, &vote);
        }
        self.seek_local_exchange()?;
        self.perform_chosen_agreement_tasks();
        Ok(())
    }

    fn agreement_offer_score(
        &self,
        actor: PersonId,
        terms: &[AgreementTerm],
    ) -> Result<i64, Error> {
        let mind = game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
            .ok_or(Error::Unavailable)?;
        let mut shares = BTreeMap::new();
        for term in terms {
            if let AgreementTerm::OutputShare {
                site,
                numerator,
                denominator,
                ..
            } = *term
            {
                let fertility = game_ecology::knowledge(&self.host.storage, actor)
                    .and_then(|known| known.known_plots.get(&site))
                    .map(|plot| plot.soil_fertility);
                let remembered_plan = self
                    .host
                    .state
                    .projects
                    .as_ref()
                    .and_then(|_| {
                        game_projects::knowledge(&self.host.storage, actor).map(|row| &row.projects)
                    })
                    .is_some_and(|known| {
                        known.values().any(
                            |plan| matches!(plan.kind,ProjectKind::Farm {plot,..} if plot==site),
                        )
                    });
                let planned = if let Some(fertility) = fertility.filter(|_| remembered_plan) {
                    let agriculture = self
                        .host
                        .state
                        .agriculture
                        .as_ref()
                        .ok_or(Error::Unavailable)?;
                    let crop = &agriculture.catalog.crops[&agriculture.catalog.default_crop];
                    Some(game_agriculture::projected_food_share(
                        crop,
                        fertility,
                        numerator,
                        denominator,
                    )?)
                } else {
                    None
                };
                let established = self
                    .host
                    .state
                    .projects
                    .as_ref()
                    .and_then(|_| {
                        game_projects::knowledge(&self.host.storage, actor)
                            .map(|row| &row.enterprises)
                    })
                    .is_some_and(|known| known.values().any(|enterprise| enterprise.site == site))
                    || self.host.state.agriculture.as_ref().is_some_and(|world| {
                        world.field(&self.host.storage, site).is_some_and(|field| {
                            game_spatial::location(
                                &self.host.storage,
                                game_ecs::StableId((actor).0),
                            ) == Some(&Location::AtArea(field.area))
                        })
                    });
                let prospect = match planned {
                    Some(expected_share) => {
                        game_agreements::ShareProspect::PlannedFood { expected_share }
                    }
                    None if established => game_agreements::ShareProspect::Established,
                    None => game_agreements::ShareProspect::Unknown,
                };
                shares.insert((site, numerator, denominator), prospect);
            }
        }
        let area = self.agreement_actor_area(actor).ok();
        let speakers = terms
            .iter()
            .filter(|term| !matches!(term, AgreementTerm::Access { .. }))
            .map(AgreementTerm::parties)
            .filter(|[debtor, creditor]| *debtor != actor && *creditor == actor)
            .map(|[speaker, _]| {
                (
                    speaker,
                    game_agreements::SpeakerFacts {
                        locally_present: area.is_some()
                            && self.agreement_actor_area(speaker).ok() == area,
                        trust: mind
                            .relationships
                            .get(&speaker)
                            .map_or(0, |relation| relation.trust),
                        observed_invocation_penalty: if area.is_some()
                            && self.agreement_actor_area(speaker).ok() == area
                        {
                            self.invocation_social_penalty(speaker)
                        } else {
                            0
                        },
                    },
                )
            })
            .collect();
        let projects = self
            .host
            .state
            .projects
            .as_ref()
            .and_then(|_| {
                game_projects::knowledge(&self.host.storage, actor).map(|row| &row.projects)
            })
            .map(|known| known.keys().copied().collect())
            .unwrap_or_default();
        let facts = game_agreements::OfferActorFacts {
            actor,
            now: self.host.clock().time,
            hunger: game_physiology::body(&self.host.storage, actor)
                .ok_or(Error::Unavailable)?
                .vitals
                .hunger,
            empathy: mind.personality.empathy,
            risk_tolerance: mind.personality.risk_tolerance,
            ambition: mind.personality.ambition,
            hedonism: mind.personality.hedonism,
            financing_utility: self.project_financing_utility(actor, terms),
        };
        game_agreements::offer_score(
            facts,
            terms,
            &OfferKnowledgeFacts {
                stocks: &mind.knowledge,
                sites: &mind.sites,
                projects,
                shares,
                speakers,
            },
        )
    }

    fn seek_local_exchange(&mut self) -> Result<(), Error> {
        let world = self.host.state.agreements.as_ref().unwrap();
        let mut proposals = Vec::new();
        for (actor, mind) in
            game_characters::decisions::minds::<game_api::Command>(&self.host.storage)
        {
            let sites: Vec<_> = mind
                .sites
                .values()
                .map(|site| game_agreements::KnownSiteRight {
                    site: site.site,
                    area: site.area,
                    accessible: site.access,
                })
                .collect();
            let facts = game_agreements::LocalExchangeFacts {
                actor,
                autonomous: actor != self.host.state.ruler,
                area: self.agreement_actor_area(actor).ok(),
                hunger: game_physiology::body(&self.host.storage, actor)
                    .ok_or(Error::Unavailable)?
                    .vitals
                    .hunger,
                stocks: &mind.knowledge,
                sites: &sites,
                custodians: &world.custodians,
            };
            if let Some(proposal) =
                world
                    .ledger
                    .local_exchange(self.host.clock().time, facts, |owner| {
                        self.agreement_actor_area(owner).ok()
                    })
            {
                proposals.push(proposal);
            }
        }
        for proposal in proposals {
            self.agreement_command(proposal.actor, &proposal.action)?;
            self.host
                .state
                .agreements
                .as_mut()
                .unwrap()
                .next_search
                .insert(proposal.actor, proposal.next_search);
        }
        Ok(())
    }

    fn perform_chosen_agreement_tasks(&mut self) {
        let mut intentions = BTreeMap::new();
        let obligations: Vec<_> = self
            .host
            .state
            .agreements
            .as_ref()
            .unwrap()
            .knowledge
            .iter()
            .flat_map(|(actor, known)| {
                known
                    .values()
                    .filter(|a| {
                        matches!(
                            a.status,
                            AgreementStatus::Active | AgreementStatus::Breached
                        )
                    })
                    .flat_map(move |a| {
                        a.obligations
                            .iter()
                            .filter(move |o| o.term.parties()[0] == *actor)
                    })
            })
            .filter(|o| o.performed < o.amount())
            .cloned()
            .collect();
        for o in &obligations {
            let debtor = o.term.parties()[0];
            if debtor == self.host.state.ruler
                || intentions.contains_key(&debtor)
                || self.combat_restricts(debtor)
                || self.in_combat(debtor)
            {
                continue;
            }
            let Ok(area) = self.agreement_actor_area(debtor) else {
                continue;
            };
            let body = &game_physiology::body(&self.host.storage, debtor).expect("body exists");
            let mind =
                &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, debtor)
                    .unwrap();
            let rules = &self.host.state.physical.as_ref().unwrap().rules;
            if !matches!(
                *self.host.storage.activity(debtor).expect("activity exists"),
                Activity::Idle | Activity::Rest
            ) || body.vitals.hunger >= rules.meal_relief
                || body.vitals.fatigue >= rules.fatigue_limit / 2
                || (mind.personality.discipline < 500 && mind.personality.honesty < 500)
            {
                continue;
            }
            let task = match o.term {
                AgreementTerm::OutputShare {
                    debtor,
                    creditor,
                    site,
                    good,
                    destination,
                    pay_by,
                    ..
                } => {
                    let Some(source) = output_source(&self.canonical_state(), site, good) else {
                        continue;
                    };
                    AgreementTerm::Delivery {
                        debtor,
                        creditor,
                        source,
                        destination,
                        quantity: Quantity(o.accrued),
                        deadline: pay_by,
                    }
                }
                ref term => term.clone(),
            };
            let command = match task {
                AgreementTerm::ConstructionLabor { project, .. } => self
                    .construction_possible(debtor, project)
                    .then_some(Command::Project {
                        action: ProjectAction::Work { project },
                    }),
                AgreementTerm::Labor { site, .. } => {
                    if self
                        .host
                        .state
                        .agriculture
                        .as_ref()
                        .is_some_and(|w| w.field(&self.host.storage, site).is_some())
                    {
                        self.available_cultivation(debtor)
                            .into_iter()
                            .find(|c| matches!(c,Command::Cultivate {field,..} if *field==site))
                    } else {
                        mind.sites.get(&site).filter(|s| s.access).and_then(|s| {
                            if s.area == area {
                                Some(Command::Work { site })
                            } else {
                                decisions::known_path(&mind.routes, area, s.area)
                                    .ok()
                                    .map(|_| Command::Move {
                                        destination: s.area,
                                    })
                            }
                        })
                    }
                }
                AgreementTerm::Delivery {
                    source,
                    destination,
                    quantity,
                    ..
                } => {
                    let good = match o.term {
                        AgreementTerm::OutputShare { good, .. } => Some(good),
                        _ => mind.knowledge.get(&source).map(|known| known.good),
                    };
                    good.and_then(|good| {
                        let mut goal =
                            self.delivery_known_inventory_destination(debtor, destination)?;
                        // The debtor remembers an adopted obligation that accepts this
                        // delivery. This intention is not fresh remote stock access.
                        goal.accessible = true;
                        let delivery = game_logistics::Delivery {
                            requirement: game_logistics::Requirement {
                                good,
                                quantity,
                                source: Some(source),
                            },
                            destination: game_logistics::Destination::Inventory(destination),
                            remaining: Quantity(quantity.0.saturating_sub(o.performed)),
                            last_receipt: 0,
                        };
                        let tags = o.cargo.iter().map(|(id, q)| (*id, Quantity(*q))).collect();
                        let step = self.plan_delivery_with_source_cargo(
                            debtor,
                            &delivery,
                            &goal,
                            Quantity(1),
                            &tags,
                        );
                        self.delivery_command(debtor, step)
                    })
                }
                _ => None,
            };
            if let Some(command) = command {
                intentions.insert(debtor, command);
            }
        }
        for (actor, command) in intentions {
            if let Command::Move { destination } = command {
                let _ = self.start_movement(actor, destination);
            } else {
                let _ = self.debug_actor_command(actor, command);
            }
        }
    }
}

#[cfg(test)]
mod share_tests {
    use super::*;
    #[test]
    fn changing_future_fraction_retains_the_fraction_already_owed() {
        let (amount, remainder, scale) = accrue_share(0, 0, 0, 1, 3, 1).unwrap();
        assert_eq!((amount, remainder, scale), (0, 1, 3));
        let (amount, remainder, scale) = accrue_share(amount, remainder, scale, 1, 2, 1).unwrap();
        assert_eq!((amount, remainder, scale), (0, 5, 6));
        assert_eq!(
            accrue_share(amount, remainder, scale, 1, 2, 1).unwrap(),
            (1, 1, 3)
        );
    }
    #[test]
    fn many_small_physical_batches_owe_the_same_share_as_one_batch() {
        let mut split = (0, 0, 0);
        for quantity in [7, 1, 3, 8, 2] {
            split = accrue_share(split.0, split.1, split.2, 2, 7, quantity).unwrap();
        }
        assert_eq!(split, accrue_share(0, 0, 0, 2, 7, 21).unwrap());
        assert_eq!(split, (6, 0, 1));
        assert_eq!(accrue_share(u64::MAX, 0, 0, 1, 1, 1), Err(Error::Overflow));
    }
}

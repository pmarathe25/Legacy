//! Voluntary local bargaining, chosen amendment procedures, and physical performance.
use crate::*;
pub use game_economy::agreements::{Agreement, Amendment, Obligation};
use game_economy::agreements::{accrue_share, stream_ended};

fn site_exists(state: &CanonicalState, site: SiteId) -> bool {
    state
        .physical
        .as_ref()
        .is_some_and(|w| w.sites.contains_key(&site))
        || state
            .environment
            .as_ref()
            .is_some_and(|w| w.fields.contains_key(&site))
        || state
            .ecology
            .as_ref()
            .is_some_and(|w| w.plots.contains_key(&site))
}
fn site_custodian(state: &CanonicalState, site: SiteId) -> Option<PersonId> {
    state
        .environment
        .as_ref()
        .and_then(|w| w.fields.get(&site))
        .and_then(|f| f.custodian)
        .or_else(|| {
            state
                .ecology
                .as_ref()
                .and_then(|w| w.plots.get(&site))
                .and_then(|p| p.custodian)
        })
}
fn output_source(state: &CanonicalState, site: SiteId, good: Good) -> Option<InventoryId> {
    state
        .environment
        .as_ref()
        .and_then(|w| w.fields.get(&site))
        .and_then(|f| match good {
            Good::Food => Some(f.food),
            Good::Seed => Some(f.seed),
            _ => None,
        })
        .or_else(|| {
            state
                .projects
                .as_ref()
                .and_then(|w| {
                    w.projects
                        .values()
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
                .physical
                .as_ref()
                .and_then(|w| w.sites.get(&site))
                .and_then(|s| (state.inventories[&s.output].good == good).then_some(s.output))
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
        && state
            .inventories
            .get(destination)
            .is_some_and(|i| i.good == *good)
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

fn same_duty(a: &AgreementTerm, b: &AgreementTerm) -> bool {
    match (a, b) {
        (
            AgreementTerm::Delivery {
                debtor: a,
                source: x,
                destination: y,
                ..
            },
            AgreementTerm::Delivery {
                debtor: b,
                source: u,
                destination: v,
                ..
            },
        ) => a == b && x == u && y == v,
        (
            AgreementTerm::Labor {
                debtor: a, site: x, ..
            },
            AgreementTerm::Labor {
                debtor: b, site: y, ..
            },
        ) => a == b && x == y,
        (
            AgreementTerm::ConstructionLabor {
                debtor: a,
                project: x,
                ..
            },
            AgreementTerm::ConstructionLabor {
                debtor: b,
                project: y,
                ..
            },
        ) => a == b && x == y,
        (AgreementTerm::Access { .. }, AgreementTerm::Access { .. })
        | (AgreementTerm::SiteAccess { .. }, AgreementTerm::SiteAccess { .. }) => a == b,
        (
            AgreementTerm::OutputShare {
                debtor: a,
                site: x,
                good: g,
                destination: y,
                ..
            },
            AgreementTerm::OutputShare {
                debtor: b,
                site: u,
                good: h,
                destination: v,
                ..
            },
        ) => a == b && x == u && g == h && y == v,
        _ => false,
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
    let Some(w) = &s.agreements else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid agreement state".into());
    let people: BTreeSet<_> = s.people.iter().map(|p| p.id).collect();
    let physical = s.physical.as_ref().ok_or_else(fail)?;
    if s.decisions.is_none()
        || w.last_event >= s.next_event
        || w.last_observation_event >= s.next_event
        || w.custodians.iter().any(|(inventory, actor)| {
            !s.inventories.contains_key(inventory) || !people.contains(actor)
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
            } => s
                .inventories
                .get(source)
                .zip(s.inventories.get(destination))
                .is_some_and(|(a, b)| {
                    a.id != b.id && a.good == b.good && quantity.0 <= b.capacity.0
                }),
            AgreementTerm::Labor { site, .. } => site_exists(s, *site),
            AgreementTerm::SiteAccess { grantor, site, .. } => {
                site_custodian(s, *site) == Some(*grantor)
            }
            AgreementTerm::OutputShare { .. } => output_share_valid(s, term),
            AgreementTerm::ConstructionLabor { project, .. } => s
                .projects
                .as_ref()
                .is_some_and(|w| w.projects.contains_key(project)),
            AgreementTerm::Access {
                grantor, inventory, ..
            } => {
                w.custodians.get(inventory) == Some(grantor)
                    || s.inventories
                        .get(inventory)
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
            let stock = s.inventories.get(inventory).ok_or_else(fail)?;
            if *quantity == 0
                || stock.location != InventoryLocation::Carried(debtor)
                || stock.good != s.inventories[&source].good
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
    if cargo_totals
        .iter()
        .any(|(id, quantity)| *quantity > s.inventories[id].quantity.0)
    {
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
    for body in physical.actors.values() {
        if let Activity::ReportAgreement { ref snapshot, .. } = body.activity
            && !valid_knowledge(s, snapshot)
        {
            return Err(fail());
        }
    }
    Ok(())
}

fn valid_knowledge(state: &CanonicalState, view: &AgreementView) -> bool {
    let Some(world) = &state.agreements else {
        return false;
    };
    let Some(actual) = world.agreements.get(&view.id) else {
        return false;
    };
    let exists = |actor| state.people.iter().any(|person| person.id == actor);
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
                    && state.inventories.contains_key(source)
                    && state.inventories.contains_key(destination)
            }
            AgreementTerm::Labor { site, .. } => site_exists(state, *site),
            AgreementTerm::SiteAccess { site, .. } => site_exists(state, *site),
            AgreementTerm::OutputShare { .. } => output_share_valid(state, term),
            AgreementTerm::ConstructionLabor { project, .. } => state
                .projects
                .as_ref()
                .is_some_and(|w| w.projects.contains_key(project)),
            AgreementTerm::Access { inventory, .. } => state.inventories.contains_key(inventory),
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
            || obligation
                .cargo
                .iter()
                .any(|(id, qty)| !state.inventories.contains_key(id) || *qty == 0)
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
    pub ledger: game_economy::agreements::State,
    /// Physical asset control remains outside the economic ledger.
    pub custodians: BTreeMap<InventoryId, PersonId>,
}
impl std::ops::Deref for World {
    type Target = game_economy::agreements::State;
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
        sim.state.scenario = ScenarioInfo {
            id: "agreement-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let local = sim.state.areas[0].id;
        let ruler = sim.state.ruler;
        let citizen = sim
            .state
            .people
            .iter()
            .find(|p| p.role == Role::Citizen && p.location == Location::AtArea(local))
            .unwrap()
            .id;
        let custodians = sim
            .state
            .inventories
            .values()
            .filter(|i| i.location == InventoryLocation::Area(local))
            .map(|i| (i.id, if i.good == Good::Food { ruler } else { citizen }))
            .collect();
        // This fixture authors one controlled store; sharing is negotiated below.
        for inventory in sim.state.inventories.values_mut() {
            if inventory.location == InventoryLocation::Area(local) && inventory.good == Good::Food
            {
                inventory.access = BTreeSet::from([ruler]);
            }
        }
        sim.state.agreements = Some(World {
            ledger: game_economy::agreements::State {
                agreements: BTreeMap::new(),
                obligations: BTreeMap::new(),
                amendments: BTreeMap::new(),
                last_event: sim.state.next_event - 1,
                next_search: BTreeMap::new(),
                pending_work: BTreeMap::new(),
                knowledge: BTreeMap::new(),
                last_observation_event: sim.state.next_event - 1,
            },
            custodians,
        });
        Ok(sim)
    }

    pub fn agreement_view(&self, actor: PersonId) -> Vec<AgreementView> {
        let Some(world) = &self.state.agreements else {
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
        let world = self.state.agreements.as_ref()?;
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
                        observed_at: self.state.time,
                        learned_at: self.state.time,
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
            observed_at: self.state.time,
            learned_at: self.state.time,
            source: Some(KnowledgeSource::Observed(actor)),
        })
    }

    /// A genuine notice discloses declared terms, not remote warehouse facts.
    pub(crate) fn notice_agreement(&mut self, actor: PersonId, agreement: AgreementId) {
        let Some(mut view) = self.actual_agreement_snapshot(actor, agreement) else {
            return;
        };
        let old = self
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
                            .is_some_and(|d| d <= self.state.time)
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
                    obligation.observed_at = self.state.time;
                    obligation.learned_at = self.state.time;
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
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .knowledge
            .entry(actor)
            .or_default()
            .insert(view.id, view);
        if let Some(mind) = self
            .state
            .decisions
            .as_mut()
            .and_then(|world| world.minds.get_mut(&actor))
        {
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
        let parties = self.state.agreements.as_ref().unwrap().agreements[&agreement]
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
        let Some(world) = &self.state.agreements else {
            return;
        };
        let cursor = world.last_observation_event;
        let events: Vec<_> = self
            .state
            .events
            .iter()
            .filter(|event| event.id.0 > cursor)
            .cloned()
            .collect();
        for event in events {
            match event.kind {
                EventKind::AgreementProposed {
                    agreement,
                    proposer,
                } if event.time == self.state.time => {
                    self.local_agreement_notice(agreement, proposer, None);
                }
                EventKind::AgreementActivated { agreement } if event.time == self.state.time => {
                    let proposer =
                        self.state.agreements.as_ref().unwrap().agreements[&agreement].proposer;
                    self.local_agreement_notice(agreement, proposer, None);
                }
                EventKind::AgreementRejected { agreement, actor }
                    if event.time == self.state.time =>
                {
                    self.local_agreement_notice(agreement, actor, None);
                }
                EventKind::ObligationAccrued {
                    obligation, actor, ..
                }
                | EventKind::ObligationPerformed {
                    obligation, actor, ..
                } => {
                    self.observe_obligation_fact(obligation, actor, event.time);
                }
                EventKind::ObligationDefaulted { obligation } => {
                    let actor = self.state.agreements.as_ref().unwrap().obligations[&obligation]
                        .term
                        .parties()[0];
                    self.observe_obligation_fact(obligation, actor, event.time);
                }
                _ => {}
            }
        }
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .last_observation_event = self.state.next_event - 1;
        // A person knows the cargo and work they themselves actually performed.
        // This does not reveal amended duties they have not yet been told about.
        let actors: Vec<_> = self
            .state
            .agreements
            .as_ref()
            .unwrap()
            .knowledge
            .keys()
            .copied()
            .collect();
        for actor in actors {
            if !self.state.physical.as_ref().unwrap().actors[&actor]
                .vitals
                .alive
                || self.state.physical.as_ref().unwrap().actors[&actor]
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
                        fact.observed_at = self.state.time;
                        fact.learned_at = self.state.time;
                        fact.source = Some(KnowledgeSource::Observed(actor));
                    }
                }
                refresh_known_status(view);
                self.store_agreement_knowledge(actor, view.clone());
            }
        }
    }

    fn agreement_view_unbounded(&self, actor: PersonId) -> Vec<AgreementView> {
        self.state
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
                self.state.physical.as_ref().unwrap().actors[actor]
                    .vitals
                    .alive
                    && self.state.physical.as_ref().unwrap().actors[actor]
                        .vitals
                        .health
                        > 0
                    && (*actor == performer
                        || at == self.state.time
                            && area.is_some()
                            && self.agreement_actor_area(*actor).ok() == area)
            })
            .collect();
        for actor in observers {
            let mut view = self.state.agreements.as_ref().unwrap().knowledge[&actor]
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
            fact.learned_at = self.state.time;
            fact.source = Some(KnowledgeSource::Observed(actor));
            view.observed_at = at;
            view.learned_at = self.state.time;
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
        let area = self.agreement_actor_area(sender)?;
        if sender == recipient
            || self.agreement_actor_area(recipient)? != area
            || self.care_index.contains_key(&sender)
            || !matches!(
                self.state.physical.as_ref().unwrap().actors[&recipient].activity,
                Activity::Idle | Activity::Rest
            )
        {
            return Err(Error::Unavailable);
        }
        let snapshot = self
            .state
            .agreements
            .as_ref()
            .and_then(|world| world.knowledge.get(&sender))
            .and_then(|known| known.get(&agreement))
            .cloned()
            .ok_or(Error::Unavailable)?;
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(MINUTE.0)
                .ok_or(Error::Overflow)?,
        );
        self.ensure_event_capacity(1)?;
        self.interrupt_actor_activity(sender);
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&sender)
            .unwrap()
            .activity = Activity::ReportAgreement {
            recipient,
            snapshot: Box::new(snapshot),
            completes,
        };
        self.state
            .due
            .entry(completes)
            .or_default()
            .push(Due::Physical(sender));
        self.revision += 1;
        Ok(())
    }

    pub(crate) fn complete_agreement_report(&mut self, sender: PersonId) {
        let Activity::ReportAgreement {
            recipient,
            snapshot,
            ..
        } = self.state.physical.as_ref().unwrap().actors[&sender]
            .activity
            .clone()
        else {
            return;
        };
        if self.agreement_actor_area(sender).ok() == self.agreement_actor_area(recipient).ok()
            && self.agreement_actor_area(sender).is_ok()
            && matches!(
                self.state.physical.as_ref().unwrap().actors[&recipient].activity,
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
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&sender)
            .unwrap()
            .activity = Activity::Idle;
    }

    fn receive_agreement_report(
        &mut self,
        sender: PersonId,
        recipient: PersonId,
        mut incoming: AgreementView,
    ) {
        incoming.learned_at = self.state.time;
        incoming.source = Some(KnowledgeSource::Reported(sender));
        for obligation in &mut incoming.obligations {
            obligation.learned_at = self.state.time;
            obligation.source = Some(KnowledgeSource::Reported(sender));
        }
        if let Some(old) = self
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
        let Some(w) = &self.state.agreements else {
            return false;
        };
        let Some(inventory) = self.state.inventories.get(&destination) else {
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
        let Some(_world) = &self.state.agreements else {
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
                        .is_some_and(|deadline| deadline <= self.state.time)
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

    pub(crate) fn agreement_command(
        &mut self,
        actor: PersonId,
        action: &AgreementAction,
    ) -> Result<(), Error> {
        let area = self.agreement_actor_area(actor)?;
        self.reserve_agreement_ids(0)?;
        let world = self.state.agreements.as_ref().ok_or(Error::Unavailable)?;
        match action {
            AgreementAction::Report {
                agreement,
                recipient,
            } => {
                return self.start_agreement_report(actor, *recipient, *agreement);
            }
            AgreementAction::Propose {
                parties,
                terms,
                procedure,
            } => {
                self.validate_proposal(actor, area, parties, terms, procedure)?;
                self.reserve_agreement_ids(1)?;
                let id = AgreementId(allocate(&mut self.state)?);
                self.state.agreements.as_mut().unwrap().agreements.insert(
                    id,
                    Agreement {
                        id,
                        proposer: actor,
                        parties: parties.clone(),
                        accepted: BTreeSet::from([actor]),
                        terms: terms.clone(),
                        procedure: procedure.clone(),
                        obligations: Vec::new(),
                        status: AgreementStatus::Proposed,
                        proposed_at: self.state.time,
                        revision: 0,
                    },
                );
                self.emit(EventKind::AgreementProposed {
                    agreement: id,
                    proposer: actor,
                });
            }
            AgreementAction::Counteroffer {
                agreement,
                terms,
                procedure,
            } => {
                let old = world.agreements.get(agreement).ok_or(Error::Unavailable)?;
                if old.status != AgreementStatus::Proposed
                    || !old.parties.contains(&actor)
                    || old.proposer == actor
                {
                    return Err(Error::Unavailable);
                }
                let parties = old.parties.clone();
                self.validate_proposal(actor, area, &parties, terms, procedure)?;
                self.reserve_agreement_ids(1)?;
                self.state
                    .agreements
                    .as_mut()
                    .unwrap()
                    .agreements
                    .get_mut(agreement)
                    .unwrap()
                    .status = AgreementStatus::Withdrawn;
                self.agreement_command(
                    actor,
                    &AgreementAction::Propose {
                        parties,
                        terms: terms.clone(),
                        procedure: procedure.clone(),
                    },
                )?;
            }
            AgreementAction::Accept { agreement } => {
                let a = world.agreements.get(agreement).ok_or(Error::Unavailable)?;
                if a.status != AgreementStatus::Proposed
                    || !a.parties.contains(&actor)
                    || a.accepted.contains(&actor)
                {
                    return Err(Error::Unavailable);
                }
                if self.agreement_actor_area(a.proposer)? != area {
                    return Err(Error::Unavailable);
                }
                self.validate_terms(&a.parties, &a.terms, true)?;
                let activate = a.accepted.len() + 1 == a.parties.len();
                if activate {
                    self.reserve_agreement_ids(a.terms.len() as u64)?;
                }
                let world = self.state.agreements.as_mut().unwrap();
                world
                    .agreements
                    .get_mut(agreement)
                    .unwrap()
                    .accepted
                    .insert(actor);
                if activate {
                    self.activate_agreement(*agreement)?;
                }
            }
            AgreementAction::Reject { agreement } | AgreementAction::Withdraw { agreement } => {
                let a = world.agreements.get(agreement).ok_or(Error::Unavailable)?;
                let withdraw = matches!(action, AgreementAction::Withdraw { .. });
                if a.status != AgreementStatus::Proposed
                    || (withdraw && a.proposer != actor)
                    || (!withdraw && !a.parties.contains(&actor))
                {
                    return Err(Error::Unavailable);
                }
                self.state
                    .agreements
                    .as_mut()
                    .unwrap()
                    .agreements
                    .get_mut(agreement)
                    .unwrap()
                    .status = if withdraw {
                    AgreementStatus::Withdrawn
                } else {
                    AgreementStatus::Rejected
                };
                self.emit(EventKind::AgreementRejected {
                    agreement: *agreement,
                    actor,
                });
            }
            AgreementAction::Amend {
                agreement,
                replacements,
            } => {
                let a = world.agreements.get(agreement).ok_or(Error::Unavailable)?;
                let procedure = a.procedure.as_ref().ok_or(Error::Unavailable)?;
                if !matches!(
                    a.status,
                    AgreementStatus::Active
                        | AgreementStatus::Breached
                        | AgreementStatus::Fulfilled
                ) || !procedure.eligible_proposers.contains(&actor)
                    || replacements.is_empty()
                    || replacements.len() > 16
                    || replacements
                        .keys()
                        .any(|index| *index as usize >= a.terms.len())
                {
                    return Err(Error::Unavailable);
                }
                self.validate_terms(
                    &a.parties,
                    &replacements.values().cloned().collect::<Vec<_>>(),
                    false,
                )?;
                let base_revision = a.revision;
                self.reserve_agreement_ids(1)?;
                let id = AmendmentId(allocate(&mut self.state)?);
                self.state.agreements.as_mut().unwrap().amendments.insert(
                    id,
                    Amendment {
                        id,
                        agreement: *agreement,
                        proposer: actor,
                        replacements: replacements.clone(),
                        votes: BTreeMap::new(),
                        status: AmendmentStatus::Proposed,
                        base_revision,
                    },
                );
            }
            AgreementAction::Vote { amendment, support } => {
                let amendment_state = world.amendments.get(amendment).ok_or(Error::Unavailable)?;
                let a = &world.agreements[&amendment_state.agreement];
                let procedure = a.procedure.as_ref().ok_or(Error::Unavailable)?;
                if amendment_state.base_revision != a.revision {
                    return Err(Error::Unavailable);
                }
                a.revision.checked_add(1).ok_or(Error::Overflow)?;
                if amendment_state.status != AmendmentStatus::Proposed
                    || (!procedure.approvers.contains(&actor)
                        && !procedure
                            .term_approvers
                            .values()
                            .any(|ids| ids.contains(&actor)))
                    || amendment_state.votes.contains_key(&actor)
                    || self.agreement_actor_area(amendment_state.proposer)? != area
                {
                    return Err(Error::Unavailable);
                }
                // Rights are checked again at adoption; losing custody cannot grant access.
                self.validate_terms(
                    &a.parties,
                    &amendment_state
                        .replacements
                        .values()
                        .cloned()
                        .collect::<Vec<_>>(),
                    false,
                )?;
                self.state
                    .agreements
                    .as_mut()
                    .unwrap()
                    .amendments
                    .get_mut(amendment)
                    .unwrap()
                    .votes
                    .insert(actor, *support);
                self.resolve_amendment(*amendment)?;
            }
        }
        let notice = match action {
            AgreementAction::Propose { .. } => self
                .state
                .agreements
                .as_ref()
                .unwrap()
                .agreements
                .keys()
                .next_back()
                .copied()
                .map(|agreement| (agreement, None)),
            AgreementAction::Counteroffer { agreement, .. }
            | AgreementAction::Accept { agreement }
            | AgreementAction::Reject { agreement }
            | AgreementAction::Withdraw { agreement } => Some((*agreement, None)),
            AgreementAction::Amend { agreement, .. } => Some((
                *agreement,
                self.state
                    .agreements
                    .as_ref()
                    .unwrap()
                    .amendments
                    .keys()
                    .next_back()
                    .copied(),
            )),
            AgreementAction::Vote { amendment, .. } => Some((
                self.state.agreements.as_ref().unwrap().amendments[amendment].agreement,
                Some(*amendment),
            )),
            AgreementAction::Report { .. } => None,
        };
        if let Some((agreement, amendment)) = notice {
            self.local_agreement_notice(agreement, actor, amendment);
        }
        self.observe_agreement_events();
        Ok(())
    }

    fn reserve_agreement_ids(&self, count: u64) -> Result<(), Error> {
        self.state
            .next_id
            .checked_add(count)
            .ok_or(Error::Overflow)?;
        let pending = self
            .state
            .agreements
            .as_ref()
            .map_or(0, |w| w.pending_work.len() as u64);
        self.state
            .next_event
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
        let Some(world) = &self.state.agreements else {
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
    pub(crate) fn preflight_agreements(&self) -> Result<(), Error> {
        if let Some(w) = &self.state.agreements {
            let maximum = (w.obligations.len() as u64)
                .checked_mul(3)
                .and_then(|v| v.checked_add(2))
                .ok_or(Error::Overflow)?;
            self.state
                .next_event
                .checked_add(maximum)
                .ok_or(Error::Overflow)?;
        }
        Ok(())
    }

    fn agreement_actor_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        let p = self
            .person_index
            .get(&actor)
            .map(|i| &self.state.people[*i])
            .ok_or(Error::Unavailable)?;
        let alive = self
            .state
            .physical
            .as_ref()
            .is_some_and(|w| w.actors[&actor].vitals.alive);
        if !alive
            || !self.independent(actor)
            || self.in_combat(actor)
            || self.state.physical.as_ref().unwrap().actors[&actor]
                .vitals
                .health
                == 0
        {
            return Err(Error::Unavailable);
        }
        match p.location {
            Location::AtArea(area) => Ok(area),
            _ => Err(Error::Unavailable),
        }
    }

    fn validate_proposal(
        &self,
        actor: PersonId,
        area: AreaId,
        parties: &BTreeSet<PersonId>,
        terms: &[AgreementTerm],
        procedure: &Option<AmendmentProcedure>,
    ) -> Result<(), Error> {
        if !(2..=8).contains(&parties.len())
            || !parties.contains(&actor)
            || parties
                .iter()
                .any(|id| self.agreement_actor_area(*id).ok() != Some(area))
        {
            return Err(Error::Unavailable);
        }
        self.validate_terms(parties, terms, true)?;
        if let Some(p) = procedure
            && (p.eligible_proposers.is_empty()
                || !p.eligible_proposers.is_subset(parties)
                || !p.approvers.is_subset(parties)
                || p.threshold == 0
                || p.threshold > p.approvers.len()
                || p.term_approvers.iter().any(|(index, ids)| {
                    *index as usize >= terms.len() || ids.is_empty() || !ids.is_subset(parties)
                }))
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }

    fn economic_terms_context(
        &self,
        terms: &[AgreementTerm],
    ) -> game_economy::agreements::TermsContext {
        use game_economy::agreements::{InventoryTerms, PermissionAsset, TermsContext};
        let mut context = TermsContext {
            now: self.state.time,
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
                if let Some(stock) = self.state.inventories.get(&id) {
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
                    if site_exists(&self.state, site) {
                        context.sites.insert(site);
                    }
                }
                AgreementTerm::ConstructionLabor { project, .. } => {
                    if self
                        .state
                        .projects
                        .as_ref()
                        .is_some_and(|w| w.projects.contains_key(&project))
                    {
                        context.projects.insert(project);
                    }
                }
                AgreementTerm::Access {
                    grantor, inventory, ..
                } => {
                    if self.state.inventories.get(&inventory).is_some_and(|i| {
                        self.state
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
                    if site_custodian(&self.state, site) == Some(grantor) {
                        context
                            .grants
                            .insert((PermissionAsset::Site(site), grantor));
                    }
                }
                AgreementTerm::OutputShare { site, good, .. } => {
                    if let Some(source) = output_source(&self.state, site, good) {
                        context.outputs.insert((site, good), source);
                    }
                }
                _ => {}
            }
        }
        context
    }
    fn validate_terms(
        &self,
        parties: &BTreeSet<PersonId>,
        terms: &[AgreementTerm],
        future: bool,
    ) -> Result<(), Error> {
        game_economy::agreements::validate_terms(
            parties,
            terms,
            &self.economic_terms_context(terms),
            future,
        )
    }

    fn activate_agreement(&mut self, id: AgreementId) -> Result<(), Error> {
        let terms = self.state.agreements.as_ref().unwrap().agreements[&id]
            .terms
            .clone();
        let count = u64::try_from(terms.len()).map_err(|_| Error::Overflow)?;
        let next = self
            .state
            .next_id
            .checked_add(count)
            .ok_or(Error::Overflow)?;
        let ids = (self.state.next_id..next)
            .map(ObligationId)
            .collect::<Vec<_>>();
        let context = self.economic_terms_context(&terms);
        let plan = self
            .state
            .agreements
            .as_ref()
            .unwrap()
            .ledger
            .prepare_activation(id, &ids, &context)?;
        // Physical effects and global IDs remain coordinator-owned. The domain
        // plan cannot change permissions or allocate an entity by itself.
        for permission in &plan.permissions {
            self.apply_permission_effect(*permission);
        }
        plan.commit(&mut self.state.agreements.as_mut().unwrap().ledger)?;
        self.state.next_id = next;
        self.emit(EventKind::AgreementActivated { agreement: id });
        Ok(())
    }
    fn apply_permission_effect(&mut self, effect: game_economy::agreements::PermissionEffect) {
        use game_economy::agreements::PermissionAsset;
        let access = match effect.asset {
            PermissionAsset::Inventory(inventory) => {
                Some(&mut self.state.inventories.get_mut(&inventory).unwrap().access)
            }
            PermissionAsset::Site(site) => self
                .state
                .environment
                .as_mut()
                .and_then(|w| w.fields.get_mut(&site))
                .map(|f| &mut f.access)
                .or_else(|| {
                    self.state
                        .ecology
                        .as_mut()
                        .and_then(|w| w.plots.get_mut(&site))
                        .map(|p| &mut p.access)
                }),
        };
        if let Some(access) = access {
            if effect.allowed {
                access.insert(effect.grantee);
            } else {
                access.remove(&effect.grantee);
            }
        }
    }
    fn apply_agreement_access(&mut self, term: &AgreementTerm) -> u64 {
        if let Some(effect) = game_economy::agreements::permission_effect(term) {
            self.apply_permission_effect(effect);
            1
        } else {
            0
        }
    }

    fn resolve_amendment(&mut self, id: AmendmentId) -> Result<(), Error> {
        let world = self.state.agreements.as_ref().unwrap();
        let amendment = world.amendments[&id].clone();
        let a = world.agreements[&amendment.agreement].clone();
        let p = a.procedure.as_ref().unwrap();
        let support = p
            .approvers
            .iter()
            .filter(|id| amendment.votes.get(id) == Some(&true))
            .count();
        let required: BTreeSet<_> = amendment
            .replacements
            .keys()
            .filter_map(|index| p.term_approvers.get(index))
            .flat_map(|ids| ids.iter().copied())
            .collect();
        let rejected = p
            .approvers
            .iter()
            .filter(|id| amendment.votes.get(id) == Some(&false))
            .count()
            > p.approvers.len() - p.threshold
            || required
                .iter()
                .any(|id| amendment.votes.get(id) == Some(&false));
        if rejected {
            self.state
                .agreements
                .as_mut()
                .unwrap()
                .amendments
                .get_mut(&id)
                .unwrap()
                .status = AmendmentStatus::Rejected;
            return Ok(());
        }
        if support < p.threshold
            || required
                .iter()
                .any(|id| amendment.votes.get(id) != Some(&true))
        {
            return Ok(());
        }
        self.flush_agreement_work();
        for (index, term) in amendment.replacements {
            let obligation = a.obligations[index as usize];
            let immediate = self.apply_agreement_access(&term);
            let world = self.state.agreements.as_mut().unwrap();
            let o = world.obligations.get_mut(&obligation).unwrap();
            // A changed physical duty starts fresh; unchanged duty retains actual performance.
            let same_kind = same_duty(&o.term, &term)
                || match (&o.term, &term) {
                    (
                        AgreementTerm::Delivery {
                            debtor: ad,
                            source: asrc,
                            destination: adst,
                            ..
                        },
                        AgreementTerm::Delivery {
                            debtor: bd,
                            source: bsrc,
                            destination: bdst,
                            ..
                        },
                    ) => ad == bd && asrc == bsrc && adst == bdst,
                    (
                        AgreementTerm::Labor {
                            debtor: ad,
                            site: asite,
                            ..
                        },
                        AgreementTerm::Labor {
                            debtor: bd,
                            site: bsite,
                            ..
                        },
                    ) => ad == bd && asite == bsite,
                    (
                        AgreementTerm::ConstructionLabor {
                            debtor: a,
                            project: ap,
                            ..
                        },
                        AgreementTerm::ConstructionLabor {
                            debtor: b,
                            project: bp,
                            ..
                        },
                    ) => a == b && ap == bp,
                    _ => false,
                };
            o.performed = if immediate == 1 {
                1
            } else if same_kind {
                o.performed
                    .min(if matches!(term, AgreementTerm::OutputShare { .. }) {
                        o.accrued
                    } else {
                        term.amount()
                    })
            } else {
                0
            };
            if !same_kind {
                o.accrued = 0;
                o.share_remainder = 0;
                o.share_scale = 0;
                o.cargo.clear();
            }
            o.term = term.clone();
            let mut remaining_cargo = o.amount() - o.performed;
            for quantity in o.cargo.values_mut() {
                *quantity = (*quantity).min(remaining_cargo);
                remaining_cargo -= *quantity;
            }
            o.cargo.retain(|_, quantity| *quantity > 0);
            o.defaulted =
                o.performed < o.amount() && term.deadline().is_some_and(|d| d <= self.state.time);
            world.agreements.get_mut(&a.id).unwrap().terms[index as usize] = term;
        }
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .amendments
            .get_mut(&id)
            .unwrap()
            .status = AmendmentStatus::Adopted;
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .agreements
            .get_mut(&a.id)
            .unwrap()
            .revision += 1;
        for other in self
            .state
            .agreements
            .as_mut()
            .unwrap()
            .amendments
            .values_mut()
        {
            if other.agreement == a.id
                && other.status == AmendmentStatus::Proposed
                && other.base_revision == a.revision
            {
                other.status = AmendmentStatus::Rejected;
            }
        }
        self.refresh_agreement_status(a.id);
        self.emit(EventKind::AgreementAmended {
            agreement: a.id,
            amendment: id,
        });
        Ok(())
    }

    fn refresh_agreement_status(&mut self, id: AgreementId) {
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .ledger
            .refresh_status(id, self.state.time)
            .expect("validated agreement ledger");
    }

    pub(crate) fn next_agreement_deadline(&self) -> Option<SimTime> {
        self.state
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
            .filter(|d| *d > self.state.time)
            .min()
    }

    /// Physical eligibility comes from the coordinator; economic allocation is
    /// performed by the independent ledger against this exact clock interval.
    pub(crate) fn record_agreement_labor_batch(
        &mut self,
        from: SimTime,
        to: SimTime,
        assignments: &[game_economy::agreements::WorkAssignment],
    ) -> Result<(), Error> {
        let Some(world) = self.state.agreements.as_mut() else {
            return Ok(());
        };
        let prepared = world.ledger.prepare_labor(from, to, assignments)?;
        prepared.commit(&mut world.ledger)
    }
    pub(crate) fn record_agreement_work(&mut self, actor: PersonId, site: SiteId, elapsed: u64) {
        let now = self.state.time;
        self.record_agreement_labor_batch(
            SimTime(now.0 - elapsed),
            now,
            &[game_economy::agreements::WorkAssignment {
                actor,
                target: game_economy::agreements::WorkTarget::Site(site),
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
        let now = self.state.time;
        self.record_agreement_labor_batch(
            SimTime(now.0 - elapsed),
            now,
            &[game_economy::agreements::WorkAssignment {
                actor,
                target: game_economy::agreements::WorkTarget::Project(project),
            }],
        )
        .expect("preflight eligible construction work");
    }

    fn flush_agreement_work(&mut self) {
        let pending = std::mem::take(&mut self.state.agreements.as_mut().unwrap().pending_work);
        for (obligation, quantity) in pending {
            let actor = self.state.agreements.as_ref().unwrap().obligations[&obligation]
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
        let Some(world) = &self.state.agreements else {
            return Ok(());
        };
        let cursor = world.last_event;
        let events: Vec<_> = self
            .state
            .events
            .iter()
            .skip(cursor as usize)
            .cloned()
            .collect();
        let flush_work = self.state.time.0.is_multiple_of(60_000)
            || events.iter().any(|e| {
                e.time == self.state.time
                    && matches!(
                        e.kind,
                        EventKind::Produced { .. } | EventKind::ProjectCompleted { .. }
                    )
            })
            || world
                .obligations
                .values()
                .any(|o| o.term.deadline() == Some(self.state.time));
        let last_event = self.state.next_event - 1;
        let mut credits = Vec::new();
        let mut accruals = Vec::new();
        let share_origins = self
            .state
            .agreements
            .as_ref()
            .unwrap()
            .obligations
            .values()
            .filter_map(|o| match o.term {
                AgreementTerm::OutputShare { site, good, .. } => {
                    output_source(&self.state, site, good).map(|i| (o.id, i))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        for e in events {
            let productions = match e.kind {
                EventKind::Produced {
                    actor,
                    site,
                    output,
                    quantity,
                } => vec![(actor, site, self.state.inventories[&output].good, quantity)],
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
                let fact = game_economy::agreements::ProductionFact {
                    actor: producer,
                    site,
                    good,
                    quantity,
                    time: e.time,
                };
                let plan = self
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
                plan.commit(&mut self.state.agreements.as_mut().unwrap().ledger)?;
            }
            match e.kind {
                EventKind::Transferred {
                    actor,
                    source,
                    destination,
                    quantity,
                } => {
                    let carried_to_actor = self.state.inventories[&destination].location
                        == InventoryLocation::Carried(actor);
                    let mut unallocated = quantity.0;
                    let world = self.state.agreements.as_mut().unwrap();
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
        for o in self.state.agreements.as_ref().unwrap().obligations.values() {
            for (inventory, quantity) in &o.cargo {
                *totals.entry(*inventory).or_default() += *quantity;
            }
        }
        for (inventory, attributed) in totals {
            let physical = self.state.inventories[&inventory].quantity.0;
            if attributed > physical {
                self.lose_agreement_cargo(inventory, attributed - physical);
            }
        }
        self.state.agreements.as_mut().unwrap().last_event = last_event;
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
            .state
            .agreements
            .as_ref()
            .unwrap()
            .ledger
            .defaults_due(self.state.time)
            .into_iter()
            .map(|id| {
                (
                    id,
                    self.state.agreements.as_ref().unwrap().obligations[&id].agreement,
                )
            })
            .collect::<Vec<_>>();
        for (obligation, agreement) in defaults {
            self.state
                .agreements
                .as_mut()
                .unwrap()
                .ledger
                .mark_default(obligation, self.state.time)?;
            self.emit(EventKind::ObligationDefaulted { obligation });
            self.refresh_agreement_status(agreement);
        }
        Ok(())
    }

    fn lose_agreement_cargo(&mut self, source: InventoryId, mut quantity: u64) {
        for o in self
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
        let Some(world) = &self.state.agreements else {
            return Ok(());
        };
        let mut answers = Vec::new();
        for (actor, known) in &world.knowledge {
            if *actor == self.state.ruler {
                continue;
            }
            for a in known.values().filter(|a| {
                a.status == AgreementStatus::Proposed
                    && a.parties.contains(actor)
                    && !a.accepted.contains(actor)
            }) {
                if self.agreement_actor_area(*actor).ok()
                    != self.agreement_actor_area(a.proposer).ok()
                {
                    continue;
                }
                let score = self.agreement_offer_score(*actor, &a.terms);
                if score >= 0 {
                    answers.push((*actor, AgreementAction::Accept { agreement: a.id }));
                } else if score >= -2_000 && a.proposer != *actor {
                    let terms = a
                        .terms
                        .iter()
                        .map(|term| match term {
                            AgreementTerm::Labor {
                                debtor,
                                creditor,
                                site,
                                millis,
                                deadline,
                            } if debtor == actor => AgreementTerm::Labor {
                                debtor: *debtor,
                                creditor: *creditor,
                                site: *site,
                                millis: (*millis / 2).max(1),
                                deadline: *deadline,
                            },
                            _ => term.clone(),
                        })
                        .collect::<Vec<_>>();
                    if terms != a.terms {
                        answers.push((
                            *actor,
                            AgreementAction::Counteroffer {
                                agreement: a.id,
                                terms,
                                procedure: a.procedure.clone(),
                            },
                        ));
                    } else {
                        answers.push((*actor, AgreementAction::Reject { agreement: a.id }));
                    }
                } else {
                    answers.push((*actor, AgreementAction::Reject { agreement: a.id }));
                }
            }
        }
        for (actor, answer) in answers {
            let _ = self.agreement_command(actor, &answer);
        }
        let mut votes = Vec::new();
        for (actor, known) in &self.state.agreements.as_ref().unwrap().knowledge {
            if *actor == self.state.ruler {
                continue;
            }
            for a in known.values().filter(|a| a.parties.contains(actor)) {
                let Some(p) = &a.procedure else { continue };
                for m in a.amendments.iter().filter(|m| {
                    m.status == AmendmentStatus::Proposed
                        && m.base_revision == a.revision
                        && !m.votes.contains_key(actor)
                }) {
                    if (!p.approvers.contains(actor)
                        && !p.term_approvers.values().any(|ids| ids.contains(actor)))
                        || self.agreement_actor_area(*actor).ok()
                            != self.agreement_actor_area(m.proposer).ok()
                    {
                        continue;
                    }
                    let mut revised = a.terms.clone();
                    for (index, term) in &m.replacements {
                        revised[*index as usize] = term.clone();
                    }
                    let mind = &self.state.decisions.as_ref().unwrap().minds[actor];
                    let concession = i64::from(mind.personality.empathy.saturating_sub(700)) * 5;
                    let support = self.agreement_offer_score(*actor, &revised) + concession
                        >= self.agreement_offer_score(*actor, &a.terms);
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

    fn agreement_offer_score(&self, actor: PersonId, terms: &[AgreementTerm]) -> i64 {
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        let mut score = self.project_financing_utility(actor, terms);
        for term in terms {
            if term.amount() == 0 && !matches!(term, AgreementTerm::OutputShare { .. }) {
                continue;
            }
            match *term {
                AgreementTerm::OutputShare {
                    creditor,
                    debtor,
                    site,
                    numerator,
                    denominator,
                    ..
                } => {
                    let fertility = self
                        .state
                        .ecology
                        .as_ref()
                        .and_then(|w| w.known_plots.get(&actor))
                        .and_then(|k| k.get(&site))
                        .map(|p| p.soil_fertility);
                    let remembered_plan = self
                        .state
                        .projects
                        .as_ref()
                        .and_then(|w| w.known_projects.get(&actor))
                        .is_some_and(|k| {
                            k.values()
                                .any(|p| matches!(p.kind,ProjectKind::Farm {plot,..} if plot==site))
                        });
                    let value = if let Some(fertility) = fertility.filter(|_| remembered_plan) {
                        let crop = &self.state.environment.as_ref().unwrap().crop;
                        let expected = u128::from(crop.food_yield.0)
                            * u128::from(fertility)
                            * u128::from(numerator)
                            / u128::from(denominator)
                            / 1_000;
                        // A known plan is a risky future food prospect, not a guarantee.
                        let estimate = (expected / 100).min(20_000) as i64;
                        estimate * i64::from(mind.personality.risk_tolerance) / 1_000
                    } else if self
                        .state
                        .projects
                        .as_ref()
                        .and_then(|w| w.known_enterprises.get(&actor))
                        .is_some_and(|k| k.values().any(|e| e.site == site))
                        || self.state.environment.as_ref().is_some_and(|w| {
                            w.fields.get(&site).is_some_and(|f| {
                                self.state.people[self.person_index[&actor]].location
                                    == Location::AtArea(f.area)
                            })
                        })
                    {
                        (numerator.saturating_mul(
                            u64::from(mind.personality.ambition)
                                + u64::from(mind.personality.hedonism),
                        ) / denominator)
                            .min(2_000) as i64
                    } else {
                        -2_000
                    };
                    if creditor == actor {
                        score += value;
                    }
                    if debtor == actor {
                        score -= value / 2;
                    }
                }
                AgreementTerm::SiteAccess {
                    grantee, allowed, ..
                } if grantee == actor => score += if allowed { 1_000 } else { -1_000 },
                AgreementTerm::Delivery {
                    debtor,
                    creditor,
                    source,
                    quantity,
                    deadline,
                    ..
                } => {
                    if debtor == actor {
                        let known = mind.knowledge.get(&source);
                        if known.is_none_or(|k| !k.accessible || k.quantity < quantity)
                            || deadline.0.saturating_sub(self.state.time.0) < 120_000
                        {
                            score -= 10_000;
                        }
                        score -= i64::try_from(quantity.0 / 2)
                            .unwrap_or(i64::MAX / 32)
                            .min(100_000);
                    }
                    if creditor == actor {
                        score += i64::try_from(quantity.0)
                            .unwrap_or(i64::MAX / 32)
                            .min(100_000);
                    }
                }
                AgreementTerm::Labor {
                    debtor,
                    site,
                    millis,
                    deadline,
                    ..
                } if debtor == actor => {
                    if !mind.sites.get(&site).is_some_and(|s| s.access)
                        || deadline.0.saturating_sub(self.state.time.0) < millis
                    {
                        score -= 10_000;
                    }
                    score -= i64::try_from(millis / 60)
                        .unwrap_or(i64::MAX / 32)
                        .min(100_000);
                }
                AgreementTerm::ConstructionLabor {
                    debtor,
                    project,
                    millis,
                    deadline,
                    ..
                } if debtor == actor => {
                    if !self.project_is_known(actor, project)
                        || deadline.0.saturating_sub(self.state.time.0) < millis
                    {
                        score -= 10_000;
                    }
                    score -= i64::try_from(millis / 60)
                        .unwrap_or(i64::MAX / 32)
                        .min(100_000);
                }
                AgreementTerm::Access {
                    grantee,
                    inventory,
                    allowed,
                    ..
                } if grantee == actor => {
                    if let Some(k) = mind.knowledge.get(&inventory) {
                        let utility = if k.good == Good::Food
                            && self.state.physical.as_ref().unwrap().actors[&actor]
                                .vitals
                                .hunger
                                > 120_000
                        {
                            3_000
                        } else {
                            1_000
                        };
                        score += if allowed { utility } else { -utility };
                    } else {
                        score -= 2_000;
                    }
                }
                _ => {}
            }
        }
        let area = self.agreement_actor_area(actor).ok();
        let speakers: BTreeSet<_> = terms
            .iter()
            .filter(|term| !matches!(term, AgreementTerm::Access { .. }))
            .map(|term| term.parties())
            .filter(|[debtor, creditor]| *debtor != actor && *creditor == actor)
            .map(|[debtor, _]| debtor)
            .collect();
        for speaker in speakers {
            // A relationship does not disclose new symptoms across distance.
            if area.is_some() && self.agreement_actor_area(speaker).ok() == area {
                let trust = i64::from(mind.relationships.get(&speaker).map_or(0, |r| r.trust))
                    .clamp(-1_000, 1_000);
                score -= self.invocation_social_penalty(speaker) * (1_000 - trust) / 1_000;
            }
        }
        score + i64::from(mind.personality.empathy) - 500
    }

    fn seek_local_exchange(&mut self) -> Result<(), Error> {
        let world = self.state.agreements.as_ref().unwrap();
        let mut proposals = Vec::new();
        for (actor, mind) in &self.state.decisions.as_ref().unwrap().minds {
            if *actor == self.state.ruler
                || world
                    .next_search
                    .get(actor)
                    .is_some_and(|time| *time > self.state.time)
                || world.knowledge.get(actor).is_some_and(|known| {
                    known.values().any(|a| {
                        a.parties.contains(actor)
                            && matches!(
                                a.status,
                                AgreementStatus::Proposed | AgreementStatus::Active
                            )
                    })
                })
            {
                continue;
            }
            let Ok(area) = self.agreement_actor_area(*actor) else {
                continue;
            };
            if self.state.physical.as_ref().unwrap().actors[actor]
                .vitals
                .hunger
                < 180_000
            {
                continue;
            }
            if mind.knowledge.values().any(|k| {
                k.area == area && k.good == Good::Food && k.accessible && k.quantity.0 >= 1_000
            }) {
                continue;
            }
            let Some(food) = mind.knowledge.values().find(|k| {
                k.area == area
                    && k.good == Good::Food
                    && k.quantity.0 >= 1_000
                    && !k.accessible
                    && world.custodians.contains_key(&k.inventory)
            }) else {
                continue;
            };
            let owner = world.custodians[&food.inventory];
            if self.agreement_actor_area(owner).ok() != Some(area) {
                continue;
            }
            let Some(site) = mind.sites.values().find(|s| s.area == area && s.access) else {
                continue;
            };
            let Some(deadline) = self.state.time.0.checked_add(180_000).map(SimTime) else {
                continue;
            };
            proposals.push((
                *actor,
                AgreementAction::Propose {
                    parties: BTreeSet::from([*actor, owner]),
                    terms: vec![
                        AgreementTerm::Access {
                            grantor: owner,
                            grantee: *actor,
                            inventory: food.inventory,
                            allowed: true,
                        },
                        AgreementTerm::Labor {
                            debtor: *actor,
                            creditor: owner,
                            site: site.site,
                            millis: 60_000,
                            deadline,
                        },
                    ],
                    procedure: None,
                },
            ));
        }
        for (actor, proposal) in proposals {
            self.agreement_command(actor, &proposal)?;
            self.state
                .agreements
                .as_mut()
                .unwrap()
                .next_search
                .insert(actor, SimTime(self.state.time.0.saturating_add(300_000)));
        }
        Ok(())
    }

    fn perform_chosen_agreement_tasks(&mut self) {
        let mut intentions = BTreeMap::new();
        let obligations: Vec<_> = self
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
            if debtor == self.state.ruler
                || intentions.contains_key(&debtor)
                || self.combat_restricts(debtor)
                || self.in_combat(debtor)
            {
                continue;
            }
            let Ok(area) = self.agreement_actor_area(debtor) else {
                continue;
            };
            let body = &self.state.physical.as_ref().unwrap().actors[&debtor];
            let mind = &self.state.decisions.as_ref().unwrap().minds[&debtor];
            let rules = &self.state.physical.as_ref().unwrap().rules;
            if !matches!(body.activity, Activity::Idle | Activity::Rest)
                || body.vitals.hunger >= rules.meal_relief
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
                    let Some(source) = output_source(&self.state, site, good) else {
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
                        .state
                        .environment
                        .as_ref()
                        .is_some_and(|w| w.fields.contains_key(&site))
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
                    let carried = o.cargo.iter().find(|(_, q)| **q > 0);
                    if let Some((cargo, q)) = carried {
                        mind.knowledge.get(&destination).and_then(|k| {
                            if k.area != area {
                                decisions::known_path(&mind.routes, area, k.area)
                                    .ok()
                                    .map(|_| Command::Move {
                                        destination: k.area,
                                    })
                            } else {
                                Some(Command::Load {
                                    source: *cargo,
                                    destination,
                                    quantity: Quantity((*q).min(quantity.0 - o.performed)),
                                })
                            }
                        })
                    } else {
                        let src = mind.knowledge.get(&source).filter(|k| k.accessible);
                        let carried = self.state.inventories.values().find(|i| {
                            i.location == InventoryLocation::Carried(debtor)
                                && mind
                                    .knowledge
                                    .get(&source)
                                    .is_some_and(|known| i.good == known.good)
                        });
                        src.zip(carried)
                            .and_then(|(k, carried)| {
                                if k.area != area {
                                    decisions::known_path(&mind.routes, area, k.area).ok().map(
                                        |_| Command::Move {
                                            destination: k.area,
                                        },
                                    )
                                } else {
                                    Some(Command::Load {
                                        source,
                                        destination: carried.id,
                                        quantity: Quantity(
                                            k.quantity.0.min(quantity.0 - o.performed).min(
                                                carried
                                                    .capacity
                                                    .0
                                                    .saturating_sub(carried.quantity.0),
                                            ),
                                        ),
                                    })
                                }
                            })
                            .filter(|c| {
                                !matches!(
                                    c,
                                    Command::Load {
                                        quantity: Quantity(0),
                                        ..
                                    }
                                )
                            })
                    }
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

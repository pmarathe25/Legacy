//! Voluntary local bargaining, chosen amendment procedures, and physical performance.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agreement {
    pub id: AgreementId,
    pub proposer: PersonId,
    pub parties: BTreeSet<PersonId>,
    pub accepted: BTreeSet<PersonId>,
    pub terms: Vec<AgreementTerm>,
    pub procedure: Option<AmendmentProcedure>,
    pub obligations: Vec<ObligationId>,
    pub status: AgreementStatus,
    pub proposed_at: SimTime,
    pub revision: u64,
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
        || w.custodians.iter().any(|(inventory, actor)| {
            !s.inventories.contains_key(inventory) || !people.contains(actor)
        })
        || w.next_search.keys().any(|id| !people.contains(id))
    {
        return Err(fail());
    }
    let valid_term = |term: &AgreementTerm, parties: &BTreeSet<PersonId>| {
        let [a, b] = term.parties();
        if a == b || !parties.contains(&a) || !parties.contains(&b) || term.amount() == 0 {
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
            AgreementTerm::Labor { site, .. } => physical.sites.contains_key(site),
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
            let fulfilled = a
                .obligations
                .iter()
                .all(|id| w.obligations[id].performed == w.obligations[id].term.amount());
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
            !matches!(o.term, AgreementTerm::Labor { .. })
                || *quantity == 0
                || *quantity > o.performed
        })
    }) {
        return Err(fail());
    }
    for (id, o) in &w.obligations {
        if *id != o.id
            || id.0 >= s.next_id
            || o.performed > o.term.amount()
            || (o.defaulted && !o.term.deadline().is_some_and(|d| d <= s.time))
        {
            return Err(fail());
        }
        let mut total = 0_u64;
        for (inventory, quantity) in &o.cargo {
            let AgreementTerm::Delivery { debtor, source, .. } = o.term else {
                return Err(fail());
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
        if total > o.term.amount() - o.performed {
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
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obligation {
    pub id: ObligationId,
    pub agreement: AgreementId,
    pub term: AgreementTerm,
    pub performed: u64,
    pub defaulted: bool,
    /// Conserved cargo loaded from the agreed source, attributed once.
    pub cargo: BTreeMap<InventoryId, u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Amendment {
    pub id: AmendmentId,
    pub agreement: AgreementId,
    pub proposer: PersonId,
    pub replacements: BTreeMap<u16, AgreementTerm>,
    pub votes: BTreeMap<PersonId, bool>,
    pub status: AmendmentStatus,
    pub base_revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub agreements: BTreeMap<AgreementId, Agreement>,
    pub obligations: BTreeMap<ObligationId, Obligation>,
    pub amendments: BTreeMap<AmendmentId, Amendment>,
    /// Actual control of stores, independent of who has permission to use them.
    pub custodians: BTreeMap<InventoryId, PersonId>,
    pub last_event: u64,
    pub next_search: BTreeMap<PersonId, SimTime>,
    pub pending_work: BTreeMap<ObligationId, u64>,
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
            agreements: BTreeMap::new(),
            obligations: BTreeMap::new(),
            amendments: BTreeMap::new(),
            custodians,
            last_event: sim.state.next_event - 1,
            next_search: BTreeMap::new(),
            pending_work: BTreeMap::new(),
        });
        Ok(sim)
    }

    pub fn agreement_view(&self, actor: PersonId) -> Vec<AgreementView> {
        let Some(world) = &self.state.agreements else {
            return Vec::new();
        };
        world
            .agreements
            .values()
            .filter(|a| a.parties.contains(&actor))
            .take(64)
            .map(|a| AgreementView {
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
                            id: *id,
                            term: o.term.clone(),
                            performed: o.performed,
                            defaulted: o.defaulted,
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
                    })
                    .collect(),
            })
            .collect()
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
            let AgreementTerm::Delivery {
                debtor,
                creditor,
                destination: goal,
                quantity: promised,
                ..
            } = o.term
            else {
                return false;
            };
            debtor == actor
                && goal == destination
                && matches!(
                    w.agreements[&o.agreement].status,
                    AgreementStatus::Active | AgreementStatus::Breached
                )
                && quantity.0 <= promised.0 - o.performed
                && (inventory.location == InventoryLocation::Carried(creditor)
                    || w.custodians.get(&destination) == Some(&creditor))
        })
    }

    pub(crate) fn available_agreement_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(w) = &self.state.agreements else {
            return Vec::new();
        };
        let Ok(area) = self.agreement_actor_area(actor) else {
            return Vec::new();
        };
        let mut commands = Vec::new();
        for a in w
            .agreements
            .values()
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
                && self.validate_terms(&a.parties, &a.terms, true).is_ok()
            {
                commands.push(Command::Agreement {
                    action: AgreementAction::Accept { agreement: a.id },
                });
            }
        }
        for m in w
            .amendments
            .values()
            .filter(|m| {
                m.status == AmendmentStatus::Proposed
                    && w.agreements[&m.agreement].parties.contains(&actor)
            })
            .take(32)
        {
            let a = &w.agreements[&m.agreement];
            let Some(p) = &a.procedure else { continue };
            if m.base_revision != a.revision
                || m.votes.contains_key(&actor)
                || self.agreement_actor_area(m.proposer).ok() != Some(area)
                || (!p.approvers.contains(&actor)
                    && !p.term_approvers.values().any(|ids| ids.contains(&actor)))
                || self
                    .validate_terms(
                        &a.parties,
                        &m.replacements.values().cloned().collect::<Vec<_>>(),
                        false,
                    )
                    .is_err()
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
        if !alive || !self.independent(actor) || self.combat_restricts(actor) {
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

    fn validate_terms(
        &self,
        parties: &BTreeSet<PersonId>,
        terms: &[AgreementTerm],
        future: bool,
    ) -> Result<(), Error> {
        if terms.is_empty() || terms.len() > 16 {
            return Err(Error::Unavailable);
        }
        for term in terms {
            let [a, b] = term.parties();
            if a == b
                || !parties.contains(&a)
                || !parties.contains(&b)
                || term.amount() == 0
                || (future && term.deadline().is_some_and(|d| d <= self.state.time))
            {
                return Err(Error::Unavailable);
            }
            match term {
                AgreementTerm::Delivery {
                    source,
                    destination,
                    quantity,
                    ..
                } => {
                    let source = self
                        .state
                        .inventories
                        .get(source)
                        .ok_or(Error::Unavailable)?;
                    let destination = self
                        .state
                        .inventories
                        .get(destination)
                        .ok_or(Error::Unavailable)?;
                    if source.id == destination.id
                        || source.good != destination.good
                        || quantity.0 > destination.capacity.0
                    {
                        return Err(Error::Unavailable);
                    }
                }
                AgreementTerm::Labor { site, .. } => {
                    if !self
                        .state
                        .physical
                        .as_ref()
                        .unwrap()
                        .sites
                        .contains_key(site)
                    {
                        return Err(Error::Unavailable);
                    }
                }
                AgreementTerm::Access {
                    grantor, inventory, ..
                } => {
                    let w = self.state.agreements.as_ref().unwrap();
                    if !self.state.inventories.get(inventory).is_some_and(|i| {
                        w.custodians.get(inventory) == Some(grantor)
                            || i.location == InventoryLocation::Carried(*grantor)
                    }) {
                        return Err(Error::Unavailable);
                    }
                }
            }
        }
        Ok(())
    }

    fn activate_agreement(&mut self, id: AgreementId) -> Result<(), Error> {
        let terms = self.state.agreements.as_ref().unwrap().agreements[&id]
            .terms
            .clone();
        for term in terms {
            let obligation = ObligationId(allocate(&mut self.state)?);
            let performed = self.apply_agreement_access(&term);
            let world = self.state.agreements.as_mut().unwrap();
            world.obligations.insert(
                obligation,
                Obligation {
                    id: obligation,
                    agreement: id,
                    term,
                    performed,
                    defaulted: false,
                    cargo: BTreeMap::new(),
                },
            );
            world
                .agreements
                .get_mut(&id)
                .unwrap()
                .obligations
                .push(obligation);
        }
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .agreements
            .get_mut(&id)
            .unwrap()
            .status = AgreementStatus::Active;
        self.refresh_agreement_status(id);
        self.emit(EventKind::AgreementActivated { agreement: id });
        Ok(())
    }

    fn apply_agreement_access(&mut self, term: &AgreementTerm) -> u64 {
        if let AgreementTerm::Access {
            grantee,
            inventory,
            allowed,
            ..
        } = term
        {
            let access = &mut self.state.inventories.get_mut(inventory).unwrap().access;
            if *allowed {
                access.insert(*grantee);
            } else {
                access.remove(grantee);
            }
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
            let same_kind = match (&o.term, &term) {
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
                _ => false,
            };
            o.performed = if immediate == 1 {
                1
            } else if same_kind {
                o.performed.min(term.amount())
            } else {
                0
            };
            if !same_kind {
                o.cargo.clear();
            }
            o.term = term.clone();
            let mut remaining_cargo = term.amount() - o.performed;
            for quantity in o.cargo.values_mut() {
                *quantity = (*quantity).min(remaining_cargo);
                remaining_cargo -= *quantity;
            }
            o.cargo.retain(|_, quantity| *quantity > 0);
            o.defaulted = o.performed < term.amount()
                && term.deadline().is_some_and(|d| d <= self.state.time);
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
        let world = self.state.agreements.as_mut().unwrap();
        let ids = &world.agreements[&id].obligations;
        let status = if ids
            .iter()
            .all(|id| world.obligations[id].performed >= world.obligations[id].term.amount())
        {
            AgreementStatus::Fulfilled
        } else if ids.iter().any(|id| world.obligations[id].defaulted) {
            AgreementStatus::Breached
        } else {
            AgreementStatus::Active
        };
        world.agreements.get_mut(&id).unwrap().status = status;
    }

    pub(crate) fn next_agreement_deadline(&self) -> Option<SimTime> {
        self.state
            .agreements
            .as_ref()?
            .obligations
            .values()
            .filter(|o| !o.defaulted && o.performed < o.term.amount())
            .filter_map(|o| o.term.deadline())
            .filter(|d| *d > self.state.time)
            .min()
    }

    /// Each eligible work millisecond can satisfy at most one exclusive labor promise.
    pub(crate) fn record_agreement_work(&mut self, actor: PersonId, site: SiteId, elapsed: u64) {
        let Some(world) = self.state.agreements.as_mut() else {
            return;
        };
        let mut remaining = elapsed;
        let mut records = Vec::new();
        for o in world.obligations.values_mut() {
            if let AgreementTerm::Labor {
                debtor,
                site: promised,
                millis,
                ..
            } = o.term
                && debtor == actor
                && promised == site
            {
                let credit = remaining.min(millis - o.performed);
                o.performed += credit;
                remaining -= credit;
                if credit > 0 {
                    records.push((o.id, o.agreement, credit));
                }
            }
        }
        for (obligation, agreement, quantity) in records {
            *self
                .state
                .agreements
                .as_mut()
                .unwrap()
                .pending_work
                .entry(obligation)
                .or_default() += quantity;
            self.refresh_agreement_status(agreement);
        }
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
            || events
                .iter()
                .any(|e| e.time == self.state.time && matches!(e.kind, EventKind::Produced { .. }))
            || world
                .obligations
                .values()
                .any(|o| o.term.deadline() == Some(self.state.time));
        let last_event = self.state.next_event - 1;
        let mut credits = Vec::new();
        for e in events {
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
                        let AgreementTerm::Delivery {
                            debtor,
                            source: origin,
                            destination: goal,
                            quantity: promised,
                            ..
                        } = o.term
                        else {
                            continue;
                        };
                        let source_cargo = o.cargo.remove(&source).unwrap_or(0);
                        let from_cargo = source_cargo.min(unallocated);
                        if source_cargo > from_cargo {
                            o.cargo.insert(source, source_cargo - from_cargo);
                        }
                        unallocated -= from_cargo;
                        let direct = source == origin && actor == debtor;
                        let eligible = if direct { unallocated } else { from_cargo };
                        let outstanding = promised.0 - o.performed;
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
        let defaults: Vec<_> = self
            .state
            .agreements
            .as_ref()
            .unwrap()
            .obligations
            .values()
            .filter(|o| {
                !o.defaulted
                    && o.performed < o.term.amount()
                    && o.term.deadline().is_some_and(|d| d <= self.state.time)
            })
            .map(|o| (o.id, o.agreement))
            .collect();
        for (obligation, agreement) in defaults {
            self.state
                .agreements
                .as_mut()
                .unwrap()
                .obligations
                .get_mut(&obligation)
                .unwrap()
                .defaulted = true;
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
        for a in world
            .agreements
            .values()
            .filter(|a| a.status == AgreementStatus::Proposed)
        {
            for actor in a
                .parties
                .iter()
                .filter(|id| **id != self.state.ruler && !a.accepted.contains(id))
            {
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
        let world = self.state.agreements.as_ref().unwrap();
        for m in world
            .amendments
            .values()
            .filter(|m| m.status == AmendmentStatus::Proposed)
        {
            let a = &world.agreements[&m.agreement];
            let p = a.procedure.as_ref().unwrap();
            let eligible: BTreeSet<_> = p
                .approvers
                .iter()
                .copied()
                .chain(
                    p.term_approvers
                        .values()
                        .flat_map(|ids| ids.iter().copied()),
                )
                .collect();
            let mut revised = a.terms.clone();
            for (index, term) in &m.replacements {
                revised[*index as usize] = term.clone();
            }
            for actor in eligible
                .iter()
                .filter(|actor| **actor != self.state.ruler && !m.votes.contains_key(actor))
            {
                if self.agreement_actor_area(*actor).ok()
                    != self.agreement_actor_area(m.proposer).ok()
                {
                    continue;
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
        for (actor, vote) in votes {
            let _ = self.agreement_command(actor, &vote);
        }
        self.seek_local_exchange()?;
        self.perform_chosen_agreement_tasks();
        Ok(())
    }

    fn agreement_offer_score(&self, actor: PersonId, terms: &[AgreementTerm]) -> i64 {
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        let mut score = 0_i64;
        for term in terms {
            match *term {
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
                || world.agreements.values().any(|a| {
                    a.parties.contains(actor)
                        && matches!(
                            a.status,
                            AgreementStatus::Proposed | AgreementStatus::Active
                        )
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
        for o in self
            .state
            .agreements
            .as_ref()
            .unwrap()
            .obligations
            .values()
            .filter(|o| o.performed < o.term.amount())
        {
            let debtor = o.term.parties()[0];
            if debtor == self.state.ruler || intentions.contains_key(&debtor) {
                continue;
            }
            let Ok(area) = self.agreement_actor_area(debtor) else {
                continue;
            };
            let body = &self.state.physical.as_ref().unwrap().actors[&debtor];
            let mind = &self.state.decisions.as_ref().unwrap().minds[&debtor];
            if !matches!(body.activity, Activity::Idle | Activity::Rest)
                || body.vitals.hunger > 120_000
                || body.vitals.fatigue > 120_000
                || (mind.personality.discipline < 500 && mind.personality.honesty < 500)
            {
                continue;
            }
            let command = match o.term {
                AgreementTerm::Labor { site, .. } => {
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
                                && i.good == self.state.inventories[&source].good
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
                let _ = self.debug_physical_command(actor, command);
            }
        }
    }
}

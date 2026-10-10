use crate::*;

/// Only the people and assets relevant to this proposed interaction. The host
/// establishes actual hearing/contact and each participant's current agency.
#[derive(Clone, Debug, Default)]
pub struct TransitionFacts {
    /// The body's owner establishes alive/agency/action capability, without
    /// granting agreement authority. Parties and chosen procedures own that.
    pub actor_available: bool,
    /// Relevant people who can actually hear this interaction now. This is not
    /// an institution's membership or a world-wide person/location catalog.
    pub present: BTreeSet<PersonId>,
    /// Current referenced asset kinds, capacity and acknowledged grant rights.
    /// A promise does not require a present balance and cannot create one.
    pub assets: TermsContext,
}
#[derive(Clone, Debug)]
pub struct TransitionRequirements {
    pub contact_people: BTreeSet<PersonId>,
    pub terms: Vec<AgreementTerm>,
    pub allocation: AllocationNeeds,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocationNeeds {
    pub agreement: bool,
    pub amendment: bool,
    pub obligations: usize,
}
#[derive(Clone, Debug, Default)]
pub struct TransitionIds {
    pub agreement: Option<AgreementId>,
    pub amendment: Option<AmendmentId>,
    pub obligations: Vec<ObligationId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransitionEvent {
    Proposed {
        agreement: AgreementId,
        proposer: PersonId,
    },
    Activated {
        agreement: AgreementId,
    },
    Rejected {
        agreement: AgreementId,
        actor: PersonId,
    },
    Amended {
        agreement: AgreementId,
        amendment: AmendmentId,
    },
    Performed {
        obligation: ObligationId,
        actor: PersonId,
        quantity: Quantity,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionNotice {
    pub agreement: AgreementId,
    pub amendment: Option<AmendmentId>,
}
#[derive(Clone, Debug)]
struct Patch<K, T> {
    id: K,
    before: Option<T>,
    after: T,
}
#[derive(Clone, Debug, Default)]
pub struct PreparedTransition {
    agreements: Vec<Patch<AgreementId, Agreement>>,
    amendments: Vec<Patch<AmendmentId, Amendment>>,
    obligations: Vec<Patch<ObligationId, Obligation>>,
    pending: Vec<(ObligationId, u64)>,
    flushed_work_ids: Option<BTreeSet<ObligationId>>,
    revision_peers: Option<(AgreementId, u64, BTreeSet<AmendmentId>)>,
    pub permissions: Vec<PermissionEffect>,
    pub events: Vec<TransitionEvent>,
    pub notices: Vec<TransitionNotice>,
}
impl PreparedTransition {
    pub fn check(&self, state: &State) -> Result<(), Error> {
        let fresh = self
            .agreements
            .iter()
            .filter(|p| p.before.is_none())
            .map(|p| p.id.0)
            .chain(
                self.amendments
                    .iter()
                    .filter(|p| p.before.is_none())
                    .map(|p| p.id.0),
            )
            .chain(
                self.obligations
                    .iter()
                    .filter(|p| p.before.is_none())
                    .map(|p| p.id.0),
            );
        if self
            .flushed_work_ids
            .as_ref()
            .is_some_and(|ids| state.pending_work.keys().copied().collect::<BTreeSet<_>>() != *ids)
            || self
                .revision_peers
                .as_ref()
                .is_some_and(|(agreement, revision, peers)| {
                    amendment_peers(state, *agreement, *revision) != *peers
                })
            || fresh.into_iter().any(|id| id_exists(state, id))
            || self
                .agreements
                .iter()
                .any(|p| state.agreements.get(&p.id) != p.before.as_ref())
            || self
                .amendments
                .iter()
                .any(|p| state.amendments.get(&p.id) != p.before.as_ref())
            || self
                .obligations
                .iter()
                .any(|p| state.obligations.get(&p.id) != p.before.as_ref())
            || self
                .pending
                .iter()
                .any(|(id, value)| state.pending_work.get(id) != Some(value))
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    /// The coordinator first checks this patch and preflights every actual
    /// permission/event/ID effect. This operation owns only agreement records.
    pub fn commit(self, state: &mut State) -> Result<(), Error> {
        self.check(state)?;
        for p in self.agreements {
            state.agreements.insert(p.id, p.after);
        }
        for p in self.amendments {
            state.amendments.insert(p.id, p.after);
        }
        for p in self.obligations {
            state.obligations.insert(p.id, p.after);
        }
        for (id, _) in self.pending {
            state.pending_work.remove(&id);
        }
        Ok(())
    }
    fn agreement(&mut self, state: &State, after: Agreement) {
        self.agreements.push(Patch {
            id: after.id,
            before: state.agreements.get(&after.id).cloned(),
            after,
        });
    }
    fn amendment(&mut self, state: &State, after: Amendment) {
        self.amendments.push(Patch {
            id: after.id,
            before: state.amendments.get(&after.id).cloned(),
            after,
        });
    }
}

impl State {
    pub fn transition_requirements(
        &self,
        action: &AgreementAction,
    ) -> Result<TransitionRequirements, Error> {
        let allocation = self.allocation_needs(action)?;
        let (contact_people, terms) = match action {
            AgreementAction::Propose { parties, terms, .. } => {
                if parties.len() > 8 || terms.len() > 16 {
                    return Err(Error::Unavailable);
                }
                (parties.clone(), terms.clone())
            }
            AgreementAction::Counteroffer {
                agreement, terms, ..
            } => {
                if terms.len() > 16 {
                    return Err(Error::Unavailable);
                }
                (
                    self.agreements
                        .get(agreement)
                        .ok_or(Error::Unavailable)?
                        .parties
                        .clone(),
                    terms.clone(),
                )
            }
            AgreementAction::Accept { agreement } => {
                let a = self.agreements.get(agreement).ok_or(Error::Unavailable)?;
                (BTreeSet::from([a.proposer]), a.terms.clone())
            }
            AgreementAction::Amend { replacements, .. } => {
                if replacements.len() > 16 {
                    return Err(Error::Unavailable);
                }
                (BTreeSet::new(), replacements.values().cloned().collect())
            }
            AgreementAction::Vote { amendment, .. } => {
                let m = self.amendments.get(amendment).ok_or(Error::Unavailable)?;
                (
                    BTreeSet::from([m.proposer]),
                    m.replacements.values().cloned().collect(),
                )
            }
            AgreementAction::Reject { .. } | AgreementAction::Withdraw { .. } => {
                (BTreeSet::new(), Vec::new())
            }
            AgreementAction::Report { .. } => return Err(Error::Unavailable),
        };
        Ok(TransitionRequirements {
            contact_people,
            terms,
            allocation,
        })
    }
    pub fn allocation_needs(&self, action: &AgreementAction) -> Result<AllocationNeeds, Error> {
        let mut needs = AllocationNeeds::default();
        match action {
            AgreementAction::Propose { .. } | AgreementAction::Counteroffer { .. } => {
                needs.agreement = true
            }
            AgreementAction::Amend { .. } => needs.amendment = true,
            AgreementAction::Accept { agreement } => {
                let a = self.agreements.get(agreement).ok_or(Error::Unavailable)?;
                if a.accepted.len() + 1 == a.parties.len() {
                    needs.obligations = a.terms.len();
                }
            }
            AgreementAction::Reject { .. }
            | AgreementAction::Withdraw { .. }
            | AgreementAction::Vote { .. } => {}
            AgreementAction::Report { .. } => return Err(Error::Unavailable),
        }
        Ok(needs)
    }
    pub fn prepare_transition(
        &self,
        actor: PersonId,
        action: &AgreementAction,
        facts: &TransitionFacts,
        ids: &TransitionIds,
    ) -> Result<PreparedTransition, Error> {
        if actor.0 == 0 || !facts.actor_available || !facts.present.contains(&actor) {
            return Err(Error::Unavailable);
        }
        let needs = self.allocation_needs(action)?;
        if ids.agreement.is_some() != needs.agreement
            || ids.amendment.is_some() != needs.amendment
            || ids.obligations.len() != needs.obligations
        {
            return Err(Error::Unavailable);
        }
        let mut supplied = BTreeSet::new();
        for id in ids
            .agreement
            .iter()
            .map(|id| id.0)
            .chain(ids.amendment.iter().map(|id| id.0))
            .chain(ids.obligations.iter().map(|id| id.0))
        {
            if id == 0 || !supplied.insert(id) || id_exists(self, id) {
                return Err(Error::Unavailable);
            }
        }
        let mut plan = PreparedTransition::default();
        let now = facts.assets.now;
        match action {
            AgreementAction::Propose {
                parties,
                terms,
                procedure,
            } => {
                validate_proposal(actor, parties, terms, procedure, facts)?;
                proposal(
                    &mut plan,
                    ids.agreement.unwrap(),
                    actor,
                    parties,
                    terms,
                    procedure,
                    now,
                );
            }
            AgreementAction::Counteroffer {
                agreement,
                terms,
                procedure,
            } => {
                let old = self.agreements.get(agreement).ok_or(Error::Unavailable)?;
                if old.status != AgreementStatus::Proposed
                    || !old.parties.contains(&actor)
                    || old.proposer == actor
                {
                    return Err(Error::Unavailable);
                }
                validate_proposal(actor, &old.parties, terms, procedure, facts)?;
                let mut withdrawn = old.clone();
                withdrawn.status = AgreementStatus::Withdrawn;
                plan.agreement(self, withdrawn);
                proposal(
                    &mut plan,
                    ids.agreement.unwrap(),
                    actor,
                    &old.parties,
                    terms,
                    procedure,
                    now,
                );
                plan.notices.push(TransitionNotice {
                    agreement: *agreement,
                    amendment: None,
                });
            }
            AgreementAction::Accept { agreement } => {
                let a = self.agreements.get(agreement).ok_or(Error::Unavailable)?;
                if a.status != AgreementStatus::Proposed
                    || !a.parties.contains(&actor)
                    || a.accepted.contains(&actor)
                    || !facts.present.contains(&a.proposer)
                {
                    return Err(Error::Unavailable);
                }
                validate_terms(&a.parties, &a.terms, &facts.assets, true)?;
                let mut after = a.clone();
                after.accepted.insert(actor);
                if after.accepted == after.parties {
                    let activation = activation_for(&after, &ids.obligations, &facts.assets)?;
                    after = activation.after;
                    plan.permissions = activation.permissions;
                    for o in activation.obligations {
                        plan.obligations.push(Patch {
                            id: o.id,
                            before: None,
                            after: o,
                        });
                    }
                    plan.events.push(TransitionEvent::Activated {
                        agreement: *agreement,
                    });
                }
                plan.agreement(self, after);
                plan.notices.push(TransitionNotice {
                    agreement: *agreement,
                    amendment: None,
                });
            }
            AgreementAction::Reject { agreement } | AgreementAction::Withdraw { agreement } => {
                let a = self.agreements.get(agreement).ok_or(Error::Unavailable)?;
                let withdrawal = matches!(action, AgreementAction::Withdraw { .. });
                if a.status != AgreementStatus::Proposed
                    || (withdrawal && a.proposer != actor)
                    || (!withdrawal && !a.parties.contains(&actor))
                {
                    return Err(Error::Unavailable);
                }
                let mut after = a.clone();
                after.status = if withdrawal {
                    AgreementStatus::Withdrawn
                } else {
                    AgreementStatus::Rejected
                };
                plan.agreement(self, after);
                plan.events.push(TransitionEvent::Rejected {
                    agreement: *agreement,
                    actor,
                });
                plan.notices.push(TransitionNotice {
                    agreement: *agreement,
                    amendment: None,
                });
            }
            AgreementAction::Amend {
                agreement,
                replacements,
            } => {
                let a = self.agreements.get(agreement).ok_or(Error::Unavailable)?;
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
                        .any(|index| usize::from(*index) >= a.terms.len())
                {
                    return Err(Error::Unavailable);
                }
                validate_terms(
                    &a.parties,
                    &replacements.values().cloned().collect::<Vec<_>>(),
                    &facts.assets,
                    false,
                )?;
                let id = ids.amendment.unwrap();
                plan.agreement(self, a.clone());
                plan.amendment(
                    self,
                    Amendment {
                        id,
                        agreement: *agreement,
                        proposer: actor,
                        replacements: replacements.clone(),
                        votes: BTreeMap::new(),
                        status: AmendmentStatus::Proposed,
                        base_revision: a.revision,
                    },
                );
                plan.notices.push(TransitionNotice {
                    agreement: *agreement,
                    amendment: Some(id),
                });
            }
            AgreementAction::Vote { amendment, support } => {
                let before = self.amendments.get(amendment).ok_or(Error::Unavailable)?;
                let a = self
                    .agreements
                    .get(&before.agreement)
                    .ok_or(Error::Unavailable)?;
                let procedure = a.procedure.as_ref().ok_or(Error::Unavailable)?;
                if before.base_revision != a.revision
                    || before.status != AmendmentStatus::Proposed
                    || (!procedure.approvers.contains(&actor)
                        && !procedure
                            .term_approvers
                            .values()
                            .any(|p| p.contains(&actor)))
                    || before.votes.contains_key(&actor)
                    || !facts.present.contains(&before.proposer)
                {
                    return Err(Error::Unavailable);
                }
                a.revision.checked_add(1).ok_or(Error::Overflow)?;
                validate_terms(
                    &a.parties,
                    &before.replacements.values().cloned().collect::<Vec<_>>(),
                    &facts.assets,
                    false,
                )?;
                let mut after = before.clone();
                after.votes.insert(actor, *support);
                resolve_vote(self, a, procedure, &mut after, &mut plan, now)?;
                if plan.agreements.is_empty() {
                    plan.agreement(self, a.clone());
                }
                plan.amendment(self, after);
                plan.notices.push(TransitionNotice {
                    agreement: a.id,
                    amendment: Some(*amendment),
                });
            }
            AgreementAction::Report { .. } => return Err(Error::Unavailable),
        }
        Ok(plan)
    }
}

fn validate_proposal(
    actor: PersonId,
    parties: &BTreeSet<PersonId>,
    terms: &[AgreementTerm],
    procedure: &Option<AmendmentProcedure>,
    facts: &TransitionFacts,
) -> Result<(), Error> {
    if !(2..=8).contains(&parties.len())
        || !parties.contains(&actor)
        || !parties.is_subset(&facts.present)
    {
        return Err(Error::Unavailable);
    }
    validate_terms(parties, terms, &facts.assets, true)?;
    if let Some(p) = procedure
        && (p.eligible_proposers.is_empty()
            || !p.eligible_proposers.is_subset(parties)
            || !p.approvers.is_subset(parties)
            || p.threshold == 0
            || p.threshold > p.approvers.len()
            || p.term_approvers.iter().any(|(index, people)| {
                usize::from(*index) >= terms.len()
                    || people.is_empty()
                    || !people.is_subset(parties)
            }))
    {
        return Err(Error::Unavailable);
    }
    Ok(())
}
fn proposal(
    plan: &mut PreparedTransition,
    id: AgreementId,
    actor: PersonId,
    parties: &BTreeSet<PersonId>,
    terms: &[AgreementTerm],
    procedure: &Option<AmendmentProcedure>,
    now: SimTime,
) {
    plan.agreements.push(Patch {
        id,
        before: None,
        after: Agreement {
            id,
            proposer: actor,
            parties: parties.clone(),
            accepted: BTreeSet::from([actor]),
            terms: terms.to_vec(),
            procedure: procedure.clone(),
            obligations: Vec::new(),
            status: AgreementStatus::Proposed,
            proposed_at: now,
            revision: 0,
        },
    });
    plan.events.push(TransitionEvent::Proposed {
        agreement: id,
        proposer: actor,
    });
    plan.notices.push(TransitionNotice {
        agreement: id,
        amendment: None,
    });
}
fn resolve_vote(
    state: &State,
    a: &Agreement,
    p: &AmendmentProcedure,
    m: &mut Amendment,
    plan: &mut PreparedTransition,
    now: SimTime,
) -> Result<(), Error> {
    let permitted_negative = p
        .approvers
        .len()
        .checked_sub(p.threshold)
        .ok_or(Error::Unavailable)?;
    if p.threshold == 0 {
        return Err(Error::Unavailable);
    }
    let support = p
        .approvers
        .iter()
        .filter(|actor| m.votes.get(actor) == Some(&true))
        .count();
    let required: BTreeSet<_> = m
        .replacements
        .keys()
        .filter_map(|index| p.term_approvers.get(index))
        .flat_map(|people| people.iter().copied())
        .collect();
    let rejected = p
        .approvers
        .iter()
        .filter(|actor| m.votes.get(actor) == Some(&false))
        .count()
        > permitted_negative
        || required
            .iter()
            .any(|actor| m.votes.get(actor) == Some(&false));
    if rejected {
        m.status = AmendmentStatus::Rejected;
        return Ok(());
    }
    if support < p.threshold
        || required
            .iter()
            .any(|actor| m.votes.get(actor) != Some(&true))
    {
        return Ok(());
    }
    // Flush logically observed prior work before an adopted amendment changes
    // the identity of a duty. No promised signature creates performance credit.
    plan.flushed_work_ids = Some(state.pending_work.keys().copied().collect());
    plan.revision_peers = Some((a.id, a.revision, amendment_peers(state, a.id, a.revision)));
    for (id, quantity) in &state.pending_work {
        let o = state.obligations.get(id).ok_or(Error::Unavailable)?;
        plan.pending.push((*id, *quantity));
        plan.events.push(TransitionEvent::Performed {
            obligation: *id,
            actor: o.term.parties()[0],
            quantity: Quantity(*quantity),
        });
    }
    let mut after = a.clone();
    let mut obligations: BTreeMap<_, _> = a
        .obligations
        .iter()
        .map(|id| {
            state
                .obligations
                .get(id)
                .cloned()
                .map(|o| (*id, o))
                .ok_or(Error::Unavailable)
        })
        .collect::<Result<_, _>>()?;
    for (index, term) in &m.replacements {
        let id = *a
            .obligations
            .get(usize::from(*index))
            .ok_or(Error::Unavailable)?;
        let o = obligations.get_mut(&id).ok_or(Error::Unavailable)?;
        let same = same_duty(&o.term, term);
        let permission = permission_effect(term);
        if let Some(effect) = permission {
            plan.permissions.push(effect);
        }
        o.performed = if permission.is_some() {
            1
        } else if same {
            o.performed
                .min(if matches!(term, AgreementTerm::OutputShare { .. }) {
                    o.accrued
                } else {
                    term.amount()
                })
        } else {
            0
        };
        if !same {
            o.accrued = 0;
            o.share_remainder = 0;
            o.share_scale = 0;
            o.cargo.clear();
        }
        o.term = term.clone();
        let mut remaining = o.amount() - o.performed;
        for quantity in o.cargo.values_mut() {
            *quantity = (*quantity).min(remaining);
            remaining -= *quantity;
        }
        o.cargo.retain(|_, quantity| *quantity > 0);
        o.defaulted = o.performed < o.amount() && term.deadline().is_some_and(|due| due <= now);
        after.terms[usize::from(*index)] = term.clone();
        plan.obligations.push(Patch {
            id,
            before: state.obligations.get(&id).cloned(),
            after: o.clone(),
        });
    }
    // Status also depends on unchanged duties. Guard their actual performance
    // so a prepared adoption cannot overwrite a later default/fulfillment.
    for (id, o) in &obligations {
        if !plan.obligations.iter().any(|p| p.id == *id) {
            plan.obligations.push(Patch {
                id: *id,
                before: state.obligations.get(id).cloned(),
                after: o.clone(),
            });
        }
    }
    for (id, _) in &plan.pending {
        if !plan.obligations.iter().any(|patch| patch.id == *id) {
            let before = state.obligations.get(id).ok_or(Error::Unavailable)?.clone();
            plan.obligations.push(Patch {
                id: *id,
                before: Some(before.clone()),
                after: before,
            });
        }
    }
    after.revision = after.revision.checked_add(1).ok_or(Error::Overflow)?;
    after.status = if obligations
        .values()
        .all(|o| o.performed >= o.amount() && stream_ended(&o.term, now))
    {
        AgreementStatus::Fulfilled
    } else if obligations.values().any(|o| o.defaulted) {
        AgreementStatus::Breached
    } else {
        AgreementStatus::Active
    };
    plan.agreement(state, after);
    m.status = AmendmentStatus::Adopted;
    for other in state.amendments.values().filter(|other| {
        other.id != m.id
            && other.agreement == a.id
            && other.status == AmendmentStatus::Proposed
            && other.base_revision == a.revision
    }) {
        let mut rejected = other.clone();
        rejected.status = AmendmentStatus::Rejected;
        plan.amendment(state, rejected);
    }
    plan.events.push(TransitionEvent::Amended {
        agreement: a.id,
        amendment: m.id,
    });
    Ok(())
}

pub fn same_duty(a: &AgreementTerm, b: &AgreementTerm) -> bool {
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
        (AgreementTerm::Access { .. }, AgreementTerm::Access { .. })
        | (AgreementTerm::SiteAccess { .. }, AgreementTerm::SiteAccess { .. }) => a == b,
        _ => false,
    }
}

fn id_exists(state: &State, id: u64) -> bool {
    state.agreements.contains_key(&AgreementId(id))
        || state.amendments.contains_key(&AmendmentId(id))
        || state.obligations.contains_key(&ObligationId(id))
}

fn amendment_peers(state: &State, agreement: AgreementId, revision: u64) -> BTreeSet<AmendmentId> {
    state
        .amendments
        .values()
        .filter(|m| {
            m.agreement == agreement
                && m.base_revision == revision
                && m.status == AmendmentStatus::Proposed
        })
        .map(|m| m.id)
        .collect()
}

/// Consent to an already communicated service offer. The service owner must
/// guard the referenced recruitment and its current terms in the same commit.
/// This is not a mandate or permission to sign arbitrary parties' promises.
#[derive(Clone, Debug)]
pub struct HeardServiceConsent {
    pub recruitment: RecruitmentId,
    pub sponsor: PersonId,
    pub recruit: PersonId,
    pub offered_terms: Vec<AgreementTerm>,
    pub accepted_terms: Vec<AgreementTerm>,
    pub unanswered_counteroffer: bool,
}
impl State {
    pub fn prepare_service_commitment(
        &self,
        consent: &HeardServiceConsent,
        facts: &TransitionFacts,
        ids: &TransitionIds,
    ) -> Result<PreparedTransition, Error> {
        if consent.recruitment.0 == 0
            || consent.sponsor == consent.recruit
            || consent.offered_terms.is_empty()
            || consent.offered_terms.len() > 8
            || consent.offered_terms != consent.accepted_terms
            || consent.unanswered_counteroffer
            || ids.obligations.len() != consent.offered_terms.len()
            || ids.amendment.is_some()
            || ids.agreement.is_none()
        {
            return Err(Error::Unavailable);
        }
        let parties = BTreeSet::from([consent.sponsor, consent.recruit]);
        let propose = AgreementAction::Propose {
            parties,
            terms: consent.offered_terms.clone(),
            procedure: None,
        };
        let proposal_ids = TransitionIds {
            agreement: ids.agreement,
            ..TransitionIds::default()
        };
        let mut plan = self.prepare_transition(consent.sponsor, &propose, facts, &proposal_ids)?;
        let mut unique = BTreeSet::from([ids.agreement.unwrap().0]);
        for id in &ids.obligations {
            if id.0 == 0 || !unique.insert(id.0) || id_exists(self, id.0) {
                return Err(Error::Unavailable);
            }
        }
        let proposal = &mut plan.agreements[0].after;
        proposal.accepted.insert(consent.recruit);
        let activation = activation_for(proposal, &ids.obligations, &facts.assets)?;
        plan.agreements[0].after = activation.after;
        plan.permissions = activation.permissions;
        plan.obligations = activation
            .obligations
            .into_iter()
            .map(|after| Patch {
                id: after.id,
                before: None,
                after,
            })
            .collect();
        plan.events.push(TransitionEvent::Activated {
            agreement: ids.agreement.unwrap(),
        });
        Ok(plan)
    }
}

//! Public choices are drawn from a person's dated terms and present contact.
use crate::*;
pub fn available_actions(
    store: &Store,
    now: SimTime,
    actor: PersonId,
    presence: &impl Presence,
) -> Vec<AgreementAction> {
    let Some(area) = contact_area(presence, actor) else {
        return Vec::new();
    };
    let known = view(store, actor);
    let mut actions = Vec::new();
    for a in known
        .iter()
        .filter(|a| a.parties.contains(&actor) && a.status == AgreementStatus::Proposed)
        .take(32)
    {
        if contact_area(presence, a.proposer) != Some(area) {
            continue;
        }
        actions.push(if a.proposer == actor {
            AgreementAction::Withdraw { agreement: a.id }
        } else {
            AgreementAction::Reject { agreement: a.id }
        });
        if !a.accepted.contains(&actor)
            && a.terms
                .iter()
                .all(|term| !term.deadline().is_some_and(|deadline| deadline <= now))
        {
            actions.push(AgreementAction::Accept { agreement: a.id });
        }
    }
    for (a, m) in known
        .iter()
        .filter(|a| a.parties.contains(&actor))
        .flat_map(|a| a.amendments.iter().map(move |m| (a, m)))
        .filter(|(_, m)| m.status == AmendmentStatus::Proposed)
        .take(32)
    {
        let Some(p) = &a.procedure else {
            continue;
        };
        if m.base_revision != a.revision
            || m.votes.contains_key(&actor)
            || contact_area(presence, m.proposer) != Some(area)
            || (!p.approvers.contains(&actor)
                && !p.term_approvers.values().any(|ids| ids.contains(&actor)))
        {
            continue;
        }
        for support in [true, false] {
            actions.push(AgreementAction::Vote {
                amendment: m.id,
                support,
            });
        }
    }
    actions
}
pub fn accepts_delivery(
    store: &Store,
    actor: PersonId,
    destination: InventoryId,
    quantity: Quantity,
    actor_area: Option<AreaId>,
    inventory_area: Option<AreaId>,
) -> bool {
    let Some(inventory) = game_cargo::inventory(store, destination) else {
        return false;
    };
    let Some(area) = actor_area else {
        return false;
    };
    if inventory_area != Some(area) {
        return false;
    }
    obligations(store).any(|(_, o)| {
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
            && agreement(store, o.agreement).is_some_and(|a| {
                matches!(
                    a.status,
                    AgreementStatus::Active | AgreementStatus::Breached
                )
            })
            && quantity.0 <= promised.saturating_sub(o.performed)
            && (inventory.location == game_cargo::InventoryLocation::Carried(creditor)
                || game_property::inventory_custodian(store, destination) == Some(creditor))
    })
}
/// Referenced people are collected by the owner, then the presence provider
/// supplies raw local facts. Admission still owns consent and chosen procedure.
pub fn transition_facts(
    store: &Store,
    actor: PersonId,
    action: &AgreementAction,
    assets: TermsContext,
    presence: &impl Presence,
) -> Result<TransitionFacts, Error> {
    let area = contact_area(presence, actor).ok_or(Error::Unavailable)?;
    let mut relevant = Ledger::new(store)
        .transition_requirements(action)?
        .contact_people;
    relevant.insert(actor);
    Ok(TransitionFacts {
        actor_available: true,
        present: relevant
            .into_iter()
            .filter(|id| contact_area(presence, *id) == Some(area))
            .collect(),
        assets,
    })
}
/// Project the exact assets referenced by these terms; asset-owner queries are
/// factual and do not author or accept the promises.
pub fn terms_context(
    store: &Store,
    now: SimTime,
    terms: &[AgreementTerm],
    assets: &impl TermAssets,
) -> TermsContext {
    let mut context = TermsContext {
        now,
        ..TermsContext::default()
    };
    for term in terms {
        let ids = match *term {
            AgreementTerm::Delivery {
                source,
                destination,
                ..
            } => vec![source, destination],
            AgreementTerm::Access { inventory, .. } => vec![inventory],
            AgreementTerm::OutputShare { destination, .. } => vec![destination],
            _ => Vec::new(),
        };
        for id in ids {
            if let Some(row) = game_cargo::inventory(store, id) {
                context.inventories.insert(
                    id,
                    InventoryTerms {
                        good: row.good,
                        capacity: row.capacity,
                    },
                );
            }
        }
        match *term {
            AgreementTerm::Labor { site, .. } => {
                if assets.site_exists(site) {
                    context.sites.insert(site);
                }
            }
            AgreementTerm::ConstructionLabor { project, .. } => {
                if assets.project_exists(project) {
                    context.projects.insert(project);
                }
            }
            AgreementTerm::Access {
                grantor, inventory, ..
            } => {
                if game_cargo::inventory(store, inventory).is_some_and(|row| {
                    game_property::inventory_custodian(store, inventory) == Some(grantor)
                        || row.location == game_cargo::InventoryLocation::Carried(grantor)
                }) {
                    context
                        .grants
                        .insert((PermissionAsset::Inventory(inventory), grantor));
                }
            }
            AgreementTerm::SiteAccess { grantor, site, .. } => {
                if assets.site_custodian(site) == Some(grantor) {
                    context
                        .grants
                        .insert((PermissionAsset::Site(site), grantor));
                }
            }
            AgreementTerm::OutputShare { site, good, .. } => {
                if let Some(source) = assets.output_source(site, good) {
                    context.outputs.insert((site, good), source);
                }
            }
            _ => {}
        }
    }
    context
}
pub fn transition_ids(first: u64, needs: AllocationNeeds) -> Result<(TransitionIds, u64), Error> {
    let mut next = first;
    let mut ids = TransitionIds::default();
    if needs.agreement {
        ids.agreement = Some(AgreementId(next));
        next = next.checked_add(1).ok_or(Error::Overflow)?;
    }
    if needs.amendment {
        ids.amendment = Some(AmendmentId(next));
        next = next.checked_add(1).ok_or(Error::Overflow)?;
    }
    let end = next
        .checked_add(u64::try_from(needs.obligations).map_err(|_| Error::Overflow)?)
        .ok_or(Error::Overflow)?;
    ids.obligations = (next..end).map(ObligationId).collect();
    Ok((ids, end))
}

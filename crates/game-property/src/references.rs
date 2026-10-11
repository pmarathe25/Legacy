//! Property owns the consistency rules for references to foreign components.
//! The provider supplies facts, never validation decisions.
use crate::*;
use game_core::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalActionReference {
    Idle,
    Taking {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        completes: SimTime,
    },
    Reporting {
        recipient: PersonId,
        incident: IncidentId,
        completes: SimTime,
    },
    Other,
}

#[derive(Clone, Copy, Debug)]
pub struct PersonReference {
    pub alive: bool,
    pub area: Option<AreaId>,
    pub action: PhysicalActionReference,
}

#[derive(Clone, Copy, Debug)]
pub struct InventoryReference {
    pub quantity: Quantity,
    pub good: Good,
    pub carrier: Option<PersonId>,
}

pub trait PropertyReferences {
    fn person(&self, id: PersonId) -> Option<PersonReference>;
    fn inventory(&self, id: InventoryId) -> Option<InventoryReference>;
    fn area_exists(&self, id: AreaId) -> bool;
    fn physical_completion_scheduled(&self, actor: PersonId, at: SimTime) -> bool;
}

#[derive(Clone, Copy, Debug)]
pub struct ValidationFacts {
    pub now: SimTime,
    pub next_id: u64,
    pub next_event: u64,
    pub inventory_count: usize,
}

fn invalid_reference() -> Error {
    Error::InvalidSave("invalid property references or physical actions".into())
}

pub fn validate_absent(
    store: &Store,
    actions: impl IntoIterator<Item = (PersonId, PhysicalActionReference)>,
) -> Result<(), Error> {
    if store.count::<Claim>() != 0
        || store.count::<Incident>() != 0
        || store.count::<Provenance>() != 0
        || store.count::<Attitude>() != 0
        || store.count::<Knowledge>() != 0
        || store.count::<Activity>() != 0
        || store.count::<Requests>() != 0
        || actions.into_iter().any(|(_, action)| {
            matches!(
                action,
                PhysicalActionReference::Taking { .. } | PhysicalActionReference::Reporting { .. }
            )
        })
    {
        return Err(invalid_reference());
    }
    Ok(())
}

impl Runtime {
    pub fn validate_composition(
        &self,
        store: &Store,
        facts: ValidationFacts,
        references: &impl PropertyReferences,
        actions: impl IntoIterator<Item = (PersonId, PhysicalActionReference)>,
    ) -> Result<(), Error> {
        self.validate_with_quantities(store, facts.now, facts.inventory_count, |id| {
            references.inventory(id).map(|stock| stock.quantity)
        })?;
        if self.last_event >= facts.next_event
            || attitudes(store)
                .map(|(id, _)| id)
                .chain(request_rows(store).map(|(id, _)| id))
                .chain(knowledges(store).map(|(id, _)| id))
                .chain(activities(store).map(|(id, _)| id))
                .any(|actor| references.person(actor).is_none())
        {
            return Err(invalid_reference());
        }
        for (id, claim) in claims(store) {
            if id.0 >= facts.next_id
                || references.person(claim.claimant).is_none()
                || references.inventory(claim.inventory).is_none()
                || claim
                    .recognized_by
                    .iter()
                    .chain(&claim.opposed_by)
                    .any(|person| references.person(*person).is_none())
            {
                return Err(invalid_reference());
            }
        }
        for (_, known) in knowledges(store) {
            if known.claims.values().any(|view| {
                view.recognized_by
                    .iter()
                    .chain(&view.opposed_by)
                    .any(|person| references.person(*person).is_none())
            }) || known
                .incidents
                .values()
                .any(|knowledge| !known_sender(references, knowledge))
            {
                return Err(invalid_reference());
            }
        }
        for (id, incident) in incidents(store) {
            if id.0 >= facts.next_id
                || references.person(incident.actor).is_none()
                || !references
                    .inventory(incident.source)
                    .zip(references.inventory(incident.destination))
                    .is_some_and(|(source, destination)| {
                        source.good == destination.good
                            && destination.carrier == Some(incident.actor)
                    })
                || !references.area_exists(incident.area)
            {
                return Err(invalid_reference());
            }
        }
        for (actor, id) in pending_rows(store) {
            let incident = incident(store, id).ok_or_else(invalid_reference)?;
            let person = references.person(actor).ok_or_else(invalid_reference)?;
            if !person.alive
                || person.area != Some(incident.area)
                || person.action
                    != (PhysicalActionReference::Taking {
                        source: incident.source,
                        destination: incident.destination,
                        quantity: incident.quantity,
                        completes: incident.completes,
                    })
                || !references.physical_completion_scheduled(actor, incident.completes)
            {
                return Err(invalid_reference());
            }
        }
        for (sender, report) in reports(store) {
            let source = references.person(sender).ok_or_else(invalid_reference)?;
            let recipient = references
                .person(report.recipient)
                .ok_or_else(invalid_reference)?;
            if !known_sender(references, &report.snapshot)
                || !source.alive
                || !recipient.alive
                || source.area != Some(report.area)
                || recipient.area != Some(report.area)
                || recipient.action != PhysicalActionReference::Idle
                || source.action
                    != (PhysicalActionReference::Reporting {
                        recipient: report.recipient,
                        incident: report.snapshot.incident.id,
                        completes: report.completes,
                    })
                || !references.physical_completion_scheduled(sender, report.completes)
            {
                return Err(invalid_reference());
            }
        }
        for (actor, action) in actions {
            match action {
                PhysicalActionReference::Taking { .. } if pending(store, actor).is_none() => {
                    return Err(invalid_reference());
                }
                PhysicalActionReference::Reporting {
                    recipient,
                    incident,
                    completes,
                } if !report(store, actor).is_some_and(|report| {
                    report.recipient == recipient
                        && report.snapshot.incident.id == incident
                        && report.completes == completes
                }) =>
                {
                    return Err(invalid_reference());
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn known_sender(references: &impl PropertyReferences, knowledge: &IncidentKnowledge) -> bool {
    !matches!(knowledge.source, IncidentSource::Reported { sender } if references.person(sender).is_none())
}

#[cfg(test)]
mod tests;

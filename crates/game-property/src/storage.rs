//! Canonical property rows in the shared store. Physical stock remains cargo-owned.
use crate::*;
use game_core::*;
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub portions: Portions,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub claims: BTreeMap<ClaimId, ClaimView>,
    pub incidents: BTreeMap<IncidentId, IncidentKnowledge>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub pending: Option<IncidentId>,
    pub report: Option<Report>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requests {
    pub next_search: SimTime,
}
fn invalid() -> Error {
    Error::InvalidConfiguration("invalid property component".into())
}
impl Claim {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.claimant.0 == 0
            || self.inventory.0 == 0
            || self.rights.is_empty()
            || self.portions.is_empty()
            || self.portions.iter().any(|id| id.0 == 0)
            || self.evidence.len() > 16
            || self.evidence.iter().any(|id| id.0 == 0)
            || !self.recognized_by.is_disjoint(&self.opposed_by)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Incident {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.actor.0 == 0
            || self.source.0 == 0
            || self.destination.0 == 0
            || self.area.0 == 0
            || self.source == self.destination
            || self.quantity.0 == 0
            || self.started_at >= self.completes
            || self.started_at > self.changed_at
            || self.outcome == IncidentOutcome::Taken && self.completes > self.changed_at
            || self.portions.values().any(|quantity| *quantity == 0)
            || self.portions.values().map(|q| u128::from(*q)).sum::<u128>()
                != u128::from(self.quantity.0)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Provenance {
    pub fn validate(&self) -> Result<(), Error> {
        if self.portions.values().any(|quantity| *quantity == 0) {
            return Err(invalid());
        }
        self.portions.values().try_fold(0_u64, |total, quantity| {
            total.checked_add(*quantity).ok_or_else(invalid)
        })?;
        Ok(())
    }
}
impl Report {
    pub fn validate(&self) -> Result<(), Error> {
        if self.sender.0 == 0
            || self.recipient.0 == 0
            || self.area.0 == 0
            || self.sender == self.recipient
            || self.started_at >= self.completes
            || self.snapshot.learned_at > self.started_at
            || self.snapshot.incident.changed_at > self.snapshot.learned_at
        {
            return Err(invalid());
        }
        self.snapshot.incident.validate()
    }
}
impl Knowledge {
    pub fn validate(&self) -> Result<(), Error> {
        for (id, claim) in &self.claims {
            if id.0 == 0 || *id != claim.id || !claim.recognized_by.is_disjoint(&claim.opposed_by) {
                return Err(invalid());
            }
        }
        for (id, knowledge) in &self.incidents {
            if *id != knowledge.incident.id || knowledge.incident.changed_at > knowledge.learned_at
            {
                return Err(invalid());
            }
            knowledge.incident.validate()?;
        }
        Ok(())
    }
}
impl Activity {
    pub fn validate(&self) -> Result<(), Error> {
        if self.pending.is_some_and(|id| id.0 == 0) {
            return Err(invalid());
        }
        if let Some(report) = &self.report {
            report.validate()?;
        }
        Ok(())
    }
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Claim>("property/claim", Claim::validate)?;
    schema.register::<Incident>("property/incident", Incident::validate)?;
    schema.register::<Provenance>("property/provenance", Provenance::validate)?;
    schema.register::<Attitude>("property/attitude", Attitude::validate)?;
    schema.register::<Knowledge>("property/knowledge", Knowledge::validate)?;
    schema.register::<Activity>("property/activity", Activity::validate)?;
    schema.register::<Requests>("property/requests", |_| Ok(()))
}
macro_rules! ports {
    ($row:ty,$key:ident,$get:ident,$get_mut:ident,$iter:ident,$insert:ident,$remove:ident) => {
        pub fn $get(store: &Store, id: $key) -> Option<&$row> {
            store.get(StableId(id.0))
        }
        pub fn $get_mut(store: &mut Store, id: $key) -> Option<&mut $row> {
            store.get_mut(StableId(id.0))
        }
        pub fn $iter(store: &Store) -> impl Iterator<Item = ($key, &$row)> {
            store.iter::<$row>().map(|(id, row)| ($key(id.0), row))
        }
        pub fn $insert(store: &mut Store, id: $key, row: $row) -> Result<(), Error> {
            store.insert_or_spawn(StableId(id.0), row)
        }
        pub fn $remove(store: &mut Store, id: $key) -> Result<Option<$row>, Error> {
            store.remove(StableId(id.0))
        }
    };
}
ports!(
    Provenance,
    InventoryId,
    provenance,
    provenance_mut,
    provenances,
    insert_provenance,
    remove_provenance
);
ports!(
    Attitude,
    PersonId,
    attitude,
    attitude_mut,
    attitudes,
    insert_attitude,
    remove_attitude
);
ports!(
    Knowledge,
    PersonId,
    knowledge,
    knowledge_mut,
    knowledges,
    insert_knowledge,
    remove_knowledge
);
ports!(
    Activity,
    PersonId,
    activity,
    activity_mut,
    activities,
    insert_activity,
    remove_activity
);
ports!(
    Requests,
    PersonId,
    requests,
    requests_mut,
    request_rows,
    insert_requests,
    remove_requests
);
pub fn claim(store: &Store, id: ClaimId) -> Option<&Claim> {
    store.get(StableId(id.0))
}
pub fn claim_mut(store: &mut Store, id: ClaimId) -> Option<&mut Claim> {
    store.get_mut(StableId(id.0))
}
pub fn claims(store: &Store) -> impl Iterator<Item = (ClaimId, &Claim)> {
    store.iter::<Claim>().map(|(id, row)| (ClaimId(id.0), row))
}
pub fn insert_claim(store: &mut Store, id: ClaimId, row: Claim) -> Result<(), Error> {
    if id != row.id {
        return Err(invalid());
    }
    store.insert_or_spawn(StableId(id.0), row)
}
pub fn remove_claim(store: &mut Store, id: ClaimId) -> Result<Option<Claim>, Error> {
    store.remove(StableId(id.0))
}
pub fn incident(store: &Store, id: IncidentId) -> Option<&Incident> {
    store.get(StableId(id.0))
}
pub fn incident_mut(store: &mut Store, id: IncidentId) -> Option<&mut Incident> {
    store.get_mut(StableId(id.0))
}
pub fn incidents(store: &Store) -> impl Iterator<Item = (IncidentId, &Incident)> {
    store
        .iter::<Incident>()
        .map(|(id, row)| (IncidentId(id.0), row))
}
pub fn insert_incident(store: &mut Store, id: IncidentId, row: Incident) -> Result<(), Error> {
    if id != row.id {
        return Err(invalid());
    }
    store.insert_or_spawn(StableId(id.0), row)
}
pub fn remove_incident(store: &mut Store, id: IncidentId) -> Result<Option<Incident>, Error> {
    store.remove(StableId(id.0))
}

/// Dated personal facts remain contained in their native knowledge row.
pub fn known_claims(store: &Store, person: PersonId) -> Option<&BTreeMap<ClaimId, ClaimView>> {
    knowledge(store, person).map(|row| &row.claims)
}
pub fn known_incidents(
    store: &Store,
    person: PersonId,
) -> Option<&BTreeMap<IncidentId, IncidentKnowledge>> {
    knowledge(store, person).map(|row| &row.incidents)
}
pub fn portions(store: &Store, inventory: InventoryId) -> Option<&Portions> {
    provenance(store, inventory).map(|row| &row.portions)
}
pub fn pending(store: &Store, person: PersonId) -> Option<&IncidentId> {
    activity(store, person).and_then(|row| row.pending.as_ref())
}
pub fn report(store: &Store, person: PersonId) -> Option<&Report> {
    activity(store, person).and_then(|row| row.report.as_ref())
}
pub fn reports(store: &Store) -> impl Iterator<Item = (PersonId, &Report)> {
    activities(store).filter_map(|(person, row)| row.report.as_ref().map(|report| (person, report)))
}
pub fn pending_rows(store: &Store) -> impl Iterator<Item = (PersonId, IncidentId)> {
    activities(store).filter_map(|(person, row)| row.pending.map(|incident| (person, incident)))
}
pub fn next_search(store: &Store, person: PersonId) -> Option<&SimTime> {
    requests(store, person).map(|row| &row.next_search)
}

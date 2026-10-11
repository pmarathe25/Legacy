use crate::*;
use game_core::*;
use std::collections::BTreeSet;

impl Attitude {
    pub fn validate(&self) -> Result<(), Error> {
        if [
            self.exclusive_claims,
            self.sharing,
            self.willingness_to_defend,
        ]
        .iter()
        .any(|value| *value > 1_000)
        {
            return Err(Error::InvalidConfiguration(
                "invalid property attitude".into(),
            ));
        }
        Ok(())
    }
}
/// Contextual owner validation reads rows directly, including the dated facts
/// people retain. Foreign stock quantities remain supplied by their own owner.
pub fn validate_runtime(runtime: &Runtime, store: &Store, now: SimTime) -> Result<(), Error> {
    runtime.validate_owned(store, now)
}
impl Runtime {
    pub fn validate_owned(&self, store: &Store, now: SimTime) -> Result<(), Error> {
        let bad = || {
            Error::InvalidSave("invalid property assertions, incidents or private knowledge".into())
        };
        for (_, value) in attitudes(store) {
            value.validate().map_err(|_| bad())?;
        }
        for (id, value) in claims(store) {
            value.validate().map_err(|_| bad())?;
            if id != value.id
                || value
                    .portions
                    .iter()
                    .any(|marker| claim(store, *marker).is_none())
                || value
                    .evidence
                    .iter()
                    .any(|id| incident(store, *id).is_none())
            {
                return Err(bad());
            }
        }
        for (_, row) in provenances(store) {
            row.validate().map_err(|_| bad())?;
            if row
                .portions
                .keys()
                .any(|marker| marker.0 != 0 && claim(store, *marker).is_none())
            {
                return Err(bad());
            }
        }
        for (observer, row) in knowledges(store) {
            row.validate().map_err(|_| bad())?;
            for (id, view) in &row.claims {
                let actual = claim(store, *id).ok_or_else(bad)?;
                if *id != view.id
                    || actual.claimant != view.claimant
                    || actual.inventory != view.inventory
                    || actual.rights != view.rights
                    || actual.evidence != view.evidence
                    || !view.recognized_by.is_disjoint(&view.opposed_by)
                {
                    return Err(bad());
                }
            }
            for (id, knowledge) in &row.incidents {
                let actual = incident(store, *id).ok_or_else(bad)?;
                if *id != knowledge.incident.id
                    || !self.incident_shape(store, &knowledge.incident, now)
                    || knowledge.learned_at > now
                    || knowledge.incident.changed_at > knowledge.learned_at
                    || knowledge.incident.changed_at > actual.changed_at
                    || !same_incident(&knowledge.incident, actual)
                    || matches!(knowledge.source,IncidentSource::Reported{sender} if sender==observer)
                {
                    return Err(bad());
                }
            }
        }
        for (id, value) in incidents(store) {
            if id != value.id
                || !self.incident_shape(store, value, now)
                || value.outcome == IncidentOutcome::Attempt
                    && pending(store, value.actor) != Some(&id)
            {
                return Err(bad());
            }
        }
        let mut participants = BTreeSet::new();
        for (sender, row) in activities(store) {
            row.validate().map_err(|_| bad())?;
            if let Some(id) = row.pending {
                let actual = incident(store, id).ok_or_else(bad)?;
                if actual.actor != sender
                    || actual.outcome != IncidentOutcome::Attempt
                    || actual.completes <= now
                {
                    return Err(bad());
                }
            }
            if let Some(report) = &row.report {
                let actual = incident(store, report.snapshot.incident.id).ok_or_else(bad)?;
                if sender != report.sender
                    || report.started_at > now
                    || report.completes <= now
                    || !self.incident_shape(store, &report.snapshot.incident, now)
                    || !same_incident(&report.snapshot.incident, actual)
                    || report.snapshot.incident.changed_at > actual.changed_at
                    || !participants.insert(report.sender)
                    || !participants.insert(report.recipient)
                {
                    return Err(bad());
                }
            }
        }
        Ok(())
    }
    fn incident_shape(&self, store: &Store, value: &Incident, now: SimTime) -> bool {
        value.validate().is_ok()
            && value.changed_at <= now
            && value
                .portions
                .keys()
                .all(|marker| marker.0 == 0 || claim(store, *marker).is_some())
    }
    pub fn validate_with_quantities(
        &self,
        store: &Store,
        now: SimTime,
        inventory_count: usize,
        quantity: impl Fn(InventoryId) -> Option<Quantity>,
    ) -> Result<(), Error> {
        self.validate_owned(store, now)?;
        if store.count::<Provenance>() != inventory_count {
            return Err(Error::InvalidSave(
                "property provenance lacks physical stock rows".into(),
            ));
        }
        for (id, row) in provenances(store) {
            let sum = row
                .portions
                .values()
                .try_fold(0_u64, |sum, q| sum.checked_add(*q).ok_or(Error::Overflow))?;
            if quantity(id) != Some(Quantity(sum)) {
                return Err(Error::InvalidSave(
                    "property provenance disagrees with physical stock".into(),
                ));
            }
        }
        Ok(())
    }
}
fn same_incident(a: &Incident, b: &Incident) -> bool {
    a.actor == b.actor
        && a.source == b.source
        && a.destination == b.destination
        && a.quantity == b.quantity
        && a.area == b.area
        && a.started_at == b.started_at
        && a.completes == b.completes
}

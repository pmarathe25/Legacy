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
        .any(|q| *q > 1_000)
        {
            return Err(Error::InvalidConfiguration(
                "invalid property attitude".into(),
            ));
        }
        Ok(())
    }
}
impl World {
    pub fn validate_owned(&self, now: SimTime) -> Result<(), Error> {
        let bad = || {
            Error::InvalidSave("invalid property assertions, incidents or private knowledge".into())
        };
        for attitude in self.attitudes.values() {
            attitude.validate().map_err(|_| bad())?;
        }
        for (id, claim) in &self.claims {
            if *id != claim.id
                || id.0 == 0
                || claim.rights.is_empty()
                || claim.portions.is_empty()
                || claim.evidence.len() > 16
                || claim
                    .portions
                    .iter()
                    .any(|marker| marker.0 == 0 || !self.claims.contains_key(marker))
                || claim
                    .evidence
                    .iter()
                    .any(|id| !self.incidents.contains_key(id))
                || !claim.recognized_by.is_disjoint(&claim.opposed_by)
            {
                return Err(bad());
            }
        }
        for parts in self.portions.values() {
            if parts
                .iter()
                .any(|(marker, q)| *q == 0 || marker.0 != 0 && !self.claims.contains_key(marker))
            {
                return Err(bad());
            }
            parts
                .values()
                .try_fold(0u64, |total, q| total.checked_add(*q).ok_or_else(bad))?;
        }
        for known in self.known_claims.values() {
            for (id, view) in known {
                let claim = self.claims.get(id).ok_or_else(bad)?;
                if *id != view.id
                    || claim.claimant != view.claimant
                    || claim.inventory != view.inventory
                    || claim.rights != view.rights
                    || claim.evidence != view.evidence
                    || !view.recognized_by.is_disjoint(&view.opposed_by)
                {
                    return Err(bad());
                }
            }
        }
        for (id, incident) in &self.incidents {
            if *id != incident.id
                || !self.incident_shape(incident, now)
                || incident.outcome == IncidentOutcome::Attempt
                    && self.pending.get(&incident.actor) != Some(id)
            {
                return Err(bad());
            }
        }
        for (observer, known) in &self.knowledge {
            for (id, knowledge) in known {
                let actual = self.incidents.get(id).ok_or_else(bad)?;
                if *id != knowledge.incident.id
                    || !self.incident_shape(&knowledge.incident, now)
                    || knowledge.learned_at > now
                    || knowledge.incident.changed_at > knowledge.learned_at
                    || knowledge.incident.changed_at > actual.changed_at
                    || !same_incident(&knowledge.incident, actual)
                    || matches!(knowledge.source, IncidentSource::Reported { sender } if sender == *observer)
                {
                    return Err(bad());
                }
            }
        }
        for (actor, id) in &self.pending {
            let incident = self.incidents.get(id).ok_or_else(bad)?;
            if incident.actor != *actor
                || incident.outcome != IncidentOutcome::Attempt
                || incident.completes <= now
            {
                return Err(bad());
            }
        }
        let mut participants = BTreeSet::new();
        for (sender, report) in &self.reports {
            let actual = self
                .incidents
                .get(&report.snapshot.incident.id)
                .ok_or_else(bad)?;
            if *sender != report.sender
                || report.sender == report.recipient
                || report.started_at > now
                || report.started_at >= report.completes
                || report.completes <= now
                || report.snapshot.learned_at > report.started_at
                || report.snapshot.incident.changed_at > report.snapshot.learned_at
                || !self.incident_shape(&report.snapshot.incident, now)
                || !same_incident(&report.snapshot.incident, actual)
                || report.snapshot.incident.changed_at > actual.changed_at
                || !participants.insert(report.sender)
                || !participants.insert(report.recipient)
            {
                return Err(bad());
            }
        }
        Ok(())
    }
    fn incident_shape(&self, incident: &Incident, now: SimTime) -> bool {
        incident.id.0 > 0
            && incident.source != incident.destination
            && incident.quantity.0 > 0
            && incident.started_at < incident.completes
            && incident.started_at <= incident.changed_at
            && incident.changed_at <= now
            && (incident.outcome != IncidentOutcome::Taken
                || incident.completes <= incident.changed_at)
            && incident
                .portions
                .iter()
                .all(|(marker, q)| *q > 0 && (marker.0 == 0 || self.claims.contains_key(marker)))
            && incident
                .portions
                .values()
                .map(|q| u128::from(*q))
                .sum::<u128>()
                == u128::from(incident.quantity.0)
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

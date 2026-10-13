//! Factual relationships supplied by lifecycle and trait owners during loading.
use crate::*;

/// Lifecycle's factual stage, independent of its pregnancy component layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransmissionStage {
    Pending,
    Frozen,
    Born(PersonId),
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PregnancyReference {
    pub pregnancy: PregnancyId,
    pub parents: [PersonId; 2],
    pub stage: TransmissionStage,
}

impl Runtime {
    /// Checks genetics' joins without depending on lifecycle or trait storage.
    /// Receiving owners validate their own expressed rows separately.
    pub fn validate_references(
        &self,
        store: &Store,
        people: impl IntoIterator<Item = PersonId>,
        loci: Vec<LocusDefinition>,
        pregnancies: impl IntoIterator<Item = PregnancyReference>,
    ) -> Result<(), Error> {
        let fail = || Error::InvalidSave("inconsistent genetic references".into());
        let people: BTreeSet<_> = people.into_iter().collect();
        if all_person_genomes(store)
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>()
            != people
            || canonicalize(loci).map_err(|_| fail())? != self.definitions
        {
            return Err(fail());
        }
        let mut references = BTreeMap::new();
        for reference in pregnancies {
            if references.insert(reference.pregnancy, reference).is_some() {
                return Err(fail());
            }
            let record = inheritance(store, reference.pregnancy);
            match (reference.stage, record) {
                (TransmissionStage::Pending | TransmissionStage::Ended, None) => {}
                (TransmissionStage::Frozen, Some(record))
                    if record.genome.is_some()
                        && record.born.is_none()
                        && record.ended_at.is_none() => {}
                (TransmissionStage::Born(child), Some(record)) if record.born == Some(child) => {}
                (TransmissionStage::Ended, Some(record)) if record.ended_at.is_some() => {}
                _ => return Err(fail()),
            }
            if record.is_some_and(|record| record.parents != reference.parents) {
                return Err(fail());
            }
        }
        if all_inheritances(store).any(|(id, _)| !references.contains_key(&id)) {
            return Err(fail());
        }
        Ok(())
    }
}

//! Medical review keeps decisions in the owner and execution in physical ports.
use crate::*;
use game_core::*;
use game_ecs::Store;

pub struct WorkReview {
    pub actor: PersonId,
    work: Work,
}
impl WorkReview {
    pub fn should_pause(&self, player_controlled: bool, needs: WorkNeeds) -> bool {
        !player_controlled && should_pause_work(&self.work, needs)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OfferReviewFacts {
    pub player_controlled: bool,
    pub patient_present: bool,
    pub caregiver_capable: bool,
    pub contact: bool,
    pub trust: Option<i32>,
}

pub struct OfferReview {
    offer: Offer,
}
impl OfferReview {
    pub fn caregiver(&self) -> PersonId {
        self.offer.caregiver
    }

    pub fn patient(&self) -> PersonId {
        self.offer.patient
    }

    pub fn response(
        &self,
        facts: OfferReviewFacts,
        symptoms: impl FnOnce() -> SymptomsView,
    ) -> Option<HealthAction> {
        if facts.player_controlled
            || self.offer.accepted
            || !facts.patient_present
            || !facts.caregiver_capable
            || !facts.contact
        {
            return None;
        }
        let trust = facts.trust?;
        Some(offer_response(self.offer.caregiver, &symptoms(), trust))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct HealerReviewFacts {
    pub is_ruler: bool,
    pub medicine: u16,
    pub capable: bool,
    pub busy: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct HealerQualifications {
    pub is_ruler: bool,
    pub medicine: u16,
}
pub struct CareReviewFacts<'a> {
    pub healer: HealerReviewFacts,
    pub body: WorkNeeds,
    pub empathy: u16,
    pub stocks: &'a [RememberedStock],
}
impl HealerReviewFacts {
    pub fn eligible(self) -> bool {
        !self.is_ruler && self.medicine > 0 && self.capable && !self.busy
    }
}

impl Runtime {
    pub fn review_due(&self, now: SimTime) -> bool {
        self.last_review != now && now.0.is_multiple_of(MINUTE.0)
    }

    pub fn record_review(&mut self, now: SimTime) {
        self.last_review = now;
    }

    /// Work is frozen at phase opening: interrupting a coupled job must not
    /// change which later actor's original obligation is reconsidered.
    pub fn review_work(&self, store: &Store) -> Vec<WorkReview> {
        jobs(store)
            .map(|(_, job)| WorkReview {
                actor: job.actor,
                work: job.work.clone(),
            })
            .collect()
    }

    pub fn review_offers(&self, store: &Store) -> Vec<OfferReview> {
        all_offers(store)
            .map(|(_, _, offer)| OfferReview {
                offer: offer.clone(),
            })
            .collect()
    }

    pub fn accepted_treatments(
        &self,
        store: &Store,
        ruler: PersonId,
    ) -> Vec<(PersonId, HealthAction)> {
        all_offers(store)
            .filter(|(_, _, offer)| offer.accepted && offer.caregiver != ruler)
            .map(|(_, _, offer)| {
                (
                    offer.caregiver,
                    HealthAction::Treat {
                        patient: offer.patient,
                        herbs: offer.herbs,
                        water: offer.water,
                    },
                )
            })
            .collect()
    }

    pub fn review_healers(
        &self,
        actors: impl Iterator<Item = PersonId>,
        mut qualifications: impl FnMut(PersonId) -> HealerQualifications,
        mut capable: impl FnMut(PersonId) -> bool,
        mut busy: impl FnMut(PersonId) -> bool,
    ) -> Vec<PersonId> {
        actors
            .filter(|actor| {
                let qualifications = qualifications(*actor);
                !qualifications.is_ruler
                    && qualifications.medicine > 0
                    && capable(*actor)
                    && !busy(*actor)
            })
            .collect()
    }

    pub fn remembered_patients(
        &self,
        store: &Store,
        caregiver: PersonId,
        mut contact: impl FnMut(PersonId) -> bool,
        mut resting: impl FnMut(PersonId) -> bool,
    ) -> Vec<PatientOpportunity> {
        knowledge(store, caregiver)
            .into_iter()
            .flat_map(|knowledge| knowledge.people.values())
            .map(|symptoms| PatientOpportunity {
                symptoms: symptoms.clone(),
                contact: contact(symptoms.person),
                offer_exists: offer(store, caregiver, symptoms.person).is_some(),
                busy: resting(symptoms.person),
            })
            .collect()
    }

    pub fn proactive_review_offer(
        &self,
        actor: PersonId,
        facts: CareReviewFacts<'_>,
        patients: &[PatientOpportunity],
    ) -> Option<HealthAction> {
        if !facts.healer.eligible() {
            return None;
        }
        proactive_offer(
            actor,
            &self.rules,
            WillingCare {
                capable: facts.healer.capable,
                medicine: facts.healer.medicine,
                busy: facts.healer.busy,
                hungry: facts.body.hunger >= facts.body.meal_relief,
                fatigued: facts.body.fatigue >= facts.body.fatigue_limit / 2,
                urgent_dependent: false,
                empathy: facts.empathy,
                trust: 0,
                stocks: facts.stocks,
            },
            patients,
        )
    }

    pub fn review_event_bound(&self, store: &Store) -> usize {
        clinicals(store).count() * 8 + jobs(store).count() * 8 + all_offers(store).count() * 4
    }
}

#[cfg(test)]
mod tests;

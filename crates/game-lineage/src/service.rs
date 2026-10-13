//! A guardian retains individually judged care obligations during outside service.
use crate::*;
use game_core::PersonId;
#[derive(Clone, Copy, Debug)]
pub struct GuardianServiceFacts {
    pub actor: PersonId,
    pub sponsor: PersonId,
    pub guardian: bool,
    pub care_urgency: i64,
    pub hydration_urgency: i64,
}
#[cfg(test)]
impl World {
    pub fn guardian_service_due(&self, facts: GuardianServiceFacts) -> bool {
        guardian_service_due(self.current(), self.guardians.get(&facts.actor), facts)
    }
}
pub(crate) fn guardian_service_due(
    current: Option<PersonId>,
    guardian: Option<&Guardian>,
    facts: GuardianServiceFacts,
) -> bool {
    facts.guardian
        && (current != Some(facts.sponsor)
            || facts.care_urgency > 0
            || facts.hydration_urgency > 0
            || !guardian
                .and_then(|g| g.assessments.get(&facts.sponsor))
                .is_some_and(|a| a.ready))
}
#[cfg(test)]
mod tests;

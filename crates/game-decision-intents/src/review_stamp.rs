//! Live barriers for reusing alternatives within one character transaction.
use super::*;
use game_character_state::Minds;

pub struct ReviewStamp {
    body: game_physiology::Body,
    urgency: Option<u64>,
    action: Option<u64>,
}
pub enum ReviewStatus {
    Current,
    Interrupted,
    Occupied,
    Unavailable,
}
impl ReviewStamp {
    pub fn capture(tx: &Transaction<'_>, person: PersonId, action: Option<u64>) -> Result<Self> {
        Ok(Self {
            body: tx.get::<game_physiology::Bodies>()?.people[&person],
            urgency: tx.get::<Minds>()?.cohort_urgency.get(&person).copied(),
            action,
        })
    }
    pub fn status(&self, tx: &Transaction<'_>, person: PersonId) -> Result<ReviewStatus> {
        let state = tx.get::<Minds>()?;
        let body = tx.get::<game_physiology::Bodies>()?.people.get(&person);
        if !state.people.contains_key(&person) || body.is_none_or(|b| !b.alive || b.health == 0) {
            return Ok(ReviewStatus::Unavailable);
        }
        if state.cohort_urgency.get(&person).copied() != self.urgency || body != Some(&self.body) {
            game_diagnostics::interaction_profile!(count "interaction.review.retained_interrupted", 1);
            return Ok(ReviewStatus::Interrupted);
        }
        if tx
            .get::<Actions>()?
            .0
            .get(&person)
            .is_some_and(|a| Some(a.id) != self.action)
        {
            game_diagnostics::interaction_profile!(count "interaction.review.retained_occupied", 1);
            return Ok(ReviewStatus::Occupied);
        }
        Ok(ReviewStatus::Current)
    }
}

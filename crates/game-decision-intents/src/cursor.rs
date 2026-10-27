//! Resume a person's ordered choice without re-running their review.
use game_choice::Proposal;
use game_utility::Decision;

/// Remaining scored alternatives from one review. Removing a candidate does
/// not itself record a failure: the caller decides whether an actual attempt
/// failed, an opportunity went stale, or a provisional claim lost arbitration.
/// It is valid only inside the review that created it and cannot be saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionCursor {
    offers: Vec<Proposal>,
    continuing: Option<u64>,
    margin: i64,
}

impl SelectionCursor {
    pub fn new(offers: Vec<Proposal>, continuing: Option<u64>, margin: i64) -> Self {
        Self {
            offers,
            continuing,
            margin,
        }
    }

    /// Refresh the caller-owned switching margin before the next selection.
    pub fn set_margin(&mut self, margin: i64) {
        self.margin = margin;
    }

    /// Choose and remove the next alternative against the current action. The
    /// old continuation is eligible for the switching margin only while the
    /// originally observed action is still running.
    pub fn next(
        &mut self,
        current_action: Option<u64>,
        observed_action: Option<u64>,
    ) -> Option<(Decision, Proposal)> {
        if current_action.is_none() {
            self.offers.retain(|offer| offer.continuation.is_none());
        }
        let continuing = current_action
            .filter(|id| Some(*id) == observed_action)
            .and(self.continuing);
        let decision = game_utility::choose_iter(
            self.offers.iter().map(|offer| &offer.motive),
            continuing,
            self.margin,
            false,
        )?;
        let index = self
            .offers
            .iter()
            .position(|offer| offer.motive.id == decision.id)
            .expect("selected motive comes from an offer");
        Some((decision, self.offers.remove(index)))
    }
}

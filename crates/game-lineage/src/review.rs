//! Guardians retain their own priorities while acting through physical operators.
use crate::{Assignment, Guardian, GuardianGoal, GuardianRequest};
use game_core::{Location, PersonId, SimTime, SiteId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardianActivity {
    Idle,
    Rest,
    Work(SiteId),
    Other,
}
impl GuardianActivity {
    pub(crate) fn can_rest(self) -> bool {
        matches!(self, Self::Idle | Self::Work(_))
    }
    pub(crate) fn can_reconsider(self) -> bool {
        matches!(self, Self::Idle | Self::Rest | Self::Work(_))
    }
}

/// Current physical facts, rather than a coordinator-selected guardian goal.
#[derive(Clone, Copy, Debug)]
pub struct ReviewFacts<'a> {
    pub now: SimTime,
    pub location: &'a Location,
    pub activity: GuardianActivity,
    pub confined: bool,
    pub incapacitated: bool,
    pub combat_restricted: bool,
    pub in_combat: bool,
    pub serving_objective: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewStep {
    Escape,
    Recover,
    ResolveEncounter,
    ServeObjective,
    FinishAssignment,
    Assist(PersonId),
    Wait { rest: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryAttempt {
    NotAttempted,
    Occupied,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewDisposition {
    AttemptDelivery,
    ContinueReview,
    Complete,
}

/// An escape opportunity precedes an accepted delivery. A delivery that
/// occupies its guardian precedes reconsideration of other lineage duties.
pub fn review_disposition(escape_available: bool, delivery: DeliveryAttempt) -> ReviewDisposition {
    if escape_available {
        ReviewDisposition::ContinueReview
    } else {
        match delivery {
            DeliveryAttempt::NotAttempted => ReviewDisposition::AttemptDelivery,
            DeliveryAttempt::Occupied => ReviewDisposition::Complete,
            DeliveryAttempt::Unavailable => ReviewDisposition::ContinueReview,
        }
    }
}

impl ReviewStep {
    pub fn goal(self) -> GuardianGoal {
        match self {
            Self::Escape => GuardianGoal::Confined,
            Self::Recover => GuardianGoal::Recovering,
            Self::ResolveEncounter | Self::ServeObjective => GuardianGoal::Searching,
            Self::FinishAssignment | Self::Assist(_) => GuardianGoal::Mentoring,
            Self::Wait { .. } => GuardianGoal::Waiting,
        }
    }

    pub fn explanation(self, attempting_escape: bool) -> &'static str {
        match self {
            Self::Escape if attempting_escape => {
                "Attempting to release the physical bindings with a carried tool"
            }
            Self::Escape => "The body is confined; the connection does not release its restraints",
            Self::Recover => "Recovering in place before resuming the lineage duty",
            Self::ResolveEncounter => {
                "The encounter must be resolved before the guardian can continue"
            }
            Self::ServeObjective => {
                "Following an accepted objective while retaining responsibility for the incarnation"
            }
            Self::FinishAssignment => {
                "Carrying out the accepted physical task before reconsidering lineage assistance"
            }
            Self::Assist(_) => "Considering assistance to the living incarnation",
            Self::Wait { .. } => "No living incarnation; other people and births continue normally",
        }
    }
}

pub fn assignment_pending(assignment: Option<&Assignment>, facts: ReviewFacts<'_>) -> bool {
    assignment.is_some_and(|assignment| match assignment.request {
        GuardianRequest::Move { destination } => {
            matches!(facts.location, Location::InTransit { .. })
                || matches!(facts.location, Location::AtArea(area) if *area != destination)
        }
        GuardianRequest::Work { site } => facts.activity == GuardianActivity::Work(site),
        GuardianRequest::Rest => facts.now <= assignment.accepted_at,
        GuardianRequest::Supply { .. } => false,
    })
}

/// Lineage owns the priority between its duty and other physical commitments.
pub fn review_priority(
    current: Option<PersonId>,
    pending_assignment: bool,
    escape_available: bool,
    facts: ReviewFacts<'_>,
) -> ReviewStep {
    if escape_available || facts.confined {
        ReviewStep::Escape
    } else if facts.incapacitated || facts.combat_restricted {
        ReviewStep::Recover
    } else if facts.in_combat {
        ReviewStep::ResolveEncounter
    } else if facts.serving_objective {
        ReviewStep::ServeObjective
    } else if pending_assignment {
        ReviewStep::FinishAssignment
    } else if let Some(target) = current {
        ReviewStep::Assist(target)
    } else {
        ReviewStep::Wait {
            rest: facts.activity.can_rest(),
        }
    }
}

pub struct ReviewObservation {
    pub target: Option<PersonId>,
    pub sensed_location: Option<Location>,
    pub now: SimTime,
    pub pending_assignment: bool,
    pub goal: GuardianGoal,
    pub explanation: String,
}

impl Guardian {
    pub fn observe_review(&mut self, observation: ReviewObservation) {
        self.target = observation.target;
        self.sensed_location = observation.sensed_location;
        self.sensed_at = observation.now;
        self.goal = observation.goal;
        self.explanation = observation.explanation;
        if !observation.pending_assignment {
            self.assignment = None;
        }
    }

    pub fn observe_failed_attempt(&mut self) {
        self.explanation = "A physical attempt failed; the guardian will reconsider without abandoning the lineage".into();
    }
}

#[cfg(test)]
mod tests;

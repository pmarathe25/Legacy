use crate::*;
use game_core::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuardianTraits {
    pub discipline: u16,
    pub patience: u16,
    pub risk_tolerance: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conduct {
    Practice { gain: u64 },
    Meal,
    Production,
    Departure { known_distress: bool },
    Arrival,
    Care,
    Expression,
    Lesson { gain: u64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    pub conduct: Conduct,
    pub stage: LifeStage,
    pub at: SimTime,
    pub witnessed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestInput {
    pub constrained: bool,
    pub in_combat: bool,
    pub physical_duty_pending: bool,
    pub ready: bool,
    pub known_distress: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    BodyUnavailable,
    CurrentDuty,
    Unready,
    ImmediateCare,
}
impl Refusal {
    pub fn reason(self) -> &'static str {
        match self {
            Self::BodyUnavailable => "My body or restraints prevent that assistance",
            Self::CurrentDuty => "I must finish my current physical duty first",
            Self::Unready => {
                "I have not yet observed enough independence, competence, and judgment to follow that order"
            }
            Self::ImmediateCare => "I will not leave while I can see that you need immediate care",
        }
    }
}
/// Facts supplied only after a person and body have actually been created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActualBirth {
    pub person: PersonId,
    pub born: SimTime,
    pub initial_age: SimDuration,
    pub parent_count: usize,
    pub alive: bool,
    pub location: Location,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub lineage: World,
    pub predecessor: PersonId,
    pub successor: PersonId,
}

pub fn judge_readiness(before: &Assessment, traits: GuardianTraits) -> Assessment {
    let mut assessment = before.clone();
    let threshold = 250 + u64::from(traits.discipline) / 5 + u64::from(traits.patience) / 10
        - u64::from(traits.risk_tolerance) / 5;
    let risk_weight = 1_000 - u64::from(traits.risk_tolerance);
    let score = (assessment.independence + assessment.competence + assessment.judgment)
        .saturating_sub(assessment.risks * risk_weight / 1_000);
    assessment.ready = assessment
        .observed_stage
        .is_some_and(|stage| stage != LifeStage::Infant)
        && assessment.independence >= 100
        && assessment.competence >= 100
        && assessment.judgment >= 75
        && score >= threshold;
    assessment.explanation = format!(
        "{}; witnessed independence {}, competence {}, judgment {}, risk {}. My required evidence score is {}, observed {}",
        if assessment.ready {
            "Ready for requests that still use my own judgment"
        } else {
            "More observed independence, competence, or judgment is needed"
        },
        assessment.independence,
        assessment.competence,
        assessment.judgment,
        assessment.risks,
        threshold,
        score
    );
    assessment
}
pub fn observe_conduct(
    before: &Assessment,
    traits: GuardianTraits,
    observation: Observation,
) -> Assessment {
    if !observation.witnessed {
        return before.clone();
    }
    let (independence, competence, judgment, risks) = match observation.conduct {
        Conduct::Practice { gain } | Conduct::Lesson { gain } if gain > 0 => {
            (0, gain.min(100), 25, 0)
        }
        Conduct::Practice { .. } | Conduct::Lesson { .. } => return before.clone(),
        Conduct::Meal => (100, 0, 50, 0),
        Conduct::Production => (100, 100, 50, 0),
        Conduct::Departure {
            known_distress: true,
        } => (100, 0, 0, 100),
        Conduct::Departure {
            known_distress: false,
        } => (100, 0, 50, 0),
        Conduct::Arrival => (50, 0, 25, 0),
        Conduct::Care => (50, 50, 100, 0),
        Conduct::Expression => (0, 0, 10, 0),
    };
    let mut next = before.clone();
    next.independence = (next.independence + independence).min(1_000);
    next.competence = (next.competence + competence).min(1_000);
    next.judgment = (next.judgment + judgment).min(1_000);
    next.risks = (next.risks + risks).min(1_000);
    next.observed_stage = Some(observation.stage);
    next.observed_at = observation.at;
    judge_readiness(&next, traits)
}
pub fn observe_stage(
    before: &Assessment,
    traits: GuardianTraits,
    stage: LifeStage,
    at: SimTime,
    witnessed: bool,
) -> Assessment {
    if !witnessed {
        return before.clone();
    }
    let mut next = before.clone();
    next.observed_stage = Some(stage);
    next.observed_at = at;
    judge_readiness(&next, traits)
}
pub fn judge_request(request: &GuardianRequest, input: RequestInput) -> Result<(), Refusal> {
    if input.constrained || input.in_combat {
        Err(Refusal::BodyUnavailable)
    } else if input.physical_duty_pending {
        Err(Refusal::CurrentDuty)
    } else if !input.ready && !matches!(request, GuardianRequest::Rest) {
        Err(Refusal::Unready)
    } else if matches!(request, GuardianRequest::Move { .. }) && input.known_distress {
        Err(Refusal::ImmediateCare)
    } else {
        Ok(())
    }
}
pub fn guardian_constraint(
    confined: bool,
    health_zero: bool,
    restricted: bool,
) -> Option<GuardianGoal> {
    if confined {
        Some(GuardianGoal::Confined)
    } else if health_zero || restricted {
        Some(GuardianGoal::Recovering)
    } else {
        None
    }
}
pub fn observe_death(
    before: &World,
    current_alive: bool,
    now: SimTime,
    constraints: &BTreeMap<PersonId, Option<GuardianGoal>>,
) -> World {
    let mut lineage = before.clone();
    let Some(current) = before.current() else {
        return lineage;
    };
    if current_alive {
        return lineage;
    }
    lineage.incarnations.last_mut().unwrap().died_at = Some(now);
    lineage.waiting = Some(Waiting {
        predecessor: current,
        died_at: now,
    });
    for (id, guardian) in &mut lineage.guardians {
        guardian.assignment = None;
        guardian.provision = None;
        guardian.requested_delivery = None;
        guardian.target = None;
        guardian.sensed_location = None;
        guardian.sensed_at = now;
        guardian.goal = constraints
            .get(id)
            .copied()
            .flatten()
            .unwrap_or(GuardianGoal::Waiting);
        guardian.explanation =
            "No living incarnation; waiting for an actual post-death birth".into();
    }
    lineage
}
pub fn first_eligible_birth(
    waiting: &Waiting,
    births: &[ActualBirth],
    now: SimTime,
) -> Option<PersonId> {
    births
        .iter()
        .filter(|birth| {
            birth.born == now
                && birth.born > waiting.died_at
                && birth.initial_age == SimDuration(0)
                && birth.parent_count == 2
                && birth.alive
        })
        .min_by_key(|birth| (birth.born, birth.person))
        .map(|birth| birth.person)
}
pub fn bind_actual_birth(
    before: &World,
    births: &[ActualBirth],
    now: SimTime,
    constraints: &BTreeMap<PersonId, Option<GuardianGoal>>,
) -> Option<Binding> {
    let waiting = before.waiting.as_ref()?;
    let successor = first_eligible_birth(waiting, births, now)?;
    let birth = births.iter().find(|birth| birth.person == successor)?;
    let mut lineage = before.clone();
    lineage.incarnations.push(Incarnation {
        person: successor,
        bound_at: now,
        died_at: None,
    });
    lineage.waiting = None;
    for (id, guardian) in &mut lineage.guardians {
        guardian.target = Some(successor);
        guardian.sensed_location = Some(birth.location.clone());
        guardian.sensed_at = now;
        guardian.goal = constraints
            .get(id)
            .copied()
            .flatten()
            .unwrap_or(GuardianGoal::Searching);
        guardian.explanation =
            "The innate connection locates the newborn; reaching them requires travel".into();
    }
    Some(Binding {
        lineage,
        predecessor: waiting.predecessor,
        successor,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeedObservation {
    pub location: Location,
    pub observed_at: SimTime,
    pub urgency: i64,
    pub hydration_urgency: i64,
    pub preference: Option<ChildhoodInterest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CareDoses {
    pub food: Quantity,
    pub water: Quantity,
}

/// Guardians may preserve a chosen observed need while absent. This describes
/// only a requirement; logistics and physical consumers own every execution step.
pub fn review_care_provision(
    before: Option<&CareProvision>,
    target: PersonId,
    now: SimTime,
    together: bool,
    fresh_need: Option<&NeedObservation>,
    doses: CareDoses,
) -> Option<CareProvision> {
    let continuing = before
        .filter(|provision| {
            provision.delivery.destination == game_logistics::Destination::Contact(target)
                && provision.delivery.remaining.0 > 0
        })
        .cloned();
    let Some(need) = fresh_need.filter(|need| together && need.observed_at == now) else {
        return continuing;
    };
    let good = if need.hydration_urgency > 0 && need.hydration_urgency >= need.urgency {
        Good::Water
    } else if need.urgency > 0 {
        Good::Food
    } else {
        return None;
    };
    let delivery = continuing
        .filter(|provision| provision.delivery.requirement.good == good)
        .map(|provision| provision.delivery)
        .or_else(|| {
            game_logistics::Delivery::new(
                game_logistics::Requirement {
                    good,
                    quantity: if good == Good::Water {
                        doses.water
                    } else {
                        doses.food
                    },
                    source: None,
                },
                game_logistics::Destination::Contact(target),
            )
            .ok()
        })?;
    Some(CareProvision {
        delivery,
        observed_at: now,
    })
}
impl World {
    /// Called only with freshly observed local operator inapplicability.
    /// Accepted supply requests have their own consumer and are unaffected.
    pub fn observe_care_unavailable(&mut self, guardian: PersonId) {
        if let Some(guardian) = self.guardians.get_mut(&guardian) {
            guardian.provision = None;
        }
    }
    pub fn record_care_receipt(
        &mut self,
        guardian: PersonId,
        target: PersonId,
        good: Good,
        quantity: Quantity,
        sequence: u64,
    ) -> Result<bool, Error> {
        let Some(provision) = self
            .guardians
            .get_mut(&guardian)
            .and_then(|guardian| guardian.provision.as_mut())
        else {
            return Ok(false);
        };
        if provision.delivery.destination != game_logistics::Destination::Contact(target)
            || provision.delivery.requirement.good != good
        {
            return Ok(false);
        }
        provision
            .delivery
            .delivered(&game_logistics::DeliveryReceipt {
                destination: provision.delivery.destination,
                good,
                quantity,
                sequence,
            })
    }
}
#[derive(Clone, Debug)]
pub struct GuardianInput<'a> {
    pub target: PersonId,
    pub own_location: &'a Location,
    pub target_location: &'a Location,
    pub now: SimTime,
    pub can_reconsider: bool,
    pub should_rest: bool,
    pub ready: bool,
    pub need: Option<NeedObservation>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardianAction {
    ApproachRoad {
        from: AreaId,
        to: AreaId,
        progress: SimDuration,
        duration: SimDuration,
    },
    Move {
        destination: AreaId,
    },
    Comfort {
        child: PersonId,
    },
    Rest,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardianIntent {
    pub goal: GuardianGoal,
    pub explanation: String,
    pub action: Option<GuardianAction>,
}
pub fn guardian_intention(input: GuardianInput<'_>) -> GuardianIntent {
    let intent = |goal, explanation: &str, action| GuardianIntent {
        goal,
        explanation: explanation.into(),
        action,
    };
    if matches!(input.own_location, Location::InTransit { .. }) {
        return intent(
            GuardianGoal::Searching,
            "Traveling along the current physical route toward the sensed incarnation",
            None,
        );
    }
    if input.own_location != input.target_location {
        if let Location::StoppedOnRoad {
            from,
            to,
            progress,
            duration,
        } = input.target_location
        {
            return intent(
                GuardianGoal::Searching,
                "The incarnation remains on the road; reaching the exact position requires travel",
                Some(GuardianAction::ApproachRoad {
                    from: *from,
                    to: *to,
                    progress: *progress,
                    duration: *duration,
                }),
            );
        }
        let destination = match input.target_location {
            Location::AtArea(area) => *area,
            Location::InTransit { to, .. } => *to,
            Location::Carried { .. } => {
                return intent(
                    GuardianGoal::Searching,
                    "The carrier's physical position must be resolved before choosing a route",
                    None,
                );
            }
            Location::StoppedOnRoad { .. } => unreachable!("handled road approach"),
        };
        return intent(
            GuardianGoal::Searching,
            "The innate connection gives a destination; a known traversable route is still needed",
            Some(GuardianAction::Move { destination }),
        );
    }
    if !input.can_reconsider {
        return intent(
            GuardianGoal::Mentoring,
            "Completing the current physical assistance before reconsidering",
            None,
        );
    }
    let Location::AtArea(area) = input.own_location else {
        return intent(
            GuardianGoal::Searching,
            "Traveling along the current physical route toward the sensed incarnation",
            None,
        );
    };
    if let Some(need) = input
        .need
        .filter(|need| need.location == Location::AtArea(*area) && need.observed_at == input.now)
    {
        if need.hydration_urgency > 0 || need.urgency > 0 {
            return intent(
                GuardianGoal::Mentoring,
                "Observed need motivates provision; actual acquisition and handoff opportunities are required",
                None,
            );
        }
        if need.preference == Some(ChildhoodInterest::Company) {
            return intent(
                GuardianGoal::Mentoring,
                "Responding to the child's observed request for company",
                Some(GuardianAction::Comfort {
                    child: input.target,
                }),
            );
        }
    }
    intent(
        GuardianGoal::Mentoring,
        if input.ready {
            "Present with the incarnation; observed readiness permits requests, which I still judge individually"
        } else {
            "Present with the incarnation; further observed competence and independence are needed before I follow orders"
        },
        input.should_rest.then_some(GuardianAction::Rest),
    )
}

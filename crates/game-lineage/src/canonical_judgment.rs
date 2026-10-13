//! Canonical request and review entry points reuse Lineage's existing policy.
use crate::*;
use game_core::*;
use game_ecs::Store;

#[derive(Clone, Debug)]
pub struct RequestFacts {
    pub now: SimTime,
    pub requester_independent: bool,
    pub requester_alive: bool,
    pub requester_idle: bool,
    pub requester_in_combat: bool,
    pub requester_restricted: bool,
    pub requester_in_care: bool,
    pub requester_location: Location,
    pub guardian_location: Location,
    pub guardian_constrained: bool,
    pub guardian_in_combat: bool,
    pub guardian_duty_pending: bool,
    pub signal_urgency: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestDecision {
    Accepted,
    Refused(&'static str),
}
pub struct AssistanceFacts<'a> {
    pub target_location: &'a Location,
    pub need: Option<NeedObservation>,
    pub provision: Option<ProvisionFacts<'a>>,
    pub transport_rescue: bool,
    pub force_rescue: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewAction {
    Physical(GuardianAction),
    Provision(CareProvision),
    Escape,
    TransportRescue,
    ForceRescue,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewDecision {
    pub step: ReviewStep,
    pub action: Option<ReviewAction>,
}

impl Runtime {
    pub fn prepare_request(
        &self,
        store: &Store,
        actor: PersonId,
        requester: PersonId,
        request: GuardianRequest,
        facts: RequestFacts,
    ) -> Result<PreparedLineage<RequestDecision>, Error> {
        let before = self.project(store, &[actor])?;
        if before.world.current() != Some(requester)
            || actor == requester
            || !facts.requester_independent
            || !facts.requester_alive
            || !facts.requester_idle
            || facts.requester_in_combat
            || facts.requester_restricted
            || facts.requester_in_care
            || facts.requester_location != facts.guardian_location
            || !matches!(facts.requester_location, Location::AtArea(_))
            || matches!(request, GuardianRequest::Supply { .. })
        {
            return Err(Error::Unavailable);
        }
        let mut after = before.world.clone();
        let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
        let decision = match judge_request(
            &request,
            RequestInput {
                constrained: facts.guardian_constrained,
                in_combat: facts.guardian_in_combat,
                physical_duty_pending: facts.guardian_duty_pending,
                ready: guardian
                    .assessments
                    .get(&requester)
                    .is_some_and(|assessment| assessment.ready),
                known_distress: facts.signal_urgency > 0,
            },
        ) {
            Err(refusal) => {
                guardian.explanation = refusal.reason().into();
                RequestDecision::Refused(refusal.reason())
            }
            Ok(()) => {
                guardian.requested_delivery = None;
                guardian.assignment = Some(Assignment {
                    request,
                    accepted_at: facts.now,
                });
                guardian.explanation =
                    "I accept this physical task while retaining my judgment and lineage duty"
                        .into();
                RequestDecision::Accepted
            }
        };
        self.change(before, after, decision)
    }
    pub fn prepare_request_failure(
        &self,
        store: &Store,
        actor: PersonId,
    ) -> Result<PreparedLineage<RequestDecision>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?.explanation = "The task exceeds my known routes, permissions, or physical resources; I remain committed to the lineage".into();
        self.change(
            before,
            after,
            RequestDecision::Refused(
                "I cannot carry this out with my knowledge, permissions, or physical resources",
            ),
        )
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "supply identity, named requirement and separate physical observations"
    )]
    pub fn prepare_supply(
        &self,
        store: &Store,
        actor: PersonId,
        requester: PersonId,
        good: Good,
        source: InventoryId,
        quantity: Quantity,
        facts: &SupplyFacts,
    ) -> Result<PreparedLineage<SupplyAnswer>, Error> {
        let before = self.project(store, &[actor])?;
        let decision = before
            .world
            .judge_supply(actor, requester, good, source, quantity, facts)?;
        let mut after = before.world.clone();
        let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
        match &decision {
            SupplyAnswer::Accept(delivery) => {
                guardian.requested_delivery = Some(delivery.clone());
                guardian.assignment = Some(Assignment {
                    request: GuardianRequest::Supply {
                        good,
                        source,
                        quantity,
                    },
                    accepted_at: facts.now,
                });
                guardian.explanation =
                    "I accepted the named goods and will use my own access and actual carrying"
                        .into();
            }
            SupplyAnswer::Refuse(reason) => guardian.explanation = (*reason).into(),
        }
        self.change(before, after, decision)
    }
    pub fn prepare_supply_refusal(
        &self,
        store: &Store,
        actor: PersonId,
        reason: &'static str,
    ) -> Result<PreparedLineage<RequestDecision>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        after
            .guardians
            .get_mut(&actor)
            .ok_or(Error::Unavailable)?
            .explanation = reason.into();
        self.change(before, after, RequestDecision::Refused(reason))
    }
    /// Sense/assignment state and assistance decisions are prepared together,
    /// avoiding separate updates to the same guardian during one review.
    pub fn prepare_review(
        &self,
        store: &Store,
        actor: PersonId,
        facts: ReviewFacts<'_>,
        sensed_location: Option<Location>,
        escape_available: bool,
        assistance: Option<AssistanceFacts<'_>>,
    ) -> Result<PreparedLineage<ReviewDecision>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        let current = after.current();
        if current.is_some() != sensed_location.is_some() {
            return Err(Error::Unavailable);
        }
        let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
        let pending = assignment_pending(guardian.assignment.as_ref(), facts);
        let step = review_priority(current, pending, escape_available, facts);
        let (goal, explanation, action) = match step {
            ReviewStep::Assist(target) => {
                let assistance = assistance.ok_or(Error::Unavailable)?;
                let care_inapplicable = assistance
                    .provision
                    .as_ref()
                    .is_some_and(|facts| facts.applicable == Some(false));
                if Some(assistance.target_location) != sensed_location.as_ref() {
                    return Err(Error::Unavailable);
                }
                if assistance.transport_rescue {
                    (
                        GuardianGoal::Searching,
                        "Providing physical support to the incarnation on the road".into(),
                        Some(ReviewAction::TransportRescue),
                    )
                } else if assistance.force_rescue {
                    (
                        GuardianGoal::Searching,
                        "Reaching the confined incarnation with a physical rescue tool".into(),
                        Some(ReviewAction::ForceRescue),
                    )
                } else {
                    if let Some(provision_facts) = assistance.provision {
                        if provision_facts.applicable == Some(false) {
                            guardian.provision = None;
                        }
                        guardian.provision = review_care_provision(
                            guardian.provision.as_ref(),
                            target,
                            provision_facts.now,
                            provision_facts.together,
                            provision_facts
                                .need
                                .filter(|_| provision_facts.applicable != Some(false)),
                            provision_facts.doses,
                        );
                    }
                    if let Some(provision) = &guardian.provision {
                        (GuardianGoal::Mentoring, "Pursuing the observed care requirement through actual acquisition, carrying and handoff".into(), Some(ReviewAction::Provision(provision.clone())))
                    } else {
                        let intent = guardian_intention(GuardianInput {
                            target,
                            own_location: facts.location,
                            target_location: assistance.target_location,
                            now: facts.now,
                            can_reconsider: facts.activity.can_reconsider(),
                            should_rest: facts.activity.can_rest(),
                            ready: guardian
                                .assessments
                                .get(&target)
                                .is_some_and(|assessment| assessment.ready),
                            need: assistance.need.filter(|_| !care_inapplicable),
                        });
                        (
                            intent.goal,
                            intent.explanation,
                            intent.action.map(ReviewAction::Physical),
                        )
                    }
                }
            }
            ReviewStep::Escape => (
                step.goal(),
                step.explanation(escape_available).into(),
                escape_available.then_some(ReviewAction::Escape),
            ),
            ReviewStep::Wait { rest } => (
                step.goal(),
                step.explanation(false).into(),
                rest.then_some(ReviewAction::Physical(GuardianAction::Rest)),
            ),
            _ => (step.goal(), step.explanation(false).into(), None),
        };
        guardian.observe_review(ReviewObservation {
            target: current,
            sensed_location,
            now: facts.now,
            pending_assignment: pending,
            goal,
            explanation,
        });
        self.change(before, after, ReviewDecision { step, action })
    }
}
pub fn supply_step_can_start<K>(step: &game_logistics::Step<K>) -> bool {
    matches!(
        step,
        game_logistics::Step::Carry { .. }
            | game_logistics::Step::Acquire(_)
            | game_logistics::Step::Travel(_)
            | game_logistics::Step::Transfer { .. }
            | game_logistics::Step::Arrived { .. }
    )
}

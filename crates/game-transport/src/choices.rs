//! Carrying intentions use current bodily facts and personally known relations.
//! An intention does not promise that its later physical execution will succeed.
use crate::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct RelationFacts {
    pub trust: i32,
    pub affection: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct OfferEligibilityFacts {
    pub actor: PersonId,
    pub ruler: PersonId,
    pub independent: bool,
}

impl OfferEligibilityFacts {
    pub fn autonomous_response(self) -> bool {
        self.actor != self.ruler && self.independent
    }
}

/// Activity classification supplied by the activity owner adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportActivity {
    Idle,
    Rest,
    Other,
}

#[derive(Clone, Copy, Debug)]
pub struct SupportAvailabilityFacts {
    pub activity: Option<SupportActivity>,
    pub lineage_duty: bool,
    pub health_duty: bool,
    pub mission_duty: bool,
}

impl SupportAvailabilityFacts {
    pub fn occupied(self) -> bool {
        !matches!(
            self.activity,
            Some(SupportActivity::Idle | SupportActivity::Rest)
        ) || self.lineage_duty
            || self.health_duty
            || self.mission_duty
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OfferResponseFacts {
    pub autonomous: bool,
    pub alive: bool,
    pub combat_restricted: bool,
    pub idle_or_resting: bool,
    pub health: u64,
    pub max_health: u64,
    pub hunger: u64,
    pub hunger_harm: u64,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub independence_goal: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferResponse {
    pub response: Response,
    pub actions: Vec<Action>,
    pub support: Option<SupportRequest>,
}

#[derive(Clone, Copy, Debug)]
pub struct SupportFacts {
    pub autonomous: bool,
    pub empathy: u16,
    pub relation: RelationFacts,
    pub hunger: u64,
    pub meal_relief: u64,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub occupied: bool,
    pub co_present: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct AvailableActionFacts {
    pub independent: bool,
    pub combat_restricted: bool,
    pub moving: bool,
    pub stopped_on_road: bool,
    pub position: Option<Position>,
}

pub fn support_observers(
    passenger: PersonId,
    present: impl IntoIterator<Item = PersonId>,
) -> Vec<PersonId> {
    present
        .into_iter()
        .filter(|observer| *observer != passenger)
        .take(64)
        .collect()
}

pub trait SupportWitnessFacts {
    fn person(&self, actor: PersonId) -> bool;
    fn tactical_contact(&self, a: PersonId, b: PersonId) -> bool;
}

/// A requested voluntary support attempt may remain pending when it cannot
/// presently be executed. Other failures keep their original error.
pub fn support_attempt_result(result: Result<(), Error>) -> Result<(), Error> {
    match result {
        Ok(()) | Err(Error::Unavailable) => Ok(()),
        Err(error) => Err(error),
    }
}
pub fn local_support_observers(
    store: &game_ecs::Store,
    candidates: &game_spatial::LocalCandidates,
    passenger: PersonId,
    now: SimTime,
    facts: &impl SupportWitnessFacts,
) -> Result<Vec<PersonId>, Error> {
    let here = game_spatial::resolve(store, game_ecs::StableId(passenger.0))?;
    Ok(candidates
        .at_location(&here)
        .map(|id| PersonId(id.0))
        .filter(|observer| *observer != passenger && facts.person(*observer))
        .filter(|observer| same_place(store, *observer, passenger, now, facts))
        .take(64)
        .collect())
}
/// Physical contact combines canonical carrier-resolved geometry with the
/// tactical owner's factual contact between terminal anchors.
pub fn same_place(
    store: &game_ecs::Store,
    a: PersonId,
    b: PersonId,
    now: SimTime,
    facts: &impl SupportWitnessFacts,
) -> bool {
    if !facts.person(a) || !facts.person(b) {
        return false;
    }
    let resolve = |actor| game_spatial::resolved_anchor(store, game_ecs::StableId(actor));
    let (Ok((a_anchor, a_row)), Ok((b_anchor, b_row))) = (resolve(a.0), resolve(b.0)) else {
        return false;
    };
    let (Ok(a_point), Ok(b_point)) = (
        position_from_location(a_row, now),
        position_from_location(b_row, now),
    ) else {
        return false;
    };
    a_point.same_point(b_point)
        && facts.tactical_contact(PersonId(a_anchor.0), PersonId(b_anchor.0))
}

impl Runtime {
    /// The body provider supplies physical facts lazily. A moving or carried
    /// actor's action list does not require a population body catalog.
    #[allow(
        clippy::too_many_arguments,
        reason = "explicit Store joins the existing bounded factual ports"
    )]
    pub fn available_actions(
        &self,
        store: &game_ecs::Store,
        actor: PersonId,
        now: SimTime,
        facts: AvailableActionFacts,
        bodies: impl FnOnce() -> Option<BTreeMap<PersonId, Body>>,
        moving: impl Fn(PersonId) -> bool,
        can_depart: impl FnOnce() -> bool,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        if let Some(group) = group(store, actor) {
            if facts.independent && !facts.combat_restricted {
                actions.push(Action::PutDown {
                    passenger: group.passenger,
                });
            }
        } else if facts.moving {
            actions.push(Action::Stop);
        } else {
            if facts.stopped_on_road && can_depart() {
                actions.push(Action::Resume);
                if let Some(Position::Road(point)) = facts.position {
                    actions.push(Action::ReturnToArea {
                        destination: point.from,
                    });
                    actions.push(Action::ReturnToArea {
                        destination: point.to,
                    });
                }
            }
            if let Some(bodies) = bodies() {
                if let Some(body) = bodies.body(actor).filter(|body| body.can_bear) {
                    for (passenger, candidate) in &bodies {
                        if *passenger != actor
                            && candidate.position == body.position
                            && group(store, *passenger).is_none()
                            && !moving(*passenger)
                        {
                            actions.push(Action::OfferCarry {
                                passenger: *passenger,
                            });
                        }
                    }
                }
                if let Some(offers) = offer_inbox(store, actor).map(|row| &row.by_bearer) {
                    let bearers: Vec<_> = offers
                        .values()
                        .filter(|offer| offer.expires > now)
                        .map(|offer| offer.bearer)
                        .collect();
                    if !bearers.is_empty() {
                        actions.push(Action::AcceptCarry { bearers });
                    }
                    for bearer in offers.keys() {
                        actions.push(Action::RefuseCarry { bearer: *bearer });
                    }
                }
            }
        }
        for group in groups(store)
            .map(|(_, row)| row)
            .filter(|group| group.bearers.contains(&actor))
        {
            actions.push(Action::PutDown {
                passenger: group.passenger,
            });
        }
        actions
    }

    /// Cheap admission before a caller assembles the connected carrying bodies.
    pub fn has_offer_to_review(
        &self,
        store: &game_ecs::Store,
        passenger: PersonId,
        now: SimTime,
        facts: OfferResponseFacts,
        co_present: impl Fn(PersonId) -> bool,
    ) -> bool {
        facts.autonomous
            && facts.alive
            && !facts.combat_restricted
            && !self.busy(store, passenger)
            && offer_inbox(store, passenger)
                .map(|row| &row.by_bearer)
                .is_some_and(|offers| {
                    offers
                        .values()
                        .any(|offer| offer.expires > now && co_present(offer.bearer))
                })
    }

    /// Select surviving, physically present offers in their stable bearer order.
    /// The relation reader supplies facts only; this owner selects and judges.
    #[allow(
        clippy::too_many_arguments,
        reason = "explicit Store joins the existing bounded factual ports"
    )]
    pub fn offer_response(
        &self,
        store: &game_ecs::Store,
        passenger: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
        facts: OfferResponseFacts,
        relation: impl Fn(PersonId) -> Option<RelationFacts>,
        co_present: impl Fn(PersonId) -> bool,
    ) -> Option<OfferResponse> {
        let passenger_body = bodies.body(passenger)?;
        if !self.has_offer_to_review(store, passenger, now, facts, |bearer| {
            co_present(bearer)
                && bodies
                    .body(bearer)
                    .is_some_and(|body| body.position.same_point(passenger_body.position))
        }) {
            return None;
        }
        let offers: Vec<_> = offer_inbox(store, passenger)
            .map(|row| &row.by_bearer)?
            .values()
            .filter(|offer| {
                offer.expires > now
                    && co_present(offer.bearer)
                    && bodies
                        .body(offer.bearer)
                        .is_some_and(|bearer| bearer.position.same_point(passenger_body.position))
            })
            .collect();
        let first = offers.first()?;
        let bearers: BTreeSet<_> = offers.iter().map(|offer| offer.bearer).collect();
        let sufficient_support =
            now.0
                .checked_add(self.rules.lift_time.0)
                .is_some_and(|completes| {
                    self.check_group(
                        store,
                        &CarryGroup {
                            passenger,
                            anchor: first.bearer,
                            bearers: bearers.clone(),
                            authority: Authority::Consented,
                            accepted_at: SimTime(completes),
                        },
                        bodies,
                        now,
                        true,
                    )
                    .is_ok()
                });
        let relation = relation(first.bearer).unwrap_or_default();
        let (kind, reason) = judge_offer(OfferChoice {
            need_help: facts.health < facts.max_health || facts.fatigue >= facts.fatigue_limit / 3,
            urgent_own_need: facts.hunger >= facts.hunger_harm,
            busy: !facts.idle_or_resting,
            trust: relation.trust,
            affection: relation.affection,
            independence_goal: facts.independence_goal,
            sufficient_support,
        });
        let actions = match kind {
            ResponseKind::Accepted => vec![Action::AcceptCarry {
                bearers: bearers.into_iter().collect(),
            }],
            ResponseKind::Refused => offers
                .iter()
                .map(|offer| Action::RefuseCarry {
                    bearer: offer.bearer,
                })
                .collect(),
            ResponseKind::WaitingForSupport => Vec::new(),
        };
        Some(OfferResponse {
            response: Response {
                passenger,
                bearer: first.bearer,
                kind,
                reason: reason.into(),
                observed_at: now,
            },
            actions,
            support: (kind == ResponseKind::WaitingForSupport).then(|| SupportRequest {
                passenger,
                requester: passenger,
                observed_at: now,
                expires: offers.iter().map(|offer| offer.expires).min().unwrap(),
            }),
        })
    }

    pub fn has_support_to_review(
        &self,
        store: &game_ecs::Store,
        actor: PersonId,
        request: &SupportRequest,
        now: SimTime,
        autonomous: bool,
        co_present: bool,
    ) -> bool {
        autonomous
            && co_present
            && request.expires > now
            && group(store, request.passenger).is_none()
            && !self.busy(store, actor)
            && !offer_inbox(store, request.passenger)
                .map(|row| &row.by_bearer)
                .is_some_and(|offers| offers.contains_key(&actor))
    }

    pub fn support_intent(
        &self,
        store: &game_ecs::Store,
        actor: PersonId,
        request: &SupportRequest,
        now: SimTime,
        bodies: &impl BodyFacts,
        facts: SupportFacts,
    ) -> Option<Action> {
        let body = bodies.body(actor)?;
        let passenger = bodies.body(request.passenger)?;
        if !self.has_support_to_review(
            store,
            actor,
            request,
            now,
            facts.autonomous,
            facts.co_present && body.position.same_point(passenger.position),
        ) || !willing_support(SupportChoice {
            empathy: facts.empathy,
            trust: facts.relation.trust,
            affection: facts.relation.affection,
            own_need: facts.hunger >= facts.meal_relief || facts.fatigue >= facts.fatigue_limit / 2,
            busy: facts.occupied,
            has_capacity: body.can_bear && body.capacity > body.cargo,
        }) {
            return None;
        }
        Some(Action::JoinCarry {
            passenger: request.passenger,
        })
    }
}

/// Strategic navigation begins from the destination of an active journey and
/// the original departure endpoint of a stopped road placement.
pub fn navigation_origin(resolved: &Location) -> Result<AreaId, Error> {
    match *resolved {
        Location::AtArea(area) => Ok(area),
        Location::InTransit { to, .. } => Ok(to),
        Location::StoppedOnRoad { from, .. } => Ok(from),
        Location::Carried { .. } => Err(Error::Unavailable),
    }
}
/// An actor can replace an active pending itinerary with its current endpoint.
/// Carried actors use resolved geometry without becoming independent movers.
pub fn include_same_origin_destination(raw: &Location, pending_route: &[AreaId]) -> bool {
    matches!(raw, Location::InTransit { .. }) && !pending_route.is_empty()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuActionKind {
    Move,
    Work,
    Load,
    Other,
}
/// Filter the basic/development action menu before other feature extensions.
/// The classifier translates public action variants only. Transport determines
/// eligibility from actual placement and the resolved carrier geometry.
pub fn filter_position_menu<A>(
    menu: &mut Vec<A>,
    raw: &Location,
    resolved: &Location,
    classify: impl Fn(&A) -> MenuActionKind,
) {
    menu.retain(|action| match classify(action) {
        MenuActionKind::Move => matches!(raw, Location::AtArea(_) | Location::InTransit { .. }),
        MenuActionKind::Work | MenuActionKind::Load => {
            !matches!(resolved, Location::StoppedOnRoad { .. })
        }
        MenuActionKind::Other => true,
    });
}

#[cfg(test)]
mod tests;

//! Carrying intentions use current bodily facts and personally known relations.
//! An intention does not promise that its later physical execution will succeed.
use crate::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct RelationFacts {
    pub trust: i32,
    pub affection: i32,
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

impl World {
    /// The body provider supplies physical facts lazily. A moving or carried
    /// actor's action list does not require a population body catalog.
    pub fn available_actions(
        &self,
        actor: PersonId,
        now: SimTime,
        facts: AvailableActionFacts,
        bodies: impl FnOnce() -> Option<BTreeMap<PersonId, Body>>,
        moving: impl Fn(PersonId) -> bool,
        can_depart: impl FnOnce() -> bool,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        if let Some(group) = self.groups.get(&actor) {
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
                if let Some(body) = bodies.get(&actor).filter(|body| body.can_bear) {
                    for (passenger, candidate) in &bodies {
                        if *passenger != actor
                            && candidate.position == body.position
                            && !self.groups.contains_key(passenger)
                            && !moving(*passenger)
                        {
                            actions.push(Action::OfferCarry {
                                passenger: *passenger,
                            });
                        }
                    }
                }
                if let Some(offers) = self.offers.get(&actor) {
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
        for group in self
            .groups
            .values()
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
        passenger: PersonId,
        now: SimTime,
        facts: OfferResponseFacts,
        co_present: impl Fn(PersonId) -> bool,
    ) -> bool {
        facts.autonomous
            && facts.alive
            && !facts.combat_restricted
            && !self.busy(passenger)
            && self.offers.get(&passenger).is_some_and(|offers| {
                offers
                    .values()
                    .any(|offer| offer.expires > now && co_present(offer.bearer))
            })
    }

    /// Select surviving, physically present offers in their stable bearer order.
    /// The relation reader supplies facts only; this owner selects and judges.
    pub fn offer_response(
        &self,
        passenger: PersonId,
        now: SimTime,
        bodies: &BTreeMap<PersonId, Body>,
        facts: OfferResponseFacts,
        relation: impl Fn(PersonId) -> Option<RelationFacts>,
        co_present: impl Fn(PersonId) -> bool,
    ) -> Option<OfferResponse> {
        let passenger_body = bodies.get(&passenger)?;
        if !self.has_offer_to_review(passenger, now, facts, |bearer| {
            co_present(bearer)
                && bodies
                    .get(&bearer)
                    .is_some_and(|body| body.position.same_point(passenger_body.position))
        }) {
            return None;
        }
        let offers: Vec<_> = self
            .offers
            .get(&passenger)?
            .values()
            .filter(|offer| {
                offer.expires > now
                    && co_present(offer.bearer)
                    && bodies
                        .get(&offer.bearer)
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

    pub fn record_response(&mut self, response: Response) {
        if self.responses.iter().rev().take(64).any(|known| {
            known.passenger == response.passenger
                && known.bearer == response.bearer
                && known.kind == response.kind
        }) {
            return;
        }
        self.responses.push(response);
        if self.responses.len() > 64 {
            self.responses.remove(0);
        }
    }

    /// Presence is observed by the spatial provider. Memory bounds and the
    /// exclusion of the requesting passenger belong to transport.
    pub fn observe_support(
        &mut self,
        request: &SupportRequest,
        present: impl IntoIterator<Item = PersonId>,
    ) {
        for observer in support_observers(request.passenger, present) {
            let known = self.known_support.entry(observer).or_default();
            if known.len() < 64 || known.contains_key(&request.passenger) {
                known.insert(request.passenger, request.clone());
            }
        }
    }

    pub fn has_support_to_review(
        &self,
        actor: PersonId,
        request: &SupportRequest,
        now: SimTime,
        autonomous: bool,
        co_present: bool,
    ) -> bool {
        autonomous
            && co_present
            && request.expires > now
            && !self.groups.contains_key(&request.passenger)
            && !self.busy(actor)
            && !self
                .offers
                .get(&request.passenger)
                .is_some_and(|offers| offers.contains_key(&actor))
    }

    pub fn support_intent(
        &self,
        actor: PersonId,
        request: &SupportRequest,
        now: SimTime,
        bodies: &BTreeMap<PersonId, Body>,
        facts: SupportFacts,
    ) -> Option<Action> {
        let body = bodies.get(&actor)?;
        let passenger = bodies.get(&request.passenger)?;
        if !self.has_support_to_review(
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

#[cfg(test)]
mod tests;

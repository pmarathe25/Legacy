//! Delivery admission and receiving consent stay with the guardian owner.
use crate::*;
use game_core::*;
use game_ecs::Store;
use game_logistics::{Destination, KnownDestination};

/// Personal path reachability is supplied by its owner. Lineage decides which
/// locations permit a source journey and when already carried goods need none.
pub fn supply_route_known(
    source_carried: bool,
    location: &Location,
    source: Option<&StockKnowledge>,
    mut path_known: impl FnMut(AreaId, AreaId) -> bool,
) -> bool {
    source_carried
        || source.is_some_and(|source| match location {
            Location::AtArea(area) => path_known(*area, source.area),
            Location::StoppedOnRoad { from, to, .. } => {
                path_known(*from, source.area) || path_known(*to, source.area)
            }
            _ => false,
        })
}

#[allow(
    clippy::large_enum_variant,
    reason = "plain prepared supply values avoid an allocation for each accepted request"
)]
pub enum SupplyAttempt {
    Refused(&'static str),
    Accepted(AcceptedSupply),
}
pub struct AcceptedSupply {
    actor: PersonId,
    request: GuardianRequest,
    at: SimTime,
    delivery: RequestedDelivery,
    previous: Option<RequestedDelivery>,
}
impl AcceptedSupply {
    pub fn delivery(&self) -> &RequestedDelivery {
        &self.delivery
    }
    pub fn prepare_rollback(
        self,
        runtime: &Runtime,
        store: &Store,
    ) -> Result<PreparedLineage<()>, Error> {
        let before = runtime.project(store, &[self.actor])?;
        let mut after = before.world.clone();
        after
            .guardians
            .get_mut(&self.actor)
            .ok_or(Error::Unavailable)?
            .requested_delivery = self.previous;
        runtime.change(before, after, ())
    }
    /// Finish an actual receiving operator's attempt. Failure restores only this
    /// owner's tentative consent and preserves independent observations.
    pub fn prepare_finish(
        self,
        runtime: &Runtime,
        store: &Store,
        success: bool,
    ) -> Result<PreparedLineage<RequestDecision>, Error> {
        let before = runtime.project(store, &[self.actor])?;
        let mut after = before.world.clone();
        let guardian = after
            .guardians
            .get_mut(&self.actor)
            .ok_or(Error::Unavailable)?;
        if success {
            guardian.assignment = Some(Assignment {
                request: self.request,
                accepted_at: self.at,
            });
            guardian.explanation =
                "I accepted the named goods and will use my own access and actual carrying".into();
        } else {
            guardian.requested_delivery = self.previous;
            guardian.explanation =
                "The actual stock, permission or carrying space prevents this delivery".into();
        }
        runtime.change(
            before,
            after,
            if success {
                RequestDecision::Accepted
            } else {
                RequestDecision::Refused(
                    "The actual stock, permission or carrying space prevents this delivery",
                )
            },
        )
    }
}
#[derive(Clone, Copy)]
pub struct DeliveryCapability {
    pub alive: bool,
    pub health: u64,
    pub incapacitation: u64,
    pub independent: bool,
    pub restricted: bool,
    pub in_combat: bool,
}
pub fn supply_capable(facts: DeliveryCapability, location: &Location) -> bool {
    facts.alive
        && facts.health > facts.incapacitation
        && facts.independent
        && !facts.restricted
        && !facts.in_combat
        && matches!(
            location,
            Location::AtArea(_) | Location::StoppedOnRoad { .. }
        )
}
pub fn requester_available(facts: &ParticipantFacts) -> bool {
    facts.alive && facts.health > 0 && !facts.in_combat && facts.idle
}
pub fn fresh_care(need: Option<&NeedObservation>, now: SimTime) -> bool {
    need.is_some_and(|need| {
        need.observed_at == now && (need.urgency > 0 || need.hydration_urgency > 0)
    })
}
pub enum DeliveryStepDecision {
    Continue(bool),
    Execute,
    Stop,
}
pub fn delivery_step_decision<K>(
    step: &game_logistics::Step<K>,
    facts: DeliveryCapability,
) -> DeliveryStepDecision {
    match step {
        game_logistics::Step::Continue => DeliveryStepDecision::Continue(
            facts.alive && facts.independent && !facts.restricted && !facts.in_combat,
        ),
        game_logistics::Step::Complete | game_logistics::Step::Blocked(_) => {
            DeliveryStepDecision::Stop
        }
        _ => DeliveryStepDecision::Execute,
    }
}
impl Runtime {
    pub fn prepare_supply_start_failure<K>(
        &self,
        store: &Store,
        actor: PersonId,
        step: &game_logistics::Step<K>,
    ) -> Result<Option<PreparedLineage<RequestDecision>>, Error> {
        if supply_step_can_start(step) {
            Ok(None)
        } else {
            self.prepare_supply_refusal(
                store,
                actor,
                "I cannot begin a safe delivery from my current observations and carrying space",
            )
            .map(Some)
        }
    }
    pub fn prepare_delivery_tracking(
        &self,
        store: &Store,
        actor: PersonId,
        sensed: Option<Location>,
        now: SimTime,
    ) -> Result<PreparedLineage<Option<RequestedDelivery>>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        let current = after.current();
        let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
        let request = match guardian.requested_delivery.as_ref() {
            Some(request)
                if Some(request.delivery.destination) == current.map(Destination::Contact) =>
            {
                guardian.target = current;
                guardian.sensed_location = Some(sensed.ok_or(Error::Unavailable)?);
                guardian.sensed_at = now;
                (request.delivery.remaining.0 > 0).then(|| request.clone())
            }
            Some(_) => {
                guardian.requested_delivery = None;
                None
            }
            None => None,
        };
        self.change(before, after, request)
    }
    pub fn delivery_receipt_cursor(&self, store: &Store) -> Option<u64> {
        guardians(store)
            .filter_map(|(_, guardian)| {
                guardian
                    .requested_delivery
                    .as_ref()
                    .map(|request| request.last_event)
            })
            .min()
    }
    pub fn prepare_delivery_receipts(
        &self,
        store: &Store,
        receipts: &[(EventId, SupplyReceipt)],
        through: u64,
    ) -> Result<PreparedLineage<()>, Error> {
        let before = self.project_all(store)?;
        let mut after = before.world.clone();
        let mut ordered: Vec<_> = receipts.iter().collect();
        ordered.sort_by_key(|(id, _)| *id);
        for (actor, guardian) in &mut after.guardians {
            if let Some(request) = &mut guardian.requested_delivery {
                if through < request.last_event || ordered.iter().any(|(id, _)| id.0 > through) {
                    return Err(Error::Unavailable);
                }
                for (id, receipt) in &ordered {
                    request.receive(*actor, *id, receipt)?;
                }
                request.last_event = through;
            }
        }
        self.change(before, after, ())
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "named supply identity and factual observations"
    )]
    pub fn prepare_supply_pending(
        &self,
        store: &Store,
        actor: PersonId,
        requester: PersonId,
        good: Good,
        source: InventoryId,
        quantity: Quantity,
        facts: &SupplyFacts,
    ) -> Result<PreparedLineage<SupplyAttempt>, Error> {
        let before = self.project(store, &[actor])?;
        let answer = before
            .world
            .judge_supply(actor, requester, good, source, quantity, facts)?;
        let mut after = before.world.clone();
        let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
        let output = match answer {
            SupplyAnswer::Refuse(reason) => {
                guardian.explanation = reason.into();
                SupplyAttempt::Refused(reason)
            }
            SupplyAnswer::Accept(delivery) => {
                let previous = guardian.requested_delivery.replace(delivery.clone());
                SupplyAttempt::Accepted(AcceptedSupply {
                    actor,
                    request: GuardianRequest::Supply {
                        good,
                        source,
                        quantity,
                    },
                    at: facts.now,
                    delivery,
                    previous,
                })
            }
        };
        self.change(before, after, output)
    }
    pub fn prepare_delivery_location(
        &self,
        store: &Store,
        actor: PersonId,
        sensed: Location,
        now: SimTime,
    ) -> Result<PreparedLineage<()>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        let current = after.current().ok_or(Error::Unavailable)?;
        let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
        guardian.target = Some(current);
        guardian.sensed_location = Some(sensed);
        guardian.sensed_at = now;
        self.change(before, after, ())
    }
    pub fn prepare_delivery_working(
        &self,
        store: &Store,
        actor: PersonId,
    ) -> Result<PreparedLineage<()>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
        guardian.goal = GuardianGoal::Mentoring;
        guardian.explanation =
            "Carrying out an accepted supply delivery using known stores and actual cargo".into();
        self.change(before, after, ())
    }
    pub fn delivery_destination(
        &self,
        store: &Store,
        actor: PersonId,
        requester: PersonId,
        contact_location: Option<Location>,
        now: SimTime,
    ) -> Option<KnownDestination> {
        let guardian = guardian(store, actor)?;
        let (location, observed_at) = if let Some(location) = contact_location {
            (location, now)
        } else if guardian.target == Some(requester) {
            let position = game_transport::position_from_location(
                guardian.sensed_location.as_ref()?,
                guardian.sensed_at,
            )
            .ok()?;
            (
                game_transport::location_from_position(position),
                guardian.sensed_at,
            )
        } else {
            return None;
        };
        Some(KnownDestination {
            destination: Destination::Contact(requester),
            location,
            observed_at,
            accessible: true,
            room: None,
            direct_transfer: false,
        })
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "source provenance and receiver facts are independently owned"
    )]
    pub fn accepts_cargo(
        &self,
        store: &Store,
        actor: PersonId,
        source_id: InventoryId,
        source: Option<StockReference>,
        destination: StockReference,
        quantity: Quantity,
        requester_alive: bool,
        contact: bool,
    ) -> bool {
        let Some(request) =
            guardian(store, actor).and_then(|guardian| guardian.requested_delivery.as_ref())
        else {
            return false;
        };
        let Destination::Contact(requester) = request.delivery.destination else {
            return false;
        };
        quantity.0 > 0
            && quantity <= request.delivery.remaining
            && request
                .cargo
                .get(&source_id)
                .is_some_and(|tagged| *tagged >= quantity.0)
            && source.is_some_and(|source| {
                source.good == request.delivery.requirement.good && source.holder == Some(actor)
            })
            && destination.good == request.delivery.requirement.good
            && destination.holder == Some(requester)
            && requester_alive
            && contact
    }
    pub fn prepare_authored_assessment(
        &self,
        store: &Store,
        actor: PersonId,
        target: PersonId,
        assessment: Assessment,
    ) -> Result<PreparedLineage<()>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        if after.current() != Some(target) {
            return Err(Error::Unavailable);
        }
        after
            .guardians
            .get_mut(&actor)
            .ok_or(Error::Unavailable)?
            .assessments
            .insert(target, assessment);
        self.change(before, after, ())
    }
}

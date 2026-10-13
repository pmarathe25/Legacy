//! Offer and support review runs in stable actor order over canonical rows.
//! Ports expose factual rows and invoke prepared owner operations. This owner
//! decides admission, reactions, witness selection and voluntary retry policy.
use crate::*;
use game_ecs::{StableId, Store};

pub trait ReviewHost: BodyFacts + SupportWitnessFacts {
    fn store(&self) -> &Store;
    fn runtime(&self) -> &Runtime;
    fn now(&self) -> SimTime;
    fn autonomous(&self, actor: PersonId) -> bool;
    fn offer_facts(&self, actor: PersonId) -> Result<OfferResponseFacts, Error>;
    fn relation(&self, actor: PersonId, other: PersonId) -> Option<RelationFacts>;
    fn contact(&self, a: PersonId, b: PersonId) -> bool;
    fn support_facts(
        &self,
        actor: PersonId,
        request: &SupportRequest,
    ) -> Result<SupportFacts, Error>;
    fn apply_review(&mut self, change: PreparedTransport<()>) -> Result<(), Error>;
    /// Invoke the production command binding with StationaryReview admission.
    fn apply_action(&mut self, actor: PersonId, action: &Action) -> Result<(), Error>;
}

pub fn review_choices(host: &mut impl ReviewHost) -> Result<(), Error> {
    let now = host.now();
    let recipients = offer_inboxes(host.store())
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let candidates = if recipients.is_empty() {
        None
    } else {
        Some(game_spatial::LocalRoster::build(host.store())?.into_candidates())
    };
    for passenger in recipients {
        let facts = host.offer_facts(passenger)?;
        if !host
            .runtime()
            .has_offer_to_review(host.store(), passenger, now, facts, |bearer| {
                host.contact(passenger, bearer)
            })
        {
            continue;
        }
        let Some(choice) = host.runtime().offer_response(
            host.store(),
            passenger,
            now,
            host,
            facts,
            |bearer| host.relation(passenger, bearer),
            |bearer| host.contact(passenger, bearer),
        ) else {
            continue;
        };
        let witnesses = if choice.support.is_some() {
            local_support_observers(
                host.store(),
                candidates.as_ref().expect("offer review candidates"),
                passenger,
                now,
                host,
            )?
        } else {
            Vec::new()
        };
        let change = host.runtime().prepare_review(
            host.store(),
            choice.response,
            choice.support.as_ref(),
            witnesses,
        )?;
        host.apply_review(change)?;
        for action in choice.actions {
            host.apply_action(passenger, &action)?;
        }
    }
    let requests = support_knowledges(host.store())
        .flat_map(|(observer, knowledge)| {
            knowledge
                .by_passenger
                .values()
                .map(move |request| (observer, request.clone()))
        })
        .collect::<Vec<_>>();
    for (actor, request) in requests {
        if !host.runtime().has_support_to_review(
            host.store(),
            actor,
            &request,
            now,
            host.autonomous(actor),
            host.contact(actor, request.passenger),
        ) {
            continue;
        }
        let facts = host.support_facts(actor, &request)?;
        if let Some(action) =
            host.runtime()
                .support_intent(host.store(), actor, &request, now, host, facts)
        {
            support_attempt_result(host.apply_action(actor, &action))?;
        }
    }
    Ok(())
}

/// Available-action candidates preserve exact directed road placement. Physical
/// contact independently uses the common carrier-resolved point comparison.
pub fn local_bodies(
    store: &Store,
    actor: PersonId,
    now: SimTime,
    facts: &(impl BodyFacts + SupportWitnessFacts),
) -> Option<BTreeMap<PersonId, Body>> {
    let here = game_spatial::resolve(store, StableId(actor.0)).ok()?;
    let position = position_from_location(&here, now).ok()?;
    let roster = game_spatial::LocalRoster::build(store).ok()?;
    roster
        .position_candidates(StableId(actor.0))
        .ok()?
        .into_iter()
        .map(|id| PersonId(id.0))
        .filter(|actor| facts.person(*actor))
        .filter(|actor| {
            game_spatial::resolve(store, StableId(actor.0))
                .ok()
                .and_then(|location| position_from_location(&location, now).ok())
                .is_some_and(|candidate| candidate == position)
        })
        .map(|actor| Some((actor, facts.body(actor)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Host {
        store: Store,
        runtime: Runtime,
        bodies: BTreeMap<PersonId, Body>,
        actions: Vec<(PersonId, Action)>,
    }
    impl BodyFacts for Host {
        fn body(&self, actor: PersonId) -> Option<Body> {
            self.bodies.body(actor)
        }
    }
    impl SupportWitnessFacts for Host {
        fn person(&self, actor: PersonId) -> bool {
            self.bodies.contains_key(&actor)
        }
        fn tactical_contact(&self, _: PersonId, _: PersonId) -> bool {
            true
        }
    }
    impl LiftActivityFacts for Host {
        fn escaping(&self, _: PersonId) -> Option<bool> {
            Some(false)
        }
    }
    impl LiftRequestFacts for Host {
        fn permission(&self, _: PersonId) -> Option<CarryPermissionInput> {
            Some(CarryPermissionInput {
                independent: true,
                combat_restricted: false,
                alive: true,
                health: 100,
                incapacitation: None,
                yielded_captors: None,
                captive_captor: None,
            })
        }
        fn moving(&self, _: PersonId) -> bool {
            false
        }
        fn contact(&self, _: PersonId, _: PersonId) -> bool {
            true
        }
    }
    impl ReviewHost for Host {
        fn store(&self) -> &Store {
            &self.store
        }
        fn runtime(&self) -> &Runtime {
            &self.runtime
        }
        fn now(&self) -> SimTime {
            SimTime(0)
        }
        fn autonomous(&self, actor: PersonId) -> bool {
            actor != PersonId(4)
        }
        fn offer_facts(&self, actor: PersonId) -> Result<OfferResponseFacts, Error> {
            Ok(OfferResponseFacts {
                autonomous: self.autonomous(actor),
                alive: true,
                combat_restricted: false,
                idle_or_resting: true,
                health: 50,
                max_health: 100,
                hunger: 0,
                hunger_harm: 100,
                fatigue: 0,
                fatigue_limit: 100,
                independence_goal: actor == PersonId(3),
            })
        }
        fn relation(&self, _: PersonId, _: PersonId) -> Option<RelationFacts> {
            Some(RelationFacts {
                trust: 200,
                affection: 100,
            })
        }
        fn contact(&self, _: PersonId, _: PersonId) -> bool {
            true
        }
        fn support_facts(&self, _: PersonId, _: &SupportRequest) -> Result<SupportFacts, Error> {
            Ok(SupportFacts {
                autonomous: true,
                empathy: 1000,
                relation: RelationFacts {
                    trust: 200,
                    affection: 100,
                },
                hunger: 0,
                meal_relief: 100,
                fatigue: 0,
                fatigue_limit: 100,
                occupied: false,
                co_present: true,
            })
        }
        fn apply_review(&mut self, change: PreparedTransport<()>) -> Result<(), Error> {
            change.commit(&mut self.runtime, &mut self.store, &self.bodies, SimTime(0))
        }
        fn apply_action(&mut self, actor: PersonId, action: &Action) -> Result<(), Error> {
            self.actions.push((actor, action.clone()));
            match action {
                Action::RefuseCarry { bearer } => self
                    .runtime
                    .prepare_refusal(&self.store, actor, *bearer)?
                    .commit(&mut self.runtime, &mut self.store, &self.bodies, SimTime(0)),
                Action::AcceptCarry { bearers } => {
                    let plan = self.runtime.prepare_accept_request(
                        &self.store,
                        AcceptCarryRequest {
                            passenger: actor,
                            bearers: bearers.clone(),
                        },
                        SimTime(0),
                        self,
                    )?;
                    let mut batch = game_ecs::Batch::new();
                    let finish =
                        plan.stage_into(&self.runtime, &self.store, self, SimTime(0), &mut batch)?;
                    finish.validate_scope(&self.store, &batch, MotionScope::StationaryReview)?;
                    self.store.apply_batch(batch)?;
                    finish.finish(&mut self.runtime);
                    Ok(())
                }
                Action::JoinCarry { .. } => Err(Error::Unavailable),
                _ => Err(Error::Unavailable),
            }
        }
    }
    fn host() -> Host {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        game_spatial::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let mut bodies = BTreeMap::new();
        for actor in 1..=4 {
            game_spatial::set_location(&mut store, StableId(actor), Location::AtArea(AreaId(10)))
                .unwrap();
            bodies.insert(
                PersonId(actor),
                Body {
                    position: Position::Area(AreaId(10)),
                    mass: Quantity(60_000),
                    cargo: Quantity(0),
                    capacity: Quantity(100_000),
                    can_hold: true,
                    can_walk: true,
                    can_bear: true,
                },
            );
        }
        Host {
            store,
            runtime: Runtime::new(Rules::default()).unwrap(),
            bodies,
            actions: Vec::new(),
        }
    }
    #[test]
    fn owner_review_applies_acceptance_and_refusal_in_stable_recipient_order() {
        let mut host = host();
        for passenger in [4, 3, 2] {
            host.runtime
                .offer(
                    &mut host.store,
                    PersonId(1),
                    PersonId(passenger),
                    SimTime(0),
                    &host.bodies,
                )
                .unwrap();
        }
        review_choices(&mut host).unwrap();
        assert_eq!(
            host.actions,
            vec![
                (
                    PersonId(2),
                    Action::AcceptCarry {
                        bearers: vec![PersonId(1)]
                    }
                ),
                (
                    PersonId(3),
                    Action::RefuseCarry {
                        bearer: PersonId(1)
                    }
                ),
            ]
        );
        assert!(lift(&host.store, PersonId(2)).is_some());
        assert!(offer_inbox(&host.store, PersonId(3)).is_none());
        assert!(offer_inbox(&host.store, PersonId(4)).is_some());
        assert_eq!(
            host.runtime
                .responses
                .iter()
                .map(|response| response.kind)
                .collect::<Vec<_>>(),
            vec![ResponseKind::Accepted, ResponseKind::Refused]
        );
        assert!(
            game_spatial::locations(&host.store)
                .all(|(_, row)| *row == Location::AtArea(AreaId(10)))
        );
    }
    #[test]
    fn waiting_support_is_published_to_local_witnesses_and_unavailable_attempts_remain_pending() {
        let mut host = host();
        host.bodies.get_mut(&PersonId(1)).unwrap().capacity = Quantity(10_000);
        host.runtime
            .offer(
                &mut host.store,
                PersonId(1),
                PersonId(2),
                SimTime(0),
                &host.bodies,
            )
            .unwrap();
        review_choices(&mut host).unwrap();
        assert_eq!(
            host.runtime.responses[0].kind,
            ResponseKind::WaitingForSupport
        );
        assert_eq!(
            host.actions,
            vec![(
                PersonId(3),
                Action::JoinCarry {
                    passenger: PersonId(2)
                }
            )]
        );
        assert!(
            support_knowledge(&host.store, PersonId(3))
                .unwrap()
                .by_passenger
                .contains_key(&PersonId(2))
        );
        assert!(lift(&host.store, PersonId(2)).is_none());
    }
    #[test]
    fn local_action_candidates_preserve_exact_rows_while_contact_resolves_reversed_points() {
        let mut host = host();
        for (actor, from, to, progress, duration) in [(1, 10, 11, 1, 2), (2, 11, 10, 2, 4)] {
            let point = RoadPosition {
                from: AreaId(from),
                to: AreaId(to),
                progress: SimDuration(progress),
                duration: SimDuration(duration),
            };
            game_spatial::set_location(
                &mut host.store,
                StableId(actor),
                location_from_position(Position::Road(point)),
            )
            .unwrap();
            host.bodies.get_mut(&PersonId(actor)).unwrap().position = Position::Road(point);
        }
        assert!(same_place(
            &host.store,
            PersonId(1),
            PersonId(2),
            SimTime(0),
            &host
        ));
        let bodies = local_bodies(&host.store, PersonId(1), SimTime(0), &host).unwrap();
        assert_eq!(
            bodies.keys().copied().collect::<Vec<_>>(),
            vec![PersonId(1)]
        );
        let actions = host.runtime.available_actions(
            &host.store,
            PersonId(1),
            SimTime(0),
            AvailableActionFacts {
                independent: true,
                combat_restricted: false,
                moving: false,
                stopped_on_road: false,
                position: Some(bodies[&PersonId(1)].position),
            },
            || Some(bodies),
            |_| false,
            || true,
        );
        assert!(actions.is_empty());
    }
}

//! Factual composition of owner-provided acquisition with actual cargo and travel.
//! The planner sees personal memories; execution remains with physical owners.
use crate::*;
use game_logistics::{
    AcquisitionOffer, Cargo, Delivery, Destination, KnownDestination, KnownStock, Step,
};

#[derive(Clone, Copy)]
pub(crate) struct DeliveryProviders<'a> {
    pub(crate) source_cargo: &'a BTreeMap<InventoryId, Quantity>,
    pub(crate) offers: &'a [AcquisitionOffer<Command>],
    pub(crate) resources: Option<&'a equipment::RankingResources>,
}

pub(crate) struct AcquisitionFacts<'a> {
    pub(crate) simulation: &'a Simulation,
    pub(crate) actor: PersonId,
    pub(crate) resources: Option<&'a equipment::RankingResources>,
    pub(crate) personality: game_characters::decisions::Personality,
}
impl game_logistics::AcquisitionSource<Command> for AcquisitionFacts<'_> {
    fn offers(
        &self,
        good: Good,
        quantity: Quantity,
        room: Quantity,
    ) -> Vec<game_logistics::Candidate<Command>> {
        let offers = self
            .simulation
            .ecology_acquisition_offers(
                self.actor,
                good,
                game_characters::orchestration::acquisition_request_quantity(quantity, room),
                room,
                self.resources,
            )
            .into_iter()
            .chain(
                self.simulation
                    .property_acquisition_offers(self.actor, good, quantity),
            )
            .chain(
                self.simulation
                    .production_acquisition_offers(self.actor, good),
            );
        game_characters::orchestration::acquisition_candidates(
            &self.personality,
            offers,
            acquisition_action_facts,
        )
    }
}
impl Simulation {
    pub(crate) fn plan_delivery(
        &self,
        actor: PersonId,
        delivery: &Delivery,
        destination: &KnownDestination,
        minimum_handoff: Quantity,
    ) -> Step<Command> {
        let attributed = self.agreement_delivery_cargo(actor, delivery);
        self.plan_delivery_with_source_cargo(
            actor,
            delivery,
            destination,
            minimum_handoff,
            &attributed,
        )
    }

    pub(crate) fn plan_delivery_with_source_cargo(
        &self,
        actor: PersonId,
        delivery: &Delivery,
        destination: &KnownDestination,
        minimum_handoff: Quantity,
        source_cargo: &BTreeMap<InventoryId, Quantity>,
    ) -> Step<Command> {
        self.plan_delivery_with_source_cargo_and_offers(
            actor,
            delivery,
            destination,
            minimum_handoff,
            DeliveryProviders {
                source_cargo,
                offers: &[],
                resources: None,
            },
        )
    }

    pub(crate) fn plan_delivery_for_planning(
        &self,
        actor: PersonId,
        delivery: &Delivery,
        destination: &KnownDestination,
        minimum_handoff: Quantity,
        offers: &[AcquisitionOffer<Command>],
        resources: Option<&equipment::RankingResources>,
    ) -> Step<Command> {
        let attributed = self.agreement_delivery_cargo(actor, delivery);
        self.plan_delivery_with_source_cargo_and_offers(
            actor,
            delivery,
            destination,
            minimum_handoff,
            DeliveryProviders {
                source_cargo: &attributed,
                offers,
                resources,
            },
        )
    }

    fn plan_delivery_with_source_cargo_and_offers(
        &self,
        actor: PersonId,
        delivery: &Delivery,
        destination: &KnownDestination,
        minimum_handoff: Quantity,
        providers: DeliveryProviders<'_>,
    ) -> Step<Command> {
        let facts = self.delivery_facts_with_source_cargo(actor, delivery, destination, providers);
        let invocation = self.invocation_personality(actor);
        let mind = game_characters::decisions::mind::<Command>(&self.host.storage, actor);
        let personality = game_characters::personality_provider::delivery_personality(
            invocation.as_ref(),
            mind.map(|m| &m.personality),
        );
        delivery.plan_with_acquisition(
            facts,
            minimum_handoff,
            &AcquisitionFacts {
                simulation: self,
                actor,
                resources: providers.resources,
                personality,
            },
        )
    }
    pub(crate) fn delivery_facts_with_source_cargo<'a>(
        &'a self,
        actor: PersonId,
        delivery: &Delivery,
        destination: &KnownDestination,
        providers: DeliveryProviders<'_>,
    ) -> game_logistics::DeliveryFacts<'a, Command> {
        let DeliveryProviders {
            source_cargo,
            offers: additional_offers,
            resources,
        } = providers;
        let Some(_) = self.person_index.get(&actor).map(|i| self.person_at(*i)) else {
            return game_logistics::DeliveryFacts::absent(false, false, false);
        };
        let Some(body) = self
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, actor))
        else {
            return game_logistics::DeliveryFacts::absent(true, false, false);
        };
        let Some(mind) = self.host.state.decisions.as_ref().and_then(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
        }) else {
            return game_logistics::DeliveryFacts::absent(true, true, false);
        };
        let place = self.resolved_location(actor);
        let capacity = self.delivery_carrying_room(actor);
        let cargo = self.delivery_cargo(actor, delivery, source_cargo, resources);
        let stocks = self.delivery_known_stocks(actor, resources);
        let contacts = match delivery.destination {
            game_logistics::Destination::Inventory(_) => BTreeSet::new(),
            game_logistics::Destination::Contact(recipient) => {
                let recipient_alive = self
                    .host
                    .state
                    .physical
                    .as_ref()
                    .and_then(|_| game_physiology::body(&self.host.storage, recipient))
                    .is_some_and(|body| body.vitals.alive);
                game_logistics::contact_recipient(
                    recipient,
                    game_logistics::ContactFacts {
                        recipient_alive,
                        same_place: recipient_alive && self.transport_same_place(actor, recipient),
                    },
                )
                .into_iter()
                .collect()
            }
        };
        let invocation = self.invocation_personality(actor);
        let personality = game_characters::personality_provider::delivery_personality(
            invocation.as_ref(),
            Some(&mind.personality),
        );
        let additional = game_characters::orchestration::acquisition_candidates(
            &personality,
            additional_offers.iter().cloned(),
            acquisition_action_facts,
        );
        let mut destination = destination.clone();
        destination.direct_transfer = match destination.destination {
            Destination::Inventory(id) => game_cargo::inventory(&self.host.storage, id)
                .is_some_and(|stock| stock.location == InventoryLocation::Carried(actor)),
            Destination::Contact(_) => destination.direct_transfer,
        };
        game_logistics::DeliveryFacts {
            now: self.host.clock().time,
            place,
            room: capacity,
            cargo,
            stocks,
            contacts,
            routes: &mind.routes,
            direct_transfer: destination.direct_transfer,
            destination: Some(destination),
            additional,
            readiness: game_logistics::Readiness {
                actor_present: true,
                body_present: true,
                mind_present: true,
                alive: body.vitals.alive,
                functional: body.vitals.health > 0,
                independent: self.independent(actor),
                restricted: self.combat_restricts(actor),
                in_combat: self.in_combat(actor),
                dependent_care: self.care_index.contains_key(&actor),
                activity: match self.host.storage.activity(actor).expect("activity exists") {
                    Activity::Idle => game_logistics::ActivityClass::Idle,
                    Activity::Rest => game_logistics::ActivityClass::Rest,
                    Activity::Work { .. } => game_logistics::ActivityClass::Work,
                    _ => game_logistics::ActivityClass::Other,
                },
                carried: matches!(
                    game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)),
                    Some(Location::Carried { .. })
                ),
                health_busy: self.health_actor_busy(actor),
                property_talk: self.property_actor_talking(actor),
                army_talk: self.army_actor_talking(actor),
                realm_busy: self.realm_actor_busy(actor),
                mission_talk: self.host.state.missions.as_ref().is_some_and(|_| {
                    self.host
                        .state
                        .missions
                        .as_ref()
                        .unwrap()
                        .actor_talking(&self.host.storage, actor)
                }),
                social_busy: self.host.state.social.as_ref().is_some_and(|_| {
                    game_characters::social::participant(&self.host.storage, actor).is_some()
                }),
                fatigue: body.vitals.fatigue,
                fatigue_limit: self.body_rules(actor).fatigue_limit,
                can_depart: self.transport_can_depart(actor),
            },
        }
    }

    pub(crate) fn delivery_known_inventory_destination(
        &self,
        actor: PersonId,
        inventory: InventoryId,
    ) -> Option<KnownDestination> {
        let fact = game_characters::decisions::stocks(&self.host.storage, actor)?
            .known
            .get(&inventory);
        let current = game_cargo::inventory(&self.host.storage, inventory).map(|stock| {
            self.delivery_current_stock_facts(
                actor,
                stock,
                stock.quantity,
                stock.room().expect("valid stock capacity"),
            )
        });
        game_characters::orchestration::delivery_destination_from_facts(
            inventory,
            fact,
            actor,
            &self.resolved_location(actor),
            current,
            self.host.clock().time,
        )
    }

    /// Physical burden includes every carried Good and supported bodies. A new
    /// compartment can be allocated by Carry; its room cannot exceed this bound.
    pub(crate) fn delivery_carrying_room(&self, actor: PersonId) -> Quantity {
        self.transport_carrying_room(actor)
    }
    fn agreement_delivery_cargo(
        &self,
        actor: PersonId,
        delivery: &Delivery,
    ) -> BTreeMap<InventoryId, Quantity> {
        game_agreements::delivery_cargo(&self.host.storage, actor, delivery)
    }
    fn delivery_cargo(
        &self,
        actor: PersonId,
        delivery: &Delivery,
        source_cargo: &BTreeMap<InventoryId, Quantity>,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<Cargo> {
        let stocks = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .filter_map(|id| {
                game_cargo::inventory(&self.host.storage, *id).map(|stock| {
                    game_cargo::CarriedDeliveryFacts {
                        stock,
                        accessible: self.has_stock_access(actor, stock),
                        unreserved: Quantity(
                            self.unreserved_stock_for_planning(*id, actor, resources),
                        ),
                    }
                })
            })
            .collect::<Vec<_>>();
        game_cargo::delivery_cargo(
            actor,
            delivery.requirement.good,
            delivery.requirement.source,
            source_cargo,
            &stocks,
        )
    }
    fn delivery_known_stocks(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<KnownStock> {
        self.host
            .state
            .decisions
            .as_ref()
            .and_then(|_| game_characters::decisions::stocks(&self.host.storage, actor))
            .into_iter()
            .flat_map(|stocks| stocks.known.values())
            .filter_map(|fact| {
                let actor_location = self.resolved_location(actor);
                let current =
                    game_cargo::inventory(&self.host.storage, fact.inventory).map(|stock| {
                        self.delivery_current_stock_facts(
                            actor,
                            stock,
                            Quantity(self.unreserved_stock_for_planning(
                                fact.inventory,
                                actor,
                                resources,
                            )),
                            Quantity(stock.capacity.0.saturating_sub(stock.quantity.0)),
                        )
                    });
                game_characters::orchestration::delivery_stock_from_facts(
                    fact,
                    actor,
                    &actor_location,
                    current,
                    self.host.clock().time,
                )
            })
            .collect()
    }
    fn delivery_current_stock_facts(
        &self,
        actor: PersonId,
        stock: &game_cargo::Inventory,
        quantity: Quantity,
        room: Quantity,
    ) -> game_characters::orchestration::CurrentDeliveryStock {
        use game_characters::orchestration::{
            CurrentDeliveryStock, DeliveryStockPlacement, LocalStockFacts,
        };
        let (placement, location) = match stock.location {
            InventoryLocation::Area(area) => {
                (DeliveryStockPlacement::Area(area), Location::AtArea(area))
            }
            InventoryLocation::Carried(holder) => (
                DeliveryStockPlacement::Carried {
                    holder,
                    same_place: self.transport_same_place(actor, holder),
                },
                self.delivery_observed_location(holder),
            ),
        };
        CurrentDeliveryStock {
            placement,
            values: LocalStockFacts {
                location,
                quantity,
                room,
                accessible: self.has_stock_access(actor, stock),
            },
        }
    }
    /// Convert actual holder coordinates; Characters decides their visibility.
    pub(crate) fn delivery_observed_location(&self, actor: PersonId) -> Location {
        match self.transport_position(actor) {
            Some(game_transport::Position::Area(area)) => Location::AtArea(area),
            Some(game_transport::Position::Road(point)) => Location::StoppedOnRoad {
                from: point.from,
                to: point.to,
                progress: point.progress,
                duration: point.duration,
            },
            None => self.resolved_location(actor),
        }
    }
    fn property_acquisition_offers(
        &self,
        actor: PersonId,
        good: Good,
        quantity: Quantity,
    ) -> Vec<AcquisitionOffer<Command>> {
        let Some(_mind) = self.host.state.decisions.as_ref().and_then(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
        }) else {
            return Vec::new();
        };
        self.property_acquisition_opportunities(actor, Some(good), quantity)
            .into_iter()
            .filter_map(|command| {
                let Command::Property { action } = command else {
                    return None;
                };
                let source = match &action {
                    PropertyAction::RequestAccess { inventory } => *inventory,
                    PropertyAction::Take { source, .. } => *source,
                    _ => return None,
                };
                let known = game_characters::decisions::stocks(&self.host.storage, actor)?
                    .known
                    .get(&source)?;
                game_property::acquisition_offer(action, known, good)
                    .map(|offer| offer.map_action(|action| Command::Property { action }))
            })
            .collect()
    }
    fn production_acquisition_offers(
        &self,
        actor: PersonId,
        good: Good,
    ) -> Vec<AcquisitionOffer<Command>> {
        let carried_outputs = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>();
        game_characters::orchestration::production_acquisition_offers(
            &self.host.storage,
            actor,
            good,
            &carried_outputs,
            |site| Command::Work { site },
        )
    }
    pub(crate) fn delivery_command(&self, actor: PersonId, step: Step<Command>) -> Option<Command> {
        match step {
            Step::Acquire(command) => Some(command),
            Step::Carry { source, quantity } => Some(Command::Carry { source, quantity }),
            Step::Transfer {
                source,
                destination,
                quantity,
            } => Some(Command::Load {
                source,
                destination,
                quantity,
            }),
            Step::Travel(location) => self.delivery_travel_command(actor, &location),
            _ => None,
        }
    }
    fn delivery_travel_command(&self, actor: PersonId, target: &Location) -> Option<Command> {
        let routes =
            &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)?
                .routes;
        match game_transport::route_intent(
            &self.resolved_location(actor),
            target,
            self.host.clock().time,
            routes,
        )
        .ok()?
        {
            game_transport::RouteIntent::MoveArea { destination } => {
                Some(Command::Move { destination })
            }
            game_transport::RouteIntent::Transport(action) => Some(Command::Transport { action }),
        }
    }
}

/// Semantic evidence for an attempted acquisition, independent of its provider.
pub(crate) fn acquisition_risk(command: &Command) -> game_characters::decisions::Risk {
    game_characters::orchestration::acquisition_risk(acquisition_action_facts(command))
}
fn acquisition_action_facts(command: &Command) -> game_characters::orchestration::AcquisitionKind {
    use game_characters::orchestration::AcquisitionKind;
    match command {
        Command::Property {
            action: PropertyAction::Take { .. },
        } => AcquisitionKind::PropertyTaking,
        Command::Ecology {
            action: EcologyAction::Gather { unauthorized, .. },
        } => AcquisitionKind::Gathering {
            unauthorized: *unauthorized,
        },
        _ => AcquisitionKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_logistics::{BlockReason, PrerequisitePlacement, Requirement};
    fn fixture() -> Simulation {
        let mut sim = Simulation::autonomy_fixture(9, Execution::default()).unwrap();
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            sim.debug_actor_command(person_id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person_id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let actor = sim.host.state.ruler;
        let (mind,stocks)=sim.host.storage.get_pair_mut::<game_characters::decisions::Mind<game_api::Command>,game_characters::decisions::Stocks>(game_ecs::StableId((actor).0)).expect("mind and stock knowledge exist");
        stocks.known.clear();
        mind.sites.clear();
        sim
    }
    fn remember(
        sim: &mut Simulation,
        source: InventoryId,
        area: AreaId,
        good: Good,
        accessible: bool,
    ) {
        let actor = sim.host.state.ruler;
        let observed_at = sim.time();
        game_characters::decisions::stocks_mut(&mut sim.host.storage, actor)
            .unwrap()
            .known
            .insert(
                source,
                StockKnowledge {
                    inventory: source,
                    area,
                    good,
                    quantity: Quantity(5_000),
                    accessible,
                    observed_at,
                    learned_at: observed_at,
                    source: KnowledgeSource::Observed(actor),
                },
            );
    }
    fn destination(sim: &Simulation) -> KnownDestination {
        KnownDestination {
            destination: Destination::Contact(sim.host.state.ruler),
            location: sim.resolved_location(sim.host.state.ruler),
            observed_at: sim.host.clock().time,
            accessible: true,
            room: None,
            direct_transfer: false,
        }
    }
    #[test]
    fn all_goods_use_actual_carry_then_arrival_without_fulfilling_by_forecast() {
        for good in [
            Good::Food,
            Good::Water,
            Good::Timber,
            Good::Tool,
            Good::Herb,
        ] {
            let mut sim = fixture();
            let actor = sim.host.state.ruler;
            let recipient = sim.person_at(1).id;
            sim.debug_move_actor(recipient, AreaId(2)).unwrap();
            let Location::InTransit { arrives, .. } = sim.resolved_location(recipient) else {
                panic!("recipient must physically travel");
            };
            sim.advance(SimDuration(arrives.0 - sim.host.clock().time.0))
                .unwrap();
            game_characters::decisions::stocks_mut(&mut sim.host.storage, actor)
                .unwrap()
                .known
                .clear();
            let source = sim
                .debug_add_inventory(
                    good,
                    InventoryLocation::Area(AreaId(1)),
                    Quantity(5_000),
                    Quantity(5_000),
                    BTreeSet::from([actor]),
                )
                .unwrap();
            remember(&mut sim, source, AreaId(1), good, true);
            let delivery = Delivery::new(
                Requirement {
                    good,
                    quantity: Quantity(1_000),
                    source: None,
                },
                Destination::Contact(recipient),
            )
            .unwrap();
            let target = KnownDestination {
                destination: Destination::Contact(recipient),
                location: Location::AtArea(AreaId(2)),
                observed_at: sim.host.clock().time,
                accessible: true,
                room: None,
                direct_transfer: false,
            };
            let step = sim.plan_delivery(actor, &delivery, &target, Quantity(1_000));
            assert_eq!(
                step,
                Step::Carry {
                    source,
                    quantity: Quantity(1_000)
                }
            );
            let command = sim.delivery_command(actor, step).unwrap();
            sim.debug_actor_command(actor, command).unwrap();
            sim.advance(MINUTE).unwrap();
            assert_eq!(
                game_cargo::inventory(&sim.host.storage, source)
                    .expect("inventory exists")
                    .quantity,
                Quantity(4_000)
            );
            let step = sim.plan_delivery(actor, &delivery, &target, Quantity(1_000));
            assert_eq!(step, Step::Travel(Location::AtArea(AreaId(2))));
            let command = sim.delivery_command(actor, step).unwrap();
            sim.submit(Submission {
                caller_sequence: sim.next_caller_sequence().unwrap(),
                command,
            })
            .unwrap();
            let Location::InTransit { arrives, .. } = sim.resolved_location(actor) else {
                panic!("carried supply must physically travel");
            };
            sim.advance(SimDuration(arrives.0 - sim.host.clock().time.0))
                .unwrap();
            let step = sim.plan_delivery(actor, &delivery, &target, Quantity(1_000));
            let Step::Arrived {
                source: cargo,
                quantity,
            } = step
            else {
                panic!("goods must actually be held: {step:?}");
            };
            assert_eq!(quantity, Quantity(1_000));
            assert_eq!(
                game_cargo::inventory(&sim.host.storage, cargo)
                    .expect("inventory exists")
                    .good,
                good
            );
            assert_eq!(
                game_cargo::inventory(&sim.host.storage, cargo)
                    .expect("inventory exists")
                    .quantity,
                quantity
            );
            assert_eq!(delivery.remaining, Quantity(1_000));
        }
    }
    #[test]
    fn unknown_and_denied_stocks_never_become_an_accessible_supply() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let source = sim
            .debug_add_inventory(
                Good::Herb,
                InventoryLocation::Area(AreaId(1)),
                Quantity(5_000),
                Quantity(5_000),
                BTreeSet::new(),
            )
            .unwrap();
        let delivery = Delivery::new(
            Requirement {
                good: Good::Herb,
                quantity: Quantity(1_000),
                source: None,
            },
            Destination::Contact(actor),
        )
        .unwrap();
        assert_eq!(
            sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000)),
            Step::Blocked(BlockReason::NoSupply)
        );
        remember(&mut sim, source, AreaId(1), Good::Herb, false);
        assert_eq!(
            sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000)),
            Step::Blocked(BlockReason::NoSupply)
        );
        assert!(
            !game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .access
                .contains(&actor)
        );
    }
    #[test]
    fn a_dated_remote_stock_does_not_refresh_to_its_actual_different_location() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let source = sim
            .debug_add_inventory(
                Good::Herb,
                InventoryLocation::Area(AreaId(3)),
                Quantity(5_000),
                Quantity(5_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        remember(&mut sim, source, AreaId(2), Good::Herb, true);
        let delivery = Delivery::new(
            Requirement {
                good: Good::Herb,
                quantity: Quantity(1_000),
                source: None,
            },
            Destination::Contact(actor),
        )
        .unwrap();
        assert_eq!(
            sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000)),
            Step::Travel(Location::AtArea(AreaId(2)))
        );
    }
    #[test]
    fn source_qualified_work_uses_only_its_owners_real_attributed_cargo_fact() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let source = sim
            .debug_add_inventory(
                Good::Herb,
                InventoryLocation::Area(AreaId(2)),
                Quantity(5_000),
                Quantity(5_000),
                BTreeSet::new(),
            )
            .unwrap();
        let cargo = sim
            .debug_add_inventory(
                Good::Herb,
                InventoryLocation::Carried(actor),
                Quantity(2_000),
                Quantity(5_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let delivery = Delivery::new(
            Requirement {
                good: Good::Herb,
                quantity: Quantity(1_000),
                source: Some(source),
            },
            Destination::Contact(actor),
        )
        .unwrap();
        assert_eq!(
            sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000)),
            Step::Blocked(BlockReason::NoSupply)
        );
        assert_eq!(
            sim.plan_delivery_with_source_cargo(
                actor,
                &delivery,
                &destination(&sim),
                Quantity(1_000),
                &BTreeMap::from([(cargo, Quantity(1_000))])
            ),
            Step::Arrived {
                source: cargo,
                quantity: Quantity(1_000)
            }
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, cargo)
                .expect("inventory exists")
                .quantity,
            Quantity(2_000)
        );
    }
    #[test]
    fn a_known_natural_source_uses_real_collection_and_restores_through_partial_time() {
        let mut sim = Simulation::ecology_fixture(9, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            sim.debug_actor_command(person_id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person_id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let (mind,stocks)=sim.host.storage.get_pair_mut::<game_characters::decisions::Mind<game_api::Command>,game_characters::decisions::Stocks>(game_ecs::StableId((actor).0)).expect("mind and stock knowledge exist");
        stocks.known.clear();
        mind.sites.clear();
        let source = game_ecology::knowledge(&sim.host.storage, actor)
            .unwrap()
            .known_resources
            .values()
            .find(|source| source.kind == ResourceKind::Spring && source.area == AreaId(1))
            .unwrap()
            .id;
        let before = sim
            .host
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&sim.host.storage, source)
            .expect("component exists")
            .remaining;
        let delivery = Delivery::new(
            Requirement {
                good: Good::Water,
                quantity: Quantity(1_000),
                source: None,
            },
            Destination::Contact(actor),
        )
        .unwrap();
        let step = sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000));
        assert_eq!(
            step,
            Step::Acquire(Command::Ecology {
                action: EcologyAction::Gather {
                    resource: source,
                    quantity: Quantity(1_000),
                    unauthorized: false
                }
            })
        );
        let command = sim.delivery_command(actor, step).unwrap();
        sim.debug_actor_command(actor, command).unwrap();
        let ends = game_ecology::gathering(&sim.host.storage, actor)
            .unwrap()
            .completes;
        let duration = ends.0 - sim.host.clock().time.0;
        let mut restored = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(SimDuration(duration)).unwrap();
        restored.advance(SimDuration(duration / 2)).unwrap();
        restored = Simulation::load(
            &restored.save().unwrap(),
            Execution {
                workers: 2,
                reverse_jobs: true,
            },
        )
        .unwrap();
        restored
            .advance(SimDuration(duration - duration / 2))
            .unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        assert_eq!(
            sim.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.host.storage, source)
                .expect("component exists")
                .remaining,
            Quantity(before.0 - 1_000)
        );
        assert!(matches!(
            sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000)),
            Step::Arrived {
                quantity: Quantity(1_000),
                ..
            }
        ));
    }
    #[test]
    fn returning_from_a_road_uses_real_remaining_travel_before_other_delivery_legs() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        sim.initialize_transport(game_transport::Rules::default())
            .unwrap();
        sim.debug_move_actor(actor, AreaId(3)).unwrap();
        sim.advance(SimDuration(17_000)).unwrap();
        sim.debug_actor_command(
            actor,
            Command::Transport {
                action: game_transport::Action::Stop,
            },
        )
        .unwrap();
        assert!(matches!(
            sim.resolved_location(actor),
            Location::StoppedOnRoad { .. }
        ));
        let command = sim
            .delivery_command(actor, Step::Travel(Location::AtArea(AreaId(1))))
            .unwrap();
        assert!(matches!(
            command,
            Command::Transport {
                action: game_transport::Action::ReturnToArea {
                    destination: AreaId(1)
                }
            }
        ));
        sim.debug_actor_command(actor, command).unwrap();
        let Location::InTransit { arrives, .. } = sim.resolved_location(actor) else {
            panic!("endpoint return must consume real travel time");
        };
        assert!(arrives > sim.host.clock().time);
        sim.advance(SimDuration(arrives.0 - sim.host.clock().time.0))
            .unwrap();
        assert_eq!(sim.resolved_location(actor), Location::AtArea(AreaId(1)));
    }
    #[test]
    fn a_conditional_operator_composes_real_equipment_carry_before_actual_harvest() {
        let mut sim = Simulation::ecology_fixture(9, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            sim.debug_actor_command(person_id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person_id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        // This opening has a known local toolkit, but no stored timber. Its
        // physical deposit is the sole personally known timber opportunity.
        for __stock_id in game_cargo::inventories(&sim.host.storage)
            .filter(|(_, stock)| stock.good == Good::Timber)
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            let stock = game_cargo::inventory_mut(&mut sim.host.storage, __stock_id)
                .expect("inventory exists");
            stock.quantity = Quantity(0);
        }
        let resource = sim
            .debug_add_resource(
                ResourceKind::Coppice,
                AreaId(1),
                Quantity(2_000),
                Quantity(2_000),
                Quantity(0),
                BTreeSet::from([actor]),
                None,
                0,
            )
            .unwrap();
        sim.observe_ecology();
        sim.review_decisions().unwrap();
        game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, actor)
            .unwrap()
            .sites
            .clear();
        let delivery = Delivery::new(
            Requirement {
                good: Good::Timber,
                quantity: Quantity(1_000),
                source: None,
            },
            Destination::Contact(actor),
        )
        .unwrap();
        let step = sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000));
        let Step::Carry {
            source: tool,
            quantity: Quantity(1_000),
        } = step
        else {
            panic!("missing equipment must be acquired through Carry: {step:?}");
        };
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, tool)
                .expect("inventory exists")
                .good,
            Good::Tool
        );
        let command = sim.delivery_command(actor, step).unwrap();
        sim.debug_actor_command(actor, command).unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(delivery.remaining, Quantity(1_000));
        let step = sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000));
        assert_eq!(
            step,
            Step::Acquire(Command::Ecology {
                action: EcologyAction::Gather {
                    resource,
                    quantity: Quantity(1_000),
                    unauthorized: false
                }
            })
        );
        let command = sim.delivery_command(actor, step).unwrap();
        sim.debug_actor_command(actor, command).unwrap();
        let completes = game_ecology::gathering(&sim.host.storage, actor)
            .unwrap()
            .completes;
        sim.advance(SimDuration(completes.0 - sim.host.clock().time.0))
            .unwrap();
        assert_eq!(
            sim.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.host.storage, resource)
                .expect("component exists")
                .remaining,
            Quantity(1_000)
        );
        assert!(matches!(
            sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000)),
            Step::Arrived {
                quantity: Quantity(1_000),
                ..
            }
        ));
        assert_eq!(delivery.remaining, Quantity(1_000));
    }
    #[test]
    fn delivery_intent_never_grants_blanket_consent_to_unconsented_acquisition() {
        let mut sim = Simulation::ecology_fixture(9, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let spring = sim
            .host
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resources(&sim.host.storage)
            .map(|(_, value)| value)
            .find(|r| r.kind == ResourceKind::Spring && r.area == AreaId(1))
            .unwrap()
            .id;
        let source = sim
            .host
            .state
            .ecology
            .as_mut()
            .unwrap()
            .resource_mut(&mut sim.host.storage, spring)
            .unwrap();
        source.access.clear();
        source.custodian = None;
        sim.observe_ecology();
        let (mind,stocks)=sim.host.storage.get_pair_mut::<game_characters::decisions::Mind<game_api::Command>,game_characters::decisions::Stocks>(game_ecs::StableId((actor).0)).expect("mind and stock knowledge exist");
        stocks.known.clear();
        mind.sites.clear();
        mind.personality.honesty = 900;
        mind.personality.risk_tolerance = 900;
        let delivery = Delivery::new(
            Requirement {
                good: Good::Water,
                quantity: Quantity(1_000),
                source: None,
            },
            Destination::Contact(actor),
        )
        .unwrap();
        // The provider offers the physically possible attempt regardless of traits.
        assert!(
            sim.ecology_acquisition_offers(
                actor,
                Good::Water,
                Quantity(1_000),
                Quantity(1_000),
                None
            )
            .iter()
            .any(|offer| acquisition_risk(&offer.action)
                == game_characters::decisions::Risk::UnconsentedTaking)
        );
        let step = sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000));
        assert!(!matches!(
            step,
            Step::Acquire(Command::Ecology {
                action: EcologyAction::Gather {
                    unauthorized: true,
                    ..
                }
            })
        ));
        // A different personal judgment can choose that same factual attempt.
        let mind =
            game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, actor)
                .unwrap();
        mind.personality.honesty = 0;
        assert_eq!(
            sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000)),
            Step::Acquire(Command::Ecology {
                action: EcologyAction::Gather {
                    resource: spring,
                    quantity: Quantity(1_000),
                    unauthorized: true,
                }
            })
        );
        assert_eq!(
            sim.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.host.storage, spring)
                .expect("component exists")
                .remaining,
            Quantity(50_000)
        );
        assert_eq!(delivery.remaining, Quantity(1_000));
    }

    #[test]
    fn production_prerequisites_identify_the_executor_actual_tool_slot() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        sim.review_decisions().unwrap();
        let site = game_workshop::sites(&sim.host.storage)
            .map(|(_, site)| site)
            .next()
            .unwrap()
            .clone();
        let good = game_cargo::inventory(&sim.host.storage, site.output)
            .expect("inventory exists")
            .good;
        let offers = sim.production_acquisition_offers(actor, good);
        let offer = offers
            .iter()
            .find(|offer| matches!(offer.action, Command::Work { site: id } if id == site.id))
            .unwrap();
        assert!(offer.requirements.iter().any(|prerequisite| {
            prerequisite.requirement.good == Good::Tool
                && prerequisite.placement
                    == PrerequisitePlacement::AccessibleAtProvider {
                        inventory: Some(site.tool),
                    }
        }));
    }
    #[test]
    fn split_equipment_is_physically_consolidated_before_a_collection_operator() {
        let mut sim = Simulation::ecology_fixture(9, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        // Collection is the only timber source; later observations must not
        // discover an easier stored supply that bypasses the tool requirement.
        for __stock_id in game_cargo::inventories(&sim.host.storage)
            .filter(|(_, stock)| stock.good == Good::Timber)
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            let stock = game_cargo::inventory_mut(&mut sim.host.storage, __stock_id)
                .expect("inventory exists");
            stock.quantity = Quantity(0);
        }
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            sim.debug_actor_command(person_id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person_id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let resource = sim
            .debug_add_resource(
                ResourceKind::Coppice,
                AreaId(1),
                Quantity(2_000),
                Quantity(2_000),
                Quantity(0),
                BTreeSet::from([actor]),
                None,
                0,
            )
            .unwrap();
        let first = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(actor),
                Quantity(600),
                Quantity(600),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let second = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(actor),
                Quantity(400),
                Quantity(400),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let empty = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(actor),
                Quantity(0),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        sim.observe_ecology();
        let (mind,stocks)=sim.host.storage.get_pair_mut::<game_characters::decisions::Mind<game_api::Command>,game_characters::decisions::Stocks>(game_ecs::StableId((actor).0)).expect("mind and stock knowledge exist");
        stocks.known.clear();
        mind.sites.clear();
        let delivery = Delivery::new(
            Requirement {
                good: Good::Timber,
                quantity: Quantity(1_000),
                source: None,
            },
            Destination::Contact(actor),
        )
        .unwrap();
        let step = sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000));
        assert_eq!(
            step,
            Step::Transfer {
                source: first,
                destination: empty,
                quantity: Quantity(600)
            }
        );
        sim.debug_actor_command(actor, sim.delivery_command(actor, step).unwrap())
            .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, first)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, empty)
                .expect("inventory exists")
                .quantity,
            Quantity(600)
        );
        let step = sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000));
        assert_eq!(
            step,
            Step::Transfer {
                source: second,
                destination: empty,
                quantity: Quantity(400)
            },
            "cargo after first consolidation: {:?}; source: {:?}",
            sim.delivery_cargo(actor, &delivery, &BTreeMap::new(), None),
            game_cargo::inventory(&sim.host.storage, second).expect("inventory exists")
        );
        sim.debug_actor_command(actor, sim.delivery_command(actor, step).unwrap())
            .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, empty)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, second)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert_eq!(
            sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000)),
            Step::Acquire(Command::Ecology {
                action: EcologyAction::Gather {
                    resource,
                    quantity: Quantity(1_000),
                    unauthorized: false,
                }
            })
        );
        assert_eq!(delivery.remaining, Quantity(1_000));
    }
    #[test]
    fn acquisition_uses_known_partial_stock_without_public_action_list_truncation() {
        let mut sim = Simulation::property_fixture(9, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let custodian = sim.person_at(3).id;
        let mut source = InventoryId(0);
        for _ in 0..25 {
            source = sim
                .debug_add_inventory(
                    Good::Food,
                    InventoryLocation::Area(AreaId(1)),
                    Quantity(600),
                    Quantity(1_000),
                    BTreeSet::from([custodian]),
                )
                .unwrap();
            game_property::declare_custody(&mut sim.host.storage, source, custodian)
                .expect("declared custody");
        }
        let (mind,stocks)=sim.host.storage.get_pair_mut::<game_characters::decisions::Mind<game_api::Command>,game_characters::decisions::Stocks>(game_ecs::StableId((actor).0)).expect("mind and stock knowledge exist");
        stocks.known.clear();
        mind.sites.clear();
        mind.personality.honesty = 0;
        mind.personality.risk_tolerance = 900;
        remember(&mut sim, source, AreaId(1), Good::Food, false);
        game_characters::decisions::stocks_mut(&mut sim.host.storage, actor)
            .unwrap()
            .known
            .get_mut(&source)
            .unwrap()
            .quantity = Quantity(600);
        assert!(!sim.available_property_commands(actor).iter().any(|command| matches!(command,
            Command::Property { action: PropertyAction::RequestAccess { inventory } } if *inventory == source)));
        let delivery = Delivery::new(
            Requirement {
                good: Good::Food,
                quantity: Quantity(1_000),
                source: None,
            },
            Destination::Contact(actor),
        )
        .unwrap();
        let before = sim.canonical_snapshot().unwrap();
        assert!(
            sim.property_acquisition_offers(actor, Good::Food, Quantity(1_000))
                .iter()
                .any(|offer| matches!(offer.action, Command::Property {
                action: PropertyAction::Take { source: from, quantity: Quantity(600), .. }
            } if from == source))
        );
        assert_eq!(
            sim.plan_delivery(actor, &delivery, &destination(&sim), Quantity(1_000)),
            Step::Acquire(Command::Property {
                action: PropertyAction::RequestAccess { inventory: source }
            })
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        assert_eq!(delivery.remaining, Quantity(1_000));
    }
}

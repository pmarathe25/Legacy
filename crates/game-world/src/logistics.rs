//! Factual composition of owner-provided acquisition with actual cargo and travel.
//! The planner sees personal memories; execution remains with physical owners.
use crate::*;
use game_logistics::{
    AcquisitionOffer, Cargo, Delivery, Destination, Input, KnownDestination, KnownStock,
    OfferYield, Prerequisite, PrerequisitePlacement, Requirement, Step,
};

struct DeliveryProviders<'a> {
    source_cargo: &'a BTreeMap<InventoryId, Quantity>,
    offers: &'a [AcquisitionOffer<Command>],
    resources: Option<&'a equipment::RankingResources>,
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
        let DeliveryProviders {
            source_cargo,
            offers: additional_offers,
            resources,
        } = providers;
        let Some(person) = self
            .person_index
            .get(&actor)
            .map(|i| &self.state.people[*i])
        else {
            return Step::Blocked(game_logistics::BlockReason::CannotTravel);
        };
        let Some(body) = self
            .state
            .physical
            .as_ref()
            .and_then(|w| w.actors.get(&actor))
        else {
            return Step::Blocked(game_logistics::BlockReason::CannotTravel);
        };
        let Some(mind) = self
            .state
            .decisions
            .as_ref()
            .and_then(|w| w.minds.get(&actor))
        else {
            return Step::Blocked(game_logistics::BlockReason::UnknownRoute);
        };
        let place = self.resolved_location(actor);
        let usable = body.vitals.alive
            && body.vitals.health > 0
            && self.independent(actor)
            && !self.combat_restricts(actor)
            && !self.in_combat(actor)
            && !self.care_index.contains_key(&actor);
        let free = usable
            && matches!(
                body.activity,
                Activity::Idle | Activity::Rest | Activity::Work { .. }
            )
            && !matches!(person.location, Location::Carried { .. })
            && !self.health_actor_busy(actor)
            && !self.property_actor_talking(actor)
            && !self.army_actor_talking(actor)
            && !self.realm_actor_busy(actor)
            && self.state.missions.as_ref().is_none_or(|world| {
                !world
                    .talks
                    .values()
                    .any(|talk| talk.speaker == actor || talk.listener == actor)
            })
            && self
                .state
                .social
                .as_ref()
                .is_none_or(|world| !world.participants.contains_key(&actor));
        let capacity = self.delivery_carrying_room(actor);
        let cargo = self.delivery_cargo(actor, delivery, source_cargo, resources);
        let stocks = self.delivery_known_stocks(actor, resources);
        let contacts = match delivery.destination {
            game_logistics::Destination::Inventory(_) => BTreeSet::new(),
            game_logistics::Destination::Contact(recipient) => {
                if self
                    .state
                    .physical
                    .as_ref()
                    .and_then(|world| world.actors.get(&recipient))
                    .is_some_and(|body| body.vitals.alive)
                    && self.transport_same_place(actor, recipient)
                {
                    BTreeSet::from([recipient])
                } else {
                    BTreeSet::new()
                }
            }
        };
        let wanted = Quantity(delivery.remaining.0.max(minimum_handoff.0).min(capacity.0));
        let mut requested = BTreeMap::from([(delivery.requirement.good, wanted)]);
        for prerequisite in additional_offers
            .iter()
            .flat_map(|offer| &offer.requirements)
        {
            requested
                .entry(prerequisite.requirement.good)
                .and_modify(|quantity| {
                    quantity.0 = quantity.0.max(prerequisite.requirement.quantity.0)
                })
                .or_insert(prerequisite.requirement.quantity);
        }
        let mut projected = BTreeSet::new();
        let mut offers = additional_offers.to_vec();
        // The operator owners declare the prerequisites. Project only the
        // personally known providers needed by that bounded dependency closure.
        while let Some((good, quantity)) = requested
            .iter()
            .find(|(good, _)| !projected.contains(*good))
            .map(|(good, quantity)| (*good, *quantity))
        {
            projected.insert(good);
            let mut owned = self.ecology_acquisition_offers(
                actor,
                good,
                Quantity(quantity.0.min(capacity.0)),
                capacity,
                resources,
            );
            owned.extend(self.property_acquisition_offers(actor, good, quantity));
            owned.extend(self.production_acquisition_offers(actor, good));
            for prerequisite in owned.iter().flat_map(|offer| &offer.requirements) {
                requested
                    .entry(prerequisite.requirement.good)
                    .and_modify(|quantity| {
                        quantity.0 = quantity.0.max(prerequisite.requirement.quantity.0)
                    })
                    .or_insert(prerequisite.requirement.quantity);
            }
            offers.extend(owned);
        }
        let personality = self
            .invocation_personality(actor)
            .unwrap_or_else(|| mind.personality.clone());
        offers.retain(|offer| personality.accepts_risk(acquisition_risk(&offer.action)));
        let prerequisite_room = requested.keys().map(|good| (*good, capacity)).collect();
        let mut destination = destination.clone();
        destination.direct_transfer = match destination.destination {
            Destination::Inventory(id) => self
                .state
                .inventories
                .get(&id)
                .is_some_and(|stock| stock.location == InventoryLocation::Carried(actor)),
            Destination::Contact(_) => destination.direct_transfer,
        };
        delivery.plan(&Input {
            now: self.state.time,
            place: &place,
            free,
            can_travel: usable
                && body.vitals.fatigue < self.body_rules(actor).fatigue_limit
                && self.transport_can_depart(actor),
            room: capacity,
            prerequisite_room: &prerequisite_room,
            minimum_handoff,
            cargo: &cargo,
            stocks: &stocks,
            destination: Some(&destination),
            contacts: &contacts,
            routes: &mind.routes,
            offers: &offers,
        })
    }

    /// Receiver facts come from this person's dated knowledge or an actual
    /// local observation. An accepted contract can supply its narrow receiving
    /// permission separately; this helper cannot grant a general access right.
    pub(crate) fn delivery_known_inventory_destination(
        &self,
        actor: PersonId,
        inventory: InventoryId,
    ) -> Option<KnownDestination> {
        let fact = self
            .state
            .decisions
            .as_ref()?
            .minds
            .get(&actor)?
            .knowledge
            .get(&inventory);
        let stock = self.state.inventories.get(&inventory)?;
        let local = match stock.location {
            InventoryLocation::Area(area) => {
                fact.is_some() && self.resolved_location(actor) == Location::AtArea(area)
            }
            InventoryLocation::Carried(holder) => {
                holder == actor || fact.is_some() && self.transport_same_place(actor, holder)
            }
        };
        if local {
            let location = match stock.location {
                InventoryLocation::Area(area) => Location::AtArea(area),
                InventoryLocation::Carried(holder) => self.delivery_observed_location(holder),
            };
            Some(KnownDestination {
                destination: Destination::Inventory(inventory),
                location,
                observed_at: self.state.time,
                accessible: self.has_stock_access(actor, stock),
                room: Some(Quantity(stock.capacity.0 - stock.quantity.0)),
                direct_transfer: false,
            })
        } else {
            let fact = fact.filter(|fact| {
                fact.observed_at <= self.state.time && fact.learned_at <= self.state.time
            })?;
            Some(KnownDestination {
                destination: Destination::Inventory(inventory),
                location: Location::AtArea(fact.area),
                observed_at: fact.observed_at,
                accessible: fact.accessible,
                room: None,
                direct_transfer: false,
            })
        }
    }

    /// Physical burden includes every carried Good and supported bodies. A new
    /// compartment can be allocated by Carry; its room cannot exceed this bound.
    pub(crate) fn delivery_carrying_room(&self, actor: PersonId) -> Quantity {
        if !self.person_index.contains_key(&actor) || self.state.physical.is_none() {
            return Quantity(0);
        }
        let held = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .map(|id| u128::from(self.state.inventories[id].quantity.0))
            .sum::<u128>();
        let mut upper = u128::from(self.body_rules(actor).carrying_capacity.0)
            .saturating_sub(held)
            .saturating_sub(u128::from(self.transport_body_load(actor).0))
            as u64;
        if self.transport_transfer_capacity(actor, upper) {
            return Quantity(upper);
        }
        let mut lower = 0;
        while lower < upper {
            let middle = lower + (upper - lower).div_ceil(2);
            if self.transport_transfer_capacity(actor, middle) {
                lower = middle;
            } else {
                upper = middle - 1;
            }
        }
        Quantity(lower)
    }
    fn agreement_delivery_cargo(
        &self,
        actor: PersonId,
        delivery: &Delivery,
    ) -> BTreeMap<InventoryId, Quantity> {
        let Some(source) = delivery.requirement.source else {
            return BTreeMap::new();
        };
        let mut attributed = BTreeMap::<InventoryId, Quantity>::new();
        for obligation in self.state.agreements.as_ref().into_iter().flat_map(|world| world.obligations.values()).filter(|obligation| {
            matches!(obligation.term, AgreementTerm::Delivery { debtor, source: from, destination: to, .. }
                if debtor == actor && from == source && matches!(delivery.destination, Destination::Inventory(target) if target == to))
                && obligation.performed < obligation.amount()
        }) {
            for (id, quantity) in &obligation.cargo {
                let total = attributed.entry(*id).or_insert(Quantity(0));
                total.0 = total.0.saturating_add(*quantity);
            }
        }
        attributed
    }
    fn delivery_cargo(
        &self,
        actor: PersonId,
        delivery: &Delivery,
        source_cargo: &BTreeMap<InventoryId, Quantity>,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<Cargo> {
        self.inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .filter_map(|id| {
                let stock = &self.state.inventories[id];
                if !self.has_stock_access(actor, stock) {
                    return None;
                }
                let quantity = self.unreserved_stock_for_planning(*id, actor, resources);
                let mut origin = None;
                let mut origin_quantity = Quantity(0);
                if let Some(source) = delivery.requirement.source
                    && stock.good == delivery.requirement.good
                    && source != *id
                {
                    origin_quantity = Quantity(
                        quantity.min(source_cargo.get(id).copied().unwrap_or(Quantity(0)).0),
                    );
                    if origin_quantity.0 > 0 {
                        origin = Some(source);
                    }
                }
                Some(Cargo {
                    inventory: *id,
                    good: stock.good,
                    quantity: Quantity(quantity),
                    origin,
                    origin_quantity,
                    room: Quantity(stock.capacity.0.saturating_sub(stock.quantity.0)),
                })
            })
            .collect()
    }
    fn delivery_known_stocks(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<KnownStock> {
        self.state
            .decisions
            .as_ref()
            .and_then(|world| world.minds.get(&actor))
            .into_iter()
            .flat_map(|mind| mind.knowledge.values())
            .filter(|fact| {
                fact.learned_at <= self.state.time && fact.observed_at <= self.state.time
            })
            .map(|fact| {
                // A remembered foreign inventory is never refreshed from its
                // current holder's whereabouts. Only an actually local stock
                // can supply current physical position/admissibility.
                let local =
                    self.state.inventories.get(&fact.inventory).filter(|stock| {
                        match stock.location {
                            InventoryLocation::Area(area) => {
                                self.resolved_location(actor) == Location::AtArea(area)
                            }
                            InventoryLocation::Carried(holder) => {
                                self.transport_same_place(actor, holder)
                            }
                        }
                    });
                let location =
                    local.map_or(Location::AtArea(fact.area), |stock| match stock.location {
                        InventoryLocation::Area(area) => Location::AtArea(area),
                        InventoryLocation::Carried(holder) => {
                            self.delivery_observed_location(holder)
                        }
                    });
                KnownStock {
                    inventory: fact.inventory,
                    good: fact.good,
                    room: local
                        .map(|stock| Quantity(stock.capacity.0.saturating_sub(stock.quantity.0))),
                    quantity: local.map_or_else(
                        || {
                            if self.resolved_location(actor) == Location::AtArea(fact.area) {
                                Quantity(0)
                            } else {
                                fact.quantity
                            }
                        },
                        |_| {
                            Quantity(self.unreserved_stock_for_planning(
                                fact.inventory,
                                actor,
                                resources,
                            ))
                        },
                    ),
                    accessible: local
                        .map_or(fact.accessible, |stock| self.has_stock_access(actor, stock)),
                    location,
                    observed_at: if local.is_some()
                        || self.resolved_location(actor) == Location::AtArea(fact.area)
                    {
                        self.state.time
                    } else {
                        fact.observed_at
                    },
                }
            })
            .collect()
    }
    /// Used only for an actor already physically perceived by the caller.
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
        let Some(mind) = self
            .state
            .decisions
            .as_ref()
            .and_then(|w| w.minds.get(&actor))
        else {
            return Vec::new();
        };
        self.property_acquisition_opportunities(actor, Some(good), quantity)
            .into_iter()
            .filter_map(|command| {
                let (source, quantity) = match command {
                    Command::Property {
                        action: PropertyAction::RequestAccess { inventory },
                    } => (inventory, Quantity(0)),
                    Command::Property {
                        action:
                            PropertyAction::Take {
                                source, quantity, ..
                            },
                    } => (source, quantity),
                    _ => return None,
                };
                let known = mind.knowledge.get(&source)?;
                if known.good != good || known.quantity.0 == 0 {
                    return None;
                }
                Some(AcquisitionOffer {
                    key: source.0,
                    good,
                    source: Some(source),
                    location: Location::AtArea(known.area),
                    observed_at: known.observed_at,
                    accessible: true,
                    quantity: OfferYield::Known(quantity),
                    requires_carrying_room: matches!(
                        &command,
                        Command::Property {
                            action: PropertyAction::Take { .. }
                        }
                    ),
                    requirements: Vec::new(),
                    action: command,
                })
            })
            .collect()
    }
    fn production_acquisition_offers(
        &self,
        actor: PersonId,
        good: Good,
    ) -> Vec<AcquisitionOffer<Command>> {
        let Some(mind) = self
            .state
            .decisions
            .as_ref()
            .and_then(|w| w.minds.get(&actor))
        else {
            return Vec::new();
        };
        mind.sites
            .values()
            .filter_map(|site| {
                let output = mind.knowledge.get(&site.output)?;
                let input = mind.knowledge.get(&site.input)?;
                let tool = mind.knowledge.get(&site.tool)?;
                if output.good != good || !site.access || !output.accessible {
                    return None;
                }
                Some(AcquisitionOffer {
                    key: site.site.0,
                    good,
                    source: Some(site.output),
                    location: Location::AtArea(site.area),
                    observed_at: output
                        .observed_at
                        .max(input.observed_at)
                        .max(tool.observed_at),
                    accessible: true,
                    quantity: OfferYield::Unknown,
                    requires_carrying_room: self
                        .inventory_index
                        .get(&InventoryLocation::Carried(actor))
                        .is_some_and(|ids| ids.contains(&site.output)),
                    requirements: vec![
                        Prerequisite {
                            requirement: Requirement {
                                good: input.good,
                                quantity: site.input_quantity,
                                source: None,
                            },
                            placement: PrerequisitePlacement::AccessibleAtProvider {
                                inventory: Some(site.input),
                            },
                        },
                        Prerequisite {
                            requirement: Requirement {
                                good: tool.good,
                                quantity: Quantity(1_000),
                                source: None,
                            },
                            placement: PrerequisitePlacement::AccessibleAtProvider {
                                inventory: Some(site.tool),
                            },
                        },
                    ],
                    action: Command::Work { site: site.site },
                })
            })
            .collect()
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
        let routes = &self.state.decisions.as_ref()?.minds.get(&actor)?.routes;
        match game_transport::route_intent(
            &self.resolved_location(actor),
            target,
            self.state.time,
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
    match command {
        Command::Property {
            action: PropertyAction::Take { .. },
        }
        | Command::Ecology {
            action: EcologyAction::Gather {
                unauthorized: true, ..
            },
        } => game_characters::decisions::Risk::UnconsentedTaking,
        _ => game_characters::decisions::Risk::Safe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_logistics::{BlockReason, Requirement};
    fn fixture() -> Simulation {
        let mut sim = Simulation::autonomy_fixture(9, Execution::default()).unwrap();
        for person in sim.state.people.clone() {
            sim.debug_actor_command(person.id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person.id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let actor = sim.state.ruler;
        let mind = sim
            .state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
            .unwrap();
        mind.knowledge.clear();
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
        let actor = sim.state.ruler;
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
            .unwrap()
            .knowledge
            .insert(
                source,
                StockKnowledge {
                    inventory: source,
                    area,
                    good,
                    quantity: Quantity(5_000),
                    accessible,
                    observed_at: sim.state.time,
                    learned_at: sim.state.time,
                    source: KnowledgeSource::Observed(actor),
                },
            );
    }
    fn destination(sim: &Simulation) -> KnownDestination {
        KnownDestination {
            destination: Destination::Contact(sim.state.ruler),
            location: sim.resolved_location(sim.state.ruler),
            observed_at: sim.state.time,
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
            let actor = sim.state.ruler;
            let recipient = sim.state.people[1].id;
            sim.debug_move_actor(recipient, AreaId(2)).unwrap();
            let Location::InTransit { arrives, .. } = sim.resolved_location(recipient) else {
                panic!("recipient must physically travel");
            };
            sim.advance(SimDuration(arrives.0 - sim.state.time.0))
                .unwrap();
            sim.state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&actor)
                .unwrap()
                .knowledge
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
                observed_at: sim.state.time,
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
            assert_eq!(sim.state.inventories[&source].quantity, Quantity(4_000));
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
            sim.advance(SimDuration(arrives.0 - sim.state.time.0))
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
            assert_eq!(sim.state.inventories[&cargo].good, good);
            assert_eq!(sim.state.inventories[&cargo].quantity, quantity);
            assert_eq!(delivery.remaining, Quantity(1_000));
        }
    }
    #[test]
    fn unknown_and_denied_stocks_never_become_an_accessible_supply() {
        let mut sim = fixture();
        let actor = sim.state.ruler;
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
        assert!(!sim.state.inventories[&source].access.contains(&actor));
    }
    #[test]
    fn a_dated_remote_stock_does_not_refresh_to_its_actual_different_location() {
        let mut sim = fixture();
        let actor = sim.state.ruler;
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
        let actor = sim.state.ruler;
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
        assert_eq!(sim.state.inventories[&cargo].quantity, Quantity(2_000));
    }
    #[test]
    fn a_known_natural_source_uses_real_collection_and_restores_through_partial_time() {
        let mut sim = Simulation::ecology_fixture(9, Execution::default()).unwrap();
        let actor = sim.state.ruler;
        for person in sim.state.people.clone() {
            sim.debug_actor_command(person.id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person.id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let mind = sim
            .state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
            .unwrap();
        mind.knowledge.clear();
        mind.sites.clear();
        let source = sim.state.ecology.as_ref().unwrap().known[&actor]
            .values()
            .find(|source| source.kind == ResourceKind::Spring && source.area == AreaId(1))
            .unwrap()
            .id;
        let before = sim.state.ecology.as_ref().unwrap().resources[&source].remaining;
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
        let ends = sim.state.ecology.as_ref().unwrap().jobs[&actor].completes;
        let duration = ends.0 - sim.state.time.0;
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
        assert_eq!(sim.state, restored.state);
        assert_eq!(
            sim.state.ecology.as_ref().unwrap().resources[&source].remaining,
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
        let actor = sim.state.ruler;
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
        assert!(arrives > sim.state.time);
        sim.advance(SimDuration(arrives.0 - sim.state.time.0))
            .unwrap();
        assert_eq!(sim.resolved_location(actor), Location::AtArea(AreaId(1)));
    }
    #[test]
    fn a_conditional_operator_composes_real_equipment_carry_before_actual_harvest() {
        let mut sim = Simulation::ecology_fixture(9, Execution::default()).unwrap();
        let actor = sim.state.ruler;
        for person in sim.state.people.clone() {
            sim.debug_actor_command(person.id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person.id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        // This opening has a known local toolkit, but no stored timber. Its
        // physical deposit is the sole personally known timber opportunity.
        for stock in sim
            .state
            .inventories
            .values_mut()
            .filter(|stock| stock.good == Good::Timber)
        {
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
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
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
        assert_eq!(sim.state.inventories[&tool].good, Good::Tool);
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
        let completes = sim.state.ecology.as_ref().unwrap().jobs[&actor].completes;
        sim.advance(SimDuration(completes.0 - sim.state.time.0))
            .unwrap();
        assert_eq!(
            sim.state.ecology.as_ref().unwrap().resources[&resource].remaining,
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
        let actor = sim.state.ruler;
        let spring = sim
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resources
            .values()
            .find(|r| r.kind == ResourceKind::Spring && r.area == AreaId(1))
            .unwrap()
            .id;
        let source = sim
            .state
            .ecology
            .as_mut()
            .unwrap()
            .resources
            .get_mut(&spring)
            .unwrap();
        source.access.clear();
        source.custodian = None;
        sim.observe_ecology();
        let mind = sim
            .state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
            .unwrap();
        mind.knowledge.clear();
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
        let mind = sim
            .state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
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
            sim.state.ecology.as_ref().unwrap().resources[&spring].remaining,
            Quantity(50_000)
        );
        assert_eq!(delivery.remaining, Quantity(1_000));
    }

    #[test]
    fn production_prerequisites_identify_the_executor_actual_tool_slot() {
        let mut sim = fixture();
        let actor = sim.state.ruler;
        sim.review_decisions().unwrap();
        let site = sim
            .state
            .physical
            .as_ref()
            .unwrap()
            .sites
            .values()
            .next()
            .unwrap()
            .clone();
        let good = sim.state.inventories[&site.output].good;
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
        let actor = sim.state.ruler;
        // Collection is the only timber source; later observations must not
        // discover an easier stored supply that bypasses the tool requirement.
        for stock in sim
            .state
            .inventories
            .values_mut()
            .filter(|stock| stock.good == Good::Timber)
        {
            stock.quantity = Quantity(0);
        }
        for person in sim.state.people.clone() {
            sim.debug_actor_command(person.id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person.id,
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
        let mind = sim
            .state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
            .unwrap();
        mind.knowledge.clear();
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
        assert_eq!(sim.state.inventories[&first].quantity, Quantity(0));
        assert_eq!(sim.state.inventories[&empty].quantity, Quantity(600));
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
            sim.state.inventories[&second]
        );
        sim.debug_actor_command(actor, sim.delivery_command(actor, step).unwrap())
            .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(sim.state.inventories[&empty].quantity, Quantity(1_000));
        assert_eq!(sim.state.inventories[&second].quantity, Quantity(0));
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
        let actor = sim.state.ruler;
        let custodian = sim.state.people[3].id;
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
            sim.state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(source, custodian);
        }
        let mind = sim
            .state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
            .unwrap();
        mind.knowledge.clear();
        mind.sites.clear();
        mind.personality.honesty = 0;
        mind.personality.risk_tolerance = 900;
        remember(&mut sim, source, AreaId(1), Good::Food, false);
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
            .unwrap()
            .knowledge
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
        let before = sim.state.clone();
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
        assert_eq!(sim.state, before);
        assert_eq!(delivery.remaining, Quantity(1_000));
    }
}

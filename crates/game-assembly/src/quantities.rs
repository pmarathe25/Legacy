//! Public quantity choices bind visible stocks and native admission. These are
//! suggestions for ordinary actions, never a delivery or acquisition workflow.
use crate::*;

impl Simulation {
    pub(crate) fn quantity_suggestions(
        &self,
        actor: PersonId,
        good: Good,
        upper_bound: Quantity,
        increment: Quantity,
        source: Option<InventoryId>,
        destination: Option<InventoryId>,
    ) -> Vec<Quantity> {
        let mut examples = Vec::new();
        if let Some(dose) = self.care_dose(good) {
            examples.push(dose);
        }
        if let Some(health) = &self.host.state.health {
            examples.extend(health.quantity_examples(good));
        }
        let visible_destination_quantity = destination.and_then(|id| {
            game_cargo::inventory(&self.host.storage, id).map(|stock| stock.quantity)
        });
        examples.extend(game_characters::orchestration::known_input_quantities::<
            Command,
        >(
            &self.host.storage,
            actor,
            good,
            destination,
            visible_destination_quantity,
        ));
        for quantity in game_agreements::known_delivery_amounts(
            &self.host.storage,
            actor,
            good,
            source,
            destination,
        ) {
            examples.push(quantity);
        }
        if let Some(quantity) = self.host.state.lineage.as_ref().and_then(|world| {
            world.requested_delivery_quantity(&self.host.storage, actor, good, source)
        }) {
            examples.push(quantity);
        }
        game_cargo::quantity_choices(upper_bound, increment, examples)
    }

    pub(crate) fn cargo_quantity_opportunities(&self, actor: PersonId) -> Vec<QuantityOpportunity> {
        if actor != self.host.state.ruler || self.past_life_controls_actor(actor) {
            return Vec::new();
        }
        let Some(view) = self.physical_view() else {
            return Vec::new();
        };
        let Ok(actor_facts) = self.cargo_actor_facts(actor) else {
            return Vec::new();
        };
        let stocks = view
            .local_stocks
            .iter()
            .filter_map(|stock| game_cargo::inventory(&self.host.storage, stock.id))
            .collect::<Vec<_>>();
        let body_room = game_cargo::carried_quantity_rows(
            self.inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .filter_map(|id| game_cargo::inventory(&self.host.storage, *id)),
        )
        .map(|held| {
            Quantity(
                self.body_rules(actor)
                    .carrying_capacity
                    .0
                    .saturating_sub(held.0),
            )
        })
        .unwrap_or(Quantity(0));
        game_cargo::quantity_opportunities(&PublicQuantityFacts {
            simulation: self,
            actor,
            actor_facts,
            stocks,
            body_room,
        })
        .into_iter()
        .map(|offer| QuantityOpportunity {
            action: match offer.operation {
                game_cargo::QuantityOperation::Carry { source } => QuantityAction::Carry { source },
                game_cargo::QuantityOperation::Load {
                    source,
                    destination,
                } => QuantityAction::Load {
                    source,
                    destination,
                },
            },
            upper_bound: offer.upper_bound,
            increment: offer.increment,
            suggested: offer.suggested,
        })
        .collect()
    }
}

struct PublicQuantityFacts<'a> {
    simulation: &'a Simulation,
    actor: PersonId,
    actor_facts: game_cargo::ActorFacts,
    stocks: Vec<&'a game_cargo::Inventory>,
    body_room: Quantity,
}
impl game_cargo::QuantityFacts for PublicQuantityFacts<'_> {
    fn actor(&self) -> game_cargo::ActorFacts {
        self.actor_facts
    }
    fn now(&self) -> SimTime {
        self.simulation.host.clock().time
    }
    fn duration(&self) -> SimDuration {
        self.simulation.body_rules(self.actor).loading_time
    }
    fn capacity(&self) -> Quantity {
        self.simulation.body_rules(self.actor).carrying_capacity
    }
    fn body_room(&self) -> Quantity {
        self.body_room
    }
    fn stocks(&self) -> impl Iterator<Item = &game_cargo::Inventory> {
        self.stocks.iter().copied()
    }
    fn increment(&self, source: InventoryId) -> Quantity {
        self.simulation.record_quantity_increment(source)
    }
    fn source(
        &self,
        source: &game_cargo::Inventory,
        quantity: Quantity,
    ) -> game_cargo::SourceFacts {
        self.simulation
            .cargo_source_facts(self.actor, source, quantity)
    }
    fn receiver(
        &self,
        source: InventoryId,
        destination: &game_cargo::Inventory,
        quantity: Quantity,
    ) -> game_cargo::ReceiverFacts {
        self.simulation
            .cargo_receiver_facts(self.actor, source, destination, quantity)
    }
    fn examples(
        &self,
        good: Good,
        bound: Quantity,
        increment: Quantity,
        source: InventoryId,
        destination: Option<InventoryId>,
    ) -> Vec<Quantity> {
        self.simulation.quantity_suggestions(
            self.actor,
            good,
            bound,
            increment,
            Some(source),
            destination,
        )
    }
    fn carry_admitted(&self, source: InventoryId, quantity: Quantity) -> bool {
        self.simulation
            .prepare_carried_compartment(self.actor, source, self.actor, quantity, false)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn personally_accepted_remaining_delivery_is_suggested_below_a_larger_stock_bound() {
        let mut sim = Simulation::agreement_fixture(1, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let recipient = sim.person_at(3).id;
        let source = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Carried(actor),
                Quantity(1000),
                Quantity(1000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let destination = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(AreaId(1)),
                Quantity(0),
                Quantity(10000),
                BTreeSet::from([recipient]),
            )
            .unwrap();
        game_property::declare_custody(&mut sim.host.storage, destination, recipient)
            .expect("declared custody");
        sim.debug_agreement_action(
            actor,
            AgreementAction::Propose {
                parties: BTreeSet::from([actor, recipient]),
                terms: vec![AgreementTerm::Delivery {
                    debtor: actor,
                    creditor: recipient,
                    source,
                    destination,
                    quantity: Quantity(250),
                    deadline: SimTime(300000),
                }],
                procedure: None,
            },
        )
        .unwrap();
        let agreement = sim.agreement_view(actor).last().unwrap().id;
        sim.debug_agreement_action(recipient, AgreementAction::Accept { agreement })
            .unwrap();
        let action = QuantityAction::Load {
            source,
            destination,
        };
        let view = sim.observe();
        let opportunity = view
            .quantity_opportunities
            .iter()
            .find(|opportunity| opportunity.action == action)
            .unwrap();
        assert_eq!(opportunity.upper_bound, Quantity(1000));
        assert_eq!(opportunity.suggested.first(), Some(&Quantity(250)));
        assert!(
            !view
                .available_commands
                .contains(&action.command(Quantity(1000)))
        );
        assert!(opportunity.suggested.contains(&Quantity(1)));
        sim.submit(Submission {
            caller_sequence: 1,
            command: action.command(Quantity(250)),
        })
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(250)
        );
        assert_eq!(
            sim.agreement_view(actor).last().unwrap().obligations[0].performed,
            250
        );
    }
    #[test]
    fn personally_known_recipe_deficit_is_an_admitted_public_quantity() {
        let mut sim = Simulation::autonomy_fixture(1, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let site = game_workshop::sites(&sim.host.storage)
            .map(|(_, site)| site)
            .find(|site| site.area == AreaId(1))
            .unwrap()
            .id;
        let (input, output, output_quantity, duration) = {
            let site = game_workshop::site(&sim.host.storage, site).expect("site exists");
            let recipe = sim.host.state.workshops.recipe(&site.recipe).unwrap();
            (
                site.input,
                site.output,
                recipe.output_quantity,
                recipe.duration,
            )
        };
        let recipe = sim
            .fixture_recipe(input, output, Quantity(250), output_quantity, duration)
            .unwrap();
        game_workshop::site_mut(&mut sim.host.storage, site)
            .unwrap()
            .recipe = recipe;
        game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, actor)
            .unwrap()
            .sites
            .get_mut(&site)
            .unwrap()
            .input_quantity = Quantity(250);
        game_cargo::inventory_mut(&mut sim.host.storage, input)
            .unwrap()
            .quantity = Quantity(0);
        let source = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Carried(actor),
                Quantity(500),
                Quantity(500),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let action = QuantityAction::Load {
            source,
            destination: input,
        };
        let view = sim.observe();
        let opportunity = view
            .quantity_opportunities
            .iter()
            .find(|opportunity| opportunity.action == action)
            .unwrap();
        assert!(opportunity.suggested.contains(&Quantity(250)));
        sim.submit(Submission {
            caller_sequence: 1,
            command: action.command(Quantity(250)),
        })
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            Quantity(250)
        );
        assert!(sim.host.events().iter().any(|event| matches!(event.event,
            EventKind::Transferred { source: from, destination: to, quantity: Quantity(250), .. }
            if from == source && to == input)));
    }
}

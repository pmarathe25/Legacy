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
        let mut quantities = Vec::new();
        let mut seen = BTreeSet::new();
        let mut add = |quantity: Quantity| {
            let quantity = quantity.0.min(upper_bound.0) / increment.0 * increment.0;
            if quantity > 0 && seen.insert(quantity) {
                quantities.push(Quantity(quantity));
            }
        };
        if let Some(dose) = self.care_dose(good) {
            add(dose);
        }
        if let Some(health) = &self.host.state.health {
            match good {
                Good::Timber => add(health.rules.boiling_fuel),
                Good::Herb => add(health.rules.treatment_herbs),
                Good::Water => add(health.rules.treatment_water),
                Good::Tool => add(Quantity(1_000)),
                _ => {}
            }
        }
        if let Some(mind) = self.host.state.decisions.as_ref().and_then(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
        }) {
            for site in mind.sites.values() {
                if mind
                    .knowledge
                    .get(&site.input)
                    .is_some_and(|stock| stock.good == good)
                {
                    let quantity = if destination == Some(site.input) {
                        // The caller supplies only freshly visible local destinations.
                        Quantity(
                            site.input_quantity.0.saturating_sub(
                                game_cargo::inventory(&self.host.storage, site.input)
                                    .expect("inventory exists")
                                    .quantity
                                    .0,
                            ),
                        )
                    } else {
                        site.input_quantity
                    };
                    add(quantity);
                }
            }
        }
        if let Some(world) = &self.host.state.agreements {
            for agreement in world
                .knowledge
                .get(&actor)
                .into_iter()
                .flat_map(|known| known.values())
            {
                for obligation in &agreement.obligations {
                    let relevant = match &obligation.term {
                        AgreementTerm::Delivery {
                            debtor,
                            source: promised_source,
                            destination: target,
                            ..
                        } => {
                            *debtor == actor
                                && destination.is_none_or(|id| id == *target)
                                && source.is_some_and(|id| {
                                    id == *promised_source || obligation.cargo.contains_key(&id)
                                })
                        }
                        AgreementTerm::OutputShare {
                            debtor,
                            good: promised_good,
                            destination: target,
                            ..
                        } => {
                            *debtor == actor
                                && *promised_good == good
                                && destination.is_none_or(|id| id == *target)
                        }
                        _ => false,
                    };
                    if relevant {
                        add(Quantity(
                            obligation.amount().saturating_sub(obligation.performed),
                        ));
                    }
                }
            }
        }
        if let Some(request) = self
            .host
            .state
            .lineage
            .as_ref()
            .and_then(|world| world.guardians.get(&actor))
            .and_then(|guardian| guardian.requested_delivery.as_ref())
            .filter(|request| request.delivery.requirement.good == good)
        {
            let quantity = source
                .and_then(|id| request.cargo.get(&id).copied())
                .map_or(request.delivery.remaining.0, |cargo| {
                    cargo.min(request.delivery.remaining.0)
                });
            add(Quantity(quantity));
        }
        add(Quantity(1_000));
        add(upper_bound);
        // A bounded legal custom amount remains discoverable even when every
        // serving or obligation example exceeds an acceptance/transport limit.
        // Keep this final so ordinary client selection retains useful portions.
        add(increment);
        quantities
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
        let rules = self.body_rules(actor);
        let capacity = rules.carrying_capacity;
        let held: u128 = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .map(|id| {
                u128::from(
                    game_cargo::inventory(&self.host.storage, *(id))
                        .expect("inventory exists")
                        .quantity
                        .0,
                )
            })
            .sum();
        let body_room = Quantity(u128::from(capacity.0).saturating_sub(held) as u64);
        let mut opportunities = Vec::new();
        for source in view
            .local_stocks
            .iter()
            .filter(|stock| stock.can_access && stock.quantity.0 > 0)
        {
            let from =
                game_cargo::inventory(&self.host.storage, source.id).expect("inventory exists");
            let increment = self.record_quantity_increment(source.id);
            let supply = self.cargo_source_facts(actor, from, increment);
            if from.location != InventoryLocation::Carried(actor)
                && let Ok(upper_bound) =
                    game_cargo::pickup_upper_bound(from, supply, body_room, capacity)
            {
                let suggested = self
                    .quantity_suggestions(
                        actor,
                        from.good,
                        upper_bound,
                        increment,
                        Some(source.id),
                        None,
                    )
                    .into_iter()
                    .filter(|quantity| {
                        self.prepare_carried_compartment(actor, source.id, actor, *quantity, false)
                            .is_ok()
                    })
                    .collect::<Vec<_>>();
                if !suggested.is_empty() {
                    opportunities.push(QuantityOpportunity {
                        action: QuantityAction::Carry { source: source.id },
                        upper_bound,
                        increment,
                        suggested,
                    });
                }
            }
            for destination in &view.local_stocks {
                let to = game_cargo::inventory(&self.host.storage, destination.id)
                    .expect("inventory exists");
                if source.id == destination.id
                    || from.good != to.good
                    || (from.location != InventoryLocation::Carried(actor)
                        && to.location != InventoryLocation::Carried(actor))
                {
                    continue;
                }
                let receiver = self.cargo_receiver_facts(actor, source.id, to, increment);
                let Ok(upper_bound) = game_cargo::load_upper_bound(from, to, supply, receiver)
                else {
                    continue;
                };
                let suggested = self
                    .quantity_suggestions(
                        actor,
                        from.good,
                        upper_bound,
                        increment,
                        Some(source.id),
                        Some(destination.id),
                    )
                    .into_iter()
                    .filter(|quantity| {
                        game_cargo::prepare_load(
                            actor_facts,
                            from,
                            to,
                            *quantity,
                            game_cargo::SourceFacts {
                                record_permission: self
                                    .record_transfer_allowed(source.id, *quantity),
                                ..supply
                            },
                            self.cargo_receiver_facts(actor, source.id, to, *quantity),
                            self.host.clock().time,
                            rules.loading_time,
                        )
                        .is_ok()
                    })
                    .collect::<Vec<_>>();
                if !suggested.is_empty() {
                    opportunities.push(QuantityOpportunity {
                        action: QuantityAction::Load {
                            source: source.id,
                            destination: destination.id,
                        },
                        upper_bound,
                        increment,
                        suggested,
                    });
                }
            }
        }
        opportunities
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn personally_accepted_remaining_delivery_is_suggested_below_a_larger_stock_bound() {
        let mut sim = Simulation::agreement_fixture(1, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let recipient = sim.host.state.people[3].id;
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
        sim.host
            .state
            .agreements
            .as_mut()
            .unwrap()
            .custodians
            .insert(destination, recipient);
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
        let input = game_workshop::site(&sim.host.storage, site)
            .expect("site exists")
            .input;
        game_workshop::site_mut(&mut sim.host.storage, site)
            .unwrap()
            .input_quantity = Quantity(250);
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

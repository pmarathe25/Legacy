//! Physical rules with explicit scenario rates. The survival fixture is test data,
//! not the setting's eventual biology, farming cycle, or autonomous social policy.
use crate::*;

pub use game_physiology::{Rules, World};
pub use game_workshop::{Job, Site};

#[derive(Clone)]
pub(crate) struct Outcome {
    id: PersonId,
    body: game_physiology::EvaluatedInterval,
    progress: Option<game_workshop::PreparedProgress>,
}

struct PhysicalAdmission {
    actor: PersonId,
    command: Command,
    activity: Activity,
    work: Option<game_workshop::Start>,
    consumption: Option<game_workshop::Consumption>,
    project_input: Option<game_projects::PreparedChange>,
    dependent: Option<game_lifecycle::PreparedDependentStart>,
    tell: Option<game_characters::receivers::PreparedTell>,
    food: Option<game_cargo::PreparedPortionConsumption>,
    compartment: Option<game_cargo::PreparedInventoryCreation>,
}

pub(crate) fn physiology_activity(activity: &Activity) -> game_physiology::ActivityKind {
    use game_physiology::ActivityKind as Kind;
    match activity {
        Activity::Idle => Kind::Idle,
        Activity::Eat { .. } => Kind::Eat,
        Activity::Care { .. } => Kind::Care,
        Activity::Comfort { .. } => Kind::Comfort,
        Activity::Express { .. } => Kind::Express,
        Activity::Lift { .. } => Kind::Lift,
        Activity::HealthWork { .. } => Kind::HealthWork,
        Activity::Unbind { .. } => Kind::Unbind,
        Activity::Escape { .. } => Kind::Escape,
        Activity::Restrain { .. } => Kind::Restrain,
        Activity::RealmTalk { .. } => Kind::RealmTalk,
        Activity::Rest => Kind::Rest,
        Activity::Work { .. } => Kind::Work,
        Activity::Gather { .. } => Kind::Gather,
        Activity::Load { .. } => Kind::Load,
        Activity::Take { .. } => Kind::Take,
        Activity::Construct { .. } => Kind::Construct,
        Activity::Combat { .. } => Kind::Combat,
        Activity::ArmyApproach { .. } => Kind::ArmyApproach,
        Activity::Play { .. } => Kind::Play,
        Activity::Cultivate { .. } => Kind::Cultivate,
        _ => Kind::Other,
    }
}

impl Simulation {
    /// Append a receipt whose receiving rows and complete record budget were
    /// already admitted by the originating transaction.
    pub(crate) fn emit_admitted(&mut self, event: EventKind) {
        self.host
            .append_many(vec![event])
            .expect("admitted publication budget");
    }

    fn physical_contact_reads(&self, actors: &[PersonId]) -> game_ecs::ReadSet {
        let mut reads = game_ecs::ReadSet::new();
        for actor in actors {
            let mut id = game_ecs::StableId(actor.0);
            let mut seen = BTreeSet::new();
            while seen.insert(id) {
                match reads.read::<Location>(&self.host.storage, id) {
                    Some(Location::Carried { carrier }) => id = game_ecs::StableId(carrier.0),
                    _ => break,
                }
            }
        }
        reads
    }
    fn prepare_dependent_physical_start(
        &self,
        actor: PersonId,
        action: game_lifecycle::DependentAction,
    ) -> Result<game_lifecycle::PreparedDependentStart, Error> {
        let child = match action {
            game_lifecycle::DependentAction::Care(child)
            | game_lifecycle::DependentAction::Comfort(child) => Some(child),
            _ => None,
        };
        let mut actors = vec![actor];
        actors.extend(child);
        let mut reads = self.physical_contact_reads(&actors);
        for person in &actors {
            reads.read::<game_physiology::Body>(&self.host.storage, game_ecs::StableId(person.0));
        }
        let body = game_physiology::body(&self.host.storage, actor).ok_or(Error::Unavailable)?;
        self.host
            .state
            .development
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_dependent_start(
                &self.host.storage,
                actor,
                action,
                self.host.clock().time,
                game_lifecycle::DependentStartFacts {
                    fatigue_available: game_physiology::exertion_available(
                        body,
                        self.body_rules(actor).fatigue_limit,
                    ),
                    child_alive: child
                        .and_then(|child| game_physiology::body(&self.host.storage, child))
                        .is_some_and(|body| body.vitals.alive),
                    same_place: child.is_some_and(|child| self.transport_same_place(actor, child)),
                    child_receiving_care: child
                        .is_some_and(|child| self.care_index.contains_key(&child)),
                },
                self.host
                    .state
                    .physical
                    .as_ref()
                    .ok_or(Error::Unavailable)?
                    .rules
                    .eating_time,
            )
            .map(|prepared| prepared.with_factual_reads(reads))
    }
    fn complete_personal_physical(
        &mut self,
        actor: PersonId,
        activity: &Activity,
    ) -> Result<Option<bool>, Error> {
        let now = self.host.clock().time;
        let opening = self.host.transaction_opening();
        let mut batch = game_ecs::Batch::new();
        let mut guards = Vec::new();
        let mut minds =
            game_characters::receivers::PreparedMindEffects::<game_api::Command>::default();
        let mut event = None;
        let mut care_child = None;
        let mut health = None;
        let mut lineage = None;
        let mut contact = self.physical_contact_reads(&[actor]);
        let mut admitted = game_ecs::ReadFootprint::new();
        let mut success = true;
        match activity {
            Activity::Express { preference, .. } => {
                let owner = self
                    .host
                    .state
                    .development
                    .as_ref()
                    .ok_or(Error::Unavailable)?;
                let prepared = owner
                    .prepare_dependent_reaction(
                        &self.host.storage,
                        actor,
                        game_lifecycle::DependentAction::Signal(*preference),
                        now,
                        true,
                        true,
                    )?
                    .ok_or(Error::Unavailable)?;
                guards.push(prepared.stage_into(owner, &self.host.storage, &mut batch)?);
                event = Some(EventKind::Expressed {
                    actor,
                    preference: *preference,
                });
            }
            Activity::Play { interest, .. } => {
                let gain = minds.receive_play(&self.host.storage, actor, *interest)?;
                event = Some(EventKind::Practiced {
                    actor,
                    interest: *interest,
                    gain,
                });
            }
            Activity::Eat { source, .. } => {
                let stock =
                    game_cargo::inventory(&self.host.storage, *source).ok_or(Error::Unavailable)?;
                let quantity = self
                    .host
                    .state
                    .physical
                    .as_ref()
                    .ok_or(Error::Unavailable)?
                    .rules
                    .meal_size;
                match game_cargo::prepare_food_consumption(
                    &self.host.storage,
                    actor,
                    *source,
                    quantity,
                    game_cargo::ConsumptionFacts {
                        good: stock.good,
                        accessible: self.has_stock_access(actor, stock),
                        contact: self.stock_here(actor, stock),
                        unreserved: stock.quantity,
                    },
                ) {
                    Ok(consumption) => {
                        guards.push(consumption.stage_into(&self.host.storage, &mut batch)?);
                        let relief = game_physiology::prepare_meal_relief(
                            &self.host.storage,
                            actor,
                            self.body_rules(actor).meal_relief,
                        )?;
                        guards.push(relief.stage_into(&self.host.storage, &mut batch)?);
                        admitted.protect::<game_physiology::Body>(game_ecs::StableId(actor.0));
                        event = Some(EventKind::Ate {
                            actor,
                            source: *source,
                            quantity,
                        });
                    }
                    Err(Error::Unavailable) => success = false,
                    Err(error) => return Err(error),
                }
            }
            Activity::Comfort { child, .. } | Activity::Care { child, .. } => {
                care_child = Some(*child);
                contact.extend(self.physical_contact_reads(&[*child]));
                let body = contact
                    .read::<game_physiology::Body>(&self.host.storage, game_ecs::StableId(child.0))
                    .ok_or(Error::Unavailable)?;
                let owner = self
                    .host
                    .state
                    .development
                    .as_ref()
                    .ok_or(Error::Unavailable)?;
                let action = if matches!(activity, Activity::Comfort { .. }) {
                    game_lifecycle::DependentAction::Comfort(*child)
                } else {
                    game_lifecycle::DependentAction::Care(*child)
                };
                let prepared = owner.prepare_dependent_reaction(
                    &self.host.storage,
                    actor,
                    action,
                    now,
                    body.vitals.alive,
                    self.transport_same_place(actor, *child),
                )?;
                if let Some(lifecycle) = prepared {
                    match activity {
                        Activity::Comfort { .. } => {
                            guards.push(lifecycle.stage_into(
                                owner,
                                &self.host.storage,
                                &mut batch,
                            )?);
                            minds.receive_comfort_relationship(
                                &self.host.storage,
                                *child,
                                actor,
                            )?;
                            event = Some(EventKind::Comforted {
                                caregiver: actor,
                                child: *child,
                            });
                        }
                        Activity::Care { source, .. } => {
                            let stock = game_cargo::inventory(&self.host.storage, *source)
                                .ok_or(Error::Unavailable)?;
                            let good = stock.good;
                            let quantity = self.care_dose(good).ok_or(Error::Unavailable)?;
                            match game_cargo::prepare_portion_consumption(
                                &self.host.storage,
                                actor,
                                *source,
                                quantity,
                                game_cargo::ConsumptionFacts {
                                    good,
                                    accessible: self.has_stock_access(actor, stock),
                                    contact: self.stock_here(actor, stock),
                                    unreserved: stock.quantity,
                                },
                            ) {
                                Ok(consumption) => {
                                    let rules = self.body_rules(*child);
                                    let nourishment = game_physiology::prepare_nourishment(
                                        &self.host.storage,
                                        *child,
                                        good,
                                        &rules,
                                    )?;
                                    if let Some(relief) = nourishment {
                                        guards.push(
                                            relief.stage_into(&self.host.storage, &mut batch)?,
                                        );
                                        admitted.protect::<game_physiology::Body>(
                                            game_ecs::StableId(child.0),
                                        );
                                    }
                                    if good == Good::Water {
                                        let health_owner = self
                                            .host
                                            .state
                                            .health
                                            .as_ref()
                                            .ok_or(Error::Unavailable)?;
                                        let prepared = health_owner.prepare_care_water_received(
                                            &self.host.storage,
                                            *child,
                                            game_health::StockFacts {
                                                id: *source,
                                                good,
                                                quantity: stock.quantity,
                                                available: stock.quantity,
                                                usable: true,
                                                tool_available: false,
                                            },
                                            quantity,
                                            self.health_physiology(*child),
                                            now,
                                        )?;
                                        health = Some(prepared.stage_into(
                                            health_owner,
                                            &self.host.storage,
                                            &mut batch,
                                        )?);
                                    }
                                    guards.push(
                                        consumption.stage_into(&self.host.storage, &mut batch)?,
                                    );
                                    guards.push(lifecycle.stage_into(
                                        owner,
                                        &self.host.storage,
                                        &mut batch,
                                    )?);
                                    minds.receive_care_relationship(
                                        &self.host.storage,
                                        *child,
                                        actor,
                                    )?;
                                    minds.receive_care_receipt(
                                        &self.host.storage,
                                        actor,
                                        *child,
                                        good,
                                        quantity,
                                        opening.next_event,
                                    )?;
                                    if let Some(world) = &self.host.state.lineage
                                        && game_lineage::guardian(&self.host.storage, actor)
                                            .is_some()
                                    {
                                        lineage = Some(world.prepare_care_receipt(
                                            &self.host.storage,
                                            actor,
                                            *child,
                                            good,
                                            quantity,
                                            opening.next_event,
                                        )?);
                                    }
                                    event = Some(EventKind::Cared {
                                        caregiver: actor,
                                        child: *child,
                                        source: *source,
                                        quantity,
                                    });
                                }
                                Err(Error::Unavailable) => success = false,
                                Err(error) => return Err(error),
                            }
                        }
                        _ => unreachable!(),
                    }
                } else {
                    success = false;
                }
            }
            Activity::Tell {
                recipient, fact, ..
            } => {
                use game_characters::receivers::{RecipientActivity, TellCompletionFacts};
                let mut reads = self.physical_contact_reads(&[actor, *recipient]);
                let receiver_body = reads
                    .read::<game_physiology::Body>(
                        &self.host.storage,
                        game_ecs::StableId(recipient.0),
                    )
                    .ok_or(Error::Unavailable)?;
                let alive = receiver_body.vitals.alive;
                let health = receiver_body.vitals.health;
                let state = match reads
                    .read::<Activity>(&self.host.storage, game_ecs::StableId(recipient.0))
                {
                    Some(Activity::Idle) => RecipientActivity::Idle,
                    Some(Activity::Rest) => RecipientActivity::Rest,
                    _ => RecipientActivity::Other,
                };
                let facts = TellCompletionFacts {
                    sender_location: game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId(actor.0),
                    )
                    .ok_or(Error::Unavailable)?
                    .clone(),
                    recipient_location: game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId(recipient.0),
                    )
                    .ok_or(Error::Unavailable)?
                    .clone(),
                    recipient_alive: alive,
                    recipient_health: health,
                    recipient_activity: state,
                };
                let prepared = game_characters::receivers::prepare_tell_result::<game_api::Command>(
                    &self.host.storage,
                    actor,
                    *recipient,
                    now,
                    fact.clone(),
                    &facts,
                    reads,
                )?;
                success = prepared.accepted();
                guards.push(prepared.stage_into(&self.host.storage, &mut batch)?);
                if success {
                    event = Some(EventKind::Reported {
                        sender: actor,
                        recipient: *recipient,
                        inventory: fact.inventory,
                    });
                }
            }
            _ => return Ok(None),
        }
        contact.read::<Activity>(&self.host.storage, game_ecs::StableId(actor.0));
        batch.insert(game_ecs::StableId(actor.0), Activity::Idle);
        admitted.protect::<Activity>(game_ecs::StableId(actor.0));
        guards.push(contact.bind(&self.host.storage, &batch, &admitted)?);
        let events = if let Some(event) = event {
            vec![event]
        } else if !success {
            vec![EventKind::Interrupted { actor }]
        } else {
            Vec::new()
        };
        let traits = if let Some(traits) = self.prepare_trait_events(&events)? {
            Some(
                traits.fold_minds(&self.host.storage, minds)?.stage_into(
                    self.host
                        .state
                        .temperament
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &mut batch,
                    &self
                        .host
                        .state
                        .genetics
                        .as_ref()
                        .ok_or(Error::Unavailable)?
                        .allele_source(&self.host.storage),
                )?,
            )
        } else {
            guards.push(minds.stage_into(&self.host.storage, &mut batch)?);
            None
        };
        let social = self
            .prepare_social_receipts(&events)?
            .map(|plan| {
                plan.stage_into(
                    self.host.state.social.as_ref().ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    now,
                    &mut batch,
                )
            })
            .transpose()?;
        let witness = events
            .first()
            .map(|event| self.prepare_lineage_witness(event, &batch))
            .transpose()?
            .flatten();
        let mut lineage_receipt = None;
        let mut lineage_witness = None;
        let mut lineage_care_witness = None;
        match (lineage, witness) {
            (Some(plan), Some(witness)) => {
                lineage_care_witness = Some(
                    witness
                        .fold_into(plan, self)?
                        .stage_into(self, &mut batch)?,
                );
            }
            (Some(plan), None) => {
                lineage_receipt = Some(plan.stage_into(
                    self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &mut batch,
                )?);
            }
            (None, Some(witness)) => {
                lineage_witness = Some(witness.stage_into(self, &mut batch)?);
            }
            (None, None) => {}
        }
        if let Some(health) = &health {
            health.validate_batch(
                self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                &self.host.storage,
                &batch,
            )?;
        }
        if let Some(lineage) = &lineage_receipt {
            lineage.validate(
                self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?,
                &self.host.storage,
                &batch,
            )?;
            guards.push(lineage.guard().clone());
        }
        if let Some(lineage) = &lineage_witness {
            lineage.validate(self, &batch)?;
            guards.extend(lineage.row_guards());
        }
        if let Some(lineage) = &lineage_care_witness {
            lineage.validate(self, &batch)?;
            guards.extend(lineage.row_guards());
        }
        if let Some(social) = &social {
            social.validate(
                self.host.state.social.as_ref().ok_or(Error::Unavailable)?,
                &self.host.storage,
                now,
                &batch,
            )?;
            guards.push(social.row_guard());
        }
        if let Some(traits) = &traits {
            self.validate_trait_events(traits, &batch)?;
            guards.extend(traits.row_guards().iter().cloned());
        }
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.guards = guards;
        transaction.events = events;
        self.host.commit_transaction(transaction)?;
        if let Some(child) = care_child {
            self.care_index.remove(&child);
        }
        if let Some(health) = health {
            let _ = health.finish();
        }
        if let Some(lineage) = lineage_receipt {
            let _ = lineage.finish();
        }
        if let Some(lineage) = lineage_witness {
            lineage.finish();
        }
        if let Some(lineage) = lineage_care_witness {
            let _ = lineage.finish();
        }
        Ok(Some(success))
    }

    fn complete_physical_load(&mut self, job: game_cargo::LoadJob) -> Result<(), Error> {
        let opening = self.host.transaction_opening();
        let intent = TransferIntent {
            actor: job.actor,
            source: job.source,
            destination: job.destination,
            quantity: job.quantity,
        };
        let request = job.closing_request(self.time(), self.cargo_closing_facts(&intent))?;
        let cargo = self.prepare_cargo_requests(vec![request], None)?;
        let granted = cargo.decisions()[0].1;
        let property = if granted {
            self.host
                .state
                .property
                .as_ref()
                .map(|owner| {
                    owner.prepare_material_receipt_with_prefix(
                        &self.host.storage,
                        &intent,
                        opening.next_event,
                        self.prepare_property_receipts()?,
                    )
                })
                .transpose()?
        } else {
            None
        };
        let source =
            game_cargo::inventory(&self.host.storage, job.source).ok_or(Error::Unavailable)?;
        let destination =
            game_cargo::inventory(&self.host.storage, job.destination).ok_or(Error::Unavailable)?;
        let water = if granted && source.good == Good::Water {
            self.prepare_health_water_reaction(game_health::WaterReaction::Transfer {
                actor: job.actor,
                source: job.source,
                destination: job.destination,
                quantity: job.quantity,
                source_before: source.quantity,
                destination_before: destination.quantity,
            })?
        } else {
            None
        };
        let records = if granted {
            self.host
                .state
                .invocation
                .as_ref()
                .map(|owner| {
                    owner.prepare_record_transfer(
                        &self.host.storage,
                        job.source,
                        job.destination,
                        job.quantity,
                        None,
                    )
                })
                .transpose()?
                .flatten()
        } else {
            None
        };
        let mut batch = game_ecs::Batch::new();
        let (_, cargo) = cargo.stage_guarded_into(&self.host.storage, &mut batch)?;
        let mut guards = vec![cargo];
        let property = property
            .map(|plan| {
                plan.stage_into(
                    self.host
                        .state
                        .property
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        let water = water
            .map(|plan| {
                plan.stage_into(
                    self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        if let Some(records) = records {
            guards.push(records.stage_guarded_into(&self.host.storage, &mut batch)?);
        }
        let mut holders = vec![job.actor];
        for stock in [source, destination] {
            if let InventoryLocation::Carried(holder) = stock.location {
                holders.push(holder);
            }
        }
        holders.sort();
        holders.dedup();
        let mut reads = self.physical_contact_reads(&holders);
        for holder in holders {
            let id = game_ecs::StableId(holder.0);
            reads.read::<game_physiology::Body>(&self.host.storage, id);
            reads.read::<game_lifecycle::Life>(&self.host.storage, id);
            reads.read::<game_characters::people::Person>(&self.host.storage, id);
            reads.read::<game_physiology::traits::Development>(&self.host.storage, id);
            reads.read::<game_genetics::PersonGenome>(&self.host.storage, id);
            reads.read::<game_skills::Skills>(&self.host.storage, id);
            reads.read::<game_lineage::Guardian>(&self.host.storage, id);
        }
        reads.read::<Activity>(&self.host.storage, game_ecs::StableId(job.actor.0));
        batch.insert(game_ecs::StableId(job.actor.0), Activity::Idle);
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<Activity>(game_ecs::StableId(job.actor.0));
        guards.push(reads.bind(&self.host.storage, &batch, &admitted)?);
        if let Some(property) = &property {
            property.validate_runtime(
                self.host
                    .state
                    .property
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
            )?;
            property.validate_batch(&self.host.storage, &batch)?;
        }
        if let Some(water) = &water {
            water.validate_batch(
                self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                &self.host.storage,
                &batch,
            )?;
        }
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.guards = guards;
        transaction.events.push(if granted {
            EventKind::Transferred {
                actor: job.actor,
                source: job.source,
                destination: job.destination,
                quantity: job.quantity,
            }
        } else {
            EventKind::Interrupted { actor: job.actor }
        });
        self.host.commit_transaction(transaction)?;
        if let Some(property) = property {
            property.finish(self.host.state.property.as_mut().unwrap());
        }
        if let Some(water) = water {
            let _ = water.finish();
        }
        Ok(())
    }

    pub(crate) fn body_rules(&self, actor: PersonId) -> Rules {
        rules_for(&self.canonical_state(), actor)
    }
    pub(crate) fn care_dose(&self, good: Good) -> Option<Quantity> {
        game_physiology::IntakeDoses {
            food: self
                .host
                .state
                .physical
                .as_ref()
                .map(|world| world.rules.meal_size),
            water: self
                .host
                .state
                .health
                .as_ref()
                .map(|world| world.rules.drink_quantity),
        }
        .care(good)
    }

    pub(crate) fn prepare_care_source(
        &self,
        actor: PersonId,
        child: PersonId,
        source: InventoryId,
    ) -> Result<Quantity, Error> {
        let life = self
            .host
            .state
            .development
            .as_ref()
            .and_then(|_| game_lifecycle::life(&self.host.storage, child))
            .ok_or(Error::Unavailable)?;
        let receiver = self
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, child))
            .ok_or(Error::Unavailable)?;
        let stock = game_cargo::inventory(&self.host.storage, source).ok_or(Error::Unavailable)?;
        let quantity = self.care_dose(stock.good).ok_or(Error::Unavailable)?;
        game_lifecycle::prepare_care_source(game_lifecycle::CareSourceFacts {
            actor,
            child,
            stage: life.stage,
            alive: receiver.vitals.alive,
            available: matches!(
                self.host.storage.activity(child),
                Some(Activity::Idle | Activity::Rest)
            ),
            assigned_caregiver: self.care_index.get(&child).copied(),
            same_place: self.transport_same_place(actor, child),
            authorized_source: self.can_use(actor, stock),
            available_quantity: self.unreserved_stock(source, actor),
            dose: quantity,
        })?;
        if stock.good == Good::Water {
            self.preflight_water_care(child, source, quantity)?;
        }
        Ok(quantity)
    }
    pub(crate) fn initialize_physical_people(&mut self, rules: Rules) -> Result<(), Error> {
        if self.host.state.physical.is_some() {
            return Err(Error::InvalidConfiguration(
                "initial bodies already exist".into(),
            ));
        }
        rules.validate()?;
        let people: Vec<_> = self.people().map(|person| person.id).collect();
        for person in people {
            game_physiology::insert_body(&mut self.host.storage, person, rules.newborn()?)?;
            self.host.storage.set_activity(person, Activity::Idle)?;
        }
        self.host.state.physical = Some(World { rules });
        Ok(())
    }
    pub fn survival_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::kernel_fixture(seed, 15, execution)?;
        sim.host.state.scenario = scenarios().remove(1);
        let village = sim.host.state.areas[0].id;
        let forest = sim.host.state.areas[2].id;
        let people: BTreeSet<_> = sim.people().map(|p| p.id).collect();
        let rules = Rules {
            hunger_harm: 600_000,
            fatigue_limit: 300_000,
            max_health: 180_000,
            meal_relief: 300_000,
            meal_size: Quantity(1_000),
            eating_time: MINUTE,
            loading_time: MINUTE,
            carrying_capacity: Quantity(10_000),
        };
        sim.initialize_physical_people(rules)?;
        let food = game_cargo::inventories(&sim.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap();
        game_cargo::inventory_mut(&mut sim.host.storage, food)
            .unwrap()
            .quantity = Quantity(100_000);
        let timber = sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(village),
            Quantity(20_000),
            Quantity(100_000),
            people.clone(),
        )?;
        let tools = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(village),
            Quantity(1_000),
            Quantity(100_000),
            people.clone(),
        )?;
        let forest_tools = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(forest),
            Quantity(1_000),
            Quantity(100_000),
            people.clone(),
        )?;
        let deposit = sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(forest),
            Quantity(20_000),
            Quantity(100_000),
            people.clone(),
        )?;
        let output = sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(forest),
            Quantity(0),
            Quantity(100_000),
            people.clone(),
        )?;
        for (area, input, tool, output) in [
            (village, timber, tools, food),
            (forest, deposit, forest_tools, output),
        ] {
            let id = SiteId(sim.host.allocate_id()?.0);
            game_workshop::insert_site(
                &mut sim.host.storage,
                Site {
                    recipe: game_workshop::RecipeId(
                        if area == village {
                            "timber-food"
                        } else {
                            "timber-timber"
                        }
                        .into(),
                    ),
                    id,
                    area,
                    input,
                    tool,
                    output,
                    access: people.clone(),
                    last_granted: None,
                },
            )
            .expect("validated site insertion");
        }
        // Each good has a separate physical stack. Carrying capacity is shared
        // across every stack carried by the same person.
        for p_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            sim.debug_add_inventory(
                Good::Timber,
                InventoryLocation::Carried(p_id),
                Quantity(0),
                Quantity(10_000),
                BTreeSet::from([p_id]),
            )?;
        }
        Ok(sim)
    }

    pub(crate) fn physical_view(&self) -> Option<PhysicalView> {
        self.host.state.physical.as_ref()?;
        let actor =
            &game_physiology::body(&self.host.storage, self.host.state.ruler).expect("body exists");
        let ruler = self.person_at(self.person_index[&self.host.state.ruler]);
        if !actor.vitals.alive {
            return Some(PhysicalView {
                vitals: actor.vitals.clone(),
                activity: self
                    .host
                    .storage
                    .activity(self.host.state.ruler)
                    .expect("activity exists")
                    .clone(),
                local_stocks: Vec::new(),
                local_sites: Vec::new(),
            });
        }
        let area = if let Location::AtArea(a) = self.resolved_location(ruler.id) {
            Some(a)
        } else {
            None
        };
        Some(PhysicalView {
            vitals: actor.vitals.clone(),
            activity: self
                .host
                .storage
                .activity(self.host.state.ruler)
                .expect("activity exists")
                .clone(),
            local_stocks: self
                .inventory_index
                .get(&InventoryLocation::Carried(ruler.id))
                .into_iter()
                .flatten()
                .chain(
                    area.and_then(|a| self.inventory_index.get(&InventoryLocation::Area(a)))
                        .into_iter()
                        .flatten(),
                )
                .map(|id| {
                    game_cargo::inventory(&self.host.storage, *(id)).expect("inventory exists")
                })
                .map(|i| StockView {
                    id: i.id,
                    good: i.good,
                    quantity: i.quantity,
                    can_access: self.has_stock_access(ruler.id, i),
                    capacity: i.capacity,
                    position: match i.location {
                        InventoryLocation::Area(id) => StockPosition::Area(id),
                        InventoryLocation::Carried(id) => StockPosition::Carried(id),
                    },
                })
                .collect(),
            local_sites: game_workshop::local_work_sites(&self.host.storage, ruler.id, area),
        })
    }

    pub(crate) fn start_physical_action(
        &mut self,
        actor: PersonId,
        command: Command,
    ) -> Result<(), Error> {
        use game_physiology::PersonalActionKind as Kind;
        let kind = match &command {
            Command::Signal { .. } => Kind::Signal,
            Command::Play { .. } => Kind::Play,
            Command::Rest => Kind::Rest,
            Command::Cancel => Kind::Cancel,
            Command::FreeCaptive { .. } => Kind::Release,
            Command::Comfort { .. } => Kind::Comfort,
            Command::Care { .. } => Kind::Care,
            Command::Tell { .. } => Kind::Tell,
            Command::Eat { .. } => Kind::Eat,
            Command::Load { .. } => Kind::Load,
            Command::Work { .. } => Kind::Work,
            _ => Kind::Other,
        };
        let body = self
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, actor));
        let area = game_physiology::prepare_personal_action_area(
            kind,
            game_physiology::PersonalActionFacts {
                conflict_allowed: game_conflict::physical_action_allowed(
                    self.combat_restricts(actor),
                    self.in_combat(actor),
                    kind,
                ),
                independent: self.independent(actor),
                receiving_care: self.care_index.contains_key(&actor),
                body,
            },
            || self.resolved_location(actor),
        )?;
        let world = self
            .host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let a = body.ok_or(Error::Unavailable)?;
        let rules = &world.rules;
        let mut food_admission = None;
        let mut dependent = None;
        let mut tell = None;
        let (activity, new_job, consume) = match command {
            Command::Signal { preference } => {
                let prepared = self.prepare_dependent_physical_start(
                    actor,
                    game_lifecycle::DependentAction::Signal(preference),
                )?;
                let completes = prepared.start().completes;
                dependent = Some(prepared);
                (
                    Activity::Express {
                        preference,
                        completes,
                    },
                    None,
                    None,
                )
            }
            Command::Play { interest } => {
                let prepared = self.prepare_dependent_physical_start(
                    actor,
                    game_lifecycle::DependentAction::Play(interest),
                )?;
                let completes = prepared.start().completes;
                dependent = Some(prepared);
                (
                    Activity::Play {
                        interest,
                        completes,
                    },
                    None,
                    None,
                )
            }
            Command::Rest => (Activity::Rest, None, None),
            Command::Cancel => (Activity::Idle, None, None),
            Command::FreeCaptive { target } => {
                game_conflict::require_release_admission(self.can_release(actor, target))?;
                self.preflight_force_intervention()?;
                let completes = rules.unbind_deadline(self.host.clock().time)?;
                (Activity::Unbind { target, completes }, None, None)
            }
            Command::Comfort { child } => {
                let prepared = self.prepare_dependent_physical_start(
                    actor,
                    game_lifecycle::DependentAction::Comfort(child),
                )?;
                let completes = prepared.start().completes;
                dependent = Some(prepared);
                (Activity::Comfort { child, completes }, None, None)
            }
            Command::Care { child, source } => {
                self.prepare_care_source(actor, child, source)?;
                let prepared = self.prepare_dependent_physical_start(
                    actor,
                    game_lifecycle::DependentAction::Care(child),
                )?;
                let completes = prepared.start().completes;
                dependent = Some(prepared);
                (
                    Activity::Care {
                        child,
                        source,
                        completes,
                    },
                    None,
                    None,
                )
            }
            Command::Tell {
                recipient,
                inventory,
            } => {
                let mut reads = game_ecs::ReadSet::new();
                reads.read::<Inventory>(&self.host.storage, game_ecs::StableId(inventory.0));
                let visible = game_cargo::inventory(&self.host.storage, inventory).map(|stock| {
                    game_characters::receivers::TellStockFacts {
                        inventory,
                        placement: match stock.location {
                            InventoryLocation::Area(area) => {
                                game_characters::receivers::StockPlacement::Area(area)
                            }
                            InventoryLocation::Carried(actor) => {
                                game_characters::receivers::StockPlacement::Carried(actor)
                            }
                        },
                        good: stock.good,
                        quantity: stock.quantity,
                        accessible: self.has_stock_access(actor, stock),
                    }
                });
                let prepared = game_characters::receivers::prepare_tell::<game_api::Command>(
                    &self.host.storage,
                    actor,
                    recipient,
                    inventory,
                    self.host.clock().time,
                    visible,
                    reads,
                )?;
                let fact = prepared.fact().clone();
                let completes = prepared.completes();
                tell = Some(prepared);
                (
                    Activity::Tell {
                        recipient,
                        fact,
                        completes,
                    },
                    None,
                    None,
                )
            }
            Command::Eat { source } => {
                let inventory =
                    game_cargo::inventory(&self.host.storage, source).ok_or(Error::Unavailable)?;
                food_admission = Some(game_cargo::prepare_food_consumption(
                    &self.host.storage,
                    actor,
                    source,
                    rules.meal_size,
                    game_cargo::ConsumptionFacts {
                        good: inventory.good,
                        accessible: self.has_stock_access(actor, inventory),
                        contact: self.stock_here(actor, inventory),
                        unreserved: Quantity(self.unreserved_stock(source, actor)),
                    },
                )?);
                let completes = rules.eating_deadline(self.host.clock().time)?;
                (Activity::Eat { source, completes }, None, None)
            }
            Command::Load {
                source,
                destination,
                quantity,
            } => {
                let job = self.prepare_cargo_load(actor, source, destination, quantity)?;
                (Self::cargo_load_activity(&job), None, None)
            }
            Command::Work { site } => {
                let site_record =
                    game_workshop::site(&self.host.storage, site).ok_or(Error::Unavailable)?;
                let tool_id = self
                    .select_work_tool(actor, site)
                    .ok_or(Error::Unavailable)?;
                let input = game_cargo::inventory(&self.host.storage, site_record.input)
                    .expect("inventory exists");
                let start = self
                    .host
                    .state
                    .workshops
                    .resolve_site(site_record)?
                    .prepare_start(
                        game_workshop::job(&self.host.storage, actor),
                        input,
                        game_cargo::inventory(&self.host.storage, tool_id)
                            .expect("inventory exists"),
                        game_cargo::inventory(&self.host.storage, site_record.output)
                            .expect("inventory exists"),
                        game_workshop::StartFacts {
                            actor,
                            area,
                            capable: game_physiology::physically_capable(a),
                            adult: game_lifecycle::adult_for_work(
                                game_lifecycle::life(&self.host.storage, actor)
                                    .map(|life| life.stage),
                                self.host.state.development.is_some(),
                            ),
                            fatigue_allows: game_physiology::exertion_available(
                                a,
                                rules.fatigue_limit,
                            ),
                            site_access: site_record.access.contains(&actor),
                            occupied_by_other: game_workshop::site_occupied_by_other(
                                actor,
                                site,
                                self.host.storage.activities().map(|(id, activity)| {
                                    let working = match *activity {
                                        Activity::Work { site } => Some(site),
                                        _ => None,
                                    };
                                    (id, working)
                                }),
                            ),
                            input_usable: self.can_use(actor, input),
                            input_unreserved: Quantity(
                                self.unreserved_stock(site_record.input, actor),
                            ),
                            tool_usable: self.can_use(
                                actor,
                                game_cargo::inventory(&self.host.storage, tool_id)
                                    .expect("inventory exists"),
                            ),
                            rate: self.effective_work_rate(actor, Skill::Craft),
                            body_rate: SkillRate {
                                numerator: u64::from(self.genetic_strength_milli(actor)),
                                denominator: 1_000,
                            },
                        },
                    )?;
                (
                    Activity::Work { site },
                    Some(start.clone()),
                    start.consumption.clone(),
                )
            }
            _ => return Err(Error::Unavailable),
        };
        let project_input = if let Command::Work { site } = command
            && let Some(consumption) = &consume
        {
            self.prepare_enterprise_input(site, consumption.quantity)?
        } else {
            None
        };
        self.apply_prepared_physical_action(PhysicalAdmission {
            actor,
            command,
            activity,
            work: new_job,
            consumption: consume,
            project_input,
            dependent,
            tell,
            food: food_admission,
            compartment: None,
        })
    }

    /// Mutation bridge for an already admitted action. Cargo allocation invokes
    /// this only after its owner checks every proposed-slot load requirement.
    fn apply_prepared_physical_action(
        &mut self,
        admission: PhysicalAdmission,
    ) -> Result<(), Error> {
        let PhysicalAdmission {
            actor,
            command,
            activity,
            work: new_job,
            consumption: consume,
            project_input,
            dependent,
            tell,
            food: food_admission,
            compartment,
        } = admission;
        if let Activity::Unbind { target, completes } = activity {
            return self.apply_physical_unbind_start(actor, target, completes);
        }
        self.host.preflight_records(0, 0, 1)?;
        let opening = self.host.transaction_opening();
        let mut admission_reads = self.physical_contact_reads(&[actor]);
        admission_reads
            .read::<game_physiology::Body>(&self.host.storage, game_ecs::StableId(actor.0));
        let interruptions =
            self.prepare_interruptions(&[actor], crate::commands::ActivityOwner::Physical)?;
        let project_events = u64::try_from(
            project_input
                .as_ref()
                .map_or(0, |change| change.effects().len()),
        )
        .map_err(|_| Error::Overflow)?;
        let event_count = interruptions
            .event_count()?
            .checked_add(u64::from(consume.is_some()))
            .ok_or(Error::Overflow)?
            .checked_add(project_events)
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(usize::try_from(event_count).map_err(|_| Error::Overflow)?)?;
        let project_receipts = project_input
            .as_ref()
            .into_iter()
            .flat_map(|change| change.effects())
            .filter_map(|effect| {
                if let game_projects::Effect::InputUsed {
                    enterprise,
                    site,
                    quantity,
                } = effect
                {
                    Some(EventKind::EnterpriseInputUsed {
                        enterprise: *enterprise,
                        site: *site,
                        quantity: *quantity,
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let water = consume
            .as_ref()
            .map(|consumption| {
                let stock = game_cargo::inventory(&self.host.storage, consumption.source)
                    .ok_or(Error::Unavailable)?;
                if stock.good == Good::Water {
                    self.prepare_health_water_reaction(game_health::WaterReaction::Consume {
                        source: consumption.source,
                        quantity: consumption.quantity,
                        source_before: stock.quantity,
                    })
                } else {
                    Ok(None)
                }
            })
            .transpose()?
            .flatten();
        let new_compartment = compartment
            .as_ref()
            .map(|creation| creation.receiving().clone());
        let compartment_property = if self.host.state.property.is_some() {
            new_compartment
                .as_ref()
                .map(|stock| game_property::prepare_empty_inventory(&self.host.storage, stock))
                .transpose()?
        } else {
            None
        };
        let mut batch = game_ecs::Batch::new();
        let work = new_job
            .as_ref()
            .map(|start| {
                start.stage_into(&self.host.state.workshops, &self.host.storage, &mut batch)
            })
            .transpose()?;
        let project = project_input
            .map(|change| {
                change.stage_into(
                    self.host
                        .state
                        .projects
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &self.host.state.workshops,
                    &mut batch,
                )
            })
            .transpose()?;
        let mut guards = Vec::new();
        if let Some(property) = compartment_property {
            guards.push(property.stage_into(&self.host.storage, &mut batch)?);
        }
        if let Some(creation) = compartment {
            guards.push(creation.stage_guarded_into(&self.host.storage, &mut batch)?);
        }
        if let Some(tell) = tell {
            guards.push(tell.stage_into(&self.host.storage, &mut batch)?);
        }
        if matches!(command, Command::Cancel) {
            guards.push(
                game_workshop::prepare_cancel(&self.host.storage, actor)
                    .stage_into(&self.host.storage, &mut batch)?,
            );
        }
        let mut interrupted = interruptions.stage_into(
            self,
            &mut batch,
            &BTreeMap::from([(actor, activity.clone())]),
        )?;
        let mut events = interrupted.events_for(&[actor]);
        events.extend(project_receipts);
        let property = if let Some(consumption) = &consume {
            self.host
                .state
                .property
                .as_ref()
                .map(|owner| {
                    owner.prepare_loss_receipt(
                        &self.host.storage,
                        consumption.source,
                        consumption.quantity,
                        opening
                            .next_event
                            .checked_add(u64::try_from(events.len()).map_err(|_| Error::Overflow)?)
                            .ok_or(Error::Overflow)?,
                        self.prepare_property_receipts()?,
                    )
                })
                .transpose()?
        } else {
            self.prepare_property_receipts()?
        };
        let source_count = u64::try_from(events.len())
            .map_err(|_| Error::Overflow)?
            .checked_add(u64::from(consume.is_some()))
            .ok_or(Error::Overflow)?;
        let through = opening
            .next_event
            .checked_add(source_count)
            .and_then(|next| next.checked_sub(1))
            .ok_or(Error::Overflow)?;
        let property = self
            .host
            .state
            .property
            .as_ref()
            .map(|owner| owner.prepare_prefix_cursor(&self.host.storage, property, through))
            .transpose()?
            .flatten();
        let property = property
            .map(|plan| {
                plan.stage_into(
                    self.host
                        .state
                        .property
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        let water = water
            .map(|plan| {
                plan.stage_into(
                    self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        if let Some(consumption) = consume {
            events.push(EventKind::Consumed {
                actor,
                source: consumption.source,
                quantity: consumption.quantity,
            });
        }
        let source_ids = u64::from(new_compartment.is_some());
        let maintenance = self.prepare_physical_maintenance(&mut batch, &events, source_ids)?;
        guards.push(admission_reads.bind(
            &self.host.storage,
            &batch,
            &game_ecs::ReadFootprint::new(),
        )?);
        if let Some(food) = food_admission {
            guards.push(food.guard_admission(&self.host.storage, &batch)?);
        }
        if let Some(dependent) = dependent {
            guards.push(
                dependent.stage_into(
                    self.host
                        .state
                        .development
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &batch,
                )?,
            );
        }
        if let Some(work) = &work {
            work.validate(&self.host.state.workshops, &self.host.storage, &batch)?;
        }
        if let Some(project) = &project {
            project.validate_batch(
                self.host
                    .state
                    .projects
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
                &self.host.storage,
                &self.host.state.workshops,
                &batch,
            )?;
        }
        interrupted.validate_batch(self, &batch)?;
        if let Some(property) = &property {
            property.validate_runtime(
                self.host
                    .state
                    .property
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
            )?;
            property.validate_batch(&self.host.storage, &batch)?;
        }
        if let Some(water) = &water {
            water.validate_batch(
                self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                &self.host.storage,
                &batch,
            )?;
        }
        maintenance.validate(self, &batch)?;
        guards.extend(maintenance.guards());
        events.extend_from_slice(&maintenance.events);
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.guards = guards;
        transaction.revisions = 1;
        transaction.ids = source_ids
            .checked_add(maintenance.ids)
            .ok_or(Error::Overflow)?;
        transaction.events = events;
        self.host.commit_transaction(transaction)?;
        if let Some(stock) = new_compartment {
            self.inventory_index
                .entry(stock.location)
                .or_default()
                .push(stock.id);
        }
        interrupted.finish_actor_metadata(self, actor);
        if let Some(project) = project {
            let _ = project.finish(
                self.host
                    .state
                    .projects
                    .as_mut()
                    .expect("admitted projects"),
            );
        }
        if let Some(property) = property {
            property.finish(self.host.state.property.as_mut().unwrap());
        }
        if let Some(water) = water {
            let _ = water.finish();
        }
        maintenance.finish(self);
        if let Activity::Care { child, .. } | Activity::Comfort { child, .. } = activity {
            self.care_index.insert(child, actor);
        }
        let deadline = match activity {
            Activity::Eat { completes, .. }
            | Activity::Load { completes, .. }
            | Activity::Tell { completes, .. }
            | Activity::Care { completes, .. }
            | Activity::Unbind { completes, .. }
            | Activity::Comfort { completes, .. }
            | Activity::Express { completes, .. }
            | Activity::Play { completes, .. } => Some(completes),
            _ => None,
        };
        if let Some(deadline) = deadline {
            self.host
                .state
                .due
                .entry(deadline)
                .or_default()
                .push(Due::Physical(actor));
        }
        Ok(())
    }

    fn apply_physical_unbind_start(
        &mut self,
        actor: PersonId,
        target: PersonId,
        completes: SimTime,
    ) -> Result<(), Error> {
        let opening = self.host.transaction_opening();
        let prepared = self.prepare_force_release_start(actor, target, completes)?;
        let continues = !prepared.planned_activities().contains_key(&actor);
        let mut batch = game_ecs::Batch::new();
        let finish = prepared.stage_into_with_activities(
            self,
            &mut batch,
            &BTreeMap::from([(actor, Activity::Unbind { target, completes })]),
        )?;
        finish.validate_batch(self, &batch)?;
        let events = finish.events();
        if let Some(encounter) = finish.encounter_id() {
            let context = self.prepare_physical_maintenance_context(&batch, &events)?;
            let followups = self.prepare_combat_opening_followups_with_force_context(
                encounter,
                &batch,
                finish.allocation_count(),
                &events,
                1,
                finish.runtime(),
                &context,
                |_| {},
                |preview| {
                    preview.update_property_events()?;
                    let mut suffix = game_ecs::Batch::new();
                    let maintenance = preview.prepare_physical_maintenance_with_context(
                        &context,
                        &mut suffix,
                        &[],
                        0,
                    )?;
                    maintenance.validate(preview, &suffix)?;
                    let mut transaction =
                        game_world::Transaction::new(preview.host.transaction_opening(), suffix);
                    transaction.ids = maintenance.ids;
                    transaction.events = maintenance.events.clone();
                    transaction.guards = maintenance.guards();
                    preview.host.commit_transaction(transaction)?;
                    maintenance.finish(preview);
                    Ok(())
                },
            )?;
            let (transaction, followups) = followups.into_transaction(self)?;
            context.validate_source(self)?;
            self.host.commit_transaction(transaction)?;
            finish.finish_metadata(self);
            followups.finish(self);
        } else {
            let prefix = self.prepare_property_receipts()?;
            let through = opening
                .next_event
                .checked_add(u64::try_from(events.len()).map_err(|_| Error::Overflow)?)
                .and_then(|next| next.checked_sub(1))
                .ok_or(Error::Overflow)?;
            let property = self
                .host
                .state
                .property
                .as_ref()
                .map(|owner| owner.prepare_prefix_cursor(&self.host.storage, prefix, through))
                .transpose()?
                .flatten();
            let property = property
                .map(|plan| {
                    plan.stage_into(
                        self.host
                            .state
                            .property
                            .as_ref()
                            .ok_or(Error::Unavailable)?,
                        &self.host.storage,
                        &mut batch,
                    )
                })
                .transpose()?;
            let maintenance =
                self.prepare_physical_maintenance(&mut batch, &events, finish.allocation_count())?;
            finish.validate_batch(self, &batch)?;
            maintenance.validate(self, &batch)?;
            if let Some(property) = &property {
                property.validate_runtime(
                    self.host
                        .state
                        .property
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                )?;
                property.validate_batch(&self.host.storage, &batch)?;
            }
            let mut transaction = game_world::Transaction::new(opening, batch);
            transaction.ids = finish
                .allocation_count()
                .checked_add(maintenance.ids)
                .ok_or(Error::Overflow)?;
            transaction.revisions = 1;
            transaction.events = events;
            transaction.events.extend_from_slice(&maintenance.events);
            transaction.guards.extend(maintenance.guards());
            self.host.commit_transaction(transaction)?;
            finish.finish_metadata(self);
            if let Some(property) = property {
                property.finish(self.host.state.property.as_mut().unwrap());
            }
            maintenance.finish(self);
            if continues {
                self.host
                    .state
                    .due
                    .entry(completes)
                    .or_default()
                    .push(Due::Physical(actor));
            }
            return Ok(());
        }
        if continues {
            self.host
                .state
                .due
                .entry(completes)
                .or_default()
                .push(Due::Physical(actor));
        }
        Ok(())
    }

    fn carried_quantity(&self, actor: PersonId) -> Result<Quantity, Error> {
        game_cargo::carried_quantity_rows(
            self.inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .map(|id| {
                    game_cargo::inventory(&self.host.storage, *id).expect("indexed stock exists")
                }),
        )
    }

    pub(crate) fn cargo_actor_facts(
        &self,
        actor: PersonId,
    ) -> Result<game_cargo::ActorFacts, Error> {
        let body = self
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, actor))
            .ok_or(Error::Unavailable)?;
        Ok(game_cargo::ActorFacts {
            actor,
            alive: body.vitals.alive,
            health: body.vitals.health,
            independent: self.independent(actor),
            confined: self.combat_restricts(actor),
            in_combat: self.in_combat(actor),
            receiving_care: self.care_index.contains_key(&actor),
            stationary: matches!(
                self.resolved_location(actor),
                Location::AtArea(_) | Location::StoppedOnRoad { .. }
            ),
            fatigue: body.vitals.fatigue,
            fatigue_limit: self.body_rules(actor).fatigue_limit,
        })
    }
    pub(crate) fn cargo_source_facts(
        &self,
        actor: PersonId,
        stock: &Inventory,
        quantity: Quantity,
    ) -> game_cargo::SourceFacts {
        game_cargo::SourceFacts {
            contact: self.stock_here(actor, stock),
            accessible: self.has_stock_access(actor, stock),
            unreserved: Quantity(self.unreserved_stock(stock.id, actor)),
            record_permission: self.record_transfer_allowed(stock.id, quantity),
        }
    }
    pub(crate) fn cargo_receiver_facts(
        &self,
        actor: PersonId,
        source: InventoryId,
        stock: &Inventory,
        quantity: Quantity,
    ) -> game_cargo::ReceiverFacts {
        let (body_room, transport_allows) = match stock.location {
            InventoryLocation::Carried(holder) => (
                self.carried_quantity(holder).map_or(Quantity(0), |held| {
                    game_physiology::carrying_room(self.body_rules(holder).carrying_capacity, held)
                }),
                self.transport_transfer_capacity(holder, quantity.0),
            ),
            InventoryLocation::Area(_) => (Quantity(u64::MAX), true),
        };
        game_cargo::ReceiverFacts {
            contact: self.stock_here(actor, stock),
            accessible: self.has_stock_access(actor, stock),
            accepted: game_cargo::receiver_authorization(
                self.agreement_accepts_delivery(actor, stock.id, quantity),
                self.guardian_accepts_cargo(actor, source, stock, quantity),
            ),
            body_room,
            transport_allows,
        }
    }
    pub(crate) fn prepare_cargo_load(
        &self,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    ) -> Result<game_cargo::LoadJob, Error> {
        let from = game_cargo::inventory(&self.host.storage, source).ok_or(Error::Unavailable)?;
        let to =
            game_cargo::inventory(&self.host.storage, destination).ok_or(Error::Unavailable)?;
        game_cargo::prepare_load(
            self.cargo_actor_facts(actor)?,
            from,
            to,
            quantity,
            self.cargo_source_facts(actor, from, quantity),
            self.cargo_receiver_facts(actor, source, to, quantity),
            self.host.clock().time,
            self.body_rules(actor).loading_time,
        )
    }
    fn cargo_load_activity(job: &game_cargo::LoadJob) -> Activity {
        Activity::Load {
            source: job.source,
            destination: job.destination,
            quantity: job.quantity,
            completes: job.completes,
        }
    }
    pub(crate) fn apply_cargo_load_with_compartment(
        &mut self,
        job: game_cargo::LoadJob,
        compartment: Option<game_cargo::PreparedInventoryCreation>,
    ) -> Result<(), Error> {
        let command = Command::Load {
            source: job.source,
            destination: job.destination,
            quantity: job.quantity,
        };
        self.apply_prepared_physical_action(PhysicalAdmission {
            actor: job.actor,
            command,
            activity: Self::cargo_load_activity(&job),
            work: None,
            consumption: None,
            project_input: None,
            dependent: None,
            tell: None,
            food: None,
            compartment,
        })
    }

    pub(crate) fn can_use(&self, actor: PersonId, inventory: &Inventory) -> bool {
        game_cargo::stock_usable(self.has_stock_access(actor, inventory), || {
            self.stock_here(actor, inventory)
        })
    }

    pub(crate) fn stock_here(&self, actor: PersonId, inventory: &Inventory) -> bool {
        game_cargo::stock_contact(
            &inventory.location,
            || self.resolved_location(actor),
            |holder| self.transport_same_place(actor, holder),
        )
    }

    pub(crate) fn reservation_totals(
        &self,
        source: Option<InventoryId>,
    ) -> Result<game_cargo::ReservationTotals, Error> {
        let physical = self
            .host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let activities = self.host.storage.activities().map(|(actor, activity)| {
            let stamp = match *activity {
                Activity::Eat { source, .. } => {
                    game_physiology::ReservationActivity::Eat { source }
                }
                Activity::Care { source, .. } => game_physiology::ReservationActivity::Care {
                    source,
                    good: game_cargo::inventory(&self.host.storage, source)
                        .expect("inventory exists")
                        .good,
                },
                Activity::Load {
                    source, quantity, ..
                } => game_physiology::ReservationActivity::Load { source, quantity },
                _ => game_physiology::ReservationActivity::Other,
            };
            (actor, stamp)
        });
        let claims = game_physiology::stock_reservations(
            activities,
            game_physiology::IntakeDoses {
                food: Some(physical.rules.meal_size),
                water: self
                    .host
                    .state
                    .health
                    .as_ref()
                    .map(|world| world.rules.drink_quantity),
            },
            source,
        )
        .into_iter()
        .map(|claim| game_cargo::Reservation {
            actor: claim.actor,
            source: claim.source,
            quantity: claim.quantity,
        });
        let health = self
            .host
            .state
            .health
            .as_ref()
            .into_iter()
            .flat_map(|owner| owner.stock_reservations(&self.host.storage, source))
            .map(|(actor, source, quantity)| game_cargo::Reservation {
                actor,
                source,
                quantity,
            });
        game_cargo::ReservationTotals::prepare(claims.chain(health))
    }

    pub(crate) fn unreserved_stock(&self, source: InventoryId, excluding: PersonId) -> u64 {
        self.unreserved_stock_for_planning(source, excluding, None)
    }

    pub(crate) fn unreserved_stock_for_planning(
        &self,
        source: InventoryId,
        excluding: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> u64 {
        let quantity = game_cargo::inventory(&self.host.storage, source)
            .expect("inventory exists")
            .quantity;
        if let Some(resources) = resources {
            return resources
                .reservations
                .available(source, quantity, excluding)
                .0;
        }
        self.reservation_totals(Some(source))
            .map_or(0, |totals| totals.available(source, quantity, excluding).0)
    }

    pub(crate) fn transfer_capacity(&self, destination: InventoryId, amount: u64) -> bool {
        if self.host.state.physical.is_none() {
            return true;
        }
        let inventory =
            game_cargo::inventory(&self.host.storage, destination).expect("inventory exists");
        if let InventoryLocation::Carried(actor) = inventory.location {
            self.carried_quantity(actor).is_ok_and(|held| {
                game_physiology::can_add_load(
                    self.body_rules(actor).carrying_capacity,
                    held,
                    Quantity(amount),
                )
            }) && self.transport_transfer_capacity(actor, amount)
        } else {
            true
        }
    }

    pub(crate) fn physical_boundary(&self, limit: SimTime) -> SimTime {
        if self.host.state.physical.is_none() {
            return limit;
        }
        game_physiology::next_work_boundary(
            self.host.clock().time,
            limit,
            game_physiology::bodies(&self.host.storage).map(|(actor, body)| {
                game_physiology::WorkDeadlineFacts {
                    alive: body.vitals.alive,
                    active: matches!(
                        self.host.storage.activity(actor),
                        Some(Activity::Work { .. })
                    ),
                    completion_after: game_workshop::job(&self.host.storage, actor).map(|job| {
                        job.completion_after(self.invocation_work_rate(
                            actor,
                            job.rate,
                            job.body_rate,
                        ))
                        .expect("validated frozen skill rate")
                    }),
                }
            }),
        )
    }

    fn body_context<'a>(
        &self,
        actor: PersonId,
        body: &'a game_physiology::Body,
        activity: &Activity,
    ) -> Result<
        (
            &'a game_physiology::Body,
            game_physiology::BodyLimits,
            game_physiology::ActivityFlags,
            bool,
        ),
        Error,
    > {
        self.host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let rules = self.body_rules(actor);
        let person = self.person_at(self.person_index[&actor]);
        let activity = game_physiology::ActivityFacts {
            activity: physiology_activity(activity),
            transport_effort: self.transport_exerting(actor),
            health_work_effort: self.health_work_exerting(actor),
            social_practice: self.social_activity_is_practical(actor),
            patient_resting: self.health_patient_resting(actor),
        }
        .flags();
        Ok((body, rules.limits(), activity, person.hunger_exempt))
    }

    /// Assemble facts from domain owners; arithmetic and clamping live in the
    /// independently compiled physiology kernel.
    pub(crate) fn integrated_vitals(
        &self,
        actor: PersonId,
        elapsed: u64,
        exposure: game_health::ExposureInput,
    ) -> Result<(Vitals, u16, u16), Error> {
        let body = game_physiology::body(&self.host.storage, actor).ok_or(Error::Unavailable)?;
        let activity = self
            .host
            .storage
            .activity(actor)
            .ok_or(Error::Unavailable)?;
        self.integrated_body(actor, body, activity, elapsed, exposure)
    }

    fn integrated_body(
        &self,
        actor: PersonId,
        body: &game_physiology::Body,
        current_activity: &Activity,
        elapsed: u64,
        exposure: game_health::ExposureInput,
    ) -> Result<(Vitals, u16, u16), Error> {
        let (snapshot, limits, activity, exempt) =
            self.body_context(actor, body, current_activity)?;
        let health = self.health_body_delta(actor, elapsed, exposure)?;
        let (strain, injury) = self.invocation_rates(actor);
        let contributions = [
            game_physiology::Contribution {
                health_milli: health.health_milli,
                fatigue_milli: health.fatigue_milli,
            },
            game_physiology::Contribution::from_strain_and_injury_rates(elapsed, strain, injury)?,
        ];
        let result = game_physiology::integrate(
            snapshot,
            limits,
            game_physiology::BodyInput {
                elapsed_ms: elapsed,
                hunger_exempt: exempt,
                immortal: exempt,
                activity,
                contributions: &contributions,
            },
        )?;
        Ok((
            result.vitals,
            result.fatigue_remainder,
            result.health_remainder,
        ))
    }

    pub(crate) fn body_boundary(&self, limit: SimTime) -> Result<SimTime, Error> {
        if self.host.state.physical.is_none() {
            return Ok(limit);
        }
        let exposures = self.health_exposures()?;
        let facts = game_physiology::bodies(&self.host.storage)
            .map(|(actor, body)| game_physiology::BoundaryFacts {
                actor,
                body: body.clone(),
                rules: self.body_rules(actor),
                hunger_exempt: self.person_at(self.person_index[&actor]).hunger_exempt,
                incapacitation: self
                    .host
                    .state
                    .combat
                    .as_ref()
                    .map(|world| world.rules.incapacitation),
            })
            .collect::<Vec<_>>();
        game_physiology::next_boundary(self.host.clock().time, limit, &facts, |actor, dt| {
            self.integrated_vitals(
                actor,
                dt,
                exposures.get(&actor).copied().unwrap_or_default(),
            )
            .map(|(vitals, _, _)| vitals)
        })
    }

    pub(crate) fn evaluate_physical(
        &self,
        elapsed: u64,
        exposures: &BTreeMap<PersonId, game_health::ExposureInput>,
    ) -> Result<Vec<Outcome>, Error> {
        if self.host.state.physical.is_none() {
            return Ok(Vec::new());
        }
        let equipment = self.equipment_availability();
        let mut input: Vec<_> = self
            .physical_query
            .iter(&self.host.storage)?
            .map(|(id, body, activity, job)| {
                (PersonId(id.0), (PersonId(id.0), body, activity, job))
            })
            .collect();
        input.sort_unstable_by_key(|(id, _)| *id);
        let roster = game_physiology::body_roster_reads(&self.host.storage);
        let roster_actor = input.first().map(|(id, _)| *id);
        let outputs = self
            .engine
            .executor()
            .map_ordered(&input, |(id, body, activity, job)| {
                let id = *id;
                let (vitals, fatigue_remainder, health_remainder) = self.integrated_body(
                    id,
                    body,
                    activity,
                    elapsed,
                    exposures.get(&id).copied().unwrap_or_default(),
                )?;
                let progress = self.host.state.workshops.prepare_progress(
                    &self.host.storage,
                    id,
                    game_workshop::ProgressFacts {
                        elapsed,
                        effective_rate: job
                            .map(|job| self.invocation_work_rate(id, job.rate, job.body_rate))
                            .unwrap_or(SkillRate {
                                numerator: 1_000,
                                denominator: 1_000,
                            }),
                        active: matches!(**activity, Activity::Work { .. }),
                        equipped: equipment.get(&id).copied().unwrap_or(false),
                    },
                )?;
                let mut reads = if Some(id) == roster_actor {
                    roster.clone()
                } else {
                    game_ecs::ReadSet::new()
                };
                reads.read::<Activity>(&self.host.storage, game_ecs::StableId(id.0));
                reads.read::<Location>(&self.host.storage, game_ecs::StableId(id.0));
                let body = game_physiology::prepare_interval_output(
                    &self.host.storage,
                    id,
                    game_physiology::Body {
                        vitals,
                        fatigue_remainder,
                        health_remainder,
                    },
                    self.genetic_phenotype(id),
                    reads,
                )?;
                Ok(Outcome { id, body, progress })
            })?;
        Ok(outputs.into_iter().map(|(_, outcome)| outcome).collect())
    }

    pub(crate) fn validate_physical_output(&self, outcomes: &[Outcome]) -> Result<(), Error> {
        game_physiology::validate_interval_outputs(
            &self.host.storage,
            self.people()
                .filter(|_| self.host.state.physical.is_some())
                .map(|person| person.id),
            outcomes.iter().map(|outcome| &outcome.body),
            |actor| self.genetic_phenotype(actor),
        )
    }

    pub(crate) fn complete_physical(&mut self, actor: PersonId) -> Result<(), Error> {
        let activity = self
            .host
            .storage
            .activity(actor)
            .expect("body exists")
            .clone();
        if matches!(activity, Activity::RealmTalk { .. }) {
            self.complete_realm_talk(actor);
            return Ok(());
        }
        if matches!(activity, Activity::Lift { .. }) {
            self.complete_transport_lift(actor);
            return Ok(());
        }
        if matches!(
            activity,
            Activity::Social { .. } | Activity::Instruction { .. } | Activity::Practice { .. }
        ) {
            self.complete_social(actor)?;
            return Ok(());
        }
        if matches!(activity, Activity::EnforcementTalk { .. }) {
            self.complete_enforcement_talk(actor);
            return Ok(());
        }
        if matches!(activity, Activity::Gather { .. }) {
            return self.complete_gathering(actor);
        }
        if matches!(
            activity,
            Activity::Escape { .. } | Activity::Restrain { .. }
        ) {
            return self.complete_force(actor);
        }
        if matches!(activity, Activity::ReportAgreement { .. }) {
            self.complete_agreement_report(actor);
            return Ok(());
        }
        if matches!(activity, Activity::Construct { .. }) {
            self.complete_construction(actor);
            return Ok(());
        }
        if matches!(activity, Activity::Lesson { .. }) {
            return self.complete_lesson(actor);
        }
        if matches!(
            activity,
            Activity::Take { .. } | Activity::ReportIncident { .. }
        ) {
            self.complete_property_action(actor)?;
            return Ok(());
        }
        if matches!(activity, Activity::Cultivate { .. }) {
            self.complete_cultivation(actor);
            return Ok(());
        }
        if let Activity::Conceive { pregnancy, .. } = activity {
            self.complete_conception(pregnancy)?;
            return Ok(());
        }
        if activity == Activity::Idle {
            return Ok(());
        }
        if self.complete_personal_physical(actor, &activity)?.is_some() {
            return Ok(());
        }
        match activity {
            Activity::Load {
                source,
                destination,
                quantity,
                completes,
            } => {
                return self.complete_physical_load(game_cargo::LoadJob {
                    actor,
                    source,
                    destination,
                    quantity,
                    completes,
                });
            }
            Activity::Unbind { target, .. } => {
                let opening = self.host.transaction_opening();
                let mut batch = game_ecs::Batch::new();
                let completion = game_conflict::release_completion(
                    actor,
                    target,
                    self.can_release(actor, target),
                );
                match completion {
                    game_conflict::ReleaseCompletion::Release { actor, target } => {
                        let release = self
                            .prepare_captive_release(actor, target)?
                            .stage_into_with_activities(
                                self,
                                &mut batch,
                                &BTreeMap::from([(completion.idle_actor(), Activity::Idle)]),
                            )?;
                        release.validate_batch(self, &batch)?;
                        let mut transaction = game_world::Transaction::new(opening, batch);
                        transaction.events = release.events();
                        self.host.commit_transaction(transaction)?;
                        release.finish_metadata(self);
                    }
                    game_conflict::ReleaseCompletion::Interrupted { actor } => {
                        let ending = game_conflict::prepare_failed_release::<
                            Activity,
                            game_characters::people::Person,
                        >(
                            &self.host.storage,
                            actor,
                            target,
                            self.physical_contact_reads(&[actor, target]),
                        );
                        let event = match ending.effect() {
                            game_physiology::ActivityEndingEffect::Interrupted { actor } => {
                                EventKind::Interrupted { actor }
                            }
                        };
                        let guard =
                            ending.stage_into(&self.host.storage, &mut batch, Activity::Idle)?;
                        let mut transaction = game_world::Transaction::new(opening, batch);
                        transaction.guards.push(guard);
                        transaction.events.push(event);
                        self.host.commit_transaction(transaction)?;
                    }
                }
                return Ok(());
            }
            _ => {}
        }
        let ending = game_physiology::prepare_activity_ending::<Activity>(
            &self.host.storage,
            actor,
            game_ecs::ReadSet::new(),
        );
        let event = match ending.effect() {
            game_physiology::ActivityEndingEffect::Interrupted { actor } => {
                EventKind::Interrupted { actor }
            }
        };
        let mut batch = game_ecs::Batch::new();
        let guard = ending.stage_into(&self.host.storage, &mut batch, Activity::Idle)?;
        let mut transaction = game_world::Transaction::new(self.host.transaction_opening(), batch);
        transaction.guards.push(guard);
        transaction.events.push(event);
        self.host.commit_transaction(transaction)?;
        Ok(())
    }

    pub(crate) fn commit_physical(
        &mut self,
        outcomes: Vec<Outcome>,
        equipment: &BTreeMap<PersonId, bool>,
        learning: &mut Option<game_skills::EvaluatedLearning>,
    ) -> Result<(), Error> {
        for out in outcomes {
            let actor = out.id;
            let previous = &game_physiology::body(&self.host.storage, actor).expect("body exists");
            let previously_alive = previous.vitals.alive;
            let rules = self.body_rules(actor);
            let ate = game_physiology::consumed_food_at(
                actor,
                self.host.clock().time,
                self.host.events().iter().rev().map(|entry| {
                    let receipt = match entry.event {
                        EventKind::Ate { actor, .. } => {
                            game_physiology::ConsumptionReceipt::Ate { actor }
                        }
                        EventKind::Cared { child, source, .. } => {
                            game_physiology::ConsumptionReceipt::Cared { child, source }
                        }
                        _ => game_physiology::ConsumptionReceipt::Other,
                    };
                    (entry.at, receipt)
                }),
                |source| {
                    game_cargo::inventory(&self.host.storage, source)
                        .expect("inventory exists")
                        .good
                },
            );
            let activity = self.host.storage.activity(actor).expect("activity exists");

            let reception = game_physiology::prepare_interval_reception(
                &self.host.storage,
                &out.body,
                &rules,
                game_physiology::IntervalReactionFacts {
                    consumed_food: ate,
                    activity: physiology_activity(activity),
                    practical_social_activity: self.social_activity_is_practical(actor),
                    health_work_activity: self.health_work_exerting(actor),
                    social_tool_selected: self.social_selected_tool(actor).is_some(),
                    health_tool_selected: self.health_selected_tool(actor).is_some(),
                    equipped: equipment.get(&actor).copied().unwrap_or(false),
                },
            )?;
            let reaction = reception.reaction();

            let work = game_workshop::prepare_work_interval(
                out.progress.as_ref(),
                &self.host.state.workshops,
                &self.host.storage,
                game_workshop::CompletionFacts {
                    actor,
                    active: matches!(activity, Activity::Work { .. }),
                    alive_at_start: previously_alive,
                    equipped: equipment.get(&actor).copied().unwrap_or(false),
                },
                reaction.died,
            )?;
            let opening = self.host.transaction_opening();

            let early_learning = if reaction.died {
                learning
                    .as_ref()
                    .map(|evaluation| {
                        self.host
                            .state
                            .skills
                            .as_ref()
                            .ok_or(Error::Unavailable)?
                            .prepare_actor_learning(&self.host.storage, evaluation, actor)
                    })
                    .transpose()?
            } else {
                None
            };

            let mut death = reaction
                .died
                .then(|| self.prepare_death_cleanup(actor))
                .transpose()?;

            let interruptions = if reaction
                .needs_interruption_after_work(work.failed(), work.production().is_some())
            {
                Some(
                    self.prepare_interruptions(&[actor], crate::commands::ActivityOwner::Physical)?,
                )
            } else {
                None
            };
            let mut batch = game_ecs::Batch::new();

            let body_guard = reception.stage_into(&self.host.storage, &mut batch)?;
            let mut events = Vec::new();
            if let Some(receipt) = work.production() {
                events.push(EventKind::Produced {
                    actor: receipt.actor,
                    site: receipt.site,
                    output: receipt.output,
                    quantity: receipt.quantity,
                });
            }

            let game_workshop::WorkIntervalPublication {
                completion: completion_guard,
                progress: work_guard,
                activity: work_activity,
            } = work.stage_into(
                &self.host.state.workshops,
                &self.host.storage,
                &mut batch,
                Activity::Idle,
            )?;
            let mut guards = vec![body_guard];
            guards.extend(work_activity);
            let water = if let Some(EventKind::Produced {
                actor,
                output,
                quantity,
                ..
            }) = events.first()
            {
                let stock =
                    game_cargo::inventory(&self.host.storage, *output).ok_or(Error::Unavailable)?;
                if stock.good == Good::Water {
                    self.prepare_health_water_reaction(game_health::WaterReaction::Produce {
                        actor: *actor,
                        output: *output,
                        quantity: *quantity,
                        quantity_before: stock.quantity,
                    })?
                } else {
                    None
                }
            } else {
                None
            };

            let water = water
                .map(|plan| {
                    plan.stage_into(
                        self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                        &self.host.storage,
                        &mut batch,
                    )
                })
                .transpose()?;

            let social = self
                .prepare_social_receipts(&events)?
                .map(|plan| {
                    plan.stage_into(
                        self.host.state.social.as_ref().ok_or(Error::Unavailable)?,
                        &self.host.storage,
                        self.time(),
                        &mut batch,
                    )
                })
                .transpose()?;

            let traits = self.stage_trait_events(&events, &mut batch)?;

            let early_learning = early_learning
                .map(|plan| {
                    plan.stage_into(
                        self.host.state.skills.as_ref().ok_or(Error::Unavailable)?,
                        &self.host.storage,
                        &mut batch,
                    )
                })
                .transpose()?;

            let witness = events
                .first()
                .map(|event| self.prepare_lineage_witness(event, &batch))
                .transpose()?
                .flatten();
            let lineage = if let Some(witness) = witness {
                if let Some(cleanup) = death.take() {
                    death = Some(cleanup.receive_lineage_witness(self, witness)?);
                    None
                } else {
                    Some(witness.stage_into(self, &mut batch)?)
                }
            } else {
                None
            };

            let death = death
                .map(|plan| {
                    plan.stage_into_with_completion(
                        self,
                        &mut batch,
                        completion_guard.as_ref().map(|guard| guard.completed_job()),
                    )
                })
                .transpose()?;

            let invocation = if reaction.died {
                self.prepare_death_invocation(actor, None, &mut batch)?
            } else {
                None
            };

            let mut interrupted = interruptions
                .map(|plan| {
                    plan.stage_into(self, &mut batch, &BTreeMap::from([(actor, Activity::Idle)]))
                })
                .transpose()?;

            if let Some(interrupted) = &interrupted {
                interrupted.validate_batch(self, &batch)?;
                events.extend(interrupted.events_for(&[actor]));
                events.push(EventKind::Interrupted { actor });
            }

            if let Some(death) = &death {
                death.validate(self, &batch)?;
                guards.extend(death.guards());
                events.extend(death.events());
            } else if reaction.recovered {
                events.push(EventKind::Recovered { actor });
            }

            if let Some(completion) = &completion_guard {
                completion.validate(&self.host.state.workshops, &self.host.storage, &batch)?;
            }

            if let Some(work) = &work_guard {
                work.validate(&self.host.state.workshops, &self.host.storage, &batch)?;
            }

            if let Some(social) = &social {
                social.validate(
                    self.host.state.social.as_ref().ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    self.time(),
                    &batch,
                )?;
                guards.push(social.row_guard());
            }

            if let Some(traits) = &traits {
                self.validate_trait_events(traits, &batch)?;
                guards.extend(traits.row_guards().iter().cloned());
            }

            if let Some(lineage) = &lineage {
                lineage.validate(self, &batch)?;
                guards.extend(lineage.row_guards());
            }

            if let Some(water) = &water {
                water.validate_batch(
                    self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &batch,
                )?;
            }

            if let Some(invocation) = &invocation {
                invocation.validate(self, &batch)?;
                guards.extend(invocation.guards());
            }

            if let Some(early_learning) = &early_learning {
                early_learning.validate(
                    self.host.state.skills.as_ref().ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &batch,
                )?;
                guards.push(early_learning.row_guard().clone());
            }

            let acknowledgement = if let Some(early_learning) = &early_learning {
                Some(
                    learning
                        .as_mut()
                        .ok_or(Error::Unavailable)?
                        .prepare_acknowledgement(
                            self.host.state.skills.as_ref().ok_or(Error::Unavailable)?,
                            &self.host.storage,
                            &batch,
                            early_learning,
                        )?,
                )
            } else {
                None
            };
            let mut transaction = game_world::Transaction::new(opening, batch);
            transaction.guards = guards;
            transaction.events = events;

            self.host.commit_transaction(transaction)?;
            if let Some(acknowledgement) = acknowledgement {
                acknowledgement.finish();
            }
            if let Some(death) = death {
                death.finish(self);
            }
            if let Some(interrupted) = &mut interrupted {
                interrupted.finish_actor_metadata(self, actor);
            }
            if let Some(lineage) = lineage {
                lineage.finish();
            }
            if let Some(water) = water {
                let _ = water.finish();
            }
            if let Some(invocation) = invocation {
                invocation.finish(self);
            }
        }
        Ok(())
    }

    pub(crate) fn possible_work_outputs(
        &self,
        outcomes: &[Outcome],
    ) -> Vec<(SiteId, Good, Quantity)> {
        let Some(_) = &self.host.state.physical else {
            return Vec::new();
        };
        outcomes
            .iter()
            .filter_map(|out| {
                if !matches!(
                    *self.host.storage.activity(out.id).expect("activity exists"),
                    Activity::Work { .. }
                ) {
                    return None;
                }
                out.progress.as_ref()?.output_forecast()
            })
            .collect()
    }

    pub fn debug_add_inventory(
        &mut self,
        good: Good,
        location: InventoryLocation,
        quantity: Quantity,
        capacity: Quantity,
        access: BTreeSet<PersonId>,
    ) -> Result<InventoryId, Error> {
        let id = self.add_inventory(good, location, quantity, capacity, access)?;
        self.update_property_events()?;
        Ok(id)
    }

    pub(crate) fn add_inventory(
        &mut self,
        good: Good,
        location: InventoryLocation,
        quantity: Quantity,
        capacity: Quantity,
        access: BTreeSet<PersonId>,
    ) -> Result<InventoryId, Error> {
        if quantity > capacity || access.iter().any(|p| !self.person_index.contains_key(p)) {
            return Err(Error::InvalidFixture("invalid inventory".into()));
        }
        match location {
            InventoryLocation::Area(a)
                if !self.host.state.areas.iter().any(|area| area.id == a) =>
            {
                return Err(Error::InvalidFixture("unknown area".into()));
            }
            InventoryLocation::Carried(p) if !self.person_index.contains_key(&p) => {
                return Err(Error::InvalidFixture("unknown carrier".into()));
            }
            _ => {}
        }
        if let InventoryLocation::Carried(actor) = location {
            let held = self.carried_quantity(actor)?;
            if self.host.state.physical.as_ref().is_some_and(|_| {
                quantity.0 > 0
                    && (!game_physiology::can_add_load(
                        self.body_rules(actor).carrying_capacity,
                        held,
                        quantity,
                    ) || !self.transport_transfer_capacity(actor, quantity.0))
            }) {
                return Err(Error::InvalidFixture("carrying capacity exceeded".into()));
            }
        }
        let id = InventoryId(self.host.allocate_id()?.0);
        self.inventory_index
            .entry(location.clone())
            .or_default()
            .push(id);
        game_cargo::insert_inventory(
            &mut self.host.storage,
            Inventory {
                id,
                location,
                quantity,
                capacity,
                access,
                last_granted: None,
                good,
                spoils_at: None,
            },
        )?;
        Ok(id)
    }

    pub fn debug_spoil_at(&mut self, inventory: InventoryId, at: SimTime) -> Result<(), Error> {
        let stock = game_cargo::inventory(&self.host.storage, inventory)
            .ok_or_else(|| Error::InvalidFixture("unknown stock".into()))?;
        if stock.good != Good::Food || at <= self.host.clock().time {
            return Err(Error::InvalidFixture("invalid spoilage".into()));
        }
        for due in self.host.state.due.values_mut() {
            due.retain(|d| !matches!(d, Due::Spoil(id) if *id == inventory));
        }
        self.host.state.due.retain(|_, due| !due.is_empty());
        game_cargo::inventory_mut(&mut self.host.storage, inventory)
            .unwrap()
            .spoils_at = Some(at);
        self.host
            .state
            .due
            .entry(at)
            .or_default()
            .push(Due::Spoil(inventory));
        Ok(())
    }

    pub fn debug_injure(&mut self, actor: PersonId, damage: u64) -> Result<(), Error> {
        if !self.person_index.get(&actor).is_some_and(|i| {
            matches!(
                *game_spatial::location(
                    &self.host.storage,
                    game_ecs::StableId((self.person_at(*i).id).0)
                )
                .expect("location exists"),
                Location::AtArea(_)
            )
        }) {
            return Err(Error::InvalidFixture(
                "fixture injuries require a stationary actor".into(),
            ));
        }
        self.host
            .state
            .physical
            .as_mut()
            .ok_or(Error::Unavailable)?;
        let hunger_exempt = self.person(actor).hunger_exempt;
        let opening = self.host.transaction_opening();
        let injury =
            game_physiology::prepare_injury(&self.host.storage, actor, damage, hunger_exempt)?;
        let lethal = injury.lethal();
        let death = lethal
            .then(|| self.prepare_death_cleanup(actor))
            .transpose()?;
        let interruptions = if lethal {
            None
        } else {
            Some(self.prepare_interruptions(&[actor], crate::commands::ActivityOwner::Physical)?)
        };
        let mut batch = game_ecs::Batch::new();
        let injury = injury.stage_into(&self.host.storage, &mut batch)?;
        let death = death
            .map(|plan| plan.stage_into(self, &mut batch))
            .transpose()?;
        let mut interrupted = interruptions
            .map(|plan| {
                plan.stage_into(self, &mut batch, &BTreeMap::from([(actor, Activity::Idle)]))
            })
            .transpose()?;
        let invocation = if lethal {
            self.prepare_death_invocation(actor, None, &mut batch)?
        } else {
            None
        };
        injury.validate(&self.host.storage, &batch)?;
        let mut events = Vec::new();
        let mut guards = Vec::new();
        if let Some(death) = &death {
            death.validate(self, &batch)?;
            guards.extend(death.guards());
            events.extend(death.events());
        }
        if let Some(interrupted) = &interrupted {
            interrupted.validate_batch(self, &batch)?;
            events.extend(interrupted.events_for(&[actor]));
        }
        if let Some(invocation) = &invocation {
            invocation.validate(self, &batch)?;
            guards.extend(invocation.guards());
        }
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.guards = guards;
        transaction.events = events;
        self.host.commit_transaction(transaction)?;
        if let Some(death) = death {
            death.finish(self);
        }
        if let Some(interrupted) = &mut interrupted {
            interrupted.finish_actor_metadata(self, actor);
        }
        if let Some(invocation) = invocation {
            invocation.finish(self);
        }
        Ok(())
    }
}

pub(crate) fn rules_for(state: &CanonicalState, actor: PersonId) -> Rules {
    state
        .state
        .physical
        .as_ref()
        .expect("physical body rules")
        .rules
        .expressed(genetics::phenotype(state, actor))
}

#[cfg(test)]
mod mortality_tests {
    use super::*;
    #[test]
    fn prepared_lethal_cleanup_rejected_by_foreign_schema_preserves_body_lineage_and_records() {
        let mut sim = Simulation::lineage_fixture(73, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let damage = game_physiology::body(&sim.host.storage, actor)
            .unwrap()
            .vitals
            .health;
        let before = sim.canonical_snapshot().unwrap();
        let opening = sim.host.transaction_opening();
        let injury =
            game_physiology::prepare_injury(&sim.host.storage, actor, damage, false).unwrap();
        let cleanup = sim.prepare_death_cleanup(actor).unwrap();
        let mut batch = game_ecs::Batch::new();
        let injury = injury.stage_into(&sim.host.storage, &mut batch).unwrap();
        let cleanup = cleanup.stage_into(&sim, &mut batch).unwrap();
        let mut invalid = game_cargo::inventories(&sim.host.storage)
            .next()
            .unwrap()
            .1
            .clone();
        invalid.quantity = Quantity(invalid.capacity.0 + 1);
        batch.insert(game_ecs::StableId(invalid.id.0), invalid);
        injury.validate(&sim.host.storage, &batch).unwrap();
        if cleanup.validate(&sim, &batch).is_ok() {
            let mut transaction = game_world::Transaction::new(opening, batch);
            transaction.guards.extend(cleanup.guards());
            transaction.events = cleanup.events();
            assert!(sim.host.commit_transaction(transaction).is_err());
        }
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        assert!(
            game_physiology::body(&sim.host.storage, actor)
                .unwrap()
                .vitals
                .alive
        );
        assert_eq!(
            sim.host
                .state
                .lineage
                .as_ref()
                .unwrap()
                .current(&sim.host.storage),
            Some(actor)
        );
    }
    #[test]
    fn lethal_debug_injury_publishes_one_death_and_lineage_waits_after_restore() {
        let mut sim = Simulation::lineage_fixture(73, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let at = sim.time();
        let damage = sim.body_rules(actor).max_health;
        sim.debug_injure(actor, damage).unwrap();
        assert!(
            !game_physiology::body(&sim.host.storage, actor)
                .unwrap()
                .vitals
                .alive
        );
        assert_eq!(
            sim.host
                .events()
                .iter()
                .filter(|event| matches!(event.event, EventKind::Died { actor: id } if id == actor))
                .count(),
            1
        );
        assert!(sim.host.events().iter().any(|event| event.at == at
            && matches!(event.event, EventKind::Died { actor: id } if id == actor)));
        assert_eq!(sim.lineage_view().unwrap().waiting_since, Some(at));
        assert_eq!(sim.lineage_view().unwrap().current, None);
        sim.debug_injure(actor, damage).unwrap();
        assert_eq!(
            sim.host
                .events()
                .iter()
                .filter(|event| matches!(event.event, EventKind::Died { actor: id } if id == actor))
                .count(),
            1
        );
        let restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        assert_eq!(restored.lineage_view().unwrap().waiting_since, Some(at));
    }
    #[test]
    fn lethal_debug_injury_ends_a_current_pregnancy_once_before_save() {
        let mut sim = Simulation::life_fixture(73, Execution::default()).unwrap();
        sim.advance(SimDuration(2 * MINUTE.0)).unwrap();
        let pregnancy = game_lifecycle::all_pregnancies(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|row| matches!(row.state, game_lifecycle::PregnancyState::Gestating { .. }))
            .expect("fixture's actual conceived pregnancy")
            .clone();
        let actor = pregnancy.gestator;
        let damage = sim.body_rules(actor).max_health;
        let at = sim.time();
        sim.debug_injure(actor, damage).unwrap();
        assert_eq!(
            game_lifecycle::pregnancy(&sim.host.storage, pregnancy.id)
                .unwrap()
                .state,
            game_lifecycle::PregnancyState::Ended
        );
        assert!(sim.host.events().iter().any(|event| event.at == at
            && matches!(event.event, EventKind::Died { actor: id } if id == actor)));
        assert_eq!(sim.host.events().iter().filter(|event| matches!(event.event, EventKind::PregnancyEnded { pregnancy: id } if id == pregnancy.id)).count(), 1);
        let restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        assert!(
            !game_physiology::body(&restored.host.storage, actor)
                .unwrap()
                .vitals
                .alive
        );
    }
}

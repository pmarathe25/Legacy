//! Project bindings, shared-store admission, and typed effect publication.
use crate::*;
pub use game_projects::{Construction, Enterprise, Project, Rules, Runtime};

impl Simulation {
    /// Authoring explicit custodians and opportunities is fixture content; it does
    /// not create a general rule assigning unused land or stores to inhabitants.
    pub fn farm_enterprise_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::ecology_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "farm-enterprise-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let __custody: BTreeMap<InventoryId, PersonId> = BTreeMap::new();
        for (id, owner) in __custody {
            game_property::declare_custody(&mut sim.host.storage, id, owner)?;
        }
        sim.host.state.agreements = Some(game_agreements::Runtime {
            last_event: sim.host.next_event() - 1,
            last_observation_event: sim.host.next_event() - 1,
        });
        sim.host.state.projects = Some(Runtime {
            rules: Rules::default(),
            last_event: sim.host.next_event() - 1,
        });
        let farmer = sim.person_at(3).id;
        let supporter = sim.person_at(5).id;
        let area = AreaId(1);
        *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId((farmer).0))
            .expect("location exists") = Location::AtArea(area);
        *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId((supporter).0))
            .expect("location exists") = Location::AtArea(area);
        sim.debug_add_plot(area, Some(farmer), BTreeSet::from([farmer]), 900)?;
        for (good, quantity, capacity, custodian, access) in [
            (Good::Timber, 0, 50_000, farmer, BTreeSet::from([farmer])),
            (Good::Tool, 1_000, 1_000, farmer, BTreeSet::from([farmer])),
            (Good::Seed, 10_000, 50_000, farmer, BTreeSet::from([farmer])),
            (
                Good::Water,
                10_000,
                50_000,
                farmer,
                BTreeSet::from([farmer]),
            ),
            (Good::Food, 0, 10_000_000, farmer, BTreeSet::from([farmer])),
            (
                Good::Timber,
                10_000,
                10_000,
                supporter,
                BTreeSet::from([supporter]),
            ),
            (
                Good::Food,
                0,
                10_000_000,
                supporter,
                BTreeSet::from([supporter]),
            ),
        ] {
            let id = sim.debug_add_inventory(
                good,
                InventoryLocation::Area(area),
                Quantity(quantity),
                Quantity(capacity),
                access,
            )?;
            game_property::declare_custody(&mut sim.host.storage, id, custodian)
                .expect("declared custody");
        }
        sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Carried(supporter),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([supporter]),
        )?;
        sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Carried(farmer),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([farmer]),
        )?;
        for actor in sim.people().map(|p| p.id).collect::<Vec<_>>() {
            sim.debug_actor_command(actor, Command::Cancel)?;
            sim.debug_set_personality(
                actor,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )?;
        }
        let mut personality = decisions::Personality::restful();
        personality.ambition = 900;
        personality.discipline = 700;
        personality.honesty = 800;
        personality.risk_tolerance = 700;
        sim.debug_set_personality(farmer, personality, decisions::Aspiration::Independence)?;
        let mut willing = decisions::Personality::restful();
        willing.empathy = 900;
        willing.discipline = 800;
        willing.honesty = 800;
        willing.hedonism = 900;
        willing.ambition = 900;
        willing.risk_tolerance = 700;
        sim.debug_set_personality(supporter, willing, decisions::Aspiration::SupportOthers)?;
        sim.observe_ecology();
        sim.observe_projects();
        sim.review_decisions()?;
        Ok(sim)
    }
    pub fn project_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::agreement_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "project-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let hour = 3_600_000;
        let rules = &mut sim.host.state.physical.as_mut().unwrap().rules;
        rules.hunger_harm = 72 * hour;
        rules.meal_relief = 24 * hour;
        rules.fatigue_limit = 16 * hour;
        rules.max_health = 48 * hour;
        let people = sim.people().map(|person| person.id).collect::<Vec<_>>();
        for person in &people {
            sim.debug_actor_command(*person, Command::Cancel)?;
            game_physiology::body_mut(&mut sim.host.storage, *person)
                .unwrap()
                .vitals
                .health = 48 * hour;
            sim.debug_set_personality(
                *person,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )?;
        }
        // The old continuous food fixture is not the workshop's recipe.
        let sites: Vec<_> = game_workshop::sites(&sim.host.storage)
            .map(|(id, _)| id)
            .collect();
        for site in sites {
            game_workshop::remove_site(&mut sim.host.storage, site)?;
        }
        let actors: Vec<_> =
            game_characters::decisions::minds::<game_api::Command>(&sim.host.storage)
                .map(|(id, _)| id)
                .collect();
        for actor in actors {
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                actor,
            )
            .unwrap();
            mind.sites.clear();
            mind.plan = None;
        }
        let sponsor = sim.person_at(3).id;
        let financier = sim.person_at(5).id;
        let area = sim.host.state.areas[0].id;
        let materials = sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(area),
            Quantity(0),
            Quantity(50_000),
            BTreeSet::from([sponsor]),
        )?;
        let tool = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(area),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([sponsor]),
        )?;
        let output = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(area),
            Quantity(0),
            Quantity(50_000),
            BTreeSet::from([sponsor]),
        )?;
        let payment = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(area),
            Quantity(0),
            Quantity(50_000),
            BTreeSet::from([financier, sponsor]),
        )?;
        let own_output = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(area),
            Quantity(0),
            Quantity(50_000),
            BTreeSet::from([financier]),
        )?;
        let provisions = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(area),
            Quantity(5_000),
            Quantity(5_000),
            BTreeSet::from([sponsor]),
        )?;
        for actor in [sponsor, financier] {
            sim.debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(actor),
                Quantity(0),
                Quantity(10_000),
                BTreeSet::from([actor]),
            )?;
        }
        let timber = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|stock| {
                stock.good == Good::Timber
                    && stock.location == InventoryLocation::Area(area)
                    && stock.id != materials
            })
            .unwrap()
            .id;
        game_cargo::inventory_mut(&mut sim.host.storage, timber)
            .unwrap()
            .access = BTreeSet::from([financier]);

        for inventory in [materials, tool, output, provisions] {
            game_property::declare_custody(&mut sim.host.storage, inventory, sponsor)?;
        }
        for inventory in [timber, payment, own_output] {
            game_property::declare_custody(&mut sim.host.storage, inventory, financier)?;
        }
        sim.debug_set_personality(
            sponsor,
            decisions::Personality::industrious(),
            decisions::Aspiration::Independence,
        )?;
        let mut investor = decisions::Personality::industrious();
        investor.risk_tolerance = 850;
        sim.debug_set_personality(financier, investor, decisions::Aspiration::Independence)?;
        sim.host.state.projects = Some(Runtime {
            rules: Rules::default(),
            last_event: sim.host.next_event() - 1,
        });
        for actor in [sponsor, financier] {
            game_projects::insert_knowledge(
                &mut sim.host.storage,
                actor,
                game_projects::Knowledge {
                    timber_hints: BTreeSet::from([sim.host.state.areas[2].id]),
                    ..game_projects::Knowledge::default()
                },
            )?;
        }
        // Fresh authored knowledge includes these physically present stocks.
        for person in people {
            let inventories = game_cargo::inventories(&sim.host.storage)
                .map(|(_, row)| row)
                .filter(|stock| sim.inventory_area(stock) == Some(area))
                .cloned()
                .collect::<Vec<_>>();
            let observed_at = sim.time();
            let (_mind,stocks)=sim.host.storage.get_pair_mut::<game_characters::decisions::Mind<game_api::Command>,game_characters::decisions::Stocks>(game_ecs::StableId((person).0)).expect("mind and stock knowledge exist");
            for stock in inventories {
                stocks.known.insert(
                    stock.id,
                    StockKnowledge {
                        inventory: stock.id,
                        area,
                        good: stock.good,
                        quantity: stock.quantity,
                        accessible: stock.access.contains(&person),
                        observed_at,
                        learned_at: observed_at,
                        source: KnowledgeSource::Observed(person),
                    },
                );
            }
        }
        Ok(sim)
    }
    fn project_agency_facts(&self, actor: PersonId) -> Option<game_projects::AgencyFacts> {
        let body = game_physiology::body(&self.host.storage, actor)?;
        let area = match game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0))? {
            Location::AtArea(area) => Some(*area),
            _ => None,
        };
        Some(game_projects::AgencyFacts {
            alive: body.vitals.alive,
            health: body.vitals.health,
            area,
            independent: self.independent(actor),
            combat_restricted: self.combat_restricts(actor),
            caring: self.care_index.contains_key(&actor),
            stage: self.host.state.development.as_ref().and_then(|_| {
                game_lifecycle::life(&self.host.storage, actor).map(|life| life.stage)
            }),
        })
    }
    fn project_area(&self, actor: PersonId) -> Option<AreaId> {
        game_projects::project_area(self.project_agency_facts(actor)?)
    }
    fn project_able(&self, actor: PersonId) -> bool {
        self.project_agency_facts(actor)
            .is_some_and(game_projects::project_capable)
    }
    fn project_usable(
        &self,
        actor: PersonId,
        inventory: InventoryId,
        good: Good,
        quantity: u64,
    ) -> bool {
        self.project_usable_for_planning(actor, inventory, good, quantity, None)
    }

    fn project_usable_for_planning(
        &self,
        actor: PersonId,
        inventory: InventoryId,
        good: Good,
        quantity: u64,
        resources: Option<&equipment::RankingResources>,
    ) -> bool {
        self.project_resource_facts(actor, inventory, resources)
            .is_some_and(|facts| game_projects::resource_usable(facts, good, Quantity(quantity)))
    }
    fn project_resource_facts(
        &self,
        actor: PersonId,
        inventory: InventoryId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<game_projects::ResourceFacts> {
        let stock = game_cargo::inventory(&self.host.storage, inventory)?;
        Some(game_projects::ResourceFacts {
            good: stock.good,
            accessible: stock.access.contains(&actor),
            area: self.inventory_area(stock),
            actor_area: self.project_area(actor),
            unreserved: Quantity(self.unreserved_stock_for_planning(inventory, actor, resources)),
        })
    }

    pub(crate) fn project_is_known(&self, actor: PersonId, project: ProjectId) -> bool {
        self.host
            .state
            .projects
            .as_ref()
            .and_then(|_| {
                game_projects::knowledge(&self.host.storage, actor).map(|row| &row.projects)
            })
            .is_some_and(|known| known.contains_key(&project))
    }
    fn project_actor_facts(
        &self,
        actor: PersonId,
        project: Option<ProjectId>,
    ) -> game_projects::ActorFacts {
        game_projects::ActorFacts {
            actor,
            now: self.host.clock().time,
            area: self.project_area(actor),
            capable: self.project_able(actor),
            knows_project: project.is_some_and(|id| self.project_is_known(actor, id)),
        }
    }
    pub(crate) fn construction_facts(
        &self,
        actor: PersonId,
        project: ProjectId,
        resources: Option<&equipment::RankingResources>,
    ) -> Result<game_projects::ConstructionFacts, Error> {
        self.host
            .state
            .projects
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let plan = game_projects::project(&self.host.storage, project).ok_or(Error::Unavailable)?;
        let physical = self
            .host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let mut facts = self.project_actor_facts(actor, Some(project));
        facts.capable &= game_projects::construction_fatigue_allows(
            game_physiology::body(&self.host.storage, actor).map(|body| body.vitals.fatigue),
            physical.rules.fatigue_limit,
        );
        Ok(game_projects::ConstructionFacts {
            actor: facts,
            land_accessible: match plan.kind {
                ProjectKind::Workshop { .. } => true,
                ProjectKind::Farm { plot, .. } => self
                    .host
                    .state
                    .ecology
                    .as_ref()
                    .and_then(|w| w.plot(&self.host.storage, plot))
                    .is_some_and(|p| p.access.contains(&actor)),
            },
            tool_available: game_projects::construction_tool_available(
                self.project_resource_facts(actor, plan.tool, resources),
                self.tool_available_for_planning(actor, plan.tool, resources),
            ),
            materials_available: if self.project_usable_for_planning(
                actor,
                plan.materials,
                Good::Timber,
                0,
                resources,
            ) {
                Quantity(self.unreserved_stock_for_planning(plan.materials, actor, resources))
            } else {
                Quantity(0)
            },
        })
    }
    pub fn debug_project_action(
        &mut self,
        actor: PersonId,
        action: ProjectAction,
    ) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        if self.host.state.projects.is_none() {
            return Err(Error::Unavailable);
        }
        let change = match &action {
            ProjectAction::Propose { .. } | ProjectAction::ProposeFarm { .. } => {
                let stock_ids = match &action {
                    ProjectAction::Propose {
                        materials,
                        tool,
                        input,
                        output,
                        ..
                    } => vec![*materials, *tool, *input, *output],
                    ProjectAction::ProposeFarm {
                        materials,
                        tool,
                        seed,
                        water,
                        food,
                        ..
                    } => vec![*materials, *tool, *seed, *water, *food],
                    _ => unreachable!(),
                };
                let stocks = stock_ids
                    .into_iter()
                    .filter_map(|id| {
                        game_cargo::inventory(&self.host.storage, id).map(|stock| {
                            (
                                id,
                                game_projects::StockFact {
                                    good: stock.good,
                                    area: match stock.location {
                                        InventoryLocation::Area(area) => Some(area),
                                        _ => None,
                                    },
                                },
                            )
                        })
                    })
                    .collect();
                let people = self.person_index.keys().copied().collect();
                let land = match &action {
                    ProjectAction::ProposeFarm { plot, .. } => self
                        .host
                        .state
                        .ecology
                        .as_ref()
                        .and_then(|w| w.plot(&self.host.storage, *plot)),
                    _ => None,
                };
                self.host
                    .state
                    .projects
                    .as_ref()
                    .unwrap()
                    .prepare_proposal(
                        &self.host.storage,
                        &self.host.state.workshops,
                        ProjectId(self.host.next_id()),
                        &action,
                        game_projects::ProposalFacts {
                            actor: self.project_actor_facts(actor, None),
                            people: &people,
                            stocks: &stocks,
                            land_area: land.map(|p| p.area),
                            land_accessible: land.is_some_and(|p| p.access.contains(&actor)),
                        },
                    )?
            }
            ProjectAction::Support { project, agreement } => {
                let contract = self
                    .host
                    .state
                    .agreements
                    .as_ref()
                    .and_then(|_| game_agreements::agreement(&self.host.storage, *(agreement)))
                    .ok_or(Error::Unavailable)?;
                self.host.state.projects.as_ref().unwrap().prepare_support(
                    &self.host.storage,
                    *project,
                    contract,
                    self.project_actor_facts(actor, Some(*project)),
                )?
            }
            ProjectAction::Work { project } => {
                let facts = self.construction_facts(actor, *project, None)?;
                self.host
                    .state
                    .projects
                    .as_ref()
                    .unwrap()
                    .prepare_construction(&self.host.storage, *project, facts)?
            }
            ProjectAction::Cancel { project } => {
                self.host.state.projects.as_ref().unwrap().prepare_cancel(
                    &self.host.storage,
                    *project,
                    self.project_actor_facts(actor, Some(*project)),
                )?
            }
            ProjectAction::Operate {
                enterprise,
                agreement,
            } => {
                let contract = self
                    .host
                    .state
                    .agreements
                    .as_ref()
                    .and_then(|_| game_agreements::agreement(&self.host.storage, *(agreement)))
                    .ok_or(Error::Unavailable)?;
                let known = game_projects::knowledge(&self.host.storage, actor)
                    .map(|row| &row.enterprises)
                    .is_some_and(|k| k.contains_key(enterprise));
                self.host
                    .state
                    .projects
                    .as_ref()
                    .unwrap()
                    .prepare_operation(
                        &self.host.storage,
                        *enterprise,
                        contract,
                        self.project_actor_facts(actor, None),
                        known,
                    )?
            }
        };
        self.apply_project_change_observing(change, true)?;
        self.host.bump_revision(1)?;
        Ok(())
    }
    fn apply_project_change(&mut self, change: game_projects::PreparedChange) -> Result<(), Error> {
        self.apply_project_change_observing(change, false)
    }
    fn apply_project_change_observing(
        &mut self,
        change: game_projects::PreparedChange,
        observe: bool,
    ) -> Result<(), Error> {
        use game_projects::{AssetEffect, Effect};
        let opening = self.host.transaction_opening();
        let mut next_id = self.host.next_id();
        let mut batch = game_ecs::Batch::new();
        let mut final_activities = BTreeMap::new();
        let mut consumed = Vec::new();
        let mut asset_guards = Vec::new();
        let mut workshop_guards = Vec::new();
        let mut plot_guards = Vec::new();
        for effect in change.effects() {
            match effect {
                Effect::Proposed { project, .. } => {
                    if project.0 != next_id {
                        return Err(Error::Unavailable);
                    }
                    next_id = next_id.checked_add(1).ok_or(Error::Overflow)?;
                }
                Effect::Consume {
                    source, quantity, ..
                } => consumed.push((*source, *quantity)),
                Effect::ConstructionStarted {
                    project,
                    actor,
                    completes,
                } => {
                    self.host
                        .storage
                        .activity(*actor)
                        .ok_or(Error::Unavailable)?;
                    final_activities.insert(
                        *actor,
                        Activity::Construct {
                            project: *project,
                            completes: *completes,
                        },
                    );
                }
                Effect::Completed {
                    enterprise,
                    site,
                    actor,
                    asset,
                    ..
                } => {
                    self.host
                        .storage
                        .activity(*actor)
                        .ok_or(Error::Unavailable)?;
                    final_activities.insert(*actor, Activity::Idle);
                    match asset {
                        AssetEffect::Workshop(asset) => {
                            if site.0 != next_id
                                || asset.site != *site
                                || self.host.state.physical.is_none()
                                || game_workshop::site(&self.host.storage, *site).is_some()
                            {
                                return Err(Error::Unavailable);
                            }
                            let bound = self.host.state.workshops.prepare_site_establishment(
                                &self.host.storage,
                                &asset.recipe,
                                game_workshop::SiteBindings {
                                    id: *site,
                                    area: asset.area,
                                    input: game_cargo::inventory(&self.host.storage, asset.input)
                                        .ok_or(Error::Unavailable)?,
                                    tool: game_cargo::inventory(&self.host.storage, asset.tool)
                                        .ok_or(Error::Unavailable)?,
                                    output: game_cargo::inventory(&self.host.storage, asset.output)
                                        .ok_or(Error::Unavailable)?,
                                    access: asset.access.clone(),
                                },
                            )?;
                            workshop_guards.push(bound.stage(
                                &self.host.state.workshops,
                                &self.host.storage,
                                &mut batch,
                            )?);
                            next_id = next_id.checked_add(1).ok_or(Error::Overflow)?;
                        }
                        AssetEffect::Farm {
                            plot,
                            area,
                            seed,
                            tool,
                            water,
                            food,
                        } => {
                            if *plot != *site {
                                return Err(Error::Unavailable);
                            }
                            let ecology =
                                self.host.state.ecology.as_ref().ok_or(Error::Unavailable)?;
                            let development = ecology.prepare_plot_development(
                                &self.host.storage,
                                *plot,
                                *area,
                            )?;
                            let source = development.plot();
                            let agriculture = self
                                .host
                                .state
                                .agriculture
                                .as_ref()
                                .ok_or(Error::Unavailable)?;
                            let establishment = agriculture.prepare_establish_field(
                                &self.host.storage,
                                agriculture::Field {
                                    id: *site,
                                    area: *area,
                                    access: source.access.clone(),
                                    custodian: source.custodian,
                                    seed: *seed,
                                    tool: *tool,
                                    water: *water,
                                    food: *food,
                                },
                            )?;
                            asset_guards.push(establishment.stage(
                                agriculture,
                                &self.host.storage,
                                &mut batch,
                            )?);
                            plot_guards
                                .push(development.stage_into(&self.host.storage, &mut batch)?);
                        }
                    }
                    if enterprise.0 != next_id {
                        return Err(Error::Unavailable);
                    }
                    next_id = next_id.checked_add(1).ok_or(Error::Overflow)?;
                }
                _ => {}
            }
        }

        let consumption_guard = game_cargo::prepare_consumptions(&self.host.storage, &consumed)?
            .stage_guarded_into(&self.host.storage, &mut batch)?;
        let actors = change
            .effects()
            .iter()
            .filter_map(|effect| match effect {
                Effect::Interrupt { actor } => Some(*actor),
                _ => None,
            })
            .collect::<Vec<_>>();
        let interruptions =
            self.prepare_interruptions(&actors, commands::ActivityOwner::Projects)?;
        let event_bound = change
            .effects()
            .iter()
            .try_fold(0u64, |total, effect| {
                total
                    .checked_add(u64::from(!matches!(effect, Effect::Interrupt { .. })))
                    .ok_or(Error::Overflow)
            })?
            .checked_add(interruptions.event_count()?)
            .ok_or(Error::Overflow)?;
        let allocations = next_id
            .checked_sub(self.host.next_id())
            .ok_or(Error::Overflow)?;
        self.host.preflight_records(allocations, event_bound, 0)?;
        let mut interrupted = interruptions.stage_into(self, &mut batch, &final_activities)?;
        let runtime = self
            .host
            .state
            .projects
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let finish = change.stage_into(
            runtime,
            &self.host.storage,
            &self.host.state.workshops,
            &mut batch,
        )?;
        let observation = if observe {
            let (observers, scene) = self.project_observers(&batch)?;
            let prepared = runtime.prepare_projected_observation(
                &self.host.storage,
                &batch,
                &finish,
                game_projects::ObservationFacts {
                    now: self.time(),
                    observers: &observers,
                    accounts_available: self.host.state.agreements.is_some(),
                },
            )?;
            let publication = prepared.stage_into(runtime, &self.host.storage, &mut batch)?;
            Some((publication, scene))
        } else {
            None
        };
        finish.validate_batch(
            runtime,
            &self.host.storage,
            &self.host.state.workshops,
            &batch,
        )?;
        interrupted.validate_batch(self, &batch)?;
        for guard in asset_guards {
            guard.validate(
                self.host
                    .state
                    .agriculture
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
                &self.host.storage,
                &batch,
            )?;
        }
        consumption_guard.validate(&self.host.storage, &batch)?;
        for guard in plot_guards {
            guard.validate(&self.host.storage, &batch)?;
        }
        for guard in workshop_guards {
            guard.validate(&self.host.state.workshops, &self.host.storage, &batch)?;
        }
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.ids = allocations;
        if let Some((publication, scene)) = &observation {
            publication.validate(runtime, &self.host.storage, &transaction.batch)?;
            scene.validate(&self.host.storage, &transaction.batch)?;
            transaction.guards.extend(publication.row_guards());
            transaction.guards.push(scene.clone());
        }
        self.host.commit_transaction(transaction)?;
        let effects = finish.finish(
            self.host
                .state
                .projects
                .as_mut()
                .expect("admitted project runtime"),
        );
        for effect in effects {
            match effect {
                Effect::InputUsed {
                    enterprise,
                    site,
                    quantity,
                } => {
                    self.emit(EventKind::EnterpriseInputUsed {
                        enterprise,
                        site,
                        quantity,
                    });
                }
                Effect::Interrupt { actor } => interrupted.finish_actor(self, actor),
                Effect::Consume {
                    actor,
                    source,
                    quantity,
                } => {
                    self.emit(EventKind::Consumed {
                        actor,
                        source,
                        quantity,
                    });
                }
                Effect::Proposed { project, sponsor } => {
                    self.emit(EventKind::ProjectProposed { project, sponsor });
                }
                Effect::Supported {
                    project,
                    actor,
                    agreement,
                } => {
                    self.emit(EventKind::ProjectSupported {
                        project,
                        actor,
                        agreement,
                    });
                }
                Effect::ConstructionStarted {
                    project,
                    actor,
                    completes,
                } => {
                    self.host
                        .state
                        .due
                        .entry(completes)
                        .or_default()
                        .push(Due::Physical(actor));
                    self.emit(EventKind::ConstructionStarted { project, actor });
                }
                Effect::Cancelled { project, actor } => {
                    self.emit(EventKind::ProjectCancelled { project, actor });
                }
                Effect::Failed { project } => {
                    self.emit(EventKind::ProjectFailed { project });
                }
                Effect::Completed {
                    project,
                    enterprise,
                    site,
                    ..
                } => {
                    self.emit(EventKind::ProjectCompleted {
                        project,
                        enterprise,
                        site,
                    });
                }
            }
        }
        Ok(())
    }
    pub(crate) fn project_boundary(&self, limit: SimTime) -> SimTime {
        self.host.state.projects.as_ref().map_or(limit, |world| {
            world.boundary(&self.host.storage, self.host.clock().time, limit)
        })
    }
    pub(crate) fn project_allocation_bound(&self, next: SimTime) -> Result<u64, Error> {
        self.host.state.projects.as_ref().map_or(Ok(0), |world| {
            world.allocation_bound(&self.host.storage, next)
        })
    }
    pub(crate) fn project_funding_proposal_bound(&self, next: SimTime) -> Result<u64, Error> {
        if self.host.state.decisions.is_none() || !next.0.is_multiple_of(60_000) {
            return Ok(0);
        }
        self.host.state.projects.as_ref().map_or(Ok(0), |world| {
            world.funding_proposal_bound(&self.host.storage, next)
        })
    }
    pub(crate) fn evaluate_projects(
        &self,
        next: SimTime,
    ) -> Result<Option<game_projects::PreparedAdvance>, Error> {
        let Some(world) = &self.host.state.projects else {
            return Ok(None);
        };
        let actors = game_physiology::bodies(&self.host.storage)
            .map(|(actor, body)| {
                let activity = match self.host.storage.activity(actor) {
                    Some(Activity::Work { site }) => game_projects::LaborActivity::Workshop(*site),
                    Some(Activity::Cultivate { field, .. }) => {
                        game_projects::LaborActivity::Farm(*field)
                    }
                    _ => game_projects::LaborActivity::Other,
                };
                game_projects::LaborFacts {
                    actor,
                    alive: body.vitals.alive,
                    health: body.vitals.health,
                    activity,
                }
            })
            .collect::<Vec<_>>();
        let output = world.prepare_labor_advance(
            &self.host.storage,
            self.host.clock().time,
            next,
            &actors,
        )?;
        Ok(Some(output))
    }
    pub(crate) fn commit_projects(
        &mut self,
        output: Option<game_projects::PreparedAdvance>,
    ) -> Result<(), Error> {
        if let Some(output) = output {
            output.commit(
                self.host
                    .state
                    .projects
                    .as_mut()
                    .ok_or(Error::Unavailable)?,
                &mut self.host.storage,
            )?;
        }
        Ok(())
    }
    pub(crate) fn complete_construction(&mut self, actor: PersonId) {
        let Some(world) = &self.host.state.projects else {
            return;
        };
        let Some(job) = game_projects::construction(&self.host.storage, actor) else {
            return;
        };
        let Some(plan) = game_projects::project(&self.host.storage, job.project) else {
            return;
        };
        let asset_available = match plan.kind {
            ProjectKind::Workshop { .. } => self.host.state.physical.is_some(),
            ProjectKind::Farm { plot, .. } => {
                self.host
                    .state
                    .ecology
                    .as_ref()
                    .is_some_and(|w| w.plot(&self.host.storage, plot).is_some())
                    && self.host.state.agriculture.is_some()
            }
        };
        let site = SiteId(self.host.next_id());
        let Some(enterprise_id) = self
            .host
            .next_id()
            .checked_add(u64::from(matches!(plan.kind, ProjectKind::Workshop { .. })))
        else {
            return;
        };
        let prepared = world.prepare_completion(
            &self.host.storage,
            &self.host.state.workshops,
            actor,
            game_projects::CompletionFacts {
                now: self.host.clock().time,
                capable: self.project_able(actor),
                asset_available,
                site,
                enterprise: EnterpriseId(enterprise_id),
            },
        );
        if let Ok(change) = prepared {
            let _ = self.apply_project_change(change);
        }
    }
    pub(crate) fn prepare_enterprise_input(
        &self,
        site: SiteId,
        quantity: Quantity,
    ) -> Result<Option<game_projects::PreparedChange>, Error> {
        self.host
            .state
            .projects
            .as_ref()
            .map(|world| {
                world.prepare_input(
                    &self.host.storage,
                    &self.host.state.workshops,
                    site,
                    self.host
                        .state
                        .workshops
                        .resolve_site(
                            game_workshop::site(&self.host.storage, site)
                                .ok_or(Error::Unavailable)?,
                        )?
                        .recipe
                        .input_good,
                    quantity,
                )
            })
            .transpose()
    }
    pub(crate) fn prepare_farm_input(
        &self,
        site: SiteId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<Option<game_projects::PreparedChange>, Error> {
        self.host
            .state
            .projects
            .as_ref()
            .map(|world| {
                world.prepare_input(
                    &self.host.storage,
                    &self.host.state.workshops,
                    site,
                    game_cargo::inventory(&self.host.storage, source)
                        .expect("inventory exists")
                        .good,
                    quantity,
                )
            })
            .transpose()
    }
    fn project_observers(
        &self,
        batch: &game_ecs::Batch,
    ) -> Result<(Vec<game_projects::Observer>, game_ecs::ReadGuard), Error> {
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        let mut observers = Vec::new();
        for person in self.people() {
            let id = game_ecs::StableId(person.id.0);
            reads.read::<game_physiology::Body>(&self.host.storage, id);
            reads.read::<Location>(&self.host.storage, id);
            if batch.touches::<game_physiology::Body>(id) {
                admitted.protect::<game_physiology::Body>(id);
            }
            if batch.touches::<Location>(id) {
                admitted.protect::<Location>(id);
            }
            let body = batch.projected::<game_physiology::Body>(&self.host.storage, id)?;
            let location = batch.projected::<Location>(&self.host.storage, id)?;
            observers.push(game_projects::Observer {
                actor: person.id,
                area: game_projects::observed_area(
                    body.is_some_and(|body| body.vitals.alive),
                    body.map_or(0, |body| body.vitals.health),
                    location,
                ),
            });
        }
        Ok((observers, reads.bind(&self.host.storage, batch, &admitted)?))
    }
    pub(crate) fn observe_projects(&mut self) {
        let Some(runtime) = self.host.state.projects.as_ref() else {
            return;
        };
        let opening = self.host.transaction_opening();
        let mut batch = game_ecs::Batch::new();
        let (observers, scene) = self
            .project_observers(&batch)
            .expect("valid observer facts");
        let prepared = runtime
            .prepare_observation(
                &self.host.storage,
                game_projects::ObservationFacts {
                    now: self.time(),
                    observers: &observers,
                    accounts_available: self.host.state.agreements.is_some(),
                },
            )
            .expect("valid project observation preparation");
        let publication = prepared
            .stage_into(runtime, &self.host.storage, &mut batch)
            .expect("valid project observations");
        publication
            .validate(runtime, &self.host.storage, &batch)
            .expect("project observation dependencies");
        scene
            .validate(&self.host.storage, &batch)
            .expect("project observer dependencies");
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.guards = publication.row_guards();
        transaction.guards.push(scene);
        self.host
            .commit_transaction(transaction)
            .expect("valid project observation transaction");
    }
    pub fn project_view(&self, actor: PersonId) -> ProjectsView {
        self.host
            .state
            .projects
            .as_ref()
            .map_or_else(ProjectsView::default, |runtime| {
                runtime.remembered_view(&self.host.storage, actor)
            })
    }
    fn pending_project_outputs(&self) -> Vec<(SiteId, Good, Quantity)> {
        let Some(world) = &self.host.state.projects else {
            return Vec::new();
        };
        self.host
            .events()
            .iter()
            .filter(|e| e.sequence > world.last_event)
            .flat_map(|event| match event.event {
                EventKind::Produced {
                    site,
                    output,
                    quantity,
                    ..
                } => vec![(
                    site,
                    game_cargo::inventory(&self.host.storage, output)
                        .expect("inventory exists")
                        .good,
                    quantity,
                )],
                EventKind::CropHarvested {
                    field, food, seed, ..
                } => vec![(field, Good::Food, food), (field, Good::Seed, seed)],
                _ => Vec::new(),
            })
            .collect()
    }
    pub(crate) fn preflight_project_accounting(
        &self,
        outputs: &[(SiteId, Good, Quantity)],
    ) -> Result<(), Error> {
        if let Some(world) = &self.host.state.projects {
            let mut pending = self.pending_project_outputs();
            pending.extend_from_slice(outputs);
            world.prepare_outputs(
                &self.host.storage,
                &self.host.state.workshops,
                &pending,
                None,
            )?;
        }
        Ok(())
    }
    pub(crate) fn update_projects(&mut self) -> Result<(), Error> {
        self.update_projects_observing(false)
    }
    fn update_projects_observing(&mut self, observe: bool) -> Result<(), Error> {
        let Some(world) = &self.host.state.projects else {
            return Ok(());
        };
        let outputs = self.pending_project_outputs();
        let project_facts = game_projects::projects(&self.host.storage)
            .map(|(_, row)| row)
            .map(|plan| game_projects::ProjectCondition {
                project: plan.id,
                sponsor_tool: game_projects::tool_stock_usable(self.project_resource_facts(
                    plan.sponsor,
                    plan.tool,
                    None,
                )),
                materials_available: if self.project_usable(
                    plan.sponsor,
                    plan.materials,
                    Good::Timber,
                    0,
                ) {
                    Quantity(self.unreserved_stock(plan.materials, plan.sponsor))
                } else {
                    Quantity(0)
                },
                usable_worker_tools: game_projects::constructions(&self.host.storage)
                    .filter(|(actor, job)| {
                        job.project == plan.id
                            && game_projects::tool_stock_usable(
                                self.project_resource_facts(*actor, plan.tool, None),
                            )
                    })
                    .map(|(actor, _)| actor)
                    .collect(),
            })
            .collect::<Vec<_>>();
        let enterprise_facts=game_projects::enterprises(&self.host.storage).map(|(_, row)| row).map(|asset| {
            let (active,crop_failed,output_room)=match asset.kind {
                ProjectKind::Farm {..}=>{
                    (game_agriculture::cultivations(&self.host.storage).any(|(_,j)|j.field==asset.site),self.host.state.agriculture.as_ref().unwrap().crop(&self.host.storage, asset.site).expect("component exists").phase==agriculture::CropPhase::Failed,Quantity(0))
                }
                ProjectKind::Workshop { .. }=>{
                    let site=&game_workshop::site(&self.host.storage,asset.site).expect("site exists");
                    let output=game_cargo::inventory(&self.host.storage, site.output).expect("inventory exists");
                    (self.host.storage.activities().any(|(_,activity)|matches!(activity,Activity::Work {site:id} if *id==site.id)),false,
                        Quantity(output.capacity.0.saturating_sub(output.quantity.0)))
                }
            };
            let workers=asset.workers.iter().map(|actor| {
                let (land_accessible,tool_available,materials_available)=match asset.kind {
                    ProjectKind::Farm {..}=>(self.host.state.agriculture.as_ref().unwrap().field(&self.host.storage, asset.site).expect("component exists").access.contains(actor),false,Quantity(0)),
                    ProjectKind::Workshop { .. }=>{
                        let site=&game_workshop::site(&self.host.storage,asset.site).expect("site exists");
                        (true,game_projects::tool_stock_usable(self.project_resource_facts(*actor,site.tool,None)),if self.project_usable(*actor,site.input,self.host.state.workshops.recipe(&site.recipe).expect("validated recipe").input_good,0) {Quantity(self.unreserved_stock(site.input,*actor))} else {Quantity(0)})
                    }
                };
                game_projects::WorkerCondition {capable:self.project_able(*actor),land_accessible,tool_available,materials_available}
            }).collect();
            game_projects::EnterpriseCondition {enterprise:asset.id,active,workers,crop_failed,output_room}
        }).collect::<Vec<_>>();
        let change = world.prepare_reconcile(
            &self.host.storage,
            &self.host.state.workshops,
            self.host.clock().time,
            &project_facts,
            &enterprise_facts,
            &outputs,
            self.host.next_event() - 1,
        )?;
        let changed = !outputs.is_empty() || !change.effects().is_empty();
        self.apply_project_change_observing(change, observe || changed)?;
        Ok(())
    }
    pub(crate) fn review_projects(&mut self) -> Result<(), Error> {
        self.update_projects_observing(true)?;
        self.seek_farm_support()?;
        self.seek_workshop_support()
    }
    fn seek_farm_support(&mut self) -> Result<(), Error> {
        self.seek_project_support(true)
    }
    fn seek_workshop_support(&mut self) -> Result<(), Error> {
        self.seek_project_support(false)
    }
    fn seek_project_support(&mut self, farm: bool) -> Result<(), Error> {
        let Some(runtime) = &self.host.state.projects else {
            return Ok(());
        };
        let plans = runtime.funding_candidates(&self.host.storage, farm);
        for project in plans {
            let plan = game_projects::project(&self.host.storage, project).expect("project exists");
            let actor = plan.sponsor;
            let mind =
                game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                    .expect("project sponsor mind exists");
            let stocks = game_characters::decisions::stocks(&self.host.storage, actor)
                .expect("stock knowledge exists");
            let supplies = stocks
                .known
                .values()
                .map(|known| {
                    let custodian =
                        game_property::inventory_custodian(&self.host.storage, known.inventory);
                    game_projects::RememberedFinancingStock {
                        known,
                        custodian,
                        custodian_area: custodian.and_then(|actor| self.project_area(actor)),
                    }
                })
                .collect::<Vec<_>>();
            let repayments = game_cargo::inventories(&self.host.storage)
                .map(|(inventory, stock)| game_projects::FinancingRepaymentLot {
                    inventory,
                    good: stock.good,
                    area: match stock.location {
                        InventoryLocation::Area(area) => Some(area),
                        _ => None,
                    },
                    custodian: game_property::inventory_custodian(&self.host.storage, inventory),
                })
                .collect::<Vec<_>>();
            let calendar = self.host.state.environment.as_ref().map(|environment| {
                game_projects::FundingCalendar {
                    day: environment.calendar.day,
                    year_days: environment.calendar.year_days(),
                }
            });
            let runtime = self.host.state.projects.as_ref().unwrap();
            let funding = runtime.search_funding(
                &self.host.storage,
                &self.host.state.workshops,
                project,
                game_projects::FundingSearchFacts {
                    now: self.host.clock().time,
                    ruler: self.host.state.ruler,
                    capable: self.project_able(actor),
                    knows_project: self.project_is_known(actor, project),
                    farm_independence: mind.dream == decisions::Aspiration::Independence,
                    workshop_independence: game_characters::independence_material_goal(mind.dream)
                        .is_some(),
                    ambition: mind.personality.ambition,
                    loading: self
                        .host
                        .state
                        .physical
                        .as_ref()
                        .unwrap()
                        .rules
                        .loading_time,
                    calendar,
                    materials_usable: self.project_usable(
                        actor,
                        plan.materials,
                        Good::Timber,
                        runtime.rules.construction_timber.0,
                    ),
                    balances: game_projects::FundingBalances {
                        materials: game_cargo::inventory(&self.host.storage, plan.materials)
                            .expect("inventory exists")
                            .quantity,
                        input: game_cargo::inventory(&self.host.storage, plan.input)
                            .expect("inventory exists")
                            .quantity,
                    },
                    supplies: &supplies,
                    repayments: &repayments,
                },
            );
            let game_projects::FundingChoice::Available(funding) = funding else {
                continue;
            };
            let before = self.host.next_id();
            self.agreement_command(
                actor,
                &AgreementAction::Propose {
                    parties: funding.parties.clone(),
                    terms: funding.terms.clone(),
                    procedure: None,
                },
            )?;
            let agreement = game_agreements::agreements(&self.host.storage)
                .map(|(_, row)| row)
                .find(|agreement| agreement.id.0 >= before && agreement.proposer == actor)
                .expect("admitted funding agreement")
                .id;
            self.debug_project_action(actor, ProjectAction::Support { project, agreement })?;
            self.host
                .state
                .projects
                .as_ref()
                .unwrap()
                .record_funding_search(&mut self.host.storage, actor, &funding)?;
        }
        Ok(())
    }
    fn workshop_procurement(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<game_logistics::Step<Command>> {
        let stocks = game_characters::decisions::stocks(&self.host.storage, actor)
            .expect("stock knowledge exists");
        use game_logistics::{AcquisitionOffer, OfferYield};
        let world = self.host.state.projects.as_ref()?;
        let target = world.procurement_target(
            &self.host.storage,
            &self.host.state.workshops,
            actor,
            &self.agreement_view(actor),
        )?;
        let destination = self.delivery_known_inventory_destination(actor, target.inventory)?;
        let procurement = world.procurement(
            &self.host.storage,
            target,
            game_projects::ProcurementFacts {
                actor,
                stocks: &stocks.known,
                destination: &destination,
                observed_location: &self.delivery_observed_location(actor),
                now: self.host.clock().time,
                actual_quantity: game_cargo::inventory(&self.host.storage, target.inventory)?
                    .quantity,
            },
        )?;
        let hints = procurement
            .hints
            .iter()
            .map(|area| AcquisitionOffer {
                key: area.0,
                good: procurement.delivery.requirement.good,
                source: None,
                location: Location::AtArea(*area),
                observed_at: procurement.observed_at,
                accessible: true,
                quantity: OfferYield::Unknown,
                requires_carrying_room: false,
                requirements: Vec::new(),
                action: Command::Move { destination: *area },
            })
            .collect::<Vec<_>>();
        let minimum_handoff = procurement.minimum_handoff();
        let delivery = procurement.delivery;
        Some(self.plan_delivery_for_planning(
            actor,
            &delivery,
            &destination,
            minimum_handoff,
            &hints,
            resources,
        ))
    }

    pub(crate) fn project_opportunities(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<Command> {
        let stocks = game_characters::decisions::stocks(&self.host.storage, actor)
            .expect("stock knowledge exists");
        let Some(world) = self.host.state.projects.as_ref() else {
            return Vec::new();
        };
        let actor_facts = self.project_actor_facts(actor, None);
        let has_mind = self.host.state.decisions.is_some()
            && game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                .is_some();
        let Some(requests) = world.planning_requests(&self.host.storage, actor_facts, has_mind)
        else {
            return Vec::new();
        };
        let construction = requests
            .construction
            .iter()
            .filter_map(|project| {
                self.construction_facts(actor, *project, resources)
                    .ok()
                    .map(|facts| (*project, facts))
            })
            .collect::<Vec<_>>();
        let procurement = self.workshop_procurement(actor, resources);
        let enterprises = requests
            .enterprises
            .iter()
            .map(|(id, kind, site)| match kind {
                ProjectKind::Farm { .. } => game_projects::EnterpriseOperationFacts {
                    enterprise: *id,
                    cultivation: self.autonomous_field_cultivation(actor, *site, resources),
                    input: None,
                    tool: None,
                    output: None,
                },
                ProjectKind::Workshop { .. } => {
                    let site = game_workshop::site(&self.host.storage, *site).expect("site exists");
                    game_projects::EnterpriseOperationFacts {
                        enterprise: *id,
                        cultivation: None,
                        input: self.project_resource_facts(actor, site.input, resources),
                        tool: self.project_resource_facts(actor, site.tool, resources),
                        output: game_cargo::inventory(&self.host.storage, site.output),
                    }
                }
            })
            .collect::<Vec<_>>();
        let stocks = stocks
            .known
            .values()
            .map(|known| game_projects::WorkshopStock {
                known,
                at_area: game_cargo::inventory(&self.host.storage, known.inventory).and_then(
                    |stock| {
                        if let InventoryLocation::Area(area) = stock.location {
                            Some(area)
                        } else {
                            None
                        }
                    },
                ),
                custodian: game_property::inventory_custodian(&self.host.storage, known.inventory),
            })
            .collect::<Vec<_>>();
        let plots = game_ecology::knowledge(&self.host.storage, actor)
            .into_iter()
            .flat_map(|knowledge| knowledge.known_plots.values())
            .map(|plot| game_projects::RememberedPlot {
                id: plot.id,
                area: plot.area,
                accessible: plot.accessible,
            })
            .collect::<Vec<_>>();
        world
            .opportunities(
                &self.host.storage,
                &self.host.state.workshops,
                game_projects::OpportunityFacts {
                    actor: actor_facts,
                    has_mind,
                    construction: &construction,
                    procurement,
                    enterprises: &enterprises,
                    stocks: &stocks,
                    plots: &plots,
                    loading: self
                        .host
                        .state
                        .physical
                        .as_ref()
                        .unwrap()
                        .rules
                        .loading_time,
                },
            )
            .into_iter()
            .filter_map(|opportunity| match opportunity {
                game_projects::ProjectOpportunity::Action(action) => {
                    Some(Command::Project { action })
                }
                game_projects::ProjectOpportunity::Procurement(step) => {
                    self.delivery_command(actor, step)
                }
                game_projects::ProjectOpportunity::Cultivation(command) => Some(command),
                game_projects::ProjectOpportunity::Workshop(site) => Some(Command::Work { site }),
            })
            .collect()
    }
    /// An individual's own blocked livelihood plan can make a first tool
    /// valuable. This is goal utility with risk, not a universal tool price.
    pub(crate) fn project_financing_utility(
        &self,
        actor: PersonId,
        terms: &[AgreementTerm],
    ) -> i64 {
        let stocks = game_characters::decisions::stocks(&self.host.storage, actor)
            .expect("stock knowledge exists");
        let Some(runtime) = &self.host.state.projects else {
            return 0;
        };
        let Some(mind) =
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
        else {
            return 0;
        };
        let Some(desired_good) = game_characters::independence_material_goal(mind.dream) else {
            return 0;
        };
        struct Knowledge<'a> {
            stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
            work_access: bool,
            store: &'a game_ecs::Store,
        }
        impl game_projects::FinancingKnowledge for Knowledge<'_> {
            fn stocks(&self) -> impl Iterator<Item = &StockKnowledge> {
                self.stocks.values()
            }
            fn stock(&self, id: InventoryId) -> Option<&StockKnowledge> {
                self.stocks.get(&id)
            }
            fn custodian(&self, id: InventoryId) -> Option<PersonId> {
                game_property::inventory_custodian(self.store, id)
            }
            fn has_work_access(&self) -> bool {
                self.work_access
            }
        }
        runtime.financing_utility(
            &self.host.storage,
            &self.host.state.workshops,
            game_projects::FinancingActorFacts {
                actor,
                now: self.host.clock().time,
                seeks_independence: true,
                desired_good,
                ambition: mind.personality.ambition,
                risk_tolerance: mind.personality.risk_tolerance,
            },
            terms,
            &Knowledge {
                stocks: &stocks.known,
                work_access: mind.sites.values().any(|site| site.access),
                store: &self.host.storage,
            },
        )
    }

    pub(crate) fn available_project_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(_) = &self.host.state.projects else {
            return Vec::new();
        };
        if !self.project_able(actor) {
            return Vec::new();
        }
        let facts = self
            .host
            .state
            .projects
            .as_ref()
            .unwrap()
            .available_action_candidates(&self.host.storage, actor)
            .into_iter()
            .filter_map(|project| {
                self.construction_facts(actor, project, None)
                    .ok()
                    .map(|facts| (project, facts))
            })
            .collect::<Vec<_>>();
        self.host
            .state
            .projects
            .as_ref()
            .unwrap()
            .available_actions(
                &self.host.storage,
                self.project_actor_facts(actor, None),
                &facts,
            )
            .into_iter()
            .map(|action| Command::Project { action })
            .collect()
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(runtime) = &state.state.projects else {
        return game_projects::validate_absent(state.components);
    };
    let actor_locations = state
        .people()
        .filter_map(|person| {
            game_spatial::location(state.components, game_ecs::StableId(person.id.0))
                .map(|location| (person.id, location.clone()))
        })
        .collect();
    let construction_activities = state
        .components
        .activities()
        .filter_map(|(actor, activity)| {
            if let Activity::Construct { project, completes } = activity {
                Some((
                    actor,
                    game_projects::ConstructionActivity {
                        project: *project,
                        completes: *completes,
                    },
                ))
            } else {
                None
            }
        })
        .collect();
    let scheduled = state
        .state
        .due
        .iter()
        .flat_map(|(time, events)| {
            events.iter().filter_map(|event| {
                if let Due::Physical(actor) = event {
                    Some((*time, *actor))
                } else {
                    None
                }
            })
        })
        .collect();
    let plots = state
        .state
        .ecology
        .as_ref()
        .into_iter()
        .flat_map(|runtime| runtime.plots(state.components))
        .map(|(id, plot)| (id, plot.area))
        .collect();
    let fields = state
        .state
        .agriculture
        .as_ref()
        .into_iter()
        .flat_map(|runtime| runtime.fields(state.components))
        .map(|(id, field)| {
            (
                id,
                game_projects::FarmFieldFacts {
                    area: field.area,
                    seed: field.seed,
                    food: field.food,
                    tool: field.tool,
                    water: field.water,
                },
            )
        })
        .collect();
    let events = state
        .events
        .iter()
        .flat_map(|event| match event.event {
            EventKind::ConstructionStarted { project, .. } => {
                vec![game_projects::AccountingEvent::ConstructionStarted { project }]
            }
            EventKind::Produced {
                site,
                output,
                quantity,
                ..
            } => vec![game_projects::AccountingEvent::Produced {
                sequence: event.sequence,
                site,
                quantity,
                good: game_cargo::inventory(state.components, output)
                    .expect("inventory exists")
                    .good,
            }],
            EventKind::CropHarvested {
                field, food, seed, ..
            } => vec![
                game_projects::AccountingEvent::Produced {
                    sequence: event.sequence,
                    site: field,
                    good: Good::Food,
                    quantity: food,
                },
                game_projects::AccountingEvent::Produced {
                    sequence: event.sequence,
                    site: field,
                    good: Good::Seed,
                    quantity: seed,
                },
            ],
            EventKind::EnterpriseInputUsed {
                enterprise,
                site,
                quantity,
            } => vec![game_projects::AccountingEvent::InputUsed {
                enterprise,
                site,
                quantity,
            }],
            _ => Vec::new(),
        })
        .collect();
    runtime.validate_composition(
        state.components,
        &state.state.workshops,
        &game_projects::ValidationFacts {
            now: state.time,
            next_id: state.next_id,
            next_event: state.next_event,
            dependencies_present: state.state.physical.is_some()
                && state.state.agreements.is_some()
                && state.state.decisions.is_some(),
            actors: state.people().map(|person| person.id).collect(),
            areas: state.state.areas.iter().map(|area| area.id).collect(),
            actor_locations,
            construction_activities,
            scheduled,
            plots,
            fields,
            events,
            alive: game_physiology::bodies(state.components)
                .filter(|(_, body)| body.vitals.alive)
                .map(|(actor, _)| actor)
                .collect(),
        },
    )
}

#[cfg(test)]
mod farm_tests {
    use super::*;
    fn fixture() -> Simulation {
        let mut sim = Simulation::farm_enterprise_fixture(67, Execution::default()).unwrap();
        let epoch = sim.time();
        let seed = sim.host.seed();
        let environment = sim.host.state.environment.as_mut().unwrap();
        environment.calendar.day = SimDuration(600_000);
        environment.climate.seasonal_temperature = [20; 4];
        environment.climate.temperature_variation = 0;
        environment.climate.seasonal_rain = [30; 4];
        environment.climate.rainfall_variation = 0;
        environment
            .reset_epoch(&mut sim.host.storage, epoch, seed)
            .unwrap();
        let agriculture = sim.host.state.agriculture.as_mut().unwrap();
        agriculture.content = None;
        let crop = agriculture
            .catalog
            .crops
            .get_mut(&agriculture.catalog.default_crop)
            .unwrap();
        crop.prepare_labor = MINUTE;
        crop.plant_labor = MINUTE;
        crop.tend_labor = MINUTE;
        crop.harvest_labor = MINUTE;
        crop.ripe_degree_days = 3;
        sim
    }

    fn ensure_unfunded_proposal(sim: &mut Simulation, actor: PersonId, farm: bool) {
        if game_projects::projects(&sim.host.storage)
            .map(|(_, row)| row)
            .any(|plan| {
                plan.sponsor == actor && matches!(plan.kind, ProjectKind::Farm { .. }) == farm
            })
        {
            return;
        }
        let command = sim
            .project_opportunities(actor, None)
            .into_iter()
            .find(|command| {
                matches!(
                    command,
                    Command::Project {
                        action: ProjectAction::ProposeFarm { .. }
                    }
                ) && farm
                    || matches!(
                        command,
                        Command::Project {
                            action: ProjectAction::Propose { .. }
                        }
                    ) && !farm
            })
            .expect("fixture offers a real locally known project");
        sim.debug_actor_command(actor, command).unwrap();
        sim.observe_projects();
    }

    #[test]
    fn farm_conversion_at_a_day_boundary_consumes_the_closing_plot_once() {
        let mut sim = fixture();
        let farmer = sim.person_at(3).id;
        let area = sim.host.state.areas[0].id;
        let epoch = sim.time();
        let seed = sim.host.seed();
        let environment = sim.host.state.environment.as_mut().unwrap();
        environment.calendar.day = SimDuration(300_000);
        environment.climate.seasonal_evaporation = [2; 4];
        environment
            .reset_epoch(&mut sim.host.storage, epoch, seed)
            .unwrap();
        let proposals: Vec<_> = game_projects::projects(&sim.host.storage)
            .map(|(_, row)| row)
            .filter(|p| p.sponsor == farmer)
            .map(|p| p.id)
            .collect();
        for project in proposals {
            sim.debug_actor_command(
                farmer,
                Command::Project {
                    action: ProjectAction::Cancel { project },
                },
            )
            .unwrap();
        }
        let plot = sim
            .host
            .state
            .ecology
            .as_ref()
            .unwrap()
            .plots(&sim.host.storage)
            .find(|(_, plot)| plot.custodian == Some(farmer))
            .unwrap()
            .0;
        let stock = |good| {
            game_cargo::inventories(&sim.host.storage)
                .map(|(_, row)| row)
                .find(|i| {
                    i.good == good
                        && i.location == InventoryLocation::Area(area)
                        && i.access == BTreeSet::from([farmer])
                })
                .unwrap()
                .id
        };
        let (tool, seed, water, food) = (
            stock(Good::Tool),
            stock(Good::Seed),
            stock(Good::Water),
            stock(Good::Food),
        );
        let materials = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                Quantity(5_000),
                Quantity(5_000),
                BTreeSet::from([farmer]),
            )
            .unwrap();
        sim.debug_actor_command(
            farmer,
            Command::Project {
                action: ProjectAction::ProposeFarm {
                    plot,
                    materials,
                    tool,
                    seed,
                    water,
                    food,
                    beneficiaries: BTreeSet::from([farmer]),
                    workers: BTreeSet::from([farmer]),
                    finish_by: SimTime(900_000),
                },
            },
        )
        .unwrap();
        let project = game_projects::projects(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|p| p.sponsor == farmer && p.status != ProjectStatus::Cancelled)
            .unwrap()
            .id;
        sim.debug_actor_command(
            farmer,
            Command::Project {
                action: ProjectAction::Work { project },
            },
        )
        .unwrap();
        assert_eq!(
            game_projects::construction(&sim.host.storage, farmer)
                .unwrap()
                .completes,
            SimTime(300_000)
        );
        sim.advance(SimDuration(300_000)).unwrap();
        let agriculture = sim.host.state.agriculture.as_ref().unwrap();
        assert!(
            !sim.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .plot(&sim.host.storage, plot)
                .is_some()
        );
        assert_eq!(
            agriculture
                .fields(&sim.host.storage)
                .filter(|(id, _)| *id == plot)
                .count(),
            1
        );
        assert_eq!(
            agriculture.soil(&sim.host.storage, plot),
            Some(&agriculture::Soil {
                fertility: 901,
                water: 103,
                capacity: 120,
            })
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        let historical = game_projects::projects(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|plan| {
                plan.status == ProjectStatus::Cancelled
                    && matches!(plan.kind, ProjectKind::Farm { plot: source, .. } if source == plot)
            })
            .expect("the cancelled proposal preserves its converted plot reference")
            .id;
        let mut invalid: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
        let rows = invalid["state"]["components"]["columns"]["projects/project"]
            .as_array_mut()
            .unwrap();
        let row = rows
            .iter_mut()
            .find(|row| row["entity"] == historical.0)
            .unwrap();
        row["value"]["status"] = serde_json::json!("SeekingSupport");
        assert!(
            Simulation::load(&serde_json::to_vec(&invalid).unwrap(), Execution::default()).is_err(),
            "an unfinished proposal still needs a live plot"
        );
    }

    #[test]
    fn valid_farm_with_unrepresentable_harvest_promise_declines_funding_without_mutation() {
        let mut sim = fixture();
        let farmer = sim.person_at(3).id;
        ensure_unfunded_proposal(&mut sim, farmer, true);
        let saved = sim.save().unwrap();
        let mut representable = Simulation::load(&saved, Execution::default()).unwrap();
        let agreements_before = game_agreements::agreements(&representable.host.storage).count();
        representable.seek_farm_support().unwrap();
        assert!(
            game_agreements::agreements(&representable.host.storage).count() > agreements_before
        );

        let environment = sim.host.state.environment.as_ref().unwrap();
        let mut calendar = environment.calendar.clone();
        calendar.day = SimDuration(u64::MAX / calendar.year_days());
        sim.host.state.environment = Some(
            game_environment::Runtime::new(
                game_environment::Config {
                    calendar,
                    climate: environment.climate.clone(),
                },
                sim.host.clock().time,
                sim.host.seed(),
            )
            .unwrap(),
        );
        let mut sim = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        let before = sim.save().unwrap();
        sim.seek_farm_support().unwrap();
        assert_eq!(sim.save().unwrap(), before);
        // Reach the actual review phase through the normal coordinator. Bodies,
        // decisions and physical production continue at their configured rates.
        sim.advance(MINUTE).unwrap();
        assert!(
            game_physiology::body(&sim.host.storage, farmer)
                .expect("body exists")
                .vitals
                .alive
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }

    #[test]
    fn valid_workshop_with_unrepresentable_financing_windows_declines_before_publication() {
        let mut sim = Simulation::project_fixture(73, Execution::default()).unwrap();
        let sponsor = sim.person_at(3).id;
        ensure_unfunded_proposal(&mut sim, sponsor, false);
        let saved = sim.save().unwrap();
        let mut representable = Simulation::load(&saved, Execution::default()).unwrap();
        let agreements_before = game_agreements::agreements(&representable.host.storage).count();
        representable.seek_workshop_support().unwrap();
        assert!(
            game_agreements::agreements(&representable.host.storage).count() > agreements_before
        );

        sim.host.state.physical.as_mut().unwrap().rules.loading_time = SimDuration(u64::MAX / 10);
        let mut sim = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        let before = sim.save().unwrap();
        sim.seek_workshop_support().unwrap();
        assert_eq!(sim.save().unwrap(), before);
    }
    #[test]
    fn citizen_bargains_for_actual_timber_constructs_farmland_and_accrues_harvest_rent() {
        let mut sim = fixture();
        let farmer = sim.person_at(3).id;
        sim.advance(SimDuration(2_400_000)).unwrap();
        let project = game_projects::projects(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|p| p.sponsor == farmer && matches!(p.kind, ProjectKind::Farm { .. }))
            .unwrap();
        assert_eq!(project.status, ProjectStatus::Completed);
        let enterprise =
            &game_projects::enterprise(&sim.host.storage, project.enterprise.unwrap()).unwrap();
        assert!(enterprise.produced.get(&Good::Food).copied().unwrap_or(0) > 0);
        assert!(enterprise.labor.get(&farmer).copied().unwrap_or(0) > 0);
        let field = enterprise.site;
        assert!(
            sim.host
                .state
                .agriculture
                .as_ref()
                .unwrap()
                .field(&sim.host.storage, field)
                .is_some()
        );
        assert!(
            !sim.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .plot(&sim.host.storage, field)
                .is_some()
        );
        assert_eq!(
            sim.host
                .state
                .agriculture
                .as_ref()
                .unwrap()
                .field(&sim.host.storage, field)
                .expect("component exists")
                .custodian,
            Some(farmer)
        );
        let contract =
            &game_agreements::agreement(&sim.host.storage, *project.support.iter().next().unwrap())
                .unwrap();
        assert_eq!(contract.accepted, contract.parties);
        let obligations =
            game_agreements::obligations(&sim.host.storage).collect::<BTreeMap<_, _>>();
        assert!(contract.obligations.iter().any(|id| matches!(
            obligations[id].term,
            AgreementTerm::Delivery { .. }
        ) && obligations[id].performed == 5_000));
        assert!(contract.obligations.iter().any(|id| matches!(
            obligations[id].term,
            AgreementTerm::OutputShare { .. }
        ) && obligations[id].accrued > 0
            && obligations[id].performed > 0));
        validate(&sim.canonical_state()).unwrap();
        sim.validate_environment().unwrap();
        agreements::validate(&sim.canonical_state()).unwrap();
    }
    #[test]
    fn cautious_supporter_refuses_future_harvest_and_no_promised_material_appears() {
        let mut sim = fixture();
        let supporter = sim.person_at(5).id;
        let farmer = sim.person_at(3).id;
        let mut personality = decisions::Personality::restful();
        personality.risk_tolerance = 0;
        personality.empathy = 0;
        sim.debug_set_personality(supporter, personality, decisions::Aspiration::Comfort)
            .unwrap();
        sim.advance(SimDuration(600_000)).unwrap();
        let project = game_projects::projects(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|p| p.sponsor == farmer)
            .unwrap();
        assert!(project.enterprise.is_none());
        assert!(!project.materials_consumed);
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, project.materials)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert!(
            game_agreements::agreements(&sim.host.storage)
                .map(|(_, row)| row)
                .any(|a| a.proposer == farmer && a.status == AgreementStatus::Rejected)
        );
    }
    #[test]
    fn share_remainders_and_debt_survive_end_and_need_real_delivery() {
        let mut sim = Simulation::project_fixture(69, Execution::default()).unwrap();
        for actor in sim.people().map(|p| p.id).collect::<Vec<_>>() {
            sim.debug_actor_command(actor, Command::Cancel).unwrap();
            sim.debug_set_personality(
                actor,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let debtor = sim.host.state.ruler;
        let creditor = sim.person_at(5).id;
        let site = game_workshop::sites(&sim.host.storage)
            .next()
            .map_or(SiteId(0), |(id, _)| id);
        // An authored existing productive asset; actual production events are the accrual trigger.
        let output = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Area(AreaId(1)),
                Quantity(0),
                Quantity(10_000),
                BTreeSet::from([debtor]),
            )
            .unwrap();
        let destination = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Carried(creditor),
                Quantity(0),
                Quantity(10_000),
                BTreeSet::from([creditor]),
            )
            .unwrap();
        let input = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(AreaId(1)),
                Quantity(3_000),
                Quantity(3_000),
                BTreeSet::from([debtor]),
            )
            .unwrap();
        let tool = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Area(AreaId(1)),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([debtor]),
            )
            .unwrap();
        let site = if site.0 == 0 {
            SiteId(sim.host.allocate_id().unwrap().0)
        } else {
            site
        };
        let chosen_recipe = sim
            .fixture_recipe(input, output, Quantity(1_000), Quantity(1), MINUTE)
            .unwrap();
        game_workshop::insert_site(
            &mut sim.host.storage,
            physical::Site {
                recipe: chosen_recipe,
                id: site,
                area: AreaId(1),
                input,

                tool,
                output,

                access: BTreeSet::from([debtor]),
                last_granted: None,
            },
        )
        .unwrap();
        sim.debug_agreement_action(
            debtor,
            AgreementAction::Propose {
                parties: BTreeSet::from([debtor, creditor]),
                terms: vec![AgreementTerm::OutputShare {
                    debtor,
                    creditor,
                    site,
                    good: Good::Food,
                    numerator: 1,
                    denominator: 3,
                    destination,
                    pay_by: SimTime(840_000),
                    ends_at: SimTime(720_001),
                }],
                procedure: None,
            },
        )
        .unwrap();
        let agreement = game_agreements::agreements(&sim.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap();
        sim.debug_agreement_action(creditor, AgreementAction::Accept { agreement })
            .unwrap();
        let obligation = game_agreements::agreement(&sim.host.storage, agreement)
            .unwrap()
            .obligations[0];
        for _ in 0..3 {
            sim.debug_actor_command(debtor, Command::Work { site })
                .unwrap();
            sim.advance(SimDuration(120_000)).unwrap();
        }
        assert_eq!(
            game_agreements::obligation(&sim.host.storage, obligation)
                .unwrap()
                .accrued,
            1
        );
        assert_eq!(
            game_agreements::obligation(&sim.host.storage, obligation)
                .unwrap()
                .share_remainder,
            0
        );
        sim.advance(SimDuration(480_000)).unwrap();
        assert_eq!(
            game_agreements::obligation(&sim.host.storage, obligation)
                .unwrap()
                .performed,
            0
        );
        assert!(
            game_agreements::obligation(&sim.host.storage, obligation)
                .unwrap()
                .defaulted
        );
        assert!(
            sim.agreement_accepts_delivery(debtor, destination, Quantity(1)),
            "share delivery permission"
        );
        let cargo = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Carried(debtor),
                Quantity(0),
                Quantity(10_000),
                BTreeSet::from([debtor]),
            )
            .unwrap();
        sim.debug_actor_command(
            debtor,
            Command::Load {
                source: output,
                destination: cargo,
                quantity: Quantity(1),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_agreements::obligation(&sim.host.storage, obligation)
                .unwrap()
                .performed,
            0
        );
        sim.debug_actor_command(
            debtor,
            Command::Load {
                source: cargo,
                destination,
                quantity: Quantity(1),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_agreements::obligation(&sim.host.storage, obligation)
                .unwrap()
                .performed,
            1
        );
        assert_eq!(
            game_agreements::agreement(&sim.host.storage, agreement)
                .unwrap()
                .status,
            AgreementStatus::Fulfilled
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(1)
        );
    }
}

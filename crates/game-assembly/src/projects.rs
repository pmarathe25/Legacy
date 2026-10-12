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
        let farmer = sim.host.state.people[3].id;
        let supporter = sim.host.state.people[5].id;
        let area = AreaId(1);
        *game_spatial::location_mut(
            &mut sim.host.storage,
            game_ecs::StableId((sim.host.state.people[sim.person_index[&farmer]].id).0),
        )
        .expect("location exists") = Location::AtArea(area);
        *game_spatial::location_mut(
            &mut sim.host.storage,
            game_ecs::StableId((sim.host.state.people[sim.person_index[&supporter]].id).0),
        )
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
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
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
        let people = sim
            .host
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect::<Vec<_>>();
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
        let sponsor = sim.host.state.people[3].id;
        let financier = sim.host.state.people[5].id;
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
    fn project_area(&self, actor: PersonId) -> Option<AreaId> {
        let body = game_physiology::body(&self.host.storage, actor)?;
        if !body.vitals.alive || body.vitals.health == 0 {
            return None;
        }
        match *game_spatial::location(&self.host.storage, game_ecs::StableId((actor).0))? {
            Location::AtArea(area) => Some(area),
            _ => None,
        }
    }
    fn project_able(&self, actor: PersonId) -> bool {
        self.project_area(actor).is_some()
            && self.independent(actor)
            && !self.combat_restricts(actor)
            && !self.care_index.contains_key(&actor)
            && self.host.state.development.as_ref().is_none_or(|_| {
                matches!(
                    game_lifecycle::life(&self.host.storage, actor)
                        .unwrap()
                        .stage,
                    LifeStage::Adult | LifeStage::Elder
                )
            })
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
        game_cargo::inventory(&self.host.storage, inventory).is_some_and(|stock| {
            stock.good == good
                && stock.access.contains(&actor)
                && self.inventory_area(stock) == self.project_area(actor)
                && (quantity == 0
                    || self.unreserved_stock_for_planning(inventory, actor, resources) >= quantity)
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
        facts.capable &= game_physiology::body(&self.host.storage, actor)
            .is_some_and(|body| body.vitals.fatigue < physical.rules.fatigue_limit);
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
            tool_available: self.project_usable_for_planning(
                actor,
                plan.tool,
                Good::Tool,
                1_000,
                resources,
            ) && self.tool_available_for_planning(actor, plan.tool, resources),
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
    pub(crate) fn construction_possible(&self, actor: PersonId, project: ProjectId) -> bool {
        self.construction_facts(actor, project, None)
            .ok()
            .is_some_and(|facts| {
                self.host
                    .state
                    .projects
                    .as_ref()
                    .unwrap()
                    .prepare_construction(&self.host.storage, project, facts)
                    .is_ok()
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
        self.apply_project_change(change)?;
        self.observe_projects();
        self.host.bump_revision(1)?;
        Ok(())
    }
    fn apply_project_change(&mut self, change: game_projects::PreparedChange) -> Result<(), Error> {
        use game_projects::{AssetEffect, Effect};
        let mut next_id = self.host.next_id();
        let mut batch = game_ecs::Batch::new();
        let mut final_activities = BTreeMap::new();
        let mut consumed = Vec::new();
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
                            let bound = self.host.state.workshops.bind(
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
                            batch.insert(game_ecs::StableId(site.0), bound);
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
                            let source = self
                                .host
                                .state
                                .ecology
                                .as_ref()
                                .and_then(|runtime| runtime.plot(&self.host.storage, *plot))
                                .ok_or(Error::Unavailable)?;
                            let agriculture = self
                                .host
                                .state
                                .agriculture
                                .as_ref()
                                .ok_or(Error::Unavailable)?;
                            if agriculture.field(&self.host.storage, *plot).is_some() {
                                return Err(Error::Unavailable);
                            }
                            let crop =
                                agriculture::Crop::new(agriculture.catalog.default_crop.clone());
                            let rules = agriculture
                                .catalog
                                .crops
                                .get(&crop.definition)
                                .ok_or(Error::Unavailable)?;
                            crop.validate(rules)?;
                            batch.remove::<game_ecology::Plot>(game_ecs::StableId(plot.0));
                            batch.insert(
                                game_ecs::StableId(site.0),
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
                            );
                            batch.insert(game_ecs::StableId(site.0), crop);
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

        game_cargo::prepare_consumptions(&self.host.storage, &consumed)?
            .stage_into(&self.host.storage, &mut batch)?;
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
            .as_mut()
            .ok_or(Error::Unavailable)?;
        let finish = change.stage_into(
            runtime,
            &self.host.storage,
            &self.host.state.workshops,
            &mut batch,
        )?;
        self.host.storage.apply_batch(batch)?;
        let effects = finish.finish(runtime);
        self.host.allocate_ids(allocations)?;
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
    pub(crate) fn interrupt_construction(&mut self, actor: PersonId) {
        if let Some(world) = &mut self.host.state.projects
            && let Ok(change) = world.prepare_interrupt(&self.host.storage, actor)
        {
            let _ = change.commit(world, &mut self.host.storage, &self.host.state.workshops);
        }
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
        let work = self
            .host
            .state
            .physical
            .as_ref()
            .into_iter()
            .flat_map(|_| game_physiology::bodies(&self.host.storage))
            .filter_map(|(actor, body)| {
                if let Activity::Work { site } | Activity::Cultivate { field: site, .. } =
                    self.host.storage.activity(actor)?
                    && body.vitals.alive
                    && body.vitals.health > 0
                {
                    Some(game_projects::EnterpriseWork { actor, site: *site })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let output =
            world.prepare_advance(&self.host.storage, self.host.clock().time, next, &work)?;
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
    pub(crate) fn commit_project_input(
        &mut self,
        prepared: Option<game_projects::PreparedChange>,
    ) -> Result<(), Error> {
        if let Some(change) = prepared {
            self.apply_project_change(change)?;
        }
        Ok(())
    }
    pub(crate) fn observe_projects(&mut self) {
        if self.host.state.projects.is_none() {
            return;
        }
        let mut observations = Vec::new();
        for person in &self.host.state.people {
            let actor = person.id;
            let Some(area) = self.project_area(actor) else {
                continue;
            };
            let projects = game_projects::projects(&self.host.storage)
                .filter(|(_, project)| project.area == area)
                .map(|(id, project)| (id, project.observed_view(self.host.clock().time)))
                .collect::<Vec<_>>();
            let enterprises = game_projects::enterprises(&self.host.storage)
                .filter(|(_, enterprise)| enterprise.area == area)
                .map(|(id, enterprise)| {
                    let books = enterprise.beneficiaries.contains(&actor)
                        || enterprise.operating_agreements.iter().any(|id| {
                            game_agreements::agreement(&self.host.storage, *id)
                                .unwrap()
                                .parties
                                .contains(&actor)
                        });
                    let accounts = books
                        .then(|| self.enterprise_accounts(enterprise))
                        .flatten();
                    (
                        id,
                        enterprise.observed_view(self.host.clock().time, accounts),
                    )
                })
                .collect::<Vec<_>>();
            observations.push((actor, projects, enterprises));
        }
        for (actor, projects, enterprises) in observations {
            if game_projects::knowledge(&self.host.storage, actor).is_none() {
                game_projects::insert_knowledge(
                    &mut self.host.storage,
                    actor,
                    game_projects::Knowledge::default(),
                )
                .expect("valid personal project knowledge");
            }
            let known = game_projects::knowledge_mut(&mut self.host.storage, actor).unwrap();
            known.projects.extend(projects);
            known.enterprises.extend(enterprises);
        }
    }
    fn enterprise_accounts(&self, enterprise: &Enterprise) -> Option<EnterpriseAccountView> {
        self.host.state.agreements.as_ref()?;
        let obligations = enterprise
            .operating_agreements
            .iter()
            .flat_map(|id| {
                game_agreements::agreement(&self.host.storage, *id)
                    .unwrap()
                    .obligations
                    .iter()
            })
            .map(|id| {
                let obligation = game_agreements::obligation(&self.host.storage, *id).unwrap();
                let delivery_good = match obligation.term {
                    AgreementTerm::Delivery { source, .. } => Some(
                        game_cargo::inventory(&self.host.storage, source)
                            .expect("inventory exists")
                            .good,
                    ),
                    _ => None,
                };
                game_projects::AccountObligation {
                    term: obligation.term.clone(),
                    performed: obligation.performed,
                    accrued: obligation.accrued,
                    defaulted: obligation.defaulted,
                    delivery_good,
                }
            })
            .collect::<Vec<_>>();
        game_projects::account_view(enterprise, &obligations).ok()
    }
    pub fn project_view(&self, actor: PersonId) -> ProjectsView {
        let Some(_) = &self.host.state.projects else {
            return ProjectsView::default();
        };
        ProjectsView {
            projects: game_projects::knowledge(&self.host.storage, actor)
                .map(|row| &row.projects)
                .map(|known| known.values().rev().take(32).cloned().collect())
                .unwrap_or_default(),
            enterprises: game_projects::knowledge(&self.host.storage, actor)
                .map(|row| &row.enterprises)
                .map(|known| known.values().rev().take(32).cloned().collect())
                .unwrap_or_default(),
        }
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
        let Some(world) = &self.host.state.projects else {
            return Ok(());
        };
        let outputs = self.pending_project_outputs();
        let project_facts = game_projects::projects(&self.host.storage)
            .map(|(_, row)| row)
            .map(|plan| game_projects::ProjectCondition {
                project: plan.id,
                sponsor_tool: self.project_usable(plan.sponsor, plan.tool, Good::Tool, 1_000),
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
                            && self.project_usable(*actor, plan.tool, Good::Tool, 1_000)
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
                        (true,self.project_usable(*actor,site.tool,Good::Tool,1_000),if self.project_usable(*actor,site.input,self.host.state.workshops.recipe(&site.recipe).expect("validated recipe").input_good,0) {Quantity(self.unreserved_stock(site.input,*actor))} else {Quantity(0)})
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
        self.apply_project_change(change)?;
        if changed {
            self.observe_projects();
        }
        Ok(())
    }
    pub(crate) fn review_projects(&mut self) -> Result<(), Error> {
        self.update_projects()?;
        self.observe_projects();
        self.seek_farm_support()?;
        self.seek_workshop_support()
    }
    fn seek_farm_support(&mut self) -> Result<(), Error> {
        let Some(_) = &self.host.state.projects else {
            return Ok(());
        };
        let plans = game_projects::projects(&self.host.storage)
            .map(|(_, row)| row)
            .filter(|p| {
                matches!(p.kind, ProjectKind::Farm { .. })
                    && !p.materials_consumed
                    && !matches!(p.status, ProjectStatus::Failed | ProjectStatus::Cancelled)
            })
            .cloned()
            .collect::<Vec<_>>();
        for plan in plans {
            let actor = plan.sponsor;
            if actor == self.host.state.ruler
                || !self.project_able(actor)
                || !self.project_is_known(actor, plan.id)
                || self.project_usable(
                    actor,
                    plan.materials,
                    Good::Timber,
                    self.host
                        .state
                        .projects
                        .as_ref()
                        .unwrap()
                        .rules
                        .construction_timber
                        .0,
                )
                || game_projects::requests(&self.host.storage, actor)
                    .map(|row| &row.next_search)
                    .is_some_and(|t| *t > self.host.clock().time)
                || plan.support.iter().any(|id| {
                    matches!(
                        game_agreements::agreement(&self.host.storage, *id)
                            .unwrap()
                            .status,
                        AgreementStatus::Proposed
                            | AgreementStatus::Active
                            | AgreementStatus::Breached
                    )
                })
            {
                continue;
            }
            let mind =
                &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                    .unwrap();
            let stocks = game_characters::decisions::stocks(&self.host.storage, actor)
                .expect("stock knowledge exists");
            if mind.dream != decisions::Aspiration::Independence || mind.personality.ambition < 600
            {
                continue;
            }
            let required = self
                .host
                .state
                .projects
                .as_ref()
                .unwrap()
                .rules
                .construction_timber;
            let offer = stocks
                .known
                .values()
                .filter(|f| {
                    f.good == Good::Timber
                        && f.area == plan.area
                        && f.quantity >= required
                        && f.inventory != plan.materials
                })
                .find_map(|f| {
                    game_property::inventory_custodian(&self.host.storage, f.inventory)
                        .filter(|p| *p != actor && self.project_area(*p) == Some(plan.area))
                        .map(|p| (p, f.inventory))
                });
            let Some((supporter, source)) = offer else {
                continue;
            };
            let destination = stocks
                .known
                .values()
                .filter(|f| {
                    f.good == Good::Food && f.area == plan.area && f.inventory != plan.output
                })
                .find(|f| {
                    game_property::inventory_custodian(&self.host.storage, f.inventory)
                        == Some(supporter)
                })
                .map(|f| f.inventory);
            let Some(destination) = destination else {
                continue;
            };
            let calendar = &self.host.state.environment.as_ref().unwrap().calendar;
            let game_projects::FundingChoice::Available(funding) = plan.prepare_farm_funding(
                &self.host.state.projects.as_ref().unwrap().rules,
                game_projects::FundingFacts {
                    now: self.host.clock().time,
                    financier: supporter,
                    source,
                    repayment_destination: destination,
                },
                game_projects::FundingCalendar {
                    day: calendar.day,
                    year_days: calendar.year_days(),
                },
            ) else {
                // A future promise that cannot be represented is unavailable.
                // No agreement, support record or retry state has been changed.
                continue;
            };
            let before = self.host.next_id();
            self.agreement_command(
                actor,
                &AgreementAction::Propose {
                    parties: funding.parties,
                    terms: funding.terms,
                    procedure: None,
                },
            )?;
            let agreement = game_agreements::agreements(&self.host.storage)
                .map(|(_, row)| row)
                .find(|a| a.id.0 >= before && a.proposer == actor)
                .unwrap()
                .id;
            self.debug_project_action(
                actor,
                ProjectAction::Support {
                    project: plan.id,
                    agreement,
                },
            )?;
            game_projects::insert_requests(
                &mut self.host.storage,
                actor,
                game_projects::Requests {
                    next_search: funding.next_search,
                },
            )?;
        }
        Ok(())
    }
    fn seek_workshop_support(&mut self) -> Result<(), Error> {
        let Some(_) = &self.host.state.projects else {
            return Ok(());
        };
        let plans = game_projects::projects(&self.host.storage)
            .map(|(_, row)| row)
            .filter(|plan| {
                matches!(plan.kind, ProjectKind::Workshop { .. })
                    && plan.support.iter().all(|id| {
                        matches!(
                            game_agreements::agreement(&self.host.storage, *id)
                                .unwrap()
                                .status,
                            AgreementStatus::Fulfilled
                                | AgreementStatus::Rejected
                                | AgreementStatus::Withdrawn
                        )
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        for plan in plans {
            if plan.sponsor == self.host.state.ruler
                || !self.project_able(plan.sponsor)
                || matches!(
                    plan.status,
                    ProjectStatus::Failed | ProjectStatus::Cancelled
                )
            {
                continue;
            }
            let actor = plan.sponsor;
            let mind =
                &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                    .unwrap();
            let stocks = game_characters::decisions::stocks(&self.host.storage, actor)
                .expect("stock knowledge exists");
            if game_projects::requests(&self.host.storage, actor)
                .is_some_and(|row| row.next_search > self.host.clock().time)
            {
                continue;
            }
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
            let game_projects::FundingChoice::Available(funding) = plan
                .prepare_remembered_workshop_funding(
                    &self.host.state.projects.as_ref().unwrap().rules,
                    &self.host.state.workshops,
                    game_projects::RememberedWorkshopFunding {
                        seeks_independence: game_characters::independence_material_goal(mind.dream)
                            .is_some(),
                        ambition: mind.personality.ambition,
                        now: self.host.clock().time,
                        loading: self
                            .host
                            .state
                            .physical
                            .as_ref()
                            .unwrap()
                            .rules
                            .loading_time,
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
                )
            else {
                continue;
            };
            let before = self.host.next_id();
            self.debug_agreement_action(
                actor,
                AgreementAction::Propose {
                    parties: funding.parties,
                    terms: funding.terms,
                    procedure: None,
                },
            )?;
            let agreement = game_agreements::agreements(&self.host.storage)
                .map(|(_, row)| row)
                .find(|agreement| agreement.id.0 >= before && agreement.proposer == actor)
                .unwrap()
                .id;
            self.debug_project_action(
                actor,
                ProjectAction::Support {
                    project: plan.id,
                    agreement,
                },
            )?;
            game_projects::insert_requests(
                &mut self.host.storage,
                actor,
                game_projects::Requests {
                    next_search: funding.next_search,
                },
            )?;
        }
        Ok(())
    }
    fn workshop_procurement(
        &self,
        actor: PersonId,
        _mind: &decisions::Mind,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<Command> {
        let stocks = game_characters::decisions::stocks(&self.host.storage, actor)
            .expect("stock knowledge exists");
        use game_logistics::{
            AcquisitionOffer, Delivery, Destination, OfferYield, Requirement, Step,
        };
        let world = self.host.state.projects.as_ref()?;
        let plan = game_projects::knowledge(&self.host.storage, actor)
            .map(|row| &row.projects)?
            .values()
            .rev()
            .find(|plan| {
                matches!(plan.kind, ProjectKind::Workshop { .. })
                    && plan.sponsor == actor
                    && !matches!(
                        plan.status,
                        ProjectStatus::Failed | ProjectStatus::Cancelled
                    )
            })?;
        let agreements = self.agreement_view(actor);
        if !plan.materials_consumed && plan.support.iter().any(|id|agreements.iter().find(|agreement|agreement.id==*id).is_some_and(|agreement|
            agreement.status==AgreementStatus::Proposed || agreement.obligations.iter().any(|obligation|
                matches!(obligation.term,AgreementTerm::Delivery {destination,..} if destination==plan.materials) && !obligation.defaulted && obligation.performed<obligation.term.amount()))) {return None;}
        let target = if plan.materials_consumed {
            plan.input
        } else {
            plan.materials
        };
        let required = if plan.materials_consumed {
            let ProjectKind::Workshop { recipe } = &plan.kind else {
                return None;
            };
            self.host
                .state
                .workshops
                .recipe(recipe)
                .ok()?
                .input_quantity
                .0
        } else {
            world.rules.construction_timber.0
        };
        let fact = stocks.known.get(&target)?;
        let destination = self.delivery_known_inventory_destination(actor, target)?;
        let available = if destination.observed_at == self.host.clock().time
            && destination.location == self.delivery_observed_location(actor)
        {
            game_cargo::inventory(&self.host.storage, target)
                .expect("inventory exists")
                .quantity
                .0
        } else {
            fact.quantity.0
        };
        let missing = required.saturating_sub(available);
        if missing == 0 {
            return None;
        }
        let delivery = Delivery::new(
            Requirement {
                good: fact.good,
                quantity: Quantity(missing),
                source: None,
            },
            Destination::Inventory(target),
        )
        .ok()?;
        let hints = game_projects::knowledge(&self.host.storage, actor)
            .map(|row| &row.timber_hints)
            .into_iter()
            .flatten()
            .filter(|area| {
                !stocks
                    .known
                    .values()
                    .any(|fact| fact.area == **area && fact.good == delivery.requirement.good)
            })
            .map(|area| AcquisitionOffer {
                key: area.0,
                good: delivery.requirement.good,
                source: None,
                location: Location::AtArea(*area),
                observed_at: plan.observed_at,
                accessible: true,
                quantity: OfferYield::Unknown,
                requires_carrying_room: false,
                requirements: Vec::new(),
                action: Command::Move { destination: *area },
            })
            .collect::<Vec<_>>();
        match self.plan_delivery_for_planning(
            actor,
            &delivery,
            &destination,
            Quantity(1),
            &hints,
            resources,
        ) {
            Step::Arrived { .. } | Step::Complete => None,
            step => self.delivery_command(actor, step),
        }
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
        if !self.project_able(actor) {
            return Vec::new();
        }
        let Some(mind) = self.host.state.decisions.as_ref().and_then(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
        }) else {
            return Vec::new();
        };
        let mut opportunities = Vec::new();
        for plan in game_projects::projects(&self.host.storage)
            .map(|(_, row)| row)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .filter(|plan| plan.workers.contains(&actor) && self.project_is_known(actor, plan.id))
            .take(8)
        {
            if self
                .construction_facts(actor, plan.id, resources)
                .is_ok_and(|facts| {
                    world
                        .prepare_construction(&self.host.storage, plan.id, facts)
                        .is_ok()
                })
            {
                opportunities.push(Command::Project {
                    action: ProjectAction::Work { project: plan.id },
                });
            }
        }
        if let Some(command) = self.workshop_procurement(actor, mind, resources) {
            opportunities.push(command);
        }
        for workshop in game_projects::enterprises(&self.host.storage)
            .map(|(_, row)| row)
            .filter(|workshop| {
                workshop.sponsor == actor && workshop.area == self.project_area(actor).unwrap()
            })
        {
            if matches!(workshop.kind, ProjectKind::Farm { .. }) {
                if let Some(command) =
                    self.autonomous_field_cultivation(actor, workshop.site, resources)
                {
                    opportunities.push(command);
                }
                continue;
            }
            let site =
                &game_workshop::site(&self.host.storage, workshop.site).expect("site exists");
            if self.project_usable_for_planning(
                actor,
                site.input,
                self.host
                    .state
                    .workshops
                    .recipe(&site.recipe)
                    .expect("validated recipe")
                    .input_good,
                self.host
                    .state
                    .workshops
                    .recipe(&site.recipe)
                    .expect("validated recipe")
                    .input_quantity
                    .0,
                resources,
            ) && self.project_usable_for_planning(actor, site.tool, Good::Tool, 1_000, resources)
                && game_cargo::inventory(&self.host.storage, site.output)
                    .expect("inventory exists")
                    .capacity
                    .0
                    - game_cargo::inventory(&self.host.storage, site.output)
                        .expect("inventory exists")
                        .quantity
                        .0
                    >= self
                        .host
                        .state
                        .workshops
                        .recipe(&site.recipe)
                        .expect("validated recipe")
                        .output_quantity
                        .0
            {
                opportunities.push(Command::Work { site: site.id });
            }
        }
        if game_projects::knowledge(&self.host.storage, actor)
            .map(|row| &row.projects)
            .is_some_and(|known| {
                known.values().any(|project| {
                    project.sponsor == actor
                        && !matches!(
                            project.status,
                            ProjectStatus::Cancelled | ProjectStatus::Failed
                        )
                })
            })
            || game_projects::requests(&self.host.storage, actor)
                .map(|row| &row.next_search)
                .is_some_and(|time| *time > self.host.clock().time)
        {
            return opportunities;
        }
        let area = self.project_area(actor).unwrap();
        let own = |fact: &&StockKnowledge| {
            fact.area == area
                && game_cargo::inventory(&self.host.storage, fact.inventory)
                    .is_some_and(|stock| stock.location == InventoryLocation::Area(area))
                && game_property::inventory_custodian(&self.host.storage, fact.inventory)
                    == Some(actor)
        };
        let Some(materials) = stocks
            .known
            .values()
            .filter(own)
            .find(|fact| fact.good == Good::Timber && fact.accessible)
            .map(|fact| fact.inventory)
        else {
            return opportunities;
        };
        let Some(tool) = stocks
            .known
            .values()
            .filter(own)
            .find(|fact| fact.good == Good::Tool && fact.accessible && fact.quantity.0 >= 1_000)
            .map(|fact| fact.inventory)
        else {
            return opportunities;
        };
        if let Some(plot) = self
            .host
            .state
            .ecology
            .as_ref()
            .and_then(|_| {
                game_ecology::knowledge(&self.host.storage, actor).map(|row| &row.known_plots)
            })
            .and_then(|k| k.values().find(|p| p.area == area && p.accessible))
        {
            let stock = |good| {
                stocks
                    .known
                    .values()
                    .filter(own)
                    .find(|f| f.good == good && f.accessible)
                    .map(|f| f.inventory)
            };
            if let (Some(seed), Some(water), Some(food)) =
                (stock(Good::Seed), stock(Good::Water), stock(Good::Food))
            {
                opportunities.push(Command::Project {
                    action: ProjectAction::ProposeFarm {
                        plot: plot.id,
                        materials,
                        tool,
                        seed,
                        water,
                        food,
                        beneficiaries: BTreeSet::from([actor]),
                        workers: BTreeSet::from([actor]),
                        finish_by: SimTime(
                            self.host.clock().time.0.saturating_add(
                                world
                                    .rules
                                    .construction_labor
                                    .0
                                    .saturating_mul(4)
                                    .saturating_add(600_000),
                            ),
                        ),
                    },
                });
            }
        }
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
        opportunities.extend(
            world
                .workshop_prospects(
                    &self.host.storage,
                    &self.host.state.workshops,
                    game_projects::WorkshopProspectFacts {
                        actor,
                        area,
                        now: self.host.clock().time,
                        capable: self.project_able(actor),
                        loading: self
                            .host
                            .state
                            .physical
                            .as_ref()
                            .unwrap()
                            .rules
                            .loading_time,
                    },
                    &stocks,
                )
                .unwrap_or_default()
                .into_iter()
                .map(|action| Command::Project { action }),
        );
        opportunities
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
        let mut actions = Vec::new();
        for plan in game_projects::projects(&self.host.storage)
            .map(|(_, row)| row)
            .filter(|project| self.project_is_known(actor, project.id))
            .take(16)
        {
            if self.construction_possible(actor, plan.id) {
                actions.push(ProjectAction::Work { project: plan.id });
            }
            if plan.sponsor == actor
                && !matches!(
                    plan.status,
                    ProjectStatus::Completed | ProjectStatus::Failed | ProjectStatus::Cancelled
                )
            {
                actions.push(ProjectAction::Cancel { project: plan.id });
            }
        }
        actions
            .into_iter()
            .map(|action| Command::Project { action })
            .collect()
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.state.projects else {
        return game_projects::validate_absent(state.components);
    };
    let fail = || Error::InvalidSave("invalid workshop project or enterprise".into());
    let people = state
        .state
        .people
        .iter()
        .map(|person| (person.id, person))
        .collect::<BTreeMap<_, _>>();
    state.state.physical.as_ref().ok_or_else(fail)?;
    state.state.agreements.as_ref().ok_or_else(fail)?;
    if state.state.decisions.is_none()
        || world.last_event >= state.next_event
        || game_projects::knowledges(state.components).any(|(actor, row)| {
            !people.contains_key(&actor)
                || row
                    .timber_hints
                    .iter()
                    .any(|area| !state.state.areas.iter().any(|known| known.id == *area))
        })
        || game_projects::request_rows(state.components)
            .any(|(actor, _)| !people.contains_key(&actor))
    {
        return Err(fail());
    }
    world
        .validate_owned(state.components, &state.state.workshops, state.time)
        .map_err(|_| fail())?;
    for (id, plan) in game_projects::projects(state.components) {
        if id != plan.id
            || id.0 >= state.next_id
            || !people.contains_key(&plan.sponsor)
            || !state.state.areas.iter().any(|area| area.id == plan.area)
            || plan
                .beneficiaries
                .iter()
                .chain(&plan.workers)
                .chain(plan.contributions.keys())
                .any(|actor| !people.contains_key(actor))
            || plan.support.iter().any(|id| {
                game_agreements::agreement(state.components, *id)
                    .is_none_or(|agreement| !agreement.parties.contains(&plan.sponsor))
            })
        {
            return Err(fail());
        }
        let (input_good, output_good) = match &plan.kind {
            ProjectKind::Farm { .. } => (Good::Seed, Good::Food),
            ProjectKind::Workshop { recipe } => {
                let recipe = state.state.workshops.recipe(recipe).map_err(|_| fail())?;
                (recipe.input_good, recipe.output_good)
            }
        };
        for (inventory, good) in [
            (plan.materials, Good::Timber),
            (plan.tool, Good::Tool),
            (plan.input, input_good),
            (plan.output, output_good),
        ] {
            if game_cargo::inventory(state.components, inventory).is_none_or(|stock| {
                stock.good != good || stock.location != InventoryLocation::Area(plan.area)
            }) {
                return Err(fail());
            }
        }
        if let ProjectKind::Farm { plot, water } = plan.kind {
            let live_plot = state
                .state
                .ecology
                .as_ref()
                .and_then(|world| world.plot(state.components, plot))
                .is_some_and(|source| source.area == plan.area);
            // A terminal proposal retains its original land reference after a
            // different, completed farm project converts that same plot.
            let historical_conversion = matches!(plan.status,
                ProjectStatus::Cancelled | ProjectStatus::Failed)
                && state.state.agriculture.as_ref().and_then(|world| {
                    world.field(state.components, plot)
                }).is_some_and(|field| field.id == plot && field.area == plan.area)
                && game_projects::enterprises(state.components).map(|(_, row)| row).any(|enterprise| {
                    enterprise.project != plan.id
                        && enterprise.site == plot
                        && enterprise.area == plan.area
                        && matches!(enterprise.kind, ProjectKind::Farm { plot: source, .. } if source == plot)
                });
            if game_cargo::inventory(state.components, water).is_none_or(|i| {
                i.good != Good::Water || i.location != InventoryLocation::Area(plan.area)
            }) || plan.enterprise.is_none() && !live_plot && !historical_conversion
            {
                return Err(fail());
            }
        }
        if plan.materials_consumed&&!state.events.iter().any(|event|matches!(event.event,EventKind::ConstructionStarted {project,..} if project==id)) {return Err(fail());}
    }
    for (actor, job) in game_projects::constructions(state.components) {
        let plan = game_projects::project(state.components, job.project).ok_or_else(fail)?;
        if !people.contains_key(&actor)
            || *game_spatial::location(state.components, game_ecs::StableId(actor.0))
                .ok_or_else(fail)?
                != Location::AtArea(plan.area)
            || game_physiology::body(state.components, actor).is_none_or(|body| {
                !body.vitals.alive
                    || state.components.activity(actor)
                        != Some(&Activity::Construct {
                            project: job.project,
                            completes: job.completes,
                        })
            })
            || !state.state.due.get(&job.completes).is_some_and(|events| {
                events
                    .iter()
                    .any(|event| matches!(event,Due::Physical(id) if *id==actor))
            })
        {
            return Err(fail());
        }
    }
    for (actor, activity) in state.components.activities() {
        if matches!(activity, Activity::Construct { .. })
            && game_projects::construction(state.components, actor).is_none()
        {
            return Err(fail());
        }
    }
    for (id, enterprise) in game_projects::enterprises(state.components) {
        let plan = game_projects::project(state.components, enterprise.project).ok_or_else(fail)?;
        if let ProjectKind::Farm { plot, water } = plan.kind {
            let field = state
                .state
                .agriculture
                .as_ref()
                .and_then(|w| w.field(state.components, plot))
                .ok_or_else(fail)?;
            if enterprise.site != plot
                || field.seed != plan.input
                || field.food != plan.output
                || field.tool != plan.tool
                || field.water != water
                || enterprise.labor.keys().any(|id| !people.contains_key(id))
            {
                return Err(fail());
            }
            let mut produced = BTreeMap::<Good, u64>::new();
            for event in state
                .events
                .iter()
                .filter(|e| e.sequence <= world.last_event)
            {
                if let EventKind::CropHarvested {
                    field: site,
                    food,
                    seed,
                    ..
                } = event.event
                    && site == plot
                {
                    for (good, quantity) in [(Good::Food, food), (Good::Seed, seed)] {
                        let total = produced.entry(good).or_default();
                        *total = total.checked_add(quantity.0).ok_or_else(fail)?;
                    }
                }
            }
            if produced != enterprise.produced
                || enterprise.operating_agreements.iter().any(|id| {
                    game_agreements::agreement(state.components, *id)
                        .is_none_or(|a| !a.parties.contains(&enterprise.sponsor))
                })
            {
                return Err(fail());
            }
            continue;
        }
        let site = game_workshop::site(state.components, enterprise.site).ok_or_else(fail)?;
        if id != enterprise.id
            || id.0 >= state.next_id
            || site.area != plan.area
            || site.input != plan.input
            || site.tool != plan.tool
            || site.output != plan.output
            || !matches!(&plan.kind, ProjectKind::Workshop { recipe } if &site.recipe == recipe)
            || state.state.workshops.resolve_site(site).is_err()
            || enterprise
                .labor
                .keys()
                .any(|actor| !people.contains_key(actor))
            || enterprise.operating_agreements.iter().any(|id| {
                game_agreements::agreement(state.components, *id)
                    .is_none_or(|agreement| !agreement.parties.contains(&enterprise.sponsor))
            })
        {
            return Err(fail());
        }
        let produced = state
            .events
            .iter()
            .filter(|event| event.sequence <= world.last_event)
            .filter_map(|event| match event.event {
                EventKind::Produced { site, quantity, .. } if site == enterprise.site => {
                    Some(quantity.0)
                }
                _ => None,
            })
            .try_fold(0_u64, |total, value| total.checked_add(value))
            .ok_or_else(fail)?;
        let used = state
            .events
            .iter()
            .filter_map(|event| match event.event {
                EventKind::EnterpriseInputUsed {
                    enterprise: target,
                    site,
                    quantity,
                } if target == id && site == enterprise.site => Some(quantity.0),
                _ => None,
            })
            .try_fold(0_u64, |total, value| total.checked_add(value))
            .ok_or_else(fail)?;
        let recipe = state
            .state
            .workshops
            .recipe(&site.recipe)
            .map_err(|_| fail())?;
        let expected_outputs = if produced == 0 {
            BTreeMap::new()
        } else {
            BTreeMap::from([(recipe.output_good, produced)])
        };
        let expected_inputs = if used == 0 {
            BTreeMap::new()
        } else {
            BTreeMap::from([(recipe.input_good, used)])
        };
        if enterprise.produced != expected_outputs || enterprise.inputs != expected_inputs {
            return Err(fail());
        }
    }
    for (actor, row) in game_projects::knowledges(state.components) {
        let known = &row.projects;
        if !people.contains_key(&actor) {
            return Err(fail());
        }
        for (id, view) in known {
            let plan = game_projects::project(state.components, *id).ok_or_else(fail)?;
            if *id != view.id
                || view.kind != plan.kind
                || view.sponsor != plan.sponsor
                || view.area != plan.area
                || view.materials != plan.materials
                || view.tool != plan.tool
                || view.input != plan.input
                || view.output != plan.output
                || view.beneficiaries != plan.beneficiaries
                || view.workers != plan.workers
                || !view.support.is_subset(&plan.support)
                || view.progress_ms > plan.progress_ms
                || view.observed_at > state.time
                || view.materials_consumed && !plan.materials_consumed
                || view.enterprise.is_some() && view.enterprise != plan.enterprise
            {
                return Err(fail());
            }
        }
    }
    for (actor, row) in game_projects::knowledges(state.components) {
        let known = &row.enterprises;
        if !people.contains_key(&actor) {
            return Err(fail());
        }
        for (id, view) in known {
            let enterprise = game_projects::enterprise(state.components, *id).ok_or_else(fail)?;
            if *id != view.id
                || view.kind != enterprise.kind
                || view.project != enterprise.project
                || view.sponsor != enterprise.sponsor
                || view.area != enterprise.area
                || view.site != enterprise.site
                || view.observed_at > state.time
                || view.beneficiaries != enterprise.beneficiaries
                || view.workers != enterprise.workers
            {
                return Err(fail());
            }
            if let Some(accounts) = &view.accounts
                && (accounts.inputs.iter().any(|(good, quantity)| {
                    enterprise
                        .inputs
                        .get(good)
                        .is_none_or(|actual| quantity > actual)
                }) || accounts.produced.iter().any(|(good, quantity)| {
                    enterprise
                        .produced
                        .get(good)
                        .is_none_or(|actual| quantity > actual)
                }) || !accounts
                    .operating_agreements
                    .is_subset(&enterprise.operating_agreements)
                    || accounts.labor.iter().any(|(actor, elapsed)| {
                        enterprise
                            .labor
                            .get(actor)
                            .is_none_or(|actual| elapsed > actual)
                    }))
            {
                return Err(fail());
            }
        }
    }
    Ok(())
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
        let farmer = sim.host.state.people[3].id;
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
        let farmer = sim.host.state.people[3].id;
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
        let sponsor = sim.host.state.people[3].id;
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
        let farmer = sim.host.state.people[3].id;
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
        let supporter = sim.host.state.people[5].id;
        let farmer = sim.host.state.people[3].id;
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
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
            sim.debug_actor_command(actor, Command::Cancel).unwrap();
            sim.debug_set_personality(
                actor,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let debtor = sim.host.state.ruler;
        let creditor = sim.host.state.people[5].id;
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

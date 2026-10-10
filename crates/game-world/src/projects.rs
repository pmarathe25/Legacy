//! A concrete wooden-tool workshop: support promises require real delivery,
//! construction consumes timber and time, and the completed site uses recipes.
use crate::*;
pub use game_projects::{Construction, Enterprise, Project, Rules, World};

impl Simulation {
    /// Authoring explicit custodians and opportunities is fixture content; it does
    /// not create a general rule assigning unused land or stores to inhabitants.
    pub fn farm_enterprise_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::ecology_fixture(seed, execution)?;
        sim.state.scenario = ScenarioInfo {
            id: "farm-enterprise-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.state.agreements = Some(agreements::World {
            ledger: game_agreements::State {
                agreements: BTreeMap::new(),
                obligations: BTreeMap::new(),
                amendments: BTreeMap::new(),
                last_event: sim.state.next_event - 1,
                next_search: BTreeMap::new(),
                pending_work: BTreeMap::new(),
                knowledge: BTreeMap::new(),
                last_observation_event: sim.state.next_event - 1,
            },
            custodians: BTreeMap::new(),
        });
        sim.state.projects = Some(World {
            rules: Rules::default(),
            projects: BTreeMap::new(),
            enterprises: BTreeMap::new(),
            jobs: BTreeMap::new(),
            known_projects: BTreeMap::new(),
            known_enterprises: BTreeMap::new(),
            timber_hints: BTreeMap::new(),
            next_search: BTreeMap::new(),
            last_event: sim.state.next_event - 1,
        });
        let farmer = sim.state.people[3].id;
        let supporter = sim.state.people[5].id;
        let area = AreaId(1);
        sim.state.people[sim.person_index[&farmer]].location = Location::AtArea(area);
        sim.state.people[sim.person_index[&supporter]].location = Location::AtArea(area);
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
            sim.state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(id, custodian);
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
        for actor in sim.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
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
        sim.state.scenario = ScenarioInfo {
            id: "project-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let hour = 3_600_000;
        let rules = &mut sim.state.physical.as_mut().unwrap().rules;
        rules.hunger_harm = 72 * hour;
        rules.meal_relief = 24 * hour;
        rules.fatigue_limit = 16 * hour;
        rules.max_health = 48 * hour;
        let people = sim
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect::<Vec<_>>();
        for person in &people {
            sim.debug_actor_command(*person, Command::Cancel)?;
            sim.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(person)
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
        sim.state.physical.as_mut().unwrap().sites.clear();
        for mind in sim.state.decisions.as_mut().unwrap().minds.values_mut() {
            mind.sites.clear();
            mind.plan = None;
        }
        let sponsor = sim.state.people[3].id;
        let financier = sim.state.people[5].id;
        let area = sim.state.areas[0].id;
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
        let timber = sim
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Timber
                    && stock.location == InventoryLocation::Area(area)
                    && stock.id != materials
            })
            .unwrap()
            .id;
        sim.state.inventories.get_mut(&timber).unwrap().access = BTreeSet::from([financier]);
        let custodians = &mut sim.state.agreements.as_mut().unwrap().custodians;
        for inventory in [materials, tool, output, provisions] {
            custodians.insert(inventory, sponsor);
        }
        for inventory in [timber, payment, own_output] {
            custodians.insert(inventory, financier);
        }
        sim.debug_set_personality(
            sponsor,
            decisions::Personality::industrious(),
            decisions::Aspiration::Independence,
        )?;
        let mut investor = decisions::Personality::industrious();
        investor.risk_tolerance = 850;
        sim.debug_set_personality(financier, investor, decisions::Aspiration::Independence)?;
        sim.state.projects = Some(World {
            rules: Rules::default(),
            projects: BTreeMap::new(),
            enterprises: BTreeMap::new(),
            jobs: BTreeMap::new(),
            known_projects: BTreeMap::new(),
            known_enterprises: BTreeMap::new(),
            timber_hints: BTreeMap::from([
                (sponsor, BTreeSet::from([sim.state.areas[2].id])),
                (financier, BTreeSet::from([sim.state.areas[2].id])),
            ]),
            next_search: BTreeMap::new(),
            last_event: sim.state.next_event - 1,
        });
        // Fresh authored knowledge includes these physically present stocks.
        for person in people {
            let inventories = sim
                .state
                .inventories
                .values()
                .filter(|stock| sim.inventory_area(stock) == Some(area))
                .cloned()
                .collect::<Vec<_>>();
            let mind = sim
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&person)
                .unwrap();
            for stock in inventories {
                mind.knowledge.insert(
                    stock.id,
                    StockKnowledge {
                        inventory: stock.id,
                        area,
                        good: stock.good,
                        quantity: stock.quantity,
                        accessible: stock.access.contains(&person),
                        observed_at: sim.state.time,
                        learned_at: sim.state.time,
                        source: KnowledgeSource::Observed(person),
                    },
                );
            }
        }
        Ok(sim)
    }
    fn project_area(&self, actor: PersonId) -> Option<AreaId> {
        let body = self.state.physical.as_ref()?.actors.get(&actor)?;
        if !body.vitals.alive || body.vitals.health == 0 {
            return None;
        }
        match self
            .state
            .people
            .get(*self.person_index.get(&actor)?)?
            .location
        {
            Location::AtArea(area) => Some(area),
            _ => None,
        }
    }
    fn project_able(&self, actor: PersonId) -> bool {
        self.project_area(actor).is_some()
            && self.independent(actor)
            && !self.combat_restricts(actor)
            && !self.care_index.contains_key(&actor)
            && self.state.development.as_ref().is_none_or(|world| {
                matches!(
                    world.lives[&actor].stage,
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
        self.state.inventories.get(&inventory).is_some_and(|stock| {
            stock.good == good
                && stock.access.contains(&actor)
                && self.inventory_area(stock) == self.project_area(actor)
                && (quantity == 0
                    || self.unreserved_stock_for_planning(inventory, actor, resources) >= quantity)
        })
    }
    pub(crate) fn project_is_known(&self, actor: PersonId, project: ProjectId) -> bool {
        self.state
            .projects
            .as_ref()
            .and_then(|world| world.known_projects.get(&actor))
            .is_some_and(|known| known.contains_key(&project))
    }
    fn project_actor_facts(
        &self,
        actor: PersonId,
        project: Option<ProjectId>,
    ) -> game_projects::ActorFacts {
        game_projects::ActorFacts {
            actor,
            now: self.state.time,
            area: self.project_area(actor),
            capable: self.project_able(actor),
            knows_project: project.is_some_and(|id| self.project_is_known(actor, id)),
        }
    }
    fn construction_facts(
        &self,
        actor: PersonId,
        project: ProjectId,
        resources: Option<&equipment::RankingResources>,
    ) -> Result<game_projects::ConstructionFacts, Error> {
        let world = self.state.projects.as_ref().ok_or(Error::Unavailable)?;
        let plan = world.projects.get(&project).ok_or(Error::Unavailable)?;
        let physical = self.state.physical.as_ref().ok_or(Error::Unavailable)?;
        let mut facts = self.project_actor_facts(actor, Some(project));
        facts.capable &= physical
            .actors
            .get(&actor)
            .is_some_and(|body| body.vitals.fatigue < physical.rules.fatigue_limit);
        Ok(game_projects::ConstructionFacts {
            actor: facts,
            land_accessible: match plan.kind {
                ProjectKind::Workshop => true,
                ProjectKind::Farm { plot, .. } => self
                    .state
                    .ecology
                    .as_ref()
                    .and_then(|w| w.plots.get(&plot))
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
                self.state
                    .projects
                    .as_ref()
                    .unwrap()
                    .prepare_construction(project, facts)
                    .is_ok()
            })
    }
    pub fn debug_project_action(
        &mut self,
        actor: PersonId,
        action: ProjectAction,
    ) -> Result<(), Error> {
        if self.state.projects.is_none() {
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
                        self.state.inventories.get(&id).map(|stock| {
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
                    ProjectAction::ProposeFarm { plot, .. } => {
                        self.state.ecology.as_ref().and_then(|w| w.plots.get(plot))
                    }
                    _ => None,
                };
                self.state.projects.as_ref().unwrap().prepare_proposal(
                    ProjectId(self.state.next_id),
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
                    .state
                    .agreements
                    .as_ref()
                    .and_then(|w| w.agreements.get(agreement))
                    .ok_or(Error::Unavailable)?;
                self.state.projects.as_ref().unwrap().prepare_support(
                    *project,
                    contract,
                    self.project_actor_facts(actor, Some(*project)),
                )?
            }
            ProjectAction::Work { project } => {
                let facts = self.construction_facts(actor, *project, None)?;
                self.state
                    .projects
                    .as_ref()
                    .unwrap()
                    .prepare_construction(*project, facts)?
            }
            ProjectAction::Cancel { project } => self
                .state
                .projects
                .as_ref()
                .unwrap()
                .prepare_cancel(*project, self.project_actor_facts(actor, Some(*project)))?,
            ProjectAction::Operate {
                enterprise,
                agreement,
            } => {
                let contract = self
                    .state
                    .agreements
                    .as_ref()
                    .and_then(|w| w.agreements.get(agreement))
                    .ok_or(Error::Unavailable)?;
                let known = self
                    .state
                    .projects
                    .as_ref()
                    .unwrap()
                    .known_enterprises
                    .get(&actor)
                    .is_some_and(|k| k.contains_key(enterprise));
                self.state.projects.as_ref().unwrap().prepare_operation(
                    *enterprise,
                    contract,
                    self.project_actor_facts(actor, None),
                    known,
                )?
            }
        };
        self.apply_project_change(change)?;
        self.observe_projects();
        self.revision += 1;
        Ok(())
    }
    fn apply_project_change(&mut self, change: game_projects::PreparedChange) -> Result<(), Error> {
        use game_projects::{AssetEffect, Effect};
        let mut next_id = self.state.next_id;
        let mut land = BTreeMap::new();
        let mut consumed = BTreeMap::<InventoryId, u128>::new();
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
                } => *consumed.entry(*source).or_default() += u128::from(quantity.0),
                Effect::Completed {
                    enterprise,
                    site,
                    asset,
                    ..
                } => {
                    match asset {
                        AssetEffect::Workshop(_) => {
                            if site.0 != next_id
                                || self
                                    .state
                                    .physical
                                    .as_ref()
                                    .is_none_or(|w| w.sites.contains_key(site))
                            {
                                return Err(Error::Unavailable);
                            }
                            next_id = next_id.checked_add(1).ok_or(Error::Overflow)?;
                        }
                        AssetEffect::Farm { plot, .. } => {
                            let source = self
                                .state
                                .ecology
                                .as_ref()
                                .and_then(|w| w.plots.get(plot))
                                .ok_or(Error::Unavailable)?
                                .clone();
                            if self
                                .state
                                .agriculture
                                .as_ref()
                                .is_none_or(|w| w.field(&self.state.components, *plot).is_some())
                            {
                                return Err(Error::Unavailable);
                            }
                            land.insert(*plot, source);
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
        if consumed.iter().any(|(id, quantity)| {
            self.state
                .inventories
                .get(id)
                .is_none_or(|s| u128::from(s.quantity.0) < *quantity)
        }) {
            return Err(Error::Unavailable);
        }
        let event_bound = change.effects().iter().try_fold(0usize, |total, effect| {
            let count = match effect {
                Effect::Interrupt { actor } => {
                    if self
                        .state
                        .physical
                        .as_ref()
                        .and_then(|w| w.actors.get(actor))
                        .is_some_and(|body| matches!(body.activity, Activity::Construct { .. }))
                    {
                        0
                    } else {
                        128
                    }
                }
                _ => 1,
            };
            total.checked_add(count).ok_or(Error::Overflow)
        })?;
        self.ensure_event_capacity(event_bound)?;
        let effects = change.commit(self.state.projects.as_mut().ok_or(Error::Unavailable)?)?;
        self.state.next_id = next_id;
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
                Effect::Interrupt { actor } => self.interrupt_actor_activity_except_projects(actor),
                Effect::Consume {
                    actor,
                    source,
                    quantity,
                } => {
                    self.state.inventories.get_mut(&source).unwrap().quantity.0 -= quantity.0;
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
                    self.state
                        .physical
                        .as_mut()
                        .unwrap()
                        .actors
                        .get_mut(&actor)
                        .unwrap()
                        .activity = Activity::Construct { project, completes };
                    self.state
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
                    actor,
                    asset,
                } => {
                    match asset {
                        AssetEffect::Workshop(asset) => {
                            self.state.physical.as_mut().unwrap().sites.insert(
                                site,
                                physical::Site {
                                    id: site,
                                    area: asset.area,
                                    input: asset.input,
                                    input_quantity: asset.input_quantity,
                                    tool: asset.tool,
                                    output: asset.output,
                                    output_quantity: asset.output_quantity,
                                    duration: asset.duration,
                                    access: asset.access,
                                    last_granted: None,
                                },
                            );
                        }
                        AssetEffect::Farm {
                            plot,
                            area,
                            seed,
                            tool,
                            water,
                            food,
                        } => {
                            let source = land.remove(&plot).unwrap();
                            self.state.ecology.as_mut().unwrap().plots.remove(&plot);
                            self.state
                                .agriculture
                                .as_mut()
                                .unwrap()
                                .insert_field(
                                    &mut self.state.components,
                                    agriculture::Field {
                                        id: site,
                                        area,
                                        access: source.access,
                                        custodian: source.custodian,
                                        seed,
                                        tool,
                                        water,
                                        food,
                                    },
                                )
                                .expect("validated component insertion");
                            let world = self.state.agriculture.as_mut().unwrap();
                            world
                                .insert_crop(
                                    &mut self.state.components,
                                    site,
                                    agriculture::Crop::new(world.catalog.default_crop.clone()),
                                )
                                .expect("validated component insertion");
                            world
                                .insert_soil(
                                    &mut self.state.components,
                                    site,
                                    agriculture::Soil {
                                        fertility: source.fertility,
                                        water: source.soil_water,
                                        capacity: source.water_capacity,
                                    },
                                )
                                .expect("validated component insertion");
                        }
                    }
                    self.state
                        .physical
                        .as_mut()
                        .unwrap()
                        .actors
                        .get_mut(&actor)
                        .unwrap()
                        .activity = Activity::Idle;
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
        if let Some(world) = &mut self.state.projects
            && let Ok(change) = world.prepare_interrupt(actor)
        {
            let _ = change.commit(world);
        }
    }
    pub(crate) fn project_boundary(&self, limit: SimTime) -> SimTime {
        self.state
            .projects
            .as_ref()
            .map_or(limit, |world| world.boundary(self.state.time, limit))
    }
    pub(crate) fn project_allocation_bound(&self, next: SimTime) -> Result<u64, Error> {
        self.state
            .projects
            .as_ref()
            .map_or(Ok(0), |world| world.allocation_bound(next))
    }
    pub(crate) fn project_funding_proposal_bound(&self, next: SimTime) -> Result<u64, Error> {
        if self.state.decisions.is_none() || !next.0.is_multiple_of(60_000) {
            return Ok(0);
        }
        self.state
            .projects
            .as_ref()
            .map_or(Ok(0), |world| world.funding_proposal_bound(next))
    }
    pub(crate) fn evaluate_projects(&self, next: SimTime) -> Result<Option<World>, Error> {
        let Some(world) = &self.state.projects else {
            return Ok(None);
        };
        let work = self
            .state
            .physical
            .as_ref()
            .into_iter()
            .flat_map(|w| w.actors.iter())
            .filter_map(|(actor, body)| {
                if let Activity::Work { site } | Activity::Cultivate { field: site, .. } =
                    body.activity
                    && body.vitals.alive
                    && body.vitals.health > 0
                {
                    Some(game_projects::EnterpriseWork {
                        actor: *actor,
                        site,
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let output = world.advance(self.state.time, next, &work)?;
        Ok(Some(output))
    }
    pub(crate) fn commit_projects(&mut self, output: Option<World>) {
        if let Some(output) = output {
            self.state.projects = Some(output);
        }
    }
    pub(crate) fn complete_construction(&mut self, actor: PersonId) {
        let Some(world) = &self.state.projects else {
            return;
        };
        let Some(job) = world.jobs.get(&actor) else {
            return;
        };
        let Some(plan) = world.projects.get(&job.project) else {
            return;
        };
        let asset_available = match plan.kind {
            ProjectKind::Workshop => self.state.physical.is_some(),
            ProjectKind::Farm { plot, .. } => {
                self.state
                    .ecology
                    .as_ref()
                    .is_some_and(|w| w.plots.contains_key(&plot))
                    && self.state.agriculture.is_some()
            }
        };
        let site = SiteId(self.state.next_id);
        let Some(enterprise_id) = self
            .state
            .next_id
            .checked_add(u64::from(matches!(plan.kind, ProjectKind::Workshop)))
        else {
            return;
        };
        let prepared = world.prepare_completion(
            actor,
            game_projects::CompletionFacts {
                now: self.state.time,
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
        self.state
            .projects
            .as_ref()
            .map(|world| world.prepare_input(site, Good::Timber, quantity))
            .transpose()
    }
    pub(crate) fn prepare_farm_input(
        &self,
        site: SiteId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<Option<game_projects::PreparedChange>, Error> {
        self.state
            .projects
            .as_ref()
            .map(|world| world.prepare_input(site, self.state.inventories[&source].good, quantity))
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
        let Some(world) = &self.state.projects else {
            return;
        };
        let projects = world.projects.values().cloned().collect::<Vec<_>>();
        let enterprises = world.enterprises.values().cloned().collect::<Vec<_>>();
        let people = self
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect::<Vec<_>>();
        for actor in people {
            let Some(area) = self.project_area(actor) else {
                continue;
            };
            for project in projects.iter().filter(|project| project.area == area) {
                let view = ProjectView {
                    kind: project.kind.clone(),
                    id: project.id,
                    sponsor: project.sponsor,
                    area,
                    materials: project.materials,
                    tool: project.tool,
                    input: project.input,
                    output: project.output,
                    beneficiaries: project.beneficiaries.clone(),
                    workers: project.workers.clone(),
                    support: project.support.clone(),
                    status: project.status,
                    blocker: project.blocker,
                    progress_ms: project.progress_ms,
                    materials_consumed: project.materials_consumed,
                    finish_by: project.finish_by,
                    enterprise: project.enterprise,
                    observed_at: self.state.time,
                };
                self.state
                    .projects
                    .as_mut()
                    .unwrap()
                    .known_projects
                    .entry(actor)
                    .or_default()
                    .insert(project.id, view);
            }
            for enterprise in enterprises
                .iter()
                .filter(|enterprise| enterprise.area == area)
            {
                // The chosen beneficiaries and contractors can consult the
                // workshop's records while physically present, not from afar.
                let books = enterprise.beneficiaries.contains(&actor)
                    || enterprise.operating_agreements.iter().any(|id| {
                        self.state.agreements.as_ref().unwrap().agreements[id]
                            .parties
                            .contains(&actor)
                    });
                let accounts = books
                    .then(|| self.enterprise_accounts(enterprise))
                    .flatten();
                let view = EnterpriseView {
                    kind: enterprise.kind.clone(),
                    id: enterprise.id,
                    project: enterprise.project,
                    sponsor: enterprise.sponsor,
                    area,
                    site: enterprise.site,
                    beneficiaries: enterprise.beneficiaries.clone(),
                    workers: enterprise.workers.clone(),
                    status: enterprise.status,
                    blocker: enterprise.blocker,
                    accounts,
                    observed_at: self.state.time,
                };
                self.state
                    .projects
                    .as_mut()
                    .unwrap()
                    .known_enterprises
                    .entry(actor)
                    .or_default()
                    .insert(enterprise.id, view);
            }
        }
    }
    fn enterprise_accounts(&self, enterprise: &Enterprise) -> Option<EnterpriseAccountView> {
        let agreements = self.state.agreements.as_ref()?;
        let obligations = enterprise
            .operating_agreements
            .iter()
            .flat_map(|id| agreements.agreements[id].obligations.iter())
            .map(|id| {
                let obligation = &agreements.obligations[id];
                let delivery_good = match obligation.term {
                    AgreementTerm::Delivery { source, .. } => {
                        Some(self.state.inventories[&source].good)
                    }
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
        let Some(world) = &self.state.projects else {
            return ProjectsView::default();
        };
        ProjectsView {
            projects: world
                .known_projects
                .get(&actor)
                .map(|known| known.values().rev().take(32).cloned().collect())
                .unwrap_or_default(),
            enterprises: world
                .known_enterprises
                .get(&actor)
                .map(|known| known.values().rev().take(32).cloned().collect())
                .unwrap_or_default(),
        }
    }
    fn pending_project_outputs(&self) -> Vec<(SiteId, Good, Quantity)> {
        let Some(world) = &self.state.projects else {
            return Vec::new();
        };
        self.state
            .events
            .iter()
            .filter(|e| e.id.0 > world.last_event)
            .flat_map(|event| match event.kind {
                EventKind::Produced {
                    site,
                    output,
                    quantity,
                    ..
                } => vec![(site, self.state.inventories[&output].good, quantity)],
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
        if let Some(world) = &self.state.projects {
            let mut pending = self.pending_project_outputs();
            pending.extend_from_slice(outputs);
            world.prepare_outputs(&pending, None)?;
        }
        Ok(())
    }
    pub(crate) fn update_projects(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.projects else {
            return Ok(());
        };
        let outputs = self.pending_project_outputs();
        let project_facts = world
            .projects
            .values()
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
                usable_worker_tools: world
                    .jobs
                    .iter()
                    .filter(|(actor, job)| {
                        job.project == plan.id
                            && self.project_usable(**actor, plan.tool, Good::Tool, 1_000)
                    })
                    .map(|(actor, _)| *actor)
                    .collect(),
            })
            .collect::<Vec<_>>();
        let enterprise_facts=world.enterprises.values().map(|asset| {
            let (active,crop_failed,output_room,required_input,required_output)=match asset.kind {
                ProjectKind::Farm {..}=>{
                    (self.state.agriculture.as_ref().unwrap().jobs.values().any(|j|j.field==asset.site),self.state.agriculture.as_ref().unwrap().crop(&self.state.components, asset.site).expect("component exists").phase==agriculture::CropPhase::Failed,Quantity(0),Quantity(0),Quantity(0))
                }
                ProjectKind::Workshop=>{
                    let site=&self.state.physical.as_ref().unwrap().sites[&asset.site];
                    let output=&self.state.inventories[&site.output];
                    (self.state.physical.as_ref().unwrap().actors.values().any(|body|matches!(body.activity,Activity::Work {site:id} if id==site.id)),false,
                        Quantity(output.capacity.0.saturating_sub(output.quantity.0)),site.input_quantity,site.output_quantity)
                }
            };
            let workers=asset.workers.iter().map(|actor| {
                let (land_accessible,tool_available,materials_available)=match asset.kind {
                    ProjectKind::Farm {..}=>(self.state.agriculture.as_ref().unwrap().field(&self.state.components, asset.site).expect("component exists").access.contains(actor),false,Quantity(0)),
                    ProjectKind::Workshop=>{
                        let site=&self.state.physical.as_ref().unwrap().sites[&asset.site];
                        (true,self.project_usable(*actor,site.tool,Good::Tool,1_000),if self.project_usable(*actor,site.input,Good::Timber,0) {Quantity(self.unreserved_stock(site.input,*actor))} else {Quantity(0)})
                    }
                };
                game_projects::WorkerCondition {capable:self.project_able(*actor),land_accessible,tool_available,materials_available}
            }).collect();
            game_projects::EnterpriseCondition {enterprise:asset.id,active,workers,crop_failed,output_room,required_input,required_output}
        }).collect::<Vec<_>>();
        let change = world.prepare_reconcile(
            self.state.time,
            &project_facts,
            &enterprise_facts,
            &outputs,
            self.state.next_event - 1,
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
        let Some(world) = &self.state.projects else {
            return Ok(());
        };
        let plans = world
            .projects
            .values()
            .filter(|p| {
                matches!(p.kind, ProjectKind::Farm { .. })
                    && !p.materials_consumed
                    && !matches!(p.status, ProjectStatus::Failed | ProjectStatus::Cancelled)
            })
            .cloned()
            .collect::<Vec<_>>();
        for plan in plans {
            let actor = plan.sponsor;
            if actor == self.state.ruler
                || !self.project_able(actor)
                || !self.project_is_known(actor, plan.id)
                || self.project_usable(
                    actor,
                    plan.materials,
                    Good::Timber,
                    self.state
                        .projects
                        .as_ref()
                        .unwrap()
                        .rules
                        .construction_timber
                        .0,
                )
                || self
                    .state
                    .projects
                    .as_ref()
                    .unwrap()
                    .next_search
                    .get(&actor)
                    .is_some_and(|t| *t > self.state.time)
                || plan.support.iter().any(|id| {
                    matches!(
                        self.state.agreements.as_ref().unwrap().agreements[id].status,
                        AgreementStatus::Proposed
                            | AgreementStatus::Active
                            | AgreementStatus::Breached
                    )
                })
            {
                continue;
            }
            let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
            if mind.dream != decisions::Aspiration::Independence || mind.personality.ambition < 600
            {
                continue;
            }
            let required = self
                .state
                .projects
                .as_ref()
                .unwrap()
                .rules
                .construction_timber;
            let offer = mind
                .knowledge
                .values()
                .filter(|f| {
                    f.good == Good::Timber
                        && f.area == plan.area
                        && f.quantity >= required
                        && f.inventory != plan.materials
                })
                .find_map(|f| {
                    self.state
                        .agreements
                        .as_ref()
                        .unwrap()
                        .custodians
                        .get(&f.inventory)
                        .copied()
                        .filter(|p| *p != actor && self.project_area(*p) == Some(plan.area))
                        .map(|p| (p, f.inventory))
                });
            let Some((supporter, source)) = offer else {
                continue;
            };
            let destination = mind
                .knowledge
                .values()
                .filter(|f| {
                    f.good == Good::Food && f.area == plan.area && f.inventory != plan.output
                })
                .find(|f| {
                    self.state
                        .agreements
                        .as_ref()
                        .unwrap()
                        .custodians
                        .get(&f.inventory)
                        == Some(&supporter)
                })
                .map(|f| f.inventory);
            let Some(destination) = destination else {
                continue;
            };
            let calendar = &self.state.environment.as_ref().unwrap().calendar;
            let game_projects::FundingChoice::Available(funding) = plan.prepare_farm_funding(
                &self.state.projects.as_ref().unwrap().rules,
                game_projects::FundingFacts {
                    now: self.state.time,
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
            let before = self.state.next_id;
            self.agreement_command(
                actor,
                &AgreementAction::Propose {
                    parties: funding.parties,
                    terms: funding.terms,
                    procedure: None,
                },
            )?;
            let agreement = self
                .state
                .agreements
                .as_ref()
                .unwrap()
                .agreements
                .values()
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
            self.state
                .projects
                .as_mut()
                .unwrap()
                .next_search
                .insert(actor, funding.next_search);
        }
        Ok(())
    }
    fn seek_workshop_support(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.projects else {
            return Ok(());
        };
        let plans = world
            .projects
            .values()
            .filter(|plan| {
                plan.kind == ProjectKind::Workshop
                    && plan.support.iter().all(|id| {
                        matches!(
                            self.state.agreements.as_ref().unwrap().agreements[id].status,
                            AgreementStatus::Fulfilled
                                | AgreementStatus::Rejected
                                | AgreementStatus::Withdrawn
                        )
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        for plan in plans {
            if plan.sponsor == self.state.ruler
                || !self.project_able(plan.sponsor)
                || matches!(
                    plan.status,
                    ProjectStatus::Failed | ProjectStatus::Cancelled
                )
            {
                continue;
            }
            let actor = plan.sponsor;
            let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
            if mind.dream != decisions::Aspiration::Independence || mind.personality.ambition < 600
            {
                continue;
            }
            if self
                .state
                .projects
                .as_ref()
                .unwrap()
                .next_search
                .get(&actor)
                .is_some_and(|time| *time > self.state.time)
            {
                continue;
            }
            let Ok(needed) =
                plan.workshop_funding_materials(&self.state.projects.as_ref().unwrap().rules)
            else {
                continue;
            };
            if self.state.inventories[&plan.materials].quantity >= needed {
                continue;
            }
            let donor = mind
                .knowledge
                .values()
                .filter(|fact| {
                    fact.good == Good::Timber
                        && fact.area == plan.area
                        && fact.inventory != plan.materials
                        && fact.quantity >= needed
                })
                .find_map(|fact| {
                    self.state
                        .agreements
                        .as_ref()
                        .unwrap()
                        .custodians
                        .get(&fact.inventory)
                        .filter(|owner| {
                            **owner != actor && self.project_area(**owner) == Some(plan.area)
                        })
                        .map(|owner| (*owner, fact.inventory))
                });
            let Some((financier, source)) = donor else {
                continue;
            };
            let recipient = self
                .state
                .inventories
                .values()
                .find(|stock| {
                    stock.good == Good::Tool
                        && stock.location == InventoryLocation::Area(plan.area)
                        && stock.id != plan.tool
                        && stock.id != plan.output
                        && self
                            .state
                            .agreements
                            .as_ref()
                            .unwrap()
                            .custodians
                            .get(&stock.id)
                            == Some(&financier)
                })
                .map(|stock| stock.id);
            let Some(destination) = recipient else {
                continue;
            };
            let game_projects::FundingChoice::Available(funding) = plan.prepare_workshop_funding(
                &self.state.projects.as_ref().unwrap().rules,
                game_projects::FundingFacts {
                    now: self.state.time,
                    financier,
                    source,
                    repayment_destination: destination,
                },
                self.state.physical.as_ref().unwrap().rules.loading_time,
            ) else {
                continue;
            };
            let before = self.state.next_id;
            self.debug_agreement_action(
                actor,
                AgreementAction::Propose {
                    parties: funding.parties,
                    terms: funding.terms,
                    procedure: None,
                },
            )?;
            let agreement = self
                .state
                .agreements
                .as_ref()
                .unwrap()
                .agreements
                .values()
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
            self.state
                .projects
                .as_mut()
                .unwrap()
                .next_search
                .insert(actor, funding.next_search);
        }
        Ok(())
    }
    fn workshop_procurement(
        &self,
        actor: PersonId,
        mind: &decisions::Mind,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<Command> {
        use game_logistics::{
            AcquisitionOffer, Delivery, Destination, OfferYield, Requirement, Step,
        };
        let world = self.state.projects.as_ref()?;
        let plan = world
            .known_projects
            .get(&actor)?
            .values()
            .rev()
            .find(|plan| {
                plan.kind == ProjectKind::Workshop
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
            world.rules.recipe_timber.0
        } else {
            world.rules.construction_timber.0
        };
        let fact = mind.knowledge.get(&target)?;
        let destination = self.delivery_known_inventory_destination(actor, target)?;
        let available = if destination.observed_at == self.state.time
            && destination.location == self.delivery_observed_location(actor)
        {
            self.state.inventories[&target].quantity.0
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
        let hints = world
            .timber_hints
            .get(&actor)
            .into_iter()
            .flatten()
            .filter(|area| {
                !mind
                    .knowledge
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
        let Some(world) = self.state.projects.as_ref() else {
            return Vec::new();
        };
        if !self.project_able(actor) {
            return Vec::new();
        }
        let Some(mind) = self
            .state
            .decisions
            .as_ref()
            .and_then(|world| world.minds.get(&actor))
        else {
            return Vec::new();
        };
        let mut opportunities = Vec::new();
        for plan in world
            .projects
            .values()
            .rev()
            .filter(|plan| plan.workers.contains(&actor) && self.project_is_known(actor, plan.id))
            .take(8)
        {
            if self
                .construction_facts(actor, plan.id, resources)
                .is_ok_and(|facts| world.prepare_construction(plan.id, facts).is_ok())
            {
                opportunities.push(Command::Project {
                    action: ProjectAction::Work { project: plan.id },
                });
            }
        }
        if let Some(command) = self.workshop_procurement(actor, mind, resources) {
            opportunities.push(command);
        }
        for workshop in world.enterprises.values().filter(|workshop| {
            workshop.sponsor == actor && workshop.area == self.project_area(actor).unwrap()
        }) {
            if matches!(workshop.kind, ProjectKind::Farm { .. }) {
                if let Some(command) =
                    self.autonomous_field_cultivation(actor, workshop.site, resources)
                {
                    opportunities.push(command);
                }
                continue;
            }
            let site = &self.state.physical.as_ref().unwrap().sites[&workshop.site];
            if self.project_usable_for_planning(
                actor,
                site.input,
                Good::Timber,
                site.input_quantity.0,
                resources,
            ) && self.project_usable_for_planning(actor, site.tool, Good::Tool, 1_000, resources)
                && self.state.inventories[&site.output].capacity.0
                    - self.state.inventories[&site.output].quantity.0
                    >= site.output_quantity.0
            {
                opportunities.push(Command::Work { site: site.id });
            }
        }
        if world.known_projects.get(&actor).is_some_and(|known| {
            known.values().any(|project| {
                project.sponsor == actor
                    && !matches!(
                        project.status,
                        ProjectStatus::Cancelled | ProjectStatus::Failed
                    )
            })
        }) || world
            .next_search
            .get(&actor)
            .is_some_and(|time| *time > self.state.time)
        {
            return opportunities;
        }
        let area = self.project_area(actor).unwrap();
        let own = |fact: &&StockKnowledge| {
            fact.area == area
                && self
                    .state
                    .inventories
                    .get(&fact.inventory)
                    .is_some_and(|stock| stock.location == InventoryLocation::Area(area))
                && self
                    .state
                    .agreements
                    .as_ref()
                    .unwrap()
                    .custodians
                    .get(&fact.inventory)
                    == Some(&actor)
        };
        let Some(materials) = mind
            .knowledge
            .values()
            .filter(own)
            .find(|fact| fact.good == Good::Timber && fact.accessible)
            .map(|fact| fact.inventory)
        else {
            return opportunities;
        };
        let Some(tool) = mind
            .knowledge
            .values()
            .filter(own)
            .find(|fact| fact.good == Good::Tool && fact.accessible && fact.quantity.0 >= 1_000)
            .map(|fact| fact.inventory)
        else {
            return opportunities;
        };
        if let Some(plot) = self
            .state
            .ecology
            .as_ref()
            .and_then(|w| w.known_plots.get(&actor))
            .and_then(|k| k.values().find(|p| p.area == area && p.accessible))
        {
            let stock = |good| {
                mind.knowledge
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
                            self.state.time.0.saturating_add(
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
        let Some(output) = mind
            .knowledge
            .values()
            .filter(own)
            .find(|fact| {
                fact.good == Good::Tool
                    && fact.inventory != tool
                    && fact.accessible
                    && fact.quantity.0 == 0
            })
            .map(|fact| fact.inventory)
        else {
            return opportunities;
        };
        let expected = world
            .rules
            .construction_labor
            .0
            .saturating_mul(4)
            .saturating_add(
                self.state
                    .physical
                    .as_ref()
                    .unwrap()
                    .rules
                    .loading_time
                    .0
                    .saturating_mul(8),
            )
            .saturating_add(120_000);
        let Some(finish_by) = self.state.time.0.checked_add(expected).map(SimTime) else {
            return opportunities;
        };
        opportunities.push(Command::Project {
            action: ProjectAction::Propose {
                area,
                materials,
                tool,
                input: materials,
                output,
                beneficiaries: BTreeSet::from([actor]),
                workers: BTreeSet::from([actor]),
                finish_by,
            },
        });
        opportunities
    }
    /// An individual's own blocked livelihood plan can make a first tool
    /// valuable. This is goal utility with risk, not a universal tool price.
    pub(crate) fn project_financing_utility(
        &self,
        actor: PersonId,
        terms: &[AgreementTerm],
    ) -> i64 {
        let Some(world) = &self.state.projects else {
            return 0;
        };
        let Some(mind) = self
            .state
            .decisions
            .as_ref()
            .and_then(|world| world.minds.get(&actor))
        else {
            return 0;
        };
        if mind.dream != decisions::Aspiration::Independence || mind.personality.ambition < 600 {
            return 0;
        }
        let own = |fact: &&StockKnowledge| {
            fact.accessible
                && self
                    .state
                    .agreements
                    .as_ref()
                    .unwrap()
                    .custodians
                    .get(&fact.inventory)
                    == Some(&actor)
        };
        if mind
            .knowledge
            .values()
            .filter(own)
            .any(|fact| fact.good == Good::Tool && fact.quantity.0 >= 1_000)
        {
            return 0;
        }
        if !mind
            .knowledge
            .values()
            .filter(own)
            .any(|fact| fact.good == Good::Tool && fact.quantity.0 == 0)
        {
            return 0;
        }
        for term in terms {
            let AgreementTerm::Delivery {
                creditor,
                source,
                quantity,
                deadline,
                ..
            } = *term
            else {
                continue;
            };
            if creditor != actor || quantity.0 < 1_000 {
                continue;
            }
            let plan = world
                .known_projects
                .get(&actor)
                .and_then(|known| known.values().find(|plan| plan.output == source));
            let Some(plan) = plan else {
                continue;
            };
            let finance = terms.iter().find_map(|term| {
                if let AgreementTerm::Delivery {
                    debtor,
                    creditor,
                    source,
                    destination,
                    quantity,
                    ..
                } = *term
                {
                    (debtor == actor && creditor == plan.sponsor && destination == plan.materials)
                        .then_some((source, quantity.0))
                } else {
                    None
                }
            });
            let Some((raw, pledged)) = finance else {
                continue;
            };
            let Some(known) = mind.knowledge.get(&raw) else {
                continue;
            };
            if known.good != Good::Timber
                || !known.accessible
                || known.quantity.0.saturating_sub(pledged) < world.rules.construction_timber.0
            {
                continue;
            }
            // The observed blueprint must be feasible under the proposed
            // supplies and time; actual delivery and completion remain uncertain.
            let output_batches = quantity.0.div_ceil(world.rules.recipe_tools.0);
            let Some(required) = output_batches
                .checked_mul(world.rules.recipe_timber.0)
                .and_then(|raw| {
                    raw.checked_add(if plan.materials_consumed {
                        0
                    } else {
                        world.rules.construction_timber.0
                    })
                })
            else {
                continue;
            };
            let minimum_time = world
                .rules
                .construction_labor
                .0
                .saturating_sub(plan.progress_ms)
                .saturating_add(output_batches.saturating_mul(world.rules.recipe_time.0))
                .saturating_add(240_000);
            if pledged < required
                || deadline.0.saturating_sub(self.state.time.0) < minimum_time
                || mind
                    .knowledge
                    .get(&plan.tool)
                    .is_none_or(|tool| tool.good != Good::Tool || tool.quantity.0 < 1_000)
            {
                continue;
            }
            let blocked = !mind.sites.values().any(|site| site.access);
            let autonomy = i64::from(mind.personality.ambition) * if blocked { 5 } else { 2 };
            return autonomy * i64::from(mind.personality.risk_tolerance) / 1_000;
        }
        0
    }

    pub(crate) fn available_project_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(world) = &self.state.projects else {
            return Vec::new();
        };
        if !self.project_able(actor) {
            return Vec::new();
        }
        let mut actions = Vec::new();
        for plan in world
            .projects
            .values()
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
    let Some(world) = &state.projects else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid workshop project or enterprise".into());
    let people = state
        .people
        .iter()
        .map(|person| (person.id, person))
        .collect::<BTreeMap<_, _>>();
    let physical = state.physical.as_ref().ok_or_else(fail)?;
    let agreements = state.agreements.as_ref().ok_or_else(fail)?;
    let rules = &world.rules;
    if state.decisions.is_none()
        || world.last_event >= state.next_event
        || world.timber_hints.iter().any(|(actor, areas)| {
            !people.contains_key(actor)
                || areas
                    .iter()
                    .any(|area| !state.areas.iter().any(|known| known.id == *area))
        })
        || world
            .next_search
            .keys()
            .any(|actor| !people.contains_key(actor))
    {
        return Err(fail());
    }
    world.validate_owned(state.time).map_err(|_| fail())?;
    for (id, plan) in &world.projects {
        if *id != plan.id
            || id.0 >= state.next_id
            || !people.contains_key(&plan.sponsor)
            || !state.areas.iter().any(|area| area.id == plan.area)
            || plan
                .beneficiaries
                .iter()
                .chain(&plan.workers)
                .chain(plan.contributions.keys())
                .any(|actor| !people.contains_key(actor))
            || plan.support.iter().any(|id| {
                agreements
                    .agreements
                    .get(id)
                    .is_none_or(|agreement| !agreement.parties.contains(&plan.sponsor))
            })
        {
            return Err(fail());
        }
        for (inventory, good) in [
            (plan.materials, Good::Timber),
            (plan.tool, Good::Tool),
            (
                plan.input,
                if matches!(plan.kind, ProjectKind::Farm { .. }) {
                    Good::Seed
                } else {
                    Good::Timber
                },
            ),
            (
                plan.output,
                if matches!(plan.kind, ProjectKind::Farm { .. }) {
                    Good::Food
                } else {
                    Good::Tool
                },
            ),
        ] {
            if state.inventories.get(&inventory).is_none_or(|stock| {
                stock.good != good || stock.location != InventoryLocation::Area(plan.area)
            }) {
                return Err(fail());
            }
        }
        if let ProjectKind::Farm { plot, water } = plan.kind
            && (state.inventories.get(&water).is_none_or(|i| {
                i.good != Good::Water || i.location != InventoryLocation::Area(plan.area)
            }) || (plan.enterprise.is_none()
                && state
                    .ecology
                    .as_ref()
                    .and_then(|w| w.plots.get(&plot))
                    .is_none_or(|p| p.area != plan.area)))
        {
            return Err(fail());
        }
        if plan.materials_consumed&&!state.events.iter().any(|event|matches!(event.kind,EventKind::ConstructionStarted {project,..} if project==*id)) {return Err(fail());}
    }
    for (actor, job) in &world.jobs {
        let plan = world.projects.get(&job.project).ok_or_else(fail)?;
        if !people.contains_key(actor)
            || people[actor].location != Location::AtArea(plan.area)
            || physical.actors.get(actor).is_none_or(|body| {
                !body.vitals.alive
                    || body.activity
                        != (Activity::Construct {
                            project: job.project,
                            completes: job.completes,
                        })
            })
            || !state.due.get(&job.completes).is_some_and(|events| {
                events
                    .iter()
                    .any(|event| matches!(event,Due::Physical(id) if id==actor))
            })
        {
            return Err(fail());
        }
    }
    for (actor, body) in &physical.actors {
        if matches!(body.activity, Activity::Construct { .. }) && !world.jobs.contains_key(actor) {
            return Err(fail());
        }
    }
    for (id, enterprise) in &world.enterprises {
        let plan = world.projects.get(&enterprise.project).ok_or_else(fail)?;
        if let ProjectKind::Farm { plot, water } = plan.kind {
            let field = state
                .agriculture
                .as_ref()
                .and_then(|w| w.field(&state.components, plot))
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
            for event in state.events.iter().filter(|e| e.id.0 <= world.last_event) {
                if let EventKind::CropHarvested {
                    field: site,
                    food,
                    seed,
                    ..
                } = event.kind
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
                    agreements
                        .agreements
                        .get(id)
                        .is_none_or(|a| !a.parties.contains(&enterprise.sponsor))
                })
            {
                return Err(fail());
            }
            continue;
        }
        let site = physical.sites.get(&enterprise.site).ok_or_else(fail)?;
        if *id != enterprise.id
            || id.0 >= state.next_id
            || site.area != plan.area
            || site.input != plan.input
            || site.tool != plan.tool
            || site.output != plan.output
            || site.input_quantity != rules.recipe_timber
            || site.output_quantity != rules.recipe_tools
            || site.duration != rules.recipe_time
            || enterprise
                .labor
                .keys()
                .any(|actor| !people.contains_key(actor))
            || enterprise.operating_agreements.iter().any(|id| {
                agreements
                    .agreements
                    .get(id)
                    .is_none_or(|agreement| !agreement.parties.contains(&enterprise.sponsor))
            })
        {
            return Err(fail());
        }
        let produced = state
            .events
            .iter()
            .filter(|event| event.id.0 <= world.last_event)
            .filter_map(|event| match event.kind {
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
            .filter_map(|event| match event.kind {
                EventKind::EnterpriseInputUsed {
                    enterprise: target,
                    site,
                    quantity,
                } if target == *id && site == enterprise.site => Some(quantity.0),
                _ => None,
            })
            .try_fold(0_u64, |total, value| total.checked_add(value))
            .ok_or_else(fail)?;
        if enterprise.tools_produced != produced || enterprise.timber_consumed != used {
            return Err(fail());
        }
    }
    for (actor, known) in &world.known_projects {
        if !people.contains_key(actor) {
            return Err(fail());
        }
        for (id, view) in known {
            let plan = world.projects.get(id).ok_or_else(fail)?;
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
    for (actor, known) in &world.known_enterprises {
        if !people.contains_key(actor) {
            return Err(fail());
        }
        for (id, view) in known {
            let enterprise = world.enterprises.get(id).ok_or_else(fail)?;
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
                }) || accounts.timber_consumed > enterprise.timber_consumed
                    || accounts.tools_produced > enterprise.tools_produced
                    || !accounts
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
        let environment = sim.state.environment.as_mut().unwrap();
        environment.calendar.day = SimDuration(600_000);
        environment.climate.seasonal_temperature = [20; 4];
        environment.climate.temperature_variation = 0;
        environment.climate.seasonal_rain = [30; 4];
        environment.climate.rainfall_variation = 0;
        environment
            .reset_epoch(sim.state.time, sim.state.seed)
            .unwrap();
        let agriculture = sim.state.agriculture.as_mut().unwrap();
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
        if sim
            .state
            .projects
            .as_ref()
            .unwrap()
            .projects
            .values()
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
    fn valid_farm_with_unrepresentable_harvest_promise_declines_funding_without_mutation() {
        let mut sim = fixture();
        let farmer = sim.state.people[3].id;
        ensure_unfunded_proposal(&mut sim, farmer, true);
        let saved = sim.save().unwrap();
        let mut representable = Simulation::load(&saved, Execution::default()).unwrap();
        let agreements_before = representable
            .state
            .agreements
            .as_ref()
            .unwrap()
            .agreements
            .len();
        representable.seek_farm_support().unwrap();
        assert!(
            representable
                .state
                .agreements
                .as_ref()
                .unwrap()
                .agreements
                .len()
                > agreements_before
        );

        let environment = sim.state.environment.as_ref().unwrap();
        let mut calendar = environment.calendar.clone();
        calendar.day = SimDuration(u64::MAX / calendar.year_days());
        sim.state.environment = Some(
            game_environment::World::new(
                game_environment::Config {
                    calendar,
                    climate: environment.climate.clone(),
                },
                sim.state.time,
                sim.state.seed,
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
            sim.state.physical.as_ref().unwrap().actors[&farmer]
                .vitals
                .alive
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }

    #[test]
    fn valid_workshop_with_unrepresentable_financing_windows_declines_before_publication() {
        let mut sim = Simulation::project_fixture(73, Execution::default()).unwrap();
        let sponsor = sim.state.people[3].id;
        ensure_unfunded_proposal(&mut sim, sponsor, false);
        let saved = sim.save().unwrap();
        let mut representable = Simulation::load(&saved, Execution::default()).unwrap();
        let agreements_before = representable
            .state
            .agreements
            .as_ref()
            .unwrap()
            .agreements
            .len();
        representable.seek_workshop_support().unwrap();
        assert!(
            representable
                .state
                .agreements
                .as_ref()
                .unwrap()
                .agreements
                .len()
                > agreements_before
        );

        sim.state.physical.as_mut().unwrap().rules.loading_time = SimDuration(u64::MAX / 10);
        let mut sim = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        let before = sim.save().unwrap();
        sim.seek_workshop_support().unwrap();
        assert_eq!(sim.save().unwrap(), before);
    }
    #[test]
    fn citizen_bargains_for_actual_timber_constructs_farmland_and_accrues_harvest_rent() {
        let mut sim = fixture();
        let farmer = sim.state.people[3].id;
        sim.advance(SimDuration(2_400_000)).unwrap();
        let project = sim
            .state
            .projects
            .as_ref()
            .unwrap()
            .projects
            .values()
            .find(|p| p.sponsor == farmer && matches!(p.kind, ProjectKind::Farm { .. }))
            .unwrap();
        assert_eq!(project.status, ProjectStatus::Completed);
        let enterprise =
            &sim.state.projects.as_ref().unwrap().enterprises[&project.enterprise.unwrap()];
        assert!(enterprise.produced.get(&Good::Food).copied().unwrap_or(0) > 0);
        assert!(enterprise.labor.get(&farmer).copied().unwrap_or(0) > 0);
        let field = enterprise.site;
        assert!(
            sim.state
                .agriculture
                .as_ref()
                .unwrap()
                .field(&sim.state.components, field)
                .is_some()
        );
        assert!(
            !sim.state
                .ecology
                .as_ref()
                .unwrap()
                .plots
                .contains_key(&field)
        );
        assert_eq!(
            sim.state
                .agriculture
                .as_ref()
                .unwrap()
                .field(&sim.state.components, field)
                .expect("component exists")
                .custodian,
            Some(farmer)
        );
        let contract = &sim.state.agreements.as_ref().unwrap().agreements
            [project.support.iter().next().unwrap()];
        assert_eq!(contract.accepted, contract.parties);
        let obligations = &sim.state.agreements.as_ref().unwrap().obligations;
        assert!(contract.obligations.iter().any(|id| matches!(
            obligations[id].term,
            AgreementTerm::Delivery { .. }
        ) && obligations[id].performed == 5_000));
        assert!(contract.obligations.iter().any(|id| matches!(
            obligations[id].term,
            AgreementTerm::OutputShare { .. }
        ) && obligations[id].accrued > 0
            && obligations[id].performed > 0));
        validate(&sim.state).unwrap();
        sim.validate_environment().unwrap();
        agreements::validate(&sim.state).unwrap();
    }
    #[test]
    fn cautious_supporter_refuses_future_harvest_and_no_promised_material_appears() {
        let mut sim = fixture();
        let supporter = sim.state.people[5].id;
        let farmer = sim.state.people[3].id;
        let mut personality = decisions::Personality::restful();
        personality.risk_tolerance = 0;
        personality.empathy = 0;
        sim.debug_set_personality(supporter, personality, decisions::Aspiration::Comfort)
            .unwrap();
        sim.advance(SimDuration(600_000)).unwrap();
        let project = sim
            .state
            .projects
            .as_ref()
            .unwrap()
            .projects
            .values()
            .find(|p| p.sponsor == farmer)
            .unwrap();
        assert!(project.enterprise.is_none());
        assert!(!project.materials_consumed);
        assert_eq!(
            sim.state.inventories[&project.materials].quantity,
            Quantity(0)
        );
        assert!(
            sim.state
                .agreements
                .as_ref()
                .unwrap()
                .agreements
                .values()
                .any(|a| a.proposer == farmer && a.status == AgreementStatus::Rejected)
        );
    }
    #[test]
    fn share_remainders_and_debt_survive_end_and_need_real_delivery() {
        let mut sim = Simulation::project_fixture(69, Execution::default()).unwrap();
        for actor in sim.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
            sim.debug_actor_command(actor, Command::Cancel).unwrap();
            sim.debug_set_personality(
                actor,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let debtor = sim.state.ruler;
        let creditor = sim.state.people[5].id;
        let site = *sim
            .state
            .physical
            .as_ref()
            .unwrap()
            .sites
            .keys()
            .next()
            .unwrap_or(&SiteId(0));
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
            SiteId(allocate(&mut sim.state).unwrap())
        } else {
            site
        };
        sim.state.physical.as_mut().unwrap().sites.insert(
            site,
            physical::Site {
                id: site,
                area: AreaId(1),
                input,
                input_quantity: Quantity(1_000),
                tool,
                output,
                output_quantity: Quantity(1),
                duration: MINUTE,
                access: BTreeSet::from([debtor]),
                last_granted: None,
            },
        );
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
        let agreement = *sim
            .state
            .agreements
            .as_ref()
            .unwrap()
            .agreements
            .keys()
            .last()
            .unwrap();
        sim.debug_agreement_action(creditor, AgreementAction::Accept { agreement })
            .unwrap();
        let obligation =
            sim.state.agreements.as_ref().unwrap().agreements[&agreement].obligations[0];
        for _ in 0..3 {
            sim.debug_actor_command(debtor, Command::Work { site })
                .unwrap();
            sim.advance(SimDuration(120_000)).unwrap();
        }
        assert_eq!(
            sim.state.agreements.as_ref().unwrap().obligations[&obligation].accrued,
            1
        );
        assert_eq!(
            sim.state.agreements.as_ref().unwrap().obligations[&obligation].share_remainder,
            0
        );
        sim.advance(SimDuration(480_000)).unwrap();
        assert_eq!(
            sim.state.agreements.as_ref().unwrap().obligations[&obligation].performed,
            0
        );
        assert!(sim.state.agreements.as_ref().unwrap().obligations[&obligation].defaulted);
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
            sim.state.agreements.as_ref().unwrap().obligations[&obligation].performed,
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
            sim.state.agreements.as_ref().unwrap().obligations[&obligation].performed,
            1
        );
        assert_eq!(
            sim.state.agreements.as_ref().unwrap().agreements[&agreement].status,
            AgreementStatus::Fulfilled
        );
        assert_eq!(sim.state.inventories[&destination].quantity, Quantity(1));
    }
}

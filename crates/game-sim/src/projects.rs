//! A concrete wooden-tool workshop: support promises require real delivery,
//! construction consumes timber and time, and the completed site uses recipes.
use crate::*;
pub use game_economy::projects::{Construction, Enterprise, Project, Rules, World};

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
            ledger: game_economy::agreements::State {
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
                decisions::Dream::Comfort,
            )?;
        }
        let mut personality = decisions::Personality::restful();
        personality.ambition = 900;
        personality.discipline = 700;
        personality.honesty = 800;
        personality.risk_tolerance = 700;
        sim.debug_set_personality(farmer, personality, decisions::Dream::Independence)?;
        let mut willing = decisions::Personality::restful();
        willing.empathy = 900;
        willing.discipline = 800;
        willing.honesty = 800;
        willing.hedonism = 900;
        willing.ambition = 900;
        willing.risk_tolerance = 700;
        sim.debug_set_personality(supporter, willing, decisions::Dream::SupportOthers)?;
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
                decisions::Dream::Comfort,
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
            decisions::Dream::Independence,
        )?;
        let mut investor = decisions::Personality::industrious();
        investor.risk_tolerance = 850;
        sim.debug_set_personality(financier, investor, decisions::Dream::Independence)?;
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
        self.state.inventories.get(&inventory).is_some_and(|stock| {
            stock.good == good
                && stock.access.contains(&actor)
                && self.inventory_area(stock) == self.project_area(actor)
                && self.unreserved_stock(inventory, actor) >= quantity
        })
    }
    pub(crate) fn project_is_known(&self, actor: PersonId, project: ProjectId) -> bool {
        self.state
            .projects
            .as_ref()
            .and_then(|world| world.known_projects.get(&actor))
            .is_some_and(|known| known.contains_key(&project))
    }
    pub(crate) fn construction_possible(&self, actor: PersonId, project: ProjectId) -> bool {
        let Some(world) = &self.state.projects else {
            return false;
        };
        let Some(project) = world.projects.get(&project) else {
            return false;
        };
        self.project_able(actor)
            && self.project_is_known(actor, project.id)
            && self.project_area(actor) == Some(project.area)
            && project.workers.contains(&actor)
            && match project.kind {
                ProjectKind::Workshop => true,
                ProjectKind::Farm { plot, .. } => self
                    .state
                    .ecology
                    .as_ref()
                    .and_then(|w| w.plots.get(&plot))
                    .is_some_and(|p| p.access.contains(&actor)),
            }
            && (project.materials_consumed
                || world
                    .projects
                    .values()
                    .filter(|other| other.area == project.area && other.materials_consumed)
                    .count()
                    < world.rules.workshops_per_area)
            && !matches!(
                project.status,
                ProjectStatus::Completed | ProjectStatus::Failed | ProjectStatus::Cancelled
            )
            && self.state.time < project.finish_by
            && self
                .state
                .time
                .0
                .checked_add(world.rules.construction_labor.0 - project.progress_ms)
                .is_some_and(|time| time <= project.finish_by.0)
            && self.state.physical.as_ref().unwrap().actors[&actor]
                .vitals
                .fatigue
                < self.state.physical.as_ref().unwrap().rules.fatigue_limit
            && !world
                .jobs
                .iter()
                .any(|(worker, job)| *worker != actor && job.project == project.id)
            && self.project_usable(actor, project.tool, Good::Tool, 1_000)
            && self.tool_available_for(actor, project.tool)
            && (project.materials_consumed
                || self.project_usable(
                    actor,
                    project.materials,
                    Good::Timber,
                    world.rules.construction_timber.0,
                ))
    }
    pub fn debug_project_action(
        &mut self,
        actor: PersonId,
        action: ProjectAction,
    ) -> Result<(), Error> {
        if self.state.projects.is_none() || !self.project_able(actor) {
            return Err(Error::Unavailable);
        }
        let area = self.project_area(actor).unwrap();
        match action {
            ProjectAction::ProposeFarm {
                plot,
                materials,
                tool,
                seed,
                water,
                food,
                beneficiaries,
                workers,
                finish_by,
            } => {
                let land = self
                    .state
                    .ecology
                    .as_ref()
                    .and_then(|w| w.plots.get(&plot))
                    .ok_or(Error::Unavailable)?;
                if land.area != area
                    || !land.access.contains(&actor)
                    || finish_by <= self.state.time
                    || beneficiaries.is_empty()
                    || !beneficiaries.contains(&actor)
                    || !workers.contains(&actor)
                    || beneficiaries.len() > 8
                    || workers.len() > 8
                    || beneficiaries
                        .iter()
                        .chain(&workers)
                        .any(|id| !self.person_index.contains_key(id))
                    || self
                        .state
                        .projects
                        .as_ref()
                        .unwrap()
                        .projects
                        .values()
                        .any(|p| {
                            matches!(p.kind,ProjectKind::Farm {plot:id,..} if id==plot)
                                && !matches!(
                                    p.status,
                                    ProjectStatus::Cancelled | ProjectStatus::Failed
                                )
                        })
                {
                    return Err(Error::Unavailable);
                }
                for (id, good) in [
                    (materials, Good::Timber),
                    (tool, Good::Tool),
                    (seed, Good::Seed),
                    (water, Good::Water),
                    (food, Good::Food),
                ] {
                    if self.state.inventories.get(&id).is_none_or(|i| {
                        i.good != good || i.location != InventoryLocation::Area(area)
                    }) {
                        return Err(Error::Unavailable);
                    }
                }
                self.ensure_event_capacity(1)?;
                let id = ProjectId(allocate(&mut self.state)?);
                self.state.projects.as_mut().unwrap().projects.insert(
                    id,
                    Project {
                        kind: ProjectKind::Farm { plot, water },
                        id,
                        sponsor: actor,
                        area,
                        materials,
                        tool,
                        input: seed,
                        output: food,
                        beneficiaries,
                        workers,
                        support: BTreeSet::new(),
                        status: ProjectStatus::SeekingSupport,
                        blocker: Some(ProjectBlocker::Materials),
                        progress_ms: 0,
                        contributions: BTreeMap::new(),
                        materials_consumed: false,
                        finish_by,
                        enterprise: None,
                    },
                );
                self.emit(EventKind::ProjectProposed {
                    project: id,
                    sponsor: actor,
                });
                self.observe_projects();
            }
            ProjectAction::Propose {
                area: location,
                materials,
                tool,
                input,
                output,
                beneficiaries,
                workers,
                finish_by,
            } => {
                if area != location
                    || finish_by <= self.state.time
                    || beneficiaries.is_empty()
                    || !beneficiaries.contains(&actor)
                    || !workers.contains(&actor)
                    || beneficiaries.len() > 8
                    || workers.len() > 8
                    || beneficiaries
                        .iter()
                        .chain(&workers)
                        .any(|id| !self.person_index.contains_key(id))
                {
                    return Err(Error::Unavailable);
                }
                for (id, good) in [
                    (materials, Good::Timber),
                    (tool, Good::Tool),
                    (input, Good::Timber),
                    (output, Good::Tool),
                ] {
                    if self.state.inventories.get(&id).is_none_or(|stock| {
                        stock.good != good || stock.location != InventoryLocation::Area(area)
                    }) {
                        return Err(Error::Unavailable);
                    }
                }
                if tool == output || output == materials || input == output {
                    return Err(Error::Unavailable);
                }
                self.state.next_id.checked_add(1).ok_or(Error::Overflow)?;
                self.ensure_event_capacity(1)?;
                let id = ProjectId(allocate(&mut self.state)?);
                self.state.projects.as_mut().unwrap().projects.insert(
                    id,
                    Project {
                        kind: ProjectKind::Workshop,
                        id,
                        sponsor: actor,
                        area,
                        materials,
                        tool,
                        input,
                        output,
                        beneficiaries,
                        workers,
                        support: BTreeSet::new(),
                        status: ProjectStatus::SeekingSupport,
                        blocker: Some(ProjectBlocker::Materials),
                        progress_ms: 0,
                        contributions: BTreeMap::new(),
                        materials_consumed: false,
                        finish_by,
                        enterprise: None,
                    },
                );
                self.emit(EventKind::ProjectProposed {
                    project: id,
                    sponsor: actor,
                });
                self.observe_projects();
            }
            ProjectAction::Support { project, agreement } => {
                if !self.project_is_known(actor, project) {
                    return Err(Error::Unavailable);
                }
                let plan = &self.state.projects.as_ref().unwrap().projects[&project];
                let contract = self
                    .state
                    .agreements
                    .as_ref()
                    .unwrap()
                    .agreements
                    .get(&agreement)
                    .ok_or(Error::Unavailable)?;
                if plan.area != area
                    || !contract.parties.contains(&actor)
                    || !contract.parties.contains(&plan.sponsor)
                    || !contract.accepted.contains(&actor)
                    || matches!(
                        contract.status,
                        AgreementStatus::Rejected | AgreementStatus::Withdrawn
                    )
                    || !contract.terms.iter().any(|term| match term {
                        AgreementTerm::SiteAccess { site, .. } => {
                            matches!(plan.kind,ProjectKind::Farm {plot,..} if plot==*site)
                        }
                        AgreementTerm::ConstructionLabor {
                            project: target, ..
                        } => *target == project,
                        AgreementTerm::Delivery { destination, .. } => {
                            *destination == plan.materials
                        }
                        AgreementTerm::Access { inventory, .. } => {
                            *inventory == plan.tool || *inventory == plan.materials
                        }
                        _ => false,
                    })
                {
                    return Err(Error::Unavailable);
                }
                self.ensure_event_capacity(1)?;
                self.state
                    .projects
                    .as_mut()
                    .unwrap()
                    .projects
                    .get_mut(&project)
                    .unwrap()
                    .support
                    .insert(agreement);
                if let Some(enterprise) =
                    self.state.projects.as_ref().unwrap().projects[&project].enterprise
                {
                    self.state
                        .projects
                        .as_mut()
                        .unwrap()
                        .enterprises
                        .get_mut(&enterprise)
                        .unwrap()
                        .operating_agreements
                        .insert(agreement);
                }
                self.emit(EventKind::ProjectSupported {
                    project,
                    actor,
                    agreement,
                });
                self.observe_projects();
            }
            ProjectAction::Work { project } => self.start_construction(actor, project)?,
            ProjectAction::Cancel { project } => {
                let plan = self
                    .state
                    .projects
                    .as_ref()
                    .unwrap()
                    .projects
                    .get(&project)
                    .ok_or(Error::Unavailable)?;
                if plan.sponsor != actor
                    || !self.project_is_known(actor, project)
                    || matches!(
                        plan.status,
                        ProjectStatus::Completed | ProjectStatus::Failed | ProjectStatus::Cancelled
                    )
                {
                    return Err(Error::Unavailable);
                }
                self.ensure_event_capacity(2)?;
                let builders = self
                    .state
                    .projects
                    .as_ref()
                    .unwrap()
                    .jobs
                    .iter()
                    .filter(|(_, job)| job.project == project)
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>();
                for builder in builders {
                    self.interrupt_actor_activity(builder);
                }
                self.state
                    .projects
                    .as_mut()
                    .unwrap()
                    .projects
                    .get_mut(&project)
                    .unwrap()
                    .status = ProjectStatus::Cancelled;
                self.emit(EventKind::ProjectCancelled { project, actor });
                self.observe_projects();
            }
            ProjectAction::Operate {
                enterprise,
                agreement,
            } => {
                let workshop = self
                    .state
                    .projects
                    .as_ref()
                    .unwrap()
                    .enterprises
                    .get(&enterprise)
                    .ok_or(Error::Unavailable)?;
                let contract = self
                    .state
                    .agreements
                    .as_ref()
                    .unwrap()
                    .agreements
                    .get(&agreement)
                    .ok_or(Error::Unavailable)?;
                if workshop.area!=area || !self.state.projects.as_ref().unwrap().known_enterprises.get(&actor).is_some_and(|known|known.contains_key(&enterprise))
                    || !contract.parties.contains(&actor) || !contract.parties.contains(&workshop.sponsor) || !contract.accepted.contains(&actor)
                    || matches!(contract.status,AgreementStatus::Rejected|AgreementStatus::Withdrawn)
                    || !contract.terms.iter().any(|term|matches!(term,AgreementTerm::Labor {site,..} if *site==workshop.site)) {return Err(Error::Unavailable);}
                self.state
                    .projects
                    .as_mut()
                    .unwrap()
                    .enterprises
                    .get_mut(&enterprise)
                    .unwrap()
                    .operating_agreements
                    .insert(agreement);
                self.observe_projects();
            }
        }
        self.revision += 1;
        Ok(())
    }
    fn start_construction(&mut self, actor: PersonId, project: ProjectId) -> Result<(), Error> {
        if !self.construction_possible(actor, project) {
            return Err(Error::Unavailable);
        }
        let world = self.state.projects.as_ref().unwrap();
        let plan = &world.projects[&project];
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(world.rules.construction_labor.0 - plan.progress_ms)
                .ok_or(Error::Overflow)?,
        );
        if completes > plan.finish_by {
            return Err(Error::Unavailable);
        }
        let consume =
            (!plan.materials_consumed).then_some((plan.materials, world.rules.construction_timber));
        self.ensure_event_capacity(2)?;
        self.interrupt_actor_activity(actor);
        if let Some((source, quantity)) = consume {
            self.state.inventories.get_mut(&source).unwrap().quantity.0 -= quantity.0;
            self.emit(EventKind::Consumed {
                actor,
                source,
                quantity,
            });
        }
        let world = self.state.projects.as_mut().unwrap();
        let plan = world.projects.get_mut(&project).unwrap();
        plan.materials_consumed = true;
        plan.status = ProjectStatus::Active;
        plan.blocker = None;
        world
            .jobs
            .insert(actor, Construction { project, completes });
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
        Ok(())
    }
    pub(crate) fn interrupt_construction(&mut self, actor: PersonId) {
        let Some(job) = self
            .state
            .projects
            .as_mut()
            .and_then(|world| world.jobs.remove(&actor))
        else {
            return;
        };
        let project = self
            .state
            .projects
            .as_mut()
            .unwrap()
            .projects
            .get_mut(&job.project)
            .unwrap();
        if !matches!(
            project.status,
            ProjectStatus::Failed | ProjectStatus::Cancelled | ProjectStatus::Completed
        ) {
            project.status = ProjectStatus::Ready;
            project.blocker = None;
        }
    }
    pub(crate) fn project_boundary(&self, limit: SimTime) -> SimTime {
        self.state
            .projects
            .as_ref()
            .and_then(|world| {
                world
                    .projects
                    .values()
                    .filter(|p| {
                        !matches!(
                            p.status,
                            ProjectStatus::Completed
                                | ProjectStatus::Failed
                                | ProjectStatus::Cancelled
                        ) && p.finish_by > self.state.time
                    })
                    .map(|p| p.finish_by)
                    .min()
            })
            .unwrap_or(limit)
            .min(limit)
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
                    Some(game_economy::projects::EnterpriseWork {
                        actor: *actor,
                        site,
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let output = world.advance(self.state.time, next, &work)?;
        let count = output
            .jobs
            .values()
            .filter(|job| job.completes == next)
            .count() as u64;
        self.state
            .next_id
            .checked_add(count.checked_mul(2).ok_or(Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        Ok(Some(output))
    }
    pub(crate) fn commit_projects(&mut self, output: Option<World>) {
        if let Some(output) = output {
            self.state.projects = Some(output);
        }
    }
    pub(crate) fn complete_construction(&mut self, actor: PersonId) {
        let Some(job) = self
            .state
            .projects
            .as_mut()
            .and_then(|world| world.jobs.remove(&actor))
        else {
            return;
        };
        let plan = self.state.projects.as_ref().unwrap().projects[&job.project].clone();
        if plan.progress_ms
            != self
                .state
                .projects
                .as_ref()
                .unwrap()
                .rules
                .construction_labor
                .0
        {
            return;
        }
        let site = match plan.kind {
            ProjectKind::Workshop => {
                SiteId(allocate(&mut self.state).expect("preflight workshop site"))
            }
            ProjectKind::Farm { plot, .. } => plot,
        };
        let enterprise = EnterpriseId(allocate(&mut self.state).expect("preflight enterprise"));
        let rules = self.state.projects.as_ref().unwrap().rules.clone();
        if let ProjectKind::Farm { plot, water } = plan.kind {
            let land = self
                .state
                .ecology
                .as_mut()
                .unwrap()
                .plots
                .remove(&plot)
                .expect("farm plot reserved by construction");
            self.state.environment.as_mut().unwrap().fields.insert(
                site,
                environment::Field {
                    id: site,
                    area: plan.area,
                    access: land.access,
                    custodian: land.custodian,
                    seed: plan.input,
                    tool: plan.tool,
                    water,
                    food: plan.output,
                    phase: environment::CropPhase::Fallow,
                    soil_fertility: land.fertility,
                    soil_water: land.soil_water,
                    water_capacity: land.water_capacity,
                    preparation_ms: 0,
                    planting_ms: 0,
                    harvest_ms: 0,
                    degree_days_milli: 0,
                    stress_days: 0,
                    tending: 0,
                    contributions: BTreeMap::new(),
                },
            );
        } else {
            self.state.physical.as_mut().unwrap().sites.insert(
                site,
                physical::Site {
                    id: site,
                    area: plan.area,
                    input: plan.input,
                    input_quantity: rules.recipe_timber,
                    tool: plan.tool,
                    output: plan.output,
                    output_quantity: rules.recipe_tools,
                    duration: rules.recipe_time,
                    access: plan.workers.clone(),
                    last_granted: None,
                },
            );
        }
        self.state.projects.as_mut().unwrap().enterprises.insert(
            enterprise,
            Enterprise {
                kind: plan.kind.clone(),
                produced: BTreeMap::new(),
                inputs: BTreeMap::new(),
                id: enterprise,
                project: plan.id,
                sponsor: plan.sponsor,
                area: plan.area,
                site,
                beneficiaries: plan.beneficiaries.clone(),
                workers: plan.workers.clone(),
                status: EnterpriseStatus::Ready,
                blocker: None,
                timber_consumed: 0,
                tools_produced: 0,
                labor: BTreeMap::new(),
                operating_agreements: plan.support.clone(),
            },
        );
        let project = self
            .state
            .projects
            .as_mut()
            .unwrap()
            .projects
            .get_mut(&plan.id)
            .unwrap();
        project.status = ProjectStatus::Completed;
        project.enterprise = Some(enterprise);
        project.blocker = None;
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = Activity::Idle;
        self.emit(EventKind::ProjectCompleted {
            project: plan.id,
            enterprise,
            site,
        });
    }
    pub(crate) fn record_enterprise_input(&mut self, site: SiteId, quantity: Quantity) {
        let enterprise = self
            .state
            .projects
            .as_mut()
            .and_then(|world| {
                world
                    .enterprises
                    .values_mut()
                    .find(|enterprise| enterprise.site == site)
            })
            .map(|enterprise| {
                let input = enterprise.inputs.entry(Good::Timber).or_default();
                *input = input
                    .checked_add(quantity.0)
                    .expect("preflight recipe input account");
                enterprise.timber_consumed = enterprise
                    .timber_consumed
                    .checked_add(quantity.0)
                    .expect("preflight recipe input account");
                enterprise.id
            });
        if let Some(enterprise) = enterprise {
            self.emit(EventKind::EnterpriseInputUsed {
                enterprise,
                site,
                quantity,
            });
        }
    }
    pub(crate) fn preflight_enterprise_input(
        &self,
        site: SiteId,
        quantity: Quantity,
    ) -> Result<(), Error> {
        if let Some(enterprise) = self.state.projects.as_ref().and_then(|world| {
            world
                .enterprises
                .values()
                .find(|enterprise| enterprise.site == site)
        }) {
            enterprise
                .inputs
                .get(&Good::Timber)
                .copied()
                .unwrap_or(0)
                .checked_add(quantity.0)
                .ok_or(Error::Overflow)?;
            enterprise
                .timber_consumed
                .checked_add(quantity.0)
                .ok_or(Error::Overflow)?;
        }
        Ok(())
    }
    pub(crate) fn preflight_farm_input(
        &self,
        site: SiteId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<(), Error> {
        if let Some(enterprise) = self
            .state
            .projects
            .as_ref()
            .and_then(|w| w.enterprises.values().find(|e| e.site == site))
        {
            let good = self.state.inventories[&source].good;
            enterprise
                .inputs
                .get(&good)
                .copied()
                .unwrap_or(0)
                .checked_add(quantity.0)
                .ok_or(Error::Overflow)?;
        }
        Ok(())
    }
    pub(crate) fn record_farm_input(
        &mut self,
        site: SiteId,
        source: InventoryId,
        quantity: Quantity,
    ) {
        let good = self.state.inventories[&source].good;
        if let Some(enterprise) = self
            .state
            .projects
            .as_mut()
            .and_then(|w| w.enterprises.values_mut().find(|e| e.site == site))
        {
            let total = enterprise.inputs.entry(good).or_default();
            *total = total
                .checked_add(quantity.0)
                .expect("preflight cultivation input");
        }
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
                let accounts = books.then(|| self.enterprise_accounts(enterprise));
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
    fn enterprise_accounts(&self, enterprise: &Enterprise) -> EnterpriseAccountView {
        let mut accounts = EnterpriseAccountView {
            inputs: enterprise.inputs.clone(),
            produced: enterprise.produced.clone(),
            timber_consumed: enterprise.timber_consumed,
            tools_produced: enterprise.tools_produced,
            labor: enterprise.labor.clone(),
            operating_agreements: enterprise.operating_agreements.clone(),
            promised_labor_ms: 0,
            performed_labor_ms: 0,
            promised_payments: BTreeMap::new(),
            performed_payments: BTreeMap::new(),
            defaults: 0,
        };
        for agreement in &enterprise.operating_agreements {
            for obligation in self.state.agreements.as_ref().unwrap().agreements[agreement]
                .obligations
                .iter()
                .map(|id| &self.state.agreements.as_ref().unwrap().obligations[id])
            {
                match obligation.term {
                    AgreementTerm::Labor { site, millis, .. } if site == enterprise.site => {
                        accounts.promised_labor_ms += u128::from(millis);
                        accounts.performed_labor_ms += u128::from(obligation.performed);
                    }
                    AgreementTerm::OutputShare { debtor, good, .. }
                        if debtor == enterprise.sponsor =>
                    {
                        *accounts.promised_payments.entry(good).or_default() +=
                            u128::from(obligation.accrued);
                        *accounts.performed_payments.entry(good).or_default() +=
                            u128::from(obligation.performed);
                    }
                    AgreementTerm::Delivery {
                        debtor,
                        source,
                        quantity,
                        ..
                    } if debtor == enterprise.sponsor => {
                        let good = self.state.inventories[&source].good;
                        *accounts.promised_payments.entry(good).or_default() +=
                            u128::from(quantity.0);
                        *accounts.performed_payments.entry(good).or_default() +=
                            u128::from(obligation.performed);
                    }
                    _ => {}
                }
                accounts.defaults += u64::from(obligation.defaulted);
            }
        }
        accounts
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
    pub(crate) fn update_projects(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.projects else {
            return Ok(());
        };
        let cursor = world.last_event;
        let production = self
            .state
            .events
            .iter()
            .filter(|event| event.id.0 > cursor)
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
            .collect::<Vec<_>>();
        for (site, good, quantity) in production.iter().copied() {
            if let Some(enterprise) = self
                .state
                .projects
                .as_mut()
                .unwrap()
                .enterprises
                .values_mut()
                .find(|enterprise| enterprise.site == site)
            {
                let produced = enterprise.produced.entry(good).or_default();
                *produced = produced.checked_add(quantity.0).ok_or(Error::Overflow)?;
                if good == Good::Tool {
                    enterprise.tools_produced = enterprise
                        .tools_produced
                        .checked_add(quantity.0)
                        .ok_or(Error::Overflow)?;
                }
            }
        }
        self.state.projects.as_mut().unwrap().last_event = self.state.next_event - 1;
        let expired = self
            .state
            .projects
            .as_ref()
            .unwrap()
            .projects
            .values()
            .filter(|project| {
                project.finish_by <= self.state.time
                    && !matches!(
                        project.status,
                        ProjectStatus::Completed | ProjectStatus::Failed | ProjectStatus::Cancelled
                    )
            })
            .map(|project| project.id)
            .collect::<Vec<_>>();
        for project in expired.iter().copied() {
            let builders = self
                .state
                .projects
                .as_ref()
                .unwrap()
                .jobs
                .iter()
                .filter(|(_, job)| job.project == project)
                .map(|(id, _)| *id)
                .collect::<Vec<_>>();
            for actor in builders {
                self.interrupt_actor_activity(actor);
            }
            let plan = self
                .state
                .projects
                .as_mut()
                .unwrap()
                .projects
                .get_mut(&project)
                .unwrap();
            plan.status = ProjectStatus::Failed;
            plan.blocker = Some(ProjectBlocker::Deadline);
            self.emit(EventKind::ProjectFailed { project });
        }
        let missing_tools = self
            .state
            .projects
            .as_ref()
            .unwrap()
            .jobs
            .iter()
            .filter(|(actor, job)| {
                !self.project_usable(
                    **actor,
                    self.state.projects.as_ref().unwrap().projects[&job.project].tool,
                    Good::Tool,
                    1_000,
                )
            })
            .map(|(actor, _)| *actor)
            .collect::<Vec<_>>();
        for actor in missing_tools {
            self.interrupt_actor_activity(actor);
        }
        let ids = self
            .state
            .projects
            .as_ref()
            .unwrap()
            .projects
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for id in ids {
            let project = self.state.projects.as_ref().unwrap().projects[&id].clone();
            if matches!(
                project.status,
                ProjectStatus::Completed
                    | ProjectStatus::Failed
                    | ProjectStatus::Cancelled
                    | ProjectStatus::Active
            ) {
                continue;
            }
            let blocker = if !self.project_usable(project.sponsor, project.tool, Good::Tool, 1_000)
            {
                Some(ProjectBlocker::Tool)
            } else if !project.materials_consumed
                && !self.project_usable(
                    project.sponsor,
                    project.materials,
                    Good::Timber,
                    self.state
                        .projects
                        .as_ref()
                        .unwrap()
                        .rules
                        .construction_timber
                        .0,
                )
            {
                Some(ProjectBlocker::Materials)
            } else {
                None
            };
            let plan = self
                .state
                .projects
                .as_mut()
                .unwrap()
                .projects
                .get_mut(&id)
                .unwrap();
            plan.blocker = blocker;
            plan.status = if blocker.is_some() {
                ProjectStatus::Blocked
            } else {
                ProjectStatus::Ready
            };
        }
        let enterprise_ids = self
            .state
            .projects
            .as_ref()
            .unwrap()
            .enterprises
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for id in enterprise_ids {
            let workshop = self.state.projects.as_ref().unwrap().enterprises[&id].clone();
            if matches!(workshop.kind, ProjectKind::Farm { .. }) {
                let field = &self.state.environment.as_ref().unwrap().fields[&workshop.site];
                let active = self
                    .state
                    .environment
                    .as_ref()
                    .unwrap()
                    .jobs
                    .values()
                    .any(|j| j.field == field.id);
                let blocker = if !workshop
                    .workers
                    .iter()
                    .any(|p| self.project_able(*p) && field.access.contains(p))
                {
                    Some(ProjectBlocker::Workers)
                } else if field.phase == environment::CropPhase::Failed {
                    Some(ProjectBlocker::Materials)
                } else {
                    None
                };
                let farm = self
                    .state
                    .projects
                    .as_mut()
                    .unwrap()
                    .enterprises
                    .get_mut(&id)
                    .unwrap();
                farm.status = if active {
                    EnterpriseStatus::Active
                } else if blocker.is_some() {
                    EnterpriseStatus::Blocked
                } else {
                    EnterpriseStatus::Ready
                };
                farm.blocker = blocker;
                continue;
            }
            let site = &self.state.physical.as_ref().unwrap().sites[&workshop.site];
            let active = self
                .state
                .physical
                .as_ref()
                .unwrap()
                .actors
                .values()
                .any(|body| matches!(body.activity,Activity::Work {site:id} if id==site.id));
            let blocker = if self.state.inventories[&site.output].capacity.0
                - self.state.inventories[&site.output].quantity.0
                < site.output_quantity.0
            {
                Some(ProjectBlocker::OutputCapacity)
            } else if !workshop.workers.iter().any(|actor| {
                self.project_able(*actor)
                    && self.project_usable(*actor, site.input, Good::Timber, site.input_quantity.0)
                    && self.project_usable(*actor, site.tool, Good::Tool, 1_000)
            }) {
                Some(ProjectBlocker::Materials)
            } else {
                None
            };
            let workshop = self
                .state
                .projects
                .as_mut()
                .unwrap()
                .enterprises
                .get_mut(&id)
                .unwrap();
            workshop.status = if active {
                EnterpriseStatus::Active
            } else if blocker.is_some() {
                EnterpriseStatus::Blocked
            } else {
                EnterpriseStatus::Ready
            };
            workshop.blocker = if active { None } else { blocker };
        }
        if !production.is_empty() || !expired.is_empty() {
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
            if mind.dream != decisions::Dream::Independence || mind.personality.ambition < 600 {
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
            let plot = match plan.kind {
                ProjectKind::Farm { plot, .. } => plot,
                _ => unreachable!(),
            };
            let year = self
                .state
                .environment
                .as_ref()
                .unwrap()
                .calendar
                .year_days()
                .checked_mul(self.state.environment.as_ref().unwrap().calendar.day.0)
                .ok_or(Error::Overflow)?;
            let ends_at = SimTime(self.state.time.0.checked_add(year).ok_or(Error::Overflow)?);
            let pay_by = SimTime(
                ends_at
                    .0
                    .checked_add(7 * self.state.environment.as_ref().unwrap().calendar.day.0)
                    .ok_or(Error::Overflow)?,
            );
            let before = self.state.next_id;
            self.agreement_command(
                actor,
                &AgreementAction::Propose {
                    parties: BTreeSet::from([actor, supporter]),
                    terms: vec![
                        AgreementTerm::Delivery {
                            debtor: supporter,
                            creditor: actor,
                            source,
                            destination: plan.materials,
                            quantity: required,
                            deadline: plan.finish_by,
                        },
                        AgreementTerm::OutputShare {
                            debtor: actor,
                            creditor: supporter,
                            site: plot,
                            good: Good::Food,
                            numerator: 1,
                            denominator: 10,
                            destination,
                            pay_by,
                            ends_at,
                        },
                    ],
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
            self.state.projects.as_mut().unwrap().next_search.insert(
                actor,
                SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(120_000)
                        .ok_or(Error::Overflow)?,
                ),
            );
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
            if mind.dream != decisions::Dream::Independence || mind.personality.ambition < 600 {
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
            let needed = if plan.enterprise.is_some() {
                3 * self.state.projects.as_ref().unwrap().rules.recipe_timber.0
            } else {
                self.state
                    .projects
                    .as_ref()
                    .unwrap()
                    .rules
                    .construction_timber
                    .0
                    + 3 * self.state.projects.as_ref().unwrap().rules.recipe_timber.0
            };
            if self.state.inventories[&plan.materials].quantity.0 >= needed {
                continue;
            }
            let donor = mind
                .knowledge
                .values()
                .filter(|fact| {
                    fact.good == Good::Timber
                        && fact.area == plan.area
                        && fact.inventory != plan.materials
                        && fact.quantity.0 >= needed
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
            let loading = self.state.physical.as_ref().unwrap().rules.loading_time.0;
            let funding_window = loading.saturating_mul(4).saturating_add(120_000);
            let production_window = self
                .state
                .projects
                .as_ref()
                .unwrap()
                .rules
                .construction_labor
                .0
                .saturating_add(
                    self.state
                        .projects
                        .as_ref()
                        .unwrap()
                        .rules
                        .recipe_time
                        .0
                        .saturating_mul(3),
                )
                .saturating_add(loading.saturating_mul(10))
                .saturating_mul(2)
                .saturating_add(120_000);
            let funding_deadline = SimTime(
                self.state
                    .time
                    .0
                    .checked_add(funding_window)
                    .ok_or(Error::Overflow)?,
            );
            let repayment_deadline = SimTime(
                self.state
                    .time
                    .0
                    .checked_add(production_window)
                    .ok_or(Error::Overflow)?,
            );
            let before = self.state.next_id;
            self.debug_agreement_action(
                actor,
                AgreementAction::Propose {
                    parties: BTreeSet::from([actor, financier]),
                    terms: vec![
                        AgreementTerm::Delivery {
                            debtor: financier,
                            creditor: actor,
                            source,
                            destination: plan.materials,
                            quantity: Quantity(needed),
                            deadline: funding_deadline,
                        },
                        AgreementTerm::Delivery {
                            debtor: actor,
                            creditor: financier,
                            source: plan.output,
                            destination,
                            quantity: Quantity(
                                3 * self.state.projects.as_ref().unwrap().rules.recipe_tools.0,
                            ),
                            deadline: repayment_deadline,
                        },
                    ],
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
                .insert(actor, SimTime(self.state.time.0.saturating_add(600_000)));
        }
        Ok(())
    }
    fn workshop_procurement(&self, actor: PersonId, mind: &decisions::Mind) -> Option<Command> {
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
        if !plan.materials_consumed && plan.support.iter().any(|id| {
            let agreement=&self.state.agreements.as_ref().unwrap().agreements[id];
            agreement.status==AgreementStatus::Proposed || agreement.obligations.iter().any(|id| {
                let obligation=&self.state.agreements.as_ref().unwrap().obligations[id];
                matches!(obligation.term,AgreementTerm::Delivery {destination,..} if destination==plan.materials) && !obligation.defaulted && obligation.performed<obligation.term.amount()
            })
        }) {return None;}
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
        if !fact.accessible {
            return None;
        }
        let insufficient = fact.quantity.0 < required;
        let area = self.project_area(actor)?;
        let cargo = self.state.inventories.values().find(|stock| {
            stock.good == Good::Timber && stock.location == InventoryLocation::Carried(actor)
        })?;
        if cargo.quantity.0 > 0 {
            if area == plan.area {
                if self.state.inventories[&target].capacity
                    == self.state.inventories[&target].quantity
                {
                    return None;
                }
                return Some(Command::Load {
                    source: cargo.id,
                    destination: target,
                    quantity: Quantity(cargo.quantity.0.min(
                        self.state.inventories[&target].capacity.0
                            - self.state.inventories[&target].quantity.0,
                    )),
                });
            }
            return decisions::known_path(&mind.routes, area, plan.area)
                .ok()
                .map(|_| Command::Move {
                    destination: plan.area,
                });
        }
        if !insufficient {
            return None;
        }
        for source in mind.knowledge.values().filter(|fact| {
            fact.good == Good::Timber
                && fact.inventory != target
                && fact.accessible
                && fact.quantity.0 >= required
        }) {
            if source.area == area {
                let held = self
                    .state
                    .inventories
                    .values()
                    .filter(|stock| stock.location == InventoryLocation::Carried(actor))
                    .map(|stock| u128::from(stock.quantity.0))
                    .sum::<u128>();
                let room = u128::from(
                    self.state
                        .physical
                        .as_ref()
                        .unwrap()
                        .rules
                        .carrying_capacity
                        .0,
                )
                .saturating_sub(held)
                .min(u128::from(cargo.capacity.0 - cargo.quantity.0))
                    as u64;
                let amount = source.quantity.0.min(room);
                if amount > 0 {
                    return Some(Command::Load {
                        source: source.inventory,
                        destination: cargo.id,
                        quantity: Quantity(amount),
                    });
                }
            } else if decisions::known_path(&mind.routes, area, source.area).is_ok() {
                return Some(Command::Move {
                    destination: source.area,
                });
            }
        }
        // These hints identify a plausible place to investigate, not an unseen
        // quantity or permission. Depletion learned on arrival ends the search.
        for hint in world.timber_hints.get(&actor).into_iter().flatten() {
            if *hint == area
                || mind
                    .knowledge
                    .values()
                    .any(|fact| fact.area == *hint && fact.good == Good::Timber)
            {
                continue;
            }
            if decisions::known_path(&mind.routes, area, *hint).is_ok() {
                return Some(Command::Move { destination: *hint });
            }
        }
        None
    }

    pub(crate) fn project_candidate(&self, actor: PersonId) -> Option<Command> {
        let world = self.state.projects.as_ref()?;
        if !self.project_able(actor) {
            return None;
        }
        let body = &self.state.physical.as_ref().unwrap().actors[&actor];
        let mind = &self.state.decisions.as_ref()?.minds[&actor];
        if body.vitals.hunger >= self.state.physical.as_ref().unwrap().rules.meal_relief
            || body.vitals.fatigue >= self.state.physical.as_ref().unwrap().rules.fatigue_limit / 2
        {
            return None;
        }
        for plan in world
            .projects
            .values()
            .rev()
            .filter(|plan| plan.workers.contains(&actor) && self.project_is_known(actor, plan.id))
            .take(8)
        {
            let obligated=plan.support.iter().filter_map(|id|self.state.agreements.as_ref().unwrap().agreements.get(id)).any(|agreement|agreement.obligations.iter().any(|id|matches!(self.state.agreements.as_ref().unwrap().obligations[id].term,AgreementTerm::ConstructionLabor {debtor,project,..} if debtor==actor&&project==plan.id)&&self.state.agreements.as_ref().unwrap().obligations[id].performed<self.state.agreements.as_ref().unwrap().obligations[id].term.amount()));
            if self.construction_possible(actor, plan.id)
                && (plan.sponsor == actor
                    && (mind.dream == decisions::Dream::Independence
                        && mind.personality.ambition >= 600
                        || mind.personality.discipline >= 700)
                    || obligated
                        && (mind.personality.discipline >= 500 || mind.personality.honesty >= 500)
                    || mind.dream == decisions::Dream::SupportOthers
                        && mind.personality.empathy >= 700
                        && plan.beneficiaries.contains(&actor))
            {
                return Some(Command::Project {
                    action: ProjectAction::Work { project: plan.id },
                });
            }
        }
        if mind.dream == decisions::Dream::Independence
            && let Some(command) = self.workshop_procurement(actor, mind)
        {
            return Some(command);
        }
        for workshop in world.enterprises.values().filter(|workshop| {
            workshop.sponsor == actor && workshop.area == self.project_area(actor).unwrap()
        }) {
            if mind.dream != decisions::Dream::Independence
                && !(mind.dream == decisions::Dream::SupportOthers
                    && mind.personality.empathy >= 700)
            {
                continue;
            }
            if matches!(workshop.kind, ProjectKind::Farm { .. }) {
                if let Some(command) = self.autonomous_field_cultivation(actor, workshop.site) {
                    return Some(command);
                }
                continue;
            }
            let site = &self.state.physical.as_ref().unwrap().sites[&workshop.site];
            if self.project_usable(actor, site.input, Good::Timber, site.input_quantity.0)
                && self.project_usable(actor, site.tool, Good::Tool, 1_000)
                && self.state.inventories[&site.output].capacity.0
                    - self.state.inventories[&site.output].quantity.0
                    >= site.output_quantity.0
            {
                return Some(Command::Work { site: site.id });
            }
        }
        if mind.dream != decisions::Dream::Independence
            || mind.personality.ambition < 700
            || world.known_projects.get(&actor).is_some_and(|known| {
                known.values().any(|project| {
                    project.sponsor == actor
                        && !matches!(
                            project.status,
                            ProjectStatus::Cancelled | ProjectStatus::Failed
                        )
                })
            })
            || world
                .next_search
                .get(&actor)
                .is_some_and(|time| *time > self.state.time)
        {
            return None;
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
        let materials = mind
            .knowledge
            .values()
            .filter(own)
            .find(|fact| fact.good == Good::Timber && fact.accessible)?
            .inventory;
        let tool = mind
            .knowledge
            .values()
            .filter(own)
            .find(|fact| fact.good == Good::Tool && fact.accessible && fact.quantity.0 >= 1_000)?
            .inventory;
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
                return Some(Command::Project {
                    action: ProjectAction::ProposeFarm {
                        plot: plot.id,
                        materials,
                        tool,
                        seed,
                        water,
                        food,
                        beneficiaries: BTreeSet::from([actor]),
                        workers: BTreeSet::from([actor]),
                        finish_by: SimTime(self.state.time.0.checked_add(
                            world.rules.construction_labor.0.saturating_mul(4) + 600_000,
                        )?),
                    },
                });
            }
        }
        let output = mind
            .knowledge
            .values()
            .filter(own)
            .find(|fact| {
                fact.good == Good::Tool
                    && fact.inventory != tool
                    && fact.accessible
                    && fact.quantity.0 == 0
            })?
            .inventory;
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
        let finish_by = SimTime(self.state.time.0.checked_add(expected)?);
        Some(Command::Project {
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
        })
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
        if mind.dream != decisions::Dream::Independence || mind.personality.ambition < 600 {
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
        || rules.construction_timber.0 == 0
        || rules.construction_labor.0 == 0
        || rules.recipe_timber.0 == 0
        || rules.recipe_tools.0 == 0
        || rules.recipe_time.0 == 0
        || rules
            .recipe_timber
            .0
            .checked_mul(3)
            .and_then(|raw| raw.checked_add(rules.construction_timber.0))
            .is_none()
        || rules.recipe_tools.0.checked_mul(3).is_none()
        || !(1..=16).contains(&rules.workshops_per_area)
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
    let mut exclusive: BTreeMap<PersonId, u128> = BTreeMap::new();
    for (id, plan) in &world.projects {
        if *id != plan.id
            || id.0 >= state.next_id
            || !people.contains_key(&plan.sponsor)
            || !state.areas.iter().any(|area| area.id == plan.area)
            || plan.beneficiaries.is_empty()
            || plan.beneficiaries.len() > 8
            || plan.workers.is_empty()
            || plan.workers.len() > 8
            || !plan.beneficiaries.contains(&plan.sponsor)
            || !plan.workers.contains(&plan.sponsor)
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
            || plan.progress_ms > rules.construction_labor.0
            || plan
                .contributions
                .values()
                .map(|value| u128::from(*value))
                .sum::<u128>()
                != u128::from(plan.progress_ms)
            || !plan.materials_consumed && plan.progress_ms != 0
            || plan.tool == plan.output
            || (plan.status == ProjectStatus::Completed) != plan.enterprise.is_some()
            || plan.status == ProjectStatus::Completed
                && plan.progress_ms != rules.construction_labor.0
            || plan.status == ProjectStatus::Failed
                && (plan.finish_by > state.time || plan.blocker != Some(ProjectBlocker::Deadline))
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
        for (actor, elapsed) in &plan.contributions {
            *exclusive.entry(*actor).or_default() += u128::from(*elapsed);
        }
        if let Some(id) = plan.enterprise
            && world
                .enterprises
                .get(&id)
                .is_none_or(|enterprise| enterprise.project != plan.id)
        {
            return Err(fail());
        }
    }
    let mut busy = BTreeSet::new();
    for (actor, job) in &world.jobs {
        let plan = world.projects.get(&job.project).ok_or_else(fail)?;
        if !people.contains_key(actor)
            || !busy.insert(job.project)
            || !plan.materials_consumed
            || plan.status != ProjectStatus::Active
            || !plan.workers.contains(actor)
            || job.completes <= state.time
            || job.completes > plan.finish_by
            || job.completes.0 - state.time.0 != rules.construction_labor.0 - plan.progress_ms
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
    for plan in world.projects.values() {
        if plan.status == ProjectStatus::Active && !busy.contains(&plan.id) {
            return Err(fail());
        }
    }
    let mut sites = BTreeSet::new();
    for (id, enterprise) in &world.enterprises {
        let plan = world.projects.get(&enterprise.project).ok_or_else(fail)?;
        if let ProjectKind::Farm { plot, water } = plan.kind {
            let field = state
                .environment
                .as_ref()
                .and_then(|w| w.fields.get(&plot))
                .ok_or_else(fail)?;
            if enterprise.kind != plan.kind
                || enterprise.site != plot
                || field.seed != plan.input
                || field.food != plan.output
                || field.tool != plan.tool
                || field.water != water
                || plan.enterprise != Some(*id)
                || enterprise.area != plan.area
                || enterprise.sponsor != plan.sponsor
                || enterprise.beneficiaries != plan.beneficiaries
                || enterprise.workers != plan.workers
                || enterprise.labor.keys().any(|id| !people.contains_key(id))
                || !sites.insert(plot)
                || enterprise.tools_produced != 0
                || enterprise.timber_consumed != 0
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
                || enterprise
                    .inputs
                    .keys()
                    .any(|g| !matches!(g, Good::Seed | Good::Water))
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
            || !sites.insert(site.id)
            || plan.enterprise != Some(*id)
            || enterprise.area != plan.area
            || enterprise.sponsor != plan.sponsor
            || enterprise.beneficiaries != plan.beneficiaries
            || enterprise.workers != plan.workers
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
            || enterprise.timber_consumed % rules.recipe_timber.0 != 0
            || enterprise.tools_produced % rules.recipe_tools.0 != 0
            || u128::from(enterprise.timber_consumed)
                < u128::from(enterprise.tools_produced / rules.recipe_tools.0)
                    * u128::from(rules.recipe_timber.0)
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
        for (actor, elapsed) in &enterprise.labor {
            *exclusive.entry(*actor).or_default() += u128::from(*elapsed);
        }
    }
    if exclusive
        .values()
        .any(|elapsed| *elapsed > u128::from(state.time.0))
    {
        return Err(fail());
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
        environment.weather = environment.climate.weather(sim.state.seed, 0, 0);
        environment.crop.prepare_labor = MINUTE;
        environment.crop.plant_labor = MINUTE;
        environment.crop.tend_labor = MINUTE;
        environment.crop.harvest_labor = MINUTE;
        environment.crop.ripe_degree_days = 3;
        sim
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
                .environment
                .as_ref()
                .unwrap()
                .fields
                .contains_key(&field)
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
            sim.state.environment.as_ref().unwrap().fields[&field].custodian,
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
        sim.debug_set_personality(supporter, personality, decisions::Dream::Comfort)
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
                decisions::Dream::Comfort,
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

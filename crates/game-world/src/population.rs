//! Initial people are authored existing residents. Once this snapshot is
//! published, population growth uses ordinary conception and birth exclusively.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub people: usize,
    pub settlement_size: usize,
}
impl Plan {
    pub fn small() -> Self {
        Self {
            people: 15,
            settlement_size: 15,
        }
    }
    fn validate(self) -> Result<(), Error> {
        if !(15..=100_000).contains(&self.people) || !(15..=128).contains(&self.settlement_size) {
            return Err(Error::InvalidConfiguration(
                "population requires 15..100000 people and 15..128 residents per settlement".into(),
            ));
        }
        Ok(())
    }
}
/// Authoring IDs, not a second registry of the live population.
#[derive(Clone, Debug)]
pub struct Settlement {
    pub center: AreaId,
    pub fields: AreaId,
    pub wood: AreaId,
    pub residents: Vec<PersonId>,
}
fn mind(
    personality: decisions::Personality,
    dream: decisions::Aspiration,
    routes: BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    now: SimTime,
) -> decisions::Mind {
    decisions::Mind {
        interests: BTreeMap::new(),
        routes,
        last_failure: None,
        child_signals: BTreeMap::new(),
        personality,
        dream,
        knowledge: BTreeMap::new(),
        sites: BTreeMap::new(),
        relationships: BTreeMap::new(),
        values: BTreeMap::new(),
        goal: decisions::Goal::Rest,
        plan: None,
        explanation: decisions::Explanation {
            goal: decisions::Goal::Rest,
            need: 0,
            benefit: 0,
            value: 0,
            total: 0,
            blocker: None,
        },
        retry_after: now,
    }
}
impl Simulation {
    pub fn populated_world(
        seed: u64,
        execution: Execution,
        config: scenarios::Config,
        plan: Plan,
    ) -> Result<Self, Error> {
        let (mut sim, settlements) =
            Self::initial_population(seed, execution, &config, plan, true)?;
        sim.state.scenario = ScenarioInfo {
            id: "settlement-founder".into(),
            version: 1,
            debug_fixture: false,
        };
        sim.install_playable_domains(&config)?;
        sim.initialize_transport(config.transport.clone())?;
        sim.initialize_realm(realm::Rules::default())?;
        sim.initialize_armies(armies::Rules::default())?;
        sim.state.ecology = Some(ecology::Runtime {
            observed_areas: BTreeMap::new(),
            rules: config.ecology.clone(),
            jobs: BTreeMap::new(),
            known: BTreeMap::new(),
            known_plots: BTreeMap::new(),
            next_request: BTreeMap::new(),
            day: sim.state.time.0 / config.environment.calendar.day.0,
        });
        sim.initialize_health(config.health.clone())?;
        for settlement in &settlements {
            sim.seed_population_settlement(settlement, &config)?;
        }
        if let Some(rules) = config.genetics.clone() {
            sim.initialize_genetics(rules)?;
        }
        sim.initialize_social(config.social.clone())?;
        for settlement in &settlements {
            // These two existing people have chosen each other. Their explicit
            // prior conversations supply intent, never a compulsory household.
            let a = settlement.residents[3];
            let b = settlement.residents[4];
            for (actor, partner) in [(a, b), (b, a)] {
                let mind = sim
                    .state
                    .decisions
                    .as_mut()
                    .unwrap()
                    .minds
                    .get_mut(&actor)
                    .unwrap();
                mind.dream = decisions::Aspiration::Parenthood;
                mind.relationships.insert(
                    partner,
                    decisions::Relationship {
                        trust: 800,
                        affection: 800,
                    },
                );
            }
            for topic in [SocialTopic::Companionship, SocialTopic::FamilyPlans] {
                sim.state
                    .social
                    .as_mut()
                    .unwrap()
                    .finish_conversation(a, b, topic)?;
                sim.emit(EventKind::Socialized {
                    first: a,
                    second: b,
                    topic,
                });
            }
        }
        for settlement in &settlements {
            let debtor = settlement.residents[8];
            let creditor = settlement.residents[3];
            // This authored, local promise is a voluntary delivery, not an
            // automatic settlement duty. Its subsequent fulfillment is physical.
            let source = sim
                .state
                .inventories
                .values()
                .find(|stock| {
                    stock.good == Good::Timber
                        && stock.location == InventoryLocation::Area(settlement.wood)
                        && stock.quantity.0 > 0
                })
                .unwrap()
                .id;
            let destination = sim
                .state
                .inventories
                .values()
                .find(|stock| {
                    stock.good == Good::Timber
                        && stock.location == InventoryLocation::Area(settlement.center)
                })
                .unwrap()
                .id;
            sim.debug_agreement_action(
                debtor,
                AgreementAction::Propose {
                    parties: BTreeSet::from([debtor, creditor]),
                    terms: vec![AgreementTerm::Delivery {
                        debtor,
                        creditor,
                        source,
                        destination,
                        quantity: Quantity(2_000),
                        deadline: SimTime(sim.state.time.0 + 6 * 3_600_000),
                    }],
                    procedure: None,
                },
            )?;
            let agreement = *sim
                .state
                .agreements
                .as_ref()
                .unwrap()
                .agreements
                .keys()
                .next_back()
                .unwrap();
            sim.debug_agreement_action(creditor, AgreementAction::Accept { agreement })?;
        }
        sim.update_property_events()?;
        sim.observe_agreement_events();
        sim.observe_ecology();
        sim.observe_health();
        sim.review_decisions()?;
        Ok(sim)
    }
    fn seed_population_settlement(
        &mut self,
        settlement: &Settlement,
        config: &scenarios::Config,
    ) -> Result<(), Error> {
        let access = settlement
            .residents
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let steward = settlement.residents[3];
        let healer = settlement.residents[6];
        let lots:Vec<_>=self.state.inventories.values().filter(|stock|matches!(stock.location,InventoryLocation::Area(area) if [settlement.center,settlement.fields,settlement.wood].contains(&area))).map(|stock|stock.id).collect();
        for id in &lots {
            self.state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(*id, steward);
        }
        for actor in &settlement.residents {
            self.state
                .projects
                .as_mut()
                .unwrap()
                .timber_hints
                .insert(*actor, BTreeSet::from([settlement.wood]));
        }
        for (kind, area, capacity, growth) in [
            (ResourceKind::Coppice, settlement.wood, 500_000, 5_000),
            (ResourceKind::Berries, settlement.wood, 100_000, 5_000),
            (ResourceKind::Herbs, settlement.wood, 100_000, 2_000),
            (ResourceKind::Spring, settlement.fields, 1_000_000, 300_000),
        ] {
            let resource = self.debug_add_resource(
                kind,
                area,
                Quantity(capacity),
                Quantity(capacity),
                Quantity(growth),
                access.clone(),
                Some(steward),
                0,
            )?;
            self.state
                .ecology
                .as_mut()
                .unwrap()
                .resource_mut(&mut self.state.components, resource)
                .unwrap()
                .labor_per_unit = match kind {
                ResourceKind::Coppice => SimDuration(3_600_000),
                ResourceKind::Spring => SimDuration(5 * MINUTE.0),
                _ => SimDuration(30 * MINUTE.0),
            };
        }
        self.debug_add_plot(settlement.fields, Some(steward), access.clone(), 900)?;
        let herbs = self.add_inventory(
            Good::Herb,
            InventoryLocation::Area(settlement.center),
            Quantity(100_000),
            Quantity(500_000),
            BTreeSet::from([healer]),
        )?;
        let water = self.add_inventory(
            Good::Water,
            InventoryLocation::Area(settlement.center),
            Quantity(1_000_000),
            Quantity(5_000_000),
            access.clone(),
        )?;
        for id in [herbs, water] {
            self.state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(id, healer);
        }
        self.collected_water_quality(water, Quantity(1_000_000), 0);
        for actor in &settlement.residents {
            self.seed_water_safety(*actor, water)?;
        }
        let levels = 10.min(config.skills.max_level);
        self.state
            .skills
            .as_mut()
            .unwrap()
            .records
            .get_mut(&healer)
            .unwrap()
            .insert(
                Skill::Medicine,
                skills::Progress {
                    practice_ms: config.skills.per_level.0 * u64::from(levels),
                    level: levels,
                    progress_ms: 0,
                },
            );
        self.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&healer)
            .unwrap()
            .personality
            .empathy = 900;
        self.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&healer)
            .unwrap()
            .dream = decisions::Aspiration::SupportOthers;
        let timber = lots
            .iter()
            .find(|id| {
                self.state.inventories[id].good == Good::Timber
                    && self.state.inventories[id].location
                        == InventoryLocation::Area(settlement.center)
            })
            .copied()
            .unwrap();
        let tool = lots
            .iter()
            .find(|id| {
                self.state.inventories[id].good == Good::Tool
                    && self.state.inventories[id].location
                        == InventoryLocation::Area(settlement.center)
            })
            .copied()
            .unwrap();
        let output = self.add_inventory(
            Good::Tool,
            InventoryLocation::Area(settlement.center),
            Quantity(0),
            Quantity(500_000),
            access.clone(),
        )?;
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .custodians
            .insert(output, steward);
        let site = SiteId(allocate(&mut self.state)?);
        let rules = self.state.projects.as_ref().unwrap().rules.clone();
        self.state.components.insert_site(physical::Site {
            id: site,
            area: settlement.center,
            input: timber,
            input_quantity: rules.recipe_timber,
            tool,
            output,
            output_quantity: rules.recipe_tools,
            duration: rules.recipe_time,
            access: access.clone(),
            last_granted: None,
        })?;
        let project = ProjectId(allocate(&mut self.state)?);
        let enterprise = EnterpriseId(allocate(&mut self.state)?);
        let participants: BTreeSet<_> = settlement.residents[3..11].iter().copied().collect();
        let workshop_history = projects::Project {
            id: project,
            kind: ProjectKind::Workshop,
            sponsor: steward,
            area: settlement.center,
            materials: timber,
            tool,
            input: timber,
            output,
            beneficiaries: participants.clone(),
            workers: participants.clone(),
            support: BTreeSet::new(),
            status: ProjectStatus::Completed,
            blocker: None,
            progress_ms: rules.construction_labor.0,
            contributions: BTreeMap::from([(steward, rules.construction_labor.0)]),
            materials_consumed: true,
            finish_by: self.state.time,
            enterprise: Some(enterprise),
        };
        let world = self.state.projects.as_mut().unwrap();
        world.projects.insert(project, workshop_history);
        world.enterprises.insert(
            enterprise,
            projects::Enterprise {
                id: enterprise,
                project,
                sponsor: steward,
                area: settlement.center,
                site,
                kind: ProjectKind::Workshop,
                beneficiaries: participants.clone(),
                workers: participants,
                status: EnterpriseStatus::Ready,
                blocker: None,
                timber_consumed: 0,
                tools_produced: 0,
                inputs: BTreeMap::new(),
                produced: BTreeMap::new(),
                labor: BTreeMap::new(),
                operating_agreements: BTreeSet::new(),
            },
        );
        // Existing workshops have an actual material cost in their initial
        // history; subsequent recipes consume their own real input lots.
        self.state.inventories.get_mut(&timber).unwrap().quantity.0 -= rules.construction_timber.0;
        self.emit(EventKind::Consumed {
            actor: steward,
            source: timber,
            quantity: rules.construction_timber,
        });
        self.emit(EventKind::ConstructionStarted {
            project,
            actor: steward,
        });
        self.emit(EventKind::ProjectCompleted {
            project,
            enterprise,
            site,
        });
        for area in [settlement.center, settlement.fields] {
            let occupants: Vec<_> = settlement
                .residents
                .iter()
                .copied()
                .filter(|id| {
                    *self
                        .state
                        .components
                        .location(self.state.people[self.person_index[id]].id)
                        .expect("location exists")
                        == Location::AtArea(area)
                })
                .collect();
            for chunk in occupants.chunks(32) {
                let capacity = chunk.len() as u32;
                let insulation = 750;
                let quantity = Quantity(
                    (u128::from(capacity)
                        * u128::from(config.health.shelter_timber_per_place.0)
                        * u128::from(500 + insulation))
                    .div_ceil(1_000) as u64,
                );
                let materials = self.add_inventory(
                    Good::Timber,
                    InventoryLocation::Area(area),
                    quantity,
                    quantity,
                    BTreeSet::from([steward]),
                )?;
                let shelter = self.seed_shelter(
                    area,
                    steward,
                    access.clone(),
                    capacity,
                    insulation,
                    materials,
                    quantity,
                    SimTime(self.state.time.0.saturating_sub(2 * 86_400_000)),
                )?;
                for person in chunk {
                    self.set_initial_shelter_occupancy(*person, shelter)?;
                }
            }
        }
        let reported:Vec<_>=self.state.inventories.values().filter(|stock|matches!(stock.location,InventoryLocation::Area(area) if [settlement.center,settlement.fields,settlement.wood].contains(&area))).cloned().collect();
        for actor in &settlement.residents {
            let mind = self
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(actor)
                .unwrap();
            for stock in &reported {
                let InventoryLocation::Area(area) = stock.location else {
                    unreachable!()
                };
                mind.knowledge.insert(
                    stock.id,
                    StockKnowledge {
                        inventory: stock.id,
                        area,
                        good: stock.good,
                        quantity: stock.quantity,
                        accessible: stock.access.contains(actor),
                        observed_at: self.state.time,
                        learned_at: self.state.time,
                        source: KnowledgeSource::Reported(steward),
                    },
                );
            }
            mind.sites.insert(
                site,
                decisions::SiteKnowledge {
                    site,
                    area: settlement.center,
                    input: timber,
                    tool,
                    input_quantity: rules.recipe_timber,
                    output,
                    access: true,
                },
            );
        }
        Ok(())
    }
    pub(crate) fn initial_population(
        seed: u64,
        execution: Execution,
        config: &scenarios::Config,
        plan: Plan,
        mixed: bool,
    ) -> Result<(Self, Vec<Settlement>), Error> {
        plan.validate()?;
        config.environment.validate()?;
        let crop_document = config.agriculture.resolve()?;
        crop_document
            .data
            .validate(Some(config.environment.calendar.year_days()))?;
        config.ecology.validate()?;
        if !config.biology.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid population life rules".into(),
            ));
        }
        // Only storage/IDs are reused. This unpublished base contains no bodies,
        // minds, biological processes, or simulated population growth.
        let mut sim = Self::kernel_fixture(seed, plan.people, execution)?;
        sim.state.time = SimTime(
            (3 * 86_400_000).max(
                config
                    .environment
                    .calendar
                    .day
                    .0
                    .checked_mul(3)
                    .ok_or(Error::Overflow)?,
            ),
        );
        sim.state.due.clear();
        sim.state.due.insert(
            SimTime(
                sim.state
                    .time
                    .0
                    .checked_add(MINUTE.0)
                    .ok_or(Error::Overflow)?,
            ),
            vec![Due::Review],
        );
        let mut chunks: Vec<_> = sim
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
            .chunks(plan.settlement_size)
            .map(|c| c.to_vec())
            .collect();
        if chunks.len() > 1 && chunks.last().unwrap().len() < 15 {
            let last = chunks.pop().unwrap();
            let previous = chunks.last_mut().unwrap();
            previous.extend(last);
            if previous.len() > 128 {
                let tail = previous.split_off(previous.len() / 2);
                chunks.push(tail);
            }
        }
        let mut settlements: Vec<Settlement> = Vec::new();
        for (index, residents) in chunks.into_iter().enumerate() {
            let areas = if index == 0 {
                [
                    sim.state.areas[0].id,
                    sim.state.areas[1].id,
                    sim.state.areas[2].id,
                ]
            } else {
                let mut ids = [AreaId(0); 3];
                for (slot, suffix) in ["settlement", "fields", "wood"].into_iter().enumerate() {
                    let id = AreaId(allocate(&mut sim.state)?);
                    ids[slot] = id;
                    sim.state.areas.push(Area {
                        id,
                        name: format!("Reach{} {suffix}", index + 1),
                        links: Vec::new(),
                    });
                }
                for (a, b, minutes) in [(0, 1, 2), (0, 2, 3)] {
                    sim.population_link(ids[a], ids[b], SimDuration(minutes * MINUTE.0))?;
                }
                sim.population_link(
                    settlements[index - 1].center,
                    ids[0],
                    SimDuration(30 * MINUTE.0),
                )?;
                ids
            };
            settlements.push(Settlement {
                center: areas[0],
                fields: areas[1],
                wood: areas[2],
                residents,
            });
        }
        let day = 86_400_000;
        sim.initialize_physical_people(physical::Rules {
            hunger_harm: 3 * day,
            fatigue_limit: 16 * 3_600_000,
            max_health: 2 * day,
            meal_relief: day,
            meal_size: Quantity(1_000),
            eating_time: MINUTE,
            loading_time: MINUTE,
            carrying_capacity: Quantity(10_000),
        })?;
        let mut minds = BTreeMap::new();
        let mut lives = BTreeMap::new();
        let year = config
            .environment
            .calendar
            .day
            .0
            .checked_mul(config.environment.calendar.year_days())
            .ok_or(Error::Overflow)?;
        for settlement in &settlements {
            let routes = sim
                .state
                .areas
                .iter()
                .filter(|a| [settlement.center, settlement.fields, settlement.wood].contains(&a.id))
                .map(|a| {
                    (
                        a.id,
                        a.links
                            .iter()
                            .filter(|l| {
                                [settlement.center, settlement.fields, settlement.wood]
                                    .contains(&l.destination)
                            })
                            .map(|l| (l.destination, l.duration))
                            .collect(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            for (local, actor) in settlement.residents.iter().copied().enumerate() {
                let p = &mut sim.state.people[sim.person_index[&actor]];
                *sim.state
                    .components
                    .location_mut(p.id)
                    .expect("location exists") =
                    Location::AtArea(if mixed && local % 4 == 1 && p.role == Role::Citizen {
                        settlement.fields
                    } else {
                        settlement.center
                    });
                p.work_rate = 1_000;
                let personality = if local % 3 == 1 {
                    decisions::Personality::restful()
                } else {
                    decisions::Personality::industrious()
                };
                let dream = if local % 3 == 1 {
                    decisions::Aspiration::Comfort
                } else {
                    decisions::Aspiration::Independence
                };
                minds.insert(
                    actor,
                    mind(personality, dream, routes.clone(), sim.state.time),
                );
                let age = if mixed
                    && local > 6
                    && local % 25 == 24
                    && config.biology.lifespan.0 > config.environment.calendar.day.0
                {
                    config.biology.lifespan.0 - config.environment.calendar.day.0
                } else if mixed && local > 6 && local % 10 == 0 {
                    config.biology.adulthood.0 * (5 + actor.0 % 13) / 18
                } else {
                    config
                        .biology
                        .adulthood
                        .0
                        .checked_add(
                            year.checked_mul(8 + actor.0 % 30)
                                .ok_or(Error::Overflow)?
                                .min((config.biology.elderhood.0 - config.biology.adulthood.0) / 2),
                        )
                        .ok_or(Error::Overflow)?
                };
                lives.insert(
                    actor,
                    development::Life {
                        id: actor,
                        born: sim.state.time,
                        initial_age: SimDuration(age),
                        stage: config.biology.stage_at(age),
                        reproduction: if p.role == Role::Guardian {
                            development::Reproduction::Neither
                        } else if local % 2 == 0 {
                            development::Reproduction::SuppliesSeed
                        } else {
                            development::Reproduction::Gestates
                        },
                        parents: Vec::new(),
                        caregivers: BTreeSet::new(),
                        preference: None,
                    },
                );
            }
        }
        sim.initialize_decision_people(minds)?;
        sim.initialize_life_people(config.biology.clone(), lives)?;
        let weather_day = sim.state.time.0 / config.environment.calendar.day.0;
        sim.state.environment = Some(environment::Runtime::new(
            config.environment.clone(),
            sim.state.time,
            seed,
        )?);
        sim.register_weather_cells()?;
        sim.state.agriculture = Some(game_agriculture::Runtime::from_content(
            crop_document,
            weather_day,
        )?);
        let first_food = *sim.state.inventories.keys().next_back().unwrap();
        let work_concept = sim.state.decisions.as_ref().unwrap().work_concept;
        for (index, settlement) in settlements.iter().enumerate() {
            let access = settlement
                .residents
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            let reserve = if mixed {
                3_000_000_u64.max(settlement.residents.len() as u64 * 180_000)
            } else {
                3_000_000
            };
            let food = if index == 0 {
                let stock = sim.state.inventories.get_mut(&first_food).unwrap();
                stock.quantity = Quantity(reserve);
                stock.capacity = Quantity(reserve * 4);
                stock.access = access.clone();
                first_food
            } else {
                sim.add_inventory(
                    Good::Food,
                    InventoryLocation::Area(settlement.center),
                    Quantity(reserve),
                    Quantity(reserve * 4),
                    access.clone(),
                )?
            };
            let timber = sim.add_inventory(
                Good::Timber,
                InventoryLocation::Area(settlement.center),
                Quantity(if mixed { 200_000 } else { 20_000 }),
                Quantity(if mixed { 1_000_000 } else { 100_000 }),
                access.clone(),
            )?;
            let tool = sim.add_inventory(
                Good::Tool,
                InventoryLocation::Area(settlement.center),
                Quantity(if mixed { 8_000 } else { 1_000 }),
                Quantity(100_000),
                access.clone(),
            )?;
            let wood_tool = sim.add_inventory(
                Good::Tool,
                InventoryLocation::Area(settlement.wood),
                Quantity(if mixed { 4_000 } else { 1_000 }),
                Quantity(100_000),
                access.clone(),
            )?;
            let _deposit = sim.add_inventory(
                Good::Timber,
                InventoryLocation::Area(settlement.wood),
                Quantity(if mixed { 200_000 } else { 20_000 }),
                Quantity(if mixed { 1_000_000 } else { 100_000 }),
                access.clone(),
            )?;
            let _wood_output = sim.add_inventory(
                Good::Timber,
                InventoryLocation::Area(settlement.wood),
                Quantity(0),
                Quantity(100_000),
                access.clone(),
            )?;
            for actor in &settlement.residents {
                sim.add_inventory(
                    Good::Timber,
                    InventoryLocation::Carried(*actor),
                    Quantity(0),
                    Quantity(10_000),
                    BTreeSet::from([*actor]),
                )?;
            }
            let seed_stock = sim.add_inventory(
                Good::Seed,
                InventoryLocation::Area(settlement.fields),
                Quantity(if mixed { 100_000 } else { 10_000 }),
                Quantity(1_000_000),
                access.clone(),
            )?;
            let farm_tool = sim.add_inventory(
                Good::Tool,
                InventoryLocation::Area(settlement.fields),
                Quantity(if mixed { 8_000 } else { 1_000 }),
                Quantity(if mixed { 100_000 } else { 1_000 }),
                access.clone(),
            )?;
            let water = sim.add_inventory(
                Good::Water,
                InventoryLocation::Area(settlement.fields),
                Quantity(if mixed { 300_000 } else { 30_000 }),
                Quantity(1_000_000),
                access.clone(),
            )?;
            let farm_food = sim.add_inventory(
                Good::Food,
                InventoryLocation::Area(settlement.fields),
                Quantity(reserve),
                Quantity(reserve * 4),
                access.clone(),
            )?;
            let field_count = if mixed {
                settlement.residents.len().div_ceil(20) + 1
            } else {
                2
            };
            for field_index in 0..field_count {
                let private = field_index + 1 == field_count;
                let id = SiteId(allocate(&mut sim.state)?);
                let permission = if private {
                    BTreeSet::from([settlement.residents[0]])
                } else {
                    access.clone()
                };
                let field_tool = if private {
                    sim.add_inventory(
                        Good::Tool,
                        InventoryLocation::Area(settlement.fields),
                        Quantity(1_000),
                        Quantity(1_000),
                        permission.clone(),
                    )?
                } else {
                    farm_tool
                };
                sim.state
                    .agriculture
                    .as_mut()
                    .unwrap()
                    .insert_field(
                        &mut sim.state.components,
                        agriculture::Field {
                            id,
                            area: settlement.fields,
                            access: permission,
                            custodian: private.then_some(settlement.residents[0]),
                            seed: seed_stock,
                            tool: field_tool,
                            water,
                            food: farm_food,
                        },
                    )
                    .expect("validated component insertion");
                let world = sim.state.agriculture.as_mut().unwrap();
                world
                    .insert_crop(
                        &mut sim.state.components,
                        id,
                        agriculture::Crop::new(world.catalog.default_crop.clone()),
                    )
                    .expect("validated component insertion");
                world
                    .insert_soil(
                        &mut sim.state.components,
                        id,
                        agriculture::Soil {
                            fertility: 900,
                            water: 75,
                            capacity: 120,
                        },
                    )
                    .expect("validated component insertion");
            }
            // Initial existing residents remember their own settlement's public
            // supplies. No global inventory/map broadcast is performed.
            let reporter = settlement.residents[3];
            let visible:Vec<_>=sim.state.inventories.values().filter(|stock|matches!(stock.location,InventoryLocation::Area(area) if [settlement.center,settlement.fields,settlement.wood].contains(&area))).cloned().collect();
            for actor in &settlement.residents {
                let mind = sim
                    .state
                    .decisions
                    .as_mut()
                    .unwrap()
                    .minds
                    .get_mut(actor)
                    .unwrap();
                for stock in &visible {
                    let InventoryLocation::Area(area) = stock.location else {
                        unreachable!()
                    };
                    mind.knowledge.insert(
                        stock.id,
                        StockKnowledge {
                            inventory: stock.id,
                            area,
                            good: stock.good,
                            quantity: stock.quantity,
                            accessible: stock.access.contains(actor),
                            observed_at: sim.state.time,
                            learned_at: sim.state.time,
                            source: KnowledgeSource::Reported(reporter),
                        },
                    );
                }
                mind.values.insert(
                    work_concept,
                    decisions::Attachment {
                        weight: 300,
                        experiences: 0,
                    },
                );
            }
            let _ = (food, timber, tool, wood_tool);
        }
        sim.dependent_index.clear();
        for person in sim.state.people.clone() {
            if let Location::AtArea(area) = *sim
                .state
                .components
                .location(person.id)
                .expect("location exists")
            {
                sim.index_dependent(person.id, area);
            }
        }
        Ok((sim, settlements))
    }
    fn population_link(
        &mut self,
        a: AreaId,
        b: AreaId,
        duration: SimDuration,
    ) -> Result<(), Error> {
        for (from, to) in [(a, b), (b, a)] {
            let area = self
                .state
                .areas
                .iter_mut()
                .find(|area| area.id == from)
                .ok_or(Error::UnknownArea)?;
            area.links.push(Link {
                destination: to,
                duration,
            });
            area.links.sort_by_key(|l| l.destination);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn world(workers: usize) -> Simulation {
        Simulation::populated_world(
            73,
            Execution {
                workers,
                reverse_jobs: workers > 1,
            },
            scenarios::Config::default(),
            Plan {
                people: 45,
                settlement_size: 15,
            },
        )
        .unwrap()
    }
    #[test]
    fn existing_residents_have_distinct_local_worlds_and_current_saves() {
        let sim = world(1);
        assert_eq!(sim.state.people.len(), 45);
        assert_eq!(sim.state.areas.len(), 9);
        assert_eq!(sim.state.components.bodies().count(), 45);
        assert_eq!(sim.state.development.as_ref().unwrap().lives.len(), 45);
        assert_eq!(sim.state.decisions.as_ref().unwrap().minds.len(), 45);
        assert!(sim.state.armies.is_some() && sim.state.realm.is_some());
        assert!(
            sim.state
                .development
                .as_ref()
                .unwrap()
                .pregnancies
                .values()
                .all(|p| matches!(p.state, development::PregnancyState::Conceiving { .. }))
        );
        for mind in sim.state.decisions.as_ref().unwrap().minds.values() {
            assert!(mind.routes.len() <= 3);
        }
        assert_eq!(
            sim.state.health.as_ref().unwrap().occupancy.len()
                + sim
                    .state
                    .people
                    .iter()
                    .filter(|p| matches!(
                        *sim.state
                            .components
                            .location(p.id)
                            .expect("location exists"),
                        Location::InTransit { .. }
                    ))
                    .count(),
            45
        );
        let loaded = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(sim.canonical_state(), loaded.canonical_state());
    }
    #[test]
    fn ordinary_work_and_supplies_are_worker_and_chunk_independent() {
        let mut one = world(1);
        let mut many = world(3);
        let start = one.state.time;
        one.advance(SimDuration(120 * MINUTE.0)).unwrap();
        for _ in 0..120 {
            many.advance(MINUTE).unwrap();
        }
        assert_eq!(one.canonical_state(), many.canonical_state());
        assert!(one.state.events.iter().any(|event| event.time > start
            && matches!(
                event.kind,
                EventKind::ResourceCollected { .. }
                    | EventKind::Produced { .. }
                    | EventKind::Transferred { .. }
                    | EventKind::Consumed { .. }
                    | EventKind::Departed { .. }
            )));
        assert!(
            one.state
                .components
                .bodies()
                .all(|(_, body)| body.vitals.alive)
        );
        let loaded = Simulation::load(&one.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(one.canonical_state(), loaded.canonical_state());
    }
    #[test]
    fn population_growth_still_requires_actual_conception_and_gestation() {
        let config = scenarios::Config {
            biology: development::Rules {
                conception_time: MINUTE,
                gestation: SimDuration(10 * MINUTE.0),
                infancy: SimDuration(5 * MINUTE.0),
                adulthood: SimDuration(20 * MINUTE.0),
                elderhood: SimDuration(60 * MINUTE.0),
                lifespan: SimDuration(90 * MINUTE.0),
            },
            ..scenarios::Config::default()
        };
        let mut one = Simulation::populated_world(
            73,
            Execution::default(),
            config.clone(),
            Plan {
                people: 30,
                settlement_size: 15,
            },
        )
        .unwrap();
        let mut many = Simulation::populated_world(
            73,
            Execution {
                workers: 3,
                reverse_jobs: true,
            },
            config,
            Plan {
                people: 30,
                settlement_size: 15,
            },
        )
        .unwrap();
        assert_eq!(one.state.people.len(), 30);
        one.advance(SimDuration(15 * MINUTE.0)).unwrap();
        for _ in 0..15 {
            many.advance(MINUTE).unwrap();
        }
        assert_eq!(one.canonical_state(), many.canonical_state());
        assert!(one.state.people.len() > 30);
        for event in &one.state.events {
            if let EventKind::Born {
                pregnancy,
                child,
                parents,
            } = &event.kind
            {
                let conceived = one
                    .state
                    .events
                    .iter()
                    .find(|e| {
                        matches!(e.kind,
                    EventKind::Conceived { pregnancy: id, .. } if id == *pregnancy)
                    })
                    .unwrap();
                assert_eq!(event.time.0 - conceived.time.0, 10 * MINUTE.0);
                assert!(
                    one.state
                        .institutions
                        .as_ref()
                        .unwrap()
                        .memories
                        .contains_key(child)
                );
                assert_eq!(
                    one.state.development.as_ref().unwrap().lives[child].parents,
                    *parents
                );
            }
        }
        let portions = &one.state.property.as_ref().unwrap().portions;
        for (inventory, stock) in &one.state.inventories {
            assert_eq!(
                portions[inventory]
                    .values()
                    .map(|quantity| u128::from(*quantity))
                    .sum::<u128>(),
                u128::from(stock.quantity.0),
                "physical stock {inventory:?} retains exactly conserved title portions"
            );
        }
        Simulation::load(&one.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn compressed_biology_is_initialized_at_the_configured_actual_age() {
        let config = scenarios::Config {
            biology: development::Rules {
                conception_time: MINUTE,
                gestation: SimDuration(10 * MINUTE.0),
                infancy: SimDuration(5 * MINUTE.0),
                adulthood: SimDuration(20 * MINUTE.0),
                elderhood: SimDuration(60 * MINUTE.0),
                lifespan: SimDuration(90 * MINUTE.0),
            },
            ..scenarios::Config::default()
        };
        let sim = Simulation::populated_world(
            73,
            Execution::default(),
            config,
            Plan {
                people: 30,
                settlement_size: 15,
            },
        )
        .unwrap();
        assert!(
            sim.state
                .development
                .as_ref()
                .unwrap()
                .lives
                .values()
                .all(|life| matches!(life.stage, LifeStage::Child | LifeStage::Adult))
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
}

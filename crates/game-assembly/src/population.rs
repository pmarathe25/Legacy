//! Initial people are authored existing residents. Once this snapshot is
//! published, population growth uses ordinary conception and birth exclusively.
use crate::*;

pub(crate) use game_openings::PopulationPlan as Plan;
/// Authoring IDs, not a second registry of the live population.
#[derive(Clone, Debug)]
pub struct Settlement {
    pub center: AreaId,
    pub fields: AreaId,
    pub wood: AreaId,
    pub residents: Vec<PersonId>,
    pub areas: BTreeMap<String, AreaId>,
    pub stocks: BTreeMap<String, InventoryId>,
    pub profile: String,
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
        let content = authored::Content::resolve(&config)?;
        let (mut sim, settlements) =
            Self::initial_population(seed, execution, &config, plan, true, &content, "populated")?;
        sim.host.state.scenario = ScenarioInfo {
            id: "settlement-founder".into(),
            version: 1,
            debug_fixture: false,
        };
        sim.install_playable_domains(&config, "populated")?;
        sim.initialize_transport(config.transport.clone())?;
        sim.initialize_realm(realm::Rules::default())?;
        sim.initialize_armies(armies::Rules::default())?;
        sim.host.state.ecology = Some(ecology::Runtime {
            rules: config.ecology.clone(),
            day: sim.host.clock().time.0 / config.environment.calendar.day.0,
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
                let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                    &mut sim.host.storage,
                    actor,
                )
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
                sim.host
                    .state
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
            let source = game_cargo::inventories(&sim.host.storage)
                .map(|(_, row)| row)
                .find(|stock| {
                    stock.good == Good::Timber
                        && stock.location == InventoryLocation::Area(settlement.wood)
                        && stock.quantity.0 > 0
                })
                .unwrap()
                .id;
            let destination = game_cargo::inventories(&sim.host.storage)
                .map(|(_, row)| row)
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
                        deadline: SimTime(sim.host.clock().time.0 + 6 * 3_600_000),
                    }],
                    procedure: None,
                },
            )?;
            let agreement = *sim
                .host
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
        let owner_now = self.host.clock().time;
        let access = settlement
            .residents
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let steward = settlement.residents[3];
        let healer = settlement.residents[6];
        let lots:Vec<_>=game_cargo::inventories(&self.host.storage).map(|(_, row)| row).filter(|stock|matches!(stock.location,InventoryLocation::Area(area) if [settlement.center,settlement.fields,settlement.wood].contains(&area))).map(|stock|stock.id).collect();
        for id in &lots {
            self.host
                .state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(*id, steward);
        }
        for actor in &settlement.residents {
            if game_projects::knowledge(&self.host.storage, *actor).is_none() {
                game_projects::insert_knowledge(
                    &mut self.host.storage,
                    *actor,
                    game_projects::Knowledge::default(),
                )?;
            }
            game_projects::knowledge_mut(&mut self.host.storage, *actor)
                .unwrap()
                .timber_hints
                .insert(settlement.wood);
        }
        let definition = self
            .host
            .state
            .openings
            .as_ref()
            .unwrap()
            .catalog
            .settlements[&settlement.profile]
            .clone();
        let authored = definition.plan(settlement.residents.len())?;
        self.bind_authored_custody(&authored, settlement)?;
        self.bind_authored_resources(&authored, settlement)?;
        self.debug_add_plot(settlement.fields, Some(steward), access.clone(), 900)?;
        let herbs = settlement.stocks["herbs"];
        let water = settlement.stocks["drinking-water"];
        for id in [herbs, water] {
            self.host
                .state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(id, healer);
        }
        self.collected_water_quality(
            water,
            game_cargo::inventory(&self.host.storage, water)
                .unwrap()
                .quantity,
            0,
        );
        for actor in &settlement.residents {
            self.seed_water_safety(*actor, water)?;
        }
        let levels = 10.min(config.skills.max_level);
        game_skills::skills_mut(&mut self.host.storage, healer)
            .unwrap()
            .records
            .insert(
                Skill::Medicine,
                skills::Progress {
                    practice_ms: config.skills.per_level.0 * u64::from(levels),
                    level: levels,
                    progress_ms: 0,
                },
            );
        game_characters::decisions::mind_mut::<game_api::Command>(&mut self.host.storage, healer)
            .unwrap()
            .personality
            .empathy = 900;
        game_characters::decisions::mind_mut::<game_api::Command>(&mut self.host.storage, healer)
            .unwrap()
            .dream = decisions::Aspiration::SupportOthers;
        let mut reported_sites = Vec::new();
        for workshop in &definition.workshops {
            let sponsor = settlement.residents[workshop.sponsor];
            let area = settlement.areas[&workshop.area];
            let materials = settlement.stocks[&workshop.materials];
            let input = settlement.stocks[&workshop.input];
            let tool = settlement.stocks[&workshop.tool];
            let output = settlement.stocks[&workshop.output];
            let recipe = game_workshop::RecipeId(workshop.recipe.clone());
            let site = self.bind_authored_workshop(workshop, settlement)?;
            let rules = self.host.state.projects.as_ref().unwrap().rules.clone();
            let project = ProjectId(self.host.allocate_id()?.0);
            let enterprise = EnterpriseId(self.host.allocate_id()?.0);
            let beneficiaries: BTreeSet<_> = workshop
                .beneficiaries
                .iter()
                .map(|i| settlement.residents[*i])
                .collect();
            let workers: BTreeSet<_> = workshop
                .workers
                .iter()
                .map(|i| settlement.residents[*i])
                .collect();
            let workshop_history = projects::Project {
                id: project,
                kind: ProjectKind::Workshop {
                    recipe: recipe.clone(),
                },
                sponsor,
                area,
                materials,
                tool,
                input,
                output,
                beneficiaries: beneficiaries.clone(),
                workers: workers.clone(),
                support: BTreeSet::new(),
                status: ProjectStatus::Completed,
                blocker: None,
                progress_ms: rules.construction_labor.0,
                contributions: BTreeMap::from([(sponsor, rules.construction_labor.0)]),
                materials_consumed: true,
                finish_by: owner_now,
                enterprise: Some(enterprise),
            };
            game_projects::insert_project(&mut self.host.storage, project, workshop_history)?;
            game_projects::insert_enterprise(
                &mut self.host.storage,
                enterprise,
                projects::Enterprise {
                    id: enterprise,
                    project,
                    sponsor,
                    area,
                    site,
                    kind: ProjectKind::Workshop {
                        recipe: recipe.clone(),
                    },
                    beneficiaries: beneficiaries.clone(),
                    workers,
                    status: EnterpriseStatus::Ready,
                    blocker: None,

                    inputs: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    labor: BTreeMap::new(),
                    operating_agreements: BTreeSet::new(),
                },
            )?;
            // Existing workshops have an actual material cost in their initial
            // history; subsequent recipes consume their own real input lots.
            let stock = game_cargo::inventory_mut(&mut self.host.storage, materials).unwrap();
            stock.quantity = Quantity(
                stock
                    .quantity
                    .0
                    .checked_sub(rules.construction_timber.0)
                    .ok_or_else(|| {
                        Error::InvalidConfiguration(
                            "authored construction stock is insufficient".into(),
                        )
                    })?,
            );
            self.emit(EventKind::Consumed {
                actor: sponsor,
                source: materials,
                quantity: rules.construction_timber,
            });
            self.emit(EventKind::ConstructionStarted {
                project,
                actor: sponsor,
            });
            self.emit(EventKind::ProjectCompleted {
                project,
                enterprise,
                site,
            });
            reported_sites.push(decisions::SiteKnowledge {
                site,
                area: settlement.areas[&workshop.area],
                input,
                tool,
                input_quantity: self.host.state.workshops.recipe(&recipe)?.input_quantity,
                output,
                access: false,
            });
        }
        for area in [settlement.center, settlement.fields] {
            let occupants: Vec<_> = settlement
                .residents
                .iter()
                .copied()
                .filter(|id| {
                    *game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId((self.host.state.people[self.person_index[id]].id).0),
                    )
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
                    SimTime(owner_now.0.saturating_sub(2 * 86_400_000)),
                )?;
                for person in chunk {
                    self.set_initial_shelter_occupancy(*person, shelter)?;
                }
            }
        }
        let reported:Vec<_>=game_cargo::inventories(&self.host.storage).map(|(_, row)| row).filter(|stock|matches!(stock.location,InventoryLocation::Area(area) if [settlement.center,settlement.fields,settlement.wood].contains(&area))).cloned().collect();
        for actor in &settlement.residents {
            let sites = reported_sites
                .iter()
                .map(|view| {
                    let mut view = view.clone();
                    view.access = game_workshop::site(&self.host.storage, view.site)
                        .expect("site exists")
                        .access
                        .contains(actor);
                    view
                })
                .collect::<Vec<_>>();
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut self.host.storage,
                *actor,
            )
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
                        observed_at: owner_now,
                        learned_at: owner_now,
                        source: KnowledgeSource::Reported(steward),
                    },
                );
            }
            for view in sites {
                mind.sites.insert(view.site, view);
            }
        }
        Ok(())
    }
    pub(crate) fn initial_population(
        seed: u64,
        execution: Execution,
        config: &scenarios::Config,
        plan: Plan,
        mixed: bool,
        content: &authored::Content,
        profile: &str,
    ) -> Result<(Self, Vec<Settlement>), Error> {
        plan.validate()?;
        let definition = content
            .openings
            .catalog
            .settlements
            .get(profile)
            .ok_or_else(|| Error::InvalidConfiguration("unknown settlement content".into()))?;
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
        sim.host.state.openings = Some(content.openings.clone());
        sim.host.state.workshops = content.workshops.clone();
        sim.host
            .state
            .content
            .insert("openings".into(), content.openings.content.clone());
        sim.host
            .state
            .content
            .insert("workshops".into(), content.workshops.content.clone());
        sim.host.publish_clock(game_sim::Clock {
            time: SimTime(
                (3 * 86_400_000).max(
                    config
                        .environment
                        .calendar
                        .day
                        .0
                        .checked_mul(3)
                        .ok_or(Error::Overflow)?,
                ),
            ),
            ..sim.host.clock()
        });
        sim.host.state.due.clear();
        sim.host.state.due.insert(
            SimTime(
                sim.host
                    .clock()
                    .time
                    .0
                    .checked_add(MINUTE.0)
                    .ok_or(Error::Overflow)?,
            ),
            vec![Due::Review],
        );
        let mut chunks: Vec<_> = sim
            .host
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
                    sim.host.state.areas[0].id,
                    sim.host.state.areas[1].id,
                    sim.host.state.areas[2].id,
                ]
            } else {
                let mut ids = [AreaId(0); 3];
                for area in &mut ids {
                    let id = AreaId(sim.host.allocate_id()?.0);
                    *area = id;
                    sim.host.state.areas.push(Area {
                        id,
                        name: String::new(),
                        links: Vec::new(),
                    });
                }
                sim.population_link(
                    settlements[index - 1].center,
                    ids[0],
                    SimDuration(30 * MINUTE.0),
                )?;
                ids
            };
            let mut authored_areas = BTreeMap::new();
            for area in &definition.areas {
                let id = if area.key == definition.centre {
                    areas[0]
                } else if area.key == definition.fields {
                    areas[1]
                } else if area.key == definition.wood {
                    areas[2]
                } else {
                    let id = AreaId(sim.host.allocate_id()?.0);
                    sim.host.state.areas.push(Area {
                        id,
                        name: String::new(),
                        links: Vec::new(),
                    });
                    id
                };
                let name = area.name.replace("{n}", &(index + 1).to_string());
                sim.host
                    .state
                    .areas
                    .iter_mut()
                    .find(|row| row.id == id)
                    .unwrap()
                    .name = name;
                authored_areas.insert(area.key.clone(), id);
            }
            for area in sim
                .host
                .state
                .areas
                .iter_mut()
                .filter(|area| authored_areas.values().any(|id| *id == area.id))
            {
                area.links
                    .retain(|link| !authored_areas.values().any(|id| *id == link.destination));
            }
            for link in &definition.links {
                sim.population_link(
                    authored_areas[&link.from],
                    authored_areas[&link.to],
                    link.duration,
                )?;
            }
            settlements.push(Settlement {
                center: areas[0],
                fields: areas[1],
                wood: areas[2],
                residents,
                areas: authored_areas,
                stocks: BTreeMap::new(),
                profile: profile.into(),
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
                .host
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
            let authored_now = sim.time();
            for (local, actor) in settlement.residents.iter().copied().enumerate() {
                let p = &mut sim.host.state.people[sim.person_index[&actor]];
                *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId((p.id).0))
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
                    mind(personality, dream, routes.clone(), authored_now),
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
                        born: authored_now,
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
        let weather_day = sim.host.clock().time.0 / config.environment.calendar.day.0;
        sim.host.state.environment = Some(environment::Runtime::new(
            config.environment.clone(),
            sim.host.clock().time,
            seed,
        )?);
        sim.register_weather_cells()?;
        sim.host.state.agriculture = Some(game_agriculture::Runtime::from_content(
            crop_document,
            weather_day,
        )?);
        let first_food = game_cargo::inventories(&sim.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap();
        let work_concept = sim.host.state.decisions.as_ref().unwrap().work_concept;
        for (index, settlement) in settlements.iter_mut().enumerate() {
            let access = settlement
                .residents
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            let authored = definition.plan(settlement.residents.len())?;
            settlement.stocks = sim.bind_authored_stocks(
                &authored,
                settlement,
                (index == 0).then_some(first_food),
            )?;
            let food = settlement.stocks["food"];
            let timber = settlement.stocks["timber"];
            let tool = settlement.stocks["tool"];
            let wood_tool = settlement.stocks["wood-tool"];
            let seed_stock = settlement.stocks["seed"];
            let farm_tool = settlement.stocks["farm-tool"];
            let water = settlement.stocks["water"];
            let farm_food = settlement.stocks["farm-food"];
            let field_count = if mixed {
                settlement.residents.len().div_ceil(20) + 1
            } else {
                2
            };
            for field_index in 0..field_count {
                let private = field_index + 1 == field_count;
                let id = SiteId(sim.host.allocate_id()?.0);
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
                sim.host
                    .state
                    .agriculture
                    .as_mut()
                    .unwrap()
                    .insert_field(
                        &mut sim.host.storage,
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
                let world = sim.host.state.agriculture.as_mut().unwrap();
                world
                    .insert_crop(
                        &mut sim.host.storage,
                        id,
                        agriculture::Crop::new(world.catalog.default_crop.clone()),
                    )
                    .expect("validated component insertion");
                world
                    .insert_soil(
                        &mut sim.host.storage,
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
            let visible:Vec<_>=game_cargo::inventories(&sim.host.storage).map(|(_, row)| row).filter(|stock|matches!(stock.location,InventoryLocation::Area(area) if [settlement.center,settlement.fields,settlement.wood].contains(&area))).cloned().collect();
            let observed_at = sim.time();
            for actor in &settlement.residents {
                let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                    &mut sim.host.storage,
                    *actor,
                )
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
                            observed_at,
                            learned_at: observed_at,
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
        for person in sim.host.state.people.clone() {
            if let Location::AtArea(area) =
                *game_spatial::location(&sim.host.storage, game_ecs::StableId((person.id).0))
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
                .host
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
        assert_eq!(sim.host.state.people.len(), 45);
        assert_eq!(sim.host.state.areas.len(), 9);
        assert_eq!(game_physiology::bodies(&sim.host.storage).count(), 45);
        assert_eq!(game_lifecycle::all_lives(&sim.host.storage).count(), 45);
        assert_eq!(
            game_characters::decisions::minds::<game_api::Command>(&sim.host.storage).count(),
            45
        );
        assert!(sim.host.state.armies.is_some() && sim.host.state.realm.is_some());
        assert!(
            game_lifecycle::all_pregnancies(&sim.host.storage)
                .map(|(_, row)| row)
                .all(|p| matches!(p.state, development::PregnancyState::Conceiving { .. }))
        );
        for mind in game_characters::decisions::minds::<game_api::Command>(&sim.host.storage)
            .map(|(_, mind)| mind)
        {
            assert!(mind.routes.len() <= 3);
        }
        assert_eq!(
            game_health::occupancies(&sim.host.storage).count()
                + sim
                    .host
                    .state
                    .people
                    .iter()
                    .filter(|p| matches!(
                        *game_spatial::location(&sim.host.storage, game_ecs::StableId((p.id).0))
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
        let start = one.host.clock().time;
        one.advance(SimDuration(120 * MINUTE.0)).unwrap();
        for _ in 0..120 {
            many.advance(MINUTE).unwrap();
        }
        assert_eq!(one.canonical_state(), many.canonical_state());
        assert!(one.host.events().iter().any(|event| event.at > start
            && matches!(
                event.event,
                EventKind::ResourceCollected { .. }
                    | EventKind::Produced { .. }
                    | EventKind::Transferred { .. }
                    | EventKind::Consumed { .. }
                    | EventKind::Departed { .. }
            )));
        assert!(game_physiology::bodies(&one.host.storage).all(|(_, body)| body.vitals.alive));
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
        assert_eq!(one.host.state.people.len(), 30);
        one.advance(SimDuration(15 * MINUTE.0)).unwrap();
        for _ in 0..15 {
            many.advance(MINUTE).unwrap();
        }
        assert_eq!(one.canonical_state(), many.canonical_state());
        assert!(one.host.state.people.len() > 30);
        for event in one.host.events() {
            if let EventKind::Born {
                pregnancy,
                child,
                parents,
            } = &event.event
            {
                let conceived = one
                    .host
                    .events()
                    .iter()
                    .find(|e| {
                        matches!(e.event,
                    EventKind::Conceived { pregnancy: id, .. } if id == *pregnancy)
                    })
                    .unwrap();
                assert_eq!(event.at.0 - conceived.at.0, 10 * MINUTE.0);
                assert!(game_society::knowledge(&one.host.storage, *child).is_some());
                assert_eq!(
                    game_lifecycle::life(&one.host.storage, *child)
                        .expect("life exists")
                        .parents,
                    *parents
                );
            }
        }
        for (inventory, stock) in game_cargo::inventories(&one.host.storage) {
            assert_eq!(
                game_property::portions(&one.host.storage, inventory)
                    .expect("stock provenance exists")
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
            game_lifecycle::all_lives(&sim.host.storage)
                .map(|(_, row)| row)
                .all(|life| matches!(life.stage, LifeStage::Child | LifeStage::Adult))
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
}

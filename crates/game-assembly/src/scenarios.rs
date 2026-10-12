//! Playable openings compose the ordinary domains. Their people, assets and
//! starting customs are authored content, rather than global government modes.
use crate::*;

#[cfg(test)]
pub const PLAYABLE: [&str; 3] = ["settlement-founder", "established-ruler", "child-successor"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub openings: game_content::DefinitionSource<game_openings::Catalog>,
    pub workshops: game_content::DefinitionSource<game_workshop::Catalog>,
    pub transport: game_transport::Rules,
    pub environment: environment::Config,
    pub agriculture: game_content::DefinitionSource<game_agriculture::Catalog>,
    pub ecology: game_ecology::EcologyRules,
    pub biology: development::Rules,
    pub skills: skills::Rules,
    pub mentorship: lineage::MentorshipRules,
    pub invocation: invocation::Rules,
    #[serde(default)]
    pub health: HealthRules,
    #[serde(default)]
    pub social: social::Rules,
    #[serde(default = "default_genetics")]
    pub genetics: Option<genetics::Config>,
}

fn default_genetics() -> Option<genetics::Config> {
    Some(genetics::Config::default())
}

impl Default for Config {
    fn default() -> Self {
        let hour = 3_600_000;
        Self {
            openings: authored::opening_source(),
            workshops: authored::workshop_source(),
            transport: game_transport::Rules::default(),
            environment: environment::Config::default(),
            agriculture: crate::agriculture::default_catalog(),
            ecology: game_ecology::EcologyRules::default(),
            biology: development::Rules::human(),
            skills: skills::Rules {
                per_level: SimDuration(hour),
                ..skills::Rules::default()
            },
            mentorship: lineage::MentorshipRules {
                lesson_duration: SimDuration(hour),
                ..lineage::MentorshipRules::default()
            },
            invocation: invocation::Rules::human(),
            health: HealthRules::human(),
            social: social::Rules::default(),
            genetics: default_genetics(),
        }
    }
}

/// Startup-only registry lookup. Saved worlds retain their own definitions.
pub fn playable_ids() -> Result<Vec<String>, Error> {
    let definitions =
        game_openings::Definitions::from_content(authored::opening_source().resolve()?)?;
    Ok(definitions.catalog.openings.keys().cloned().collect())
}
pub fn choose(seed: u64) -> Result<String, Error> {
    let definitions =
        game_openings::Definitions::from_content(authored::opening_source().resolve()?)?;
    Ok(definitions.catalog.choose(seed)?.to_owned())
}

impl Simulation {
    pub fn playable_scenario(
        id: &str,
        seed: u64,
        execution: Execution,
        config: Config,
    ) -> Result<Self, Error> {
        let content = authored::Content::resolve(&config)?;
        let id = if id == "random" {
            content.openings.catalog.choose(seed)?
        } else {
            id
        };
        let opening = content
            .openings
            .catalog
            .openings
            .get(id)
            .ok_or_else(|| Error::InvalidConfiguration("unknown playable opening".into()))?
            .clone();
        let biology = &config.biology;
        if biology.conception_time.0 == 0
            || biology.gestation.0 == 0
            || !(SimDuration(0) < biology.infancy
                && biology.infancy < biology.adulthood
                && biology.adulthood < biology.elderhood
                && biology.elderhood < biology.lifespan)
            || config.mentorship.lesson_duration.0 == 0
            || !(1..=1_000).contains(&config.mentorship.lesson_gain)
            || !(1..=1_000).contains(&config.mentorship.invocation_access_threshold)
        {
            return Err(Error::InvalidConfiguration(
                "invalid playable development rules".into(),
            ));
        }
        let (mut sim, settlements) = Self::initial_population(
            seed,
            execution,
            &config,
            population::Plan {
                people: opening.initial_residents,
                settlement_size: opening.initial_residents,
            },
            false,
            &content,
            &opening.settlement,
        )?;
        sim.host.publish_clock(game_sim::Clock {
            time: SimTime(
                config
                    .environment
                    .calendar
                    .day
                    .0
                    .checked_mul(3)
                    .ok_or(Error::Overflow)?
                    .max(3 * 24 * 3_600_000),
            ),
            ..sim.host.clock()
        });
        let epoch = sim.time();
        let seed = sim.host.seed();
        let environment = sim.host.state.environment.as_mut().unwrap();
        environment.reset_epoch(&mut sim.host.storage, epoch, seed)?;
        sim.host.state.scenario = ScenarioInfo {
            id: id.into(),
            version: 1,
            debug_fixture: false,
        };

        sim.prepare_playable_people(&config)?;
        sim.install_playable_domains(&config, &opening.settlement)?;
        sim.initialize_transport(config.transport.clone())?;
        sim.initialize_realm(realm::Rules::default())?;
        sim.initialize_armies(armies::Rules::default())?;
        sim.seed_playable_resources(&settlements[0])?;
        sim.seed_playable_workshop(opening.established_workshop, &settlements[0])?;
        if let Some(rules) = config.genetics.clone() {
            sim.initialize_genetics(rules)?;
        }
        sim.initialize_social(config.social.clone())?;
        sim.seed_playable_family_plans()?;
        sim.establish_starting_custom(opening.starting_customs)?;
        if opening.child_successor {
            sim.author_child_opening()?;
        }
        sim.seed_playable_shelters(&config.health, opening.established_workshop)?;
        sim.update_property_events()?;
        sim.observe_agreement_events();
        sim.update_projects()?;
        sim.reconcile_institutions()?;
        Ok(sim)
    }

    fn prepare_playable_people(&mut self, config: &Config) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        let village = self.host.state.areas[0].id;
        let year = config
            .environment
            .calendar
            .day
            .0
            .checked_mul(config.environment.calendar.year_days())
            .ok_or(Error::Overflow)?;
        let adult_extra = year
            .checked_mul(8)
            .ok_or(Error::Overflow)?
            .min((config.biology.elderhood.0 - config.biology.adulthood.0) / 2);
        let initial_age = SimDuration(
            config
                .biology
                .adulthood
                .0
                .checked_add(adult_extra)
                .ok_or(Error::Overflow)?,
        );
        for actor in self
            .host
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect::<Vec<_>>()
        {
            self.interrupt_actor_activity(actor);
            game_workshop::remove_job(&mut self.host.storage, actor).map(|_| ())?;
        }
        self.host.state.development.as_mut().unwrap().rules = config.biology.clone();
        for person in &mut self.host.state.people {
            *game_spatial::location_mut(
                &mut self.host.storage,
                game_ecs::StableId((person.id).0),
            )
            .expect("location exists") = Location::AtArea(village);
            game_transport::replace_route(&mut self.host.storage, person.id, Vec::new())
                .expect("actor route");
        }
        for person in game_lifecycle::all_lives(&self.host.storage)
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            let life =
                game_lifecycle::life_mut(&mut self.host.storage, person).expect("life exists");
            life.initial_age = initial_age;
            life.stage = LifeStage::Adult;
        }
        self.host.state.due.clear();
        self.host
            .state
            .due
            .entry(SimTime(
                owner_now.0.checked_add(MINUTE.0).ok_or(Error::Overflow)?,
            ))
            .or_default()
            .push(Due::Review);
        for person in self
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
            self.schedule_age(person);
        }
        // These residents remember the existing fields, granaries and routes.
        // The reports predate control; newborns acquire their own knowledge later.
        let reporter = self.host.state.people[3].id;
        for person in &self.host.state.people {
            let known: Vec<_> = game_cargo::inventories(&self.host.storage)
                .map(|(_, row)| row)
                .filter_map(|stock| {
                    let InventoryLocation::Area(area) = stock.location else {
                        return None;
                    };
                    Some(StockKnowledge {
                        inventory: stock.id,
                        area,
                        good: stock.good,
                        quantity: stock.quantity,
                        accessible: stock.access.contains(&person.id),
                        observed_at: owner_now,
                        learned_at: owner_now,
                        source: KnowledgeSource::Reported(reporter),
                    })
                })
                .collect();
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut self.host.storage,
                person.id,
            )
            .unwrap();
            mind.plan = None;
            mind.retry_after = owner_now;
            for fact in known {
                mind.knowledge.insert(fact.inventory, fact);
            }
        }
        let parents = [self.host.state.people[3].id, self.host.state.people[4].id];
        for (actor, partner) in [(parents[0], parents[1]), (parents[1], parents[0])] {
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut self.host.storage,
                actor,
            )
            .unwrap();
            mind.dream = decisions::Aspiration::Parenthood;
            mind.personality.empathy = 900;
            mind.relationships.insert(
                partner,
                decisions::Relationship {
                    trust: 800,
                    affection: 800,
                },
            );
        }
        // Shared cultivation is this settlement's starting land arrangement.
        // It does not prescribe property or caregiving for other societies.
        let residents: BTreeSet<_> = self.host.state.people.iter().map(|p| p.id).collect();
        let world = self.host.state.agriculture.as_mut().unwrap();
        let fields: Vec<_> = world.fields(&self.host.storage).map(|(id, _)| id).collect();
        for id in fields {
            let field = world.field_mut(&mut self.host.storage, id).unwrap();
            field.access = residents.clone();
            let tool = field.tool;
            game_cargo::inventory_mut(&mut self.host.storage, tool)
                .unwrap()
                .access = residents.clone();
        }
        Ok(())
    }

    pub(crate) fn install_playable_domains(
        &mut self,
        config: &Config,
        profile: &str,
    ) -> Result<(), Error> {
        let people: BTreeSet<_> = self.host.state.people.iter().map(|p| p.id).collect();
        let ruler = self.host.state.ruler;
        let village = self.host.state.areas[0].id;
        self.host.state.skills = Some(skills::Runtime::new(config.skills.clone())?);
        for person in &people {
            game_skills::insert_skills(
                &mut self.host.storage,
                *person,
                game_skills::Skills::default(),
            )?;
        }
        skills::validate(&self.canonical_state())?;
        let cap = config
            .skills
            .per_level
            .0
            .checked_mul(u64::from(config.skills.max_level))
            .ok_or(Error::Overflow)?;
        let guardians = self
            .host
            .state
            .people
            .iter()
            .filter(|person| person.role == Role::Guardian)
            .map(|person| {
                (
                    person.id,
                    lineage::Guardian {
                        target: Some(ruler),
                        sensed_location: Some(Location::AtArea(village)),
                        sensed_at: self.host.clock().time,
                        goal: lineage::GuardianGoal::Mentoring,
                        explanation: "Following the lineage; political offices remain separate"
                            .into(),
                        assessments: BTreeMap::new(),
                        assignment: None,
                        provision: None,
                        requested_delivery: None,
                        knows_invocation: true,
                    },
                )
            })
            .collect();
        for actor in self
            .host
            .state
            .people
            .iter()
            .filter(|p| p.role == Role::Guardian)
            .map(|p| p.id)
        {
            game_skills::skills_mut(&mut self.host.storage, actor)
                .unwrap()
                .records
                .insert(
                    Skill::Invocation,
                    skills::Progress {
                        practice_ms: cap,
                        level: config.skills.max_level,
                        progress_ms: 0,
                    },
                );
        }
        self.host.state.lineage = Some(lineage::World {
            incarnations: vec![lineage::Incarnation {
                person: ruler,
                bound_at: self.host.clock().time,
                died_at: None,
            }],
            waiting: None,
            guardians,
            rules: config.mentorship.clone(),
            invocation_learning: BTreeMap::new(),
        });
        self.host.state.combat = Some(combat::World {
            rules: combat::Rules {
                encounter_limit: 128,
                round: SimDuration(6_000),
                movement: 5_000,
                reach: 2_000,
                damage: self.host.state.physical.as_ref().unwrap().rules.max_health / 4,
                defence: self.host.state.physical.as_ref().unwrap().rules.max_health / 6,
                incapacitation: self.host.state.physical.as_ref().unwrap().rules.max_health / 4,
            },
            profiles: people
                .iter()
                .map(|person| {
                    (
                        *person,
                        combat::Profile {
                            initiative: if *person == ruler { 200 } else { 100 },
                        },
                    )
                })
                .collect(),
            maps: BTreeMap::new(),
            road_maps: BTreeMap::new(),
            encounters: BTreeMap::new(),
            captivity: BTreeMap::new(),
        });
        let custodians = game_cargo::inventories(&self.host.storage)
            .map(|(_, row)| row)
            .map(|stock| {
                (
                    stock.id,
                    match stock.location {
                        InventoryLocation::Carried(person) => person,
                        InventoryLocation::Area(_) => ruler,
                    },
                )
            })
            .collect();
        let __custody: BTreeMap<InventoryId, PersonId> = custodians;
        for (id, owner) in __custody {
            game_property::declare_custody(&mut self.host.storage, id, owner)?;
        }
        self.host.state.agreements = Some(game_agreements::Runtime {
            last_event: self.host.next_event() - 1,
            last_observation_event: self.host.next_event() - 1,
        });
        let mut property_rows = game_ecs::Batch::new();
        for person in &people {
            property_rows.insert(
                game_ecs::StableId(person.0),
                self.host
                    .state
                    .openings
                    .as_ref()
                    .unwrap()
                    .catalog
                    .settlements[profile]
                    .attitude
                    .clone(),
            );
        }
        for (id, stock) in game_cargo::inventories(&self.host.storage) {
            property_rows.insert(
                game_ecs::StableId(id.0),
                game_property::Provenance {
                    portions: if stock.quantity.0 == 0 {
                        BTreeMap::new()
                    } else {
                        BTreeMap::from([(ClaimId(0), stock.quantity.0)])
                    },
                },
            );
        }
        self.host.storage.apply_batch(property_rows)?;
        self.host.state.property = Some(game_property::Runtime {
            last_event: self.host.next_event() - 1,
        });
        self.host.state.institutions = Some(institutions::Runtime {
            last_event: self.host.next_event() - 1,
        });
        for person in &self.host.state.people {
            game_society::insert_knowledge(
                &mut self.host.storage,
                person.id,
                game_society::Knowledge::default(),
            )?;
        }
        self.host.state.projects = Some(game_projects::Runtime {
            rules: projects::Rules {
                construction_labor: SimDuration(16 * 3_600_000),
                ..projects::Rules::default()
            },
            last_event: self.host.next_event() - 1,
        });
        for person in &people {
            game_projects::insert_knowledge(
                &mut self.host.storage,
                *person,
                game_projects::Knowledge {
                    timber_hints: BTreeSet::from([self.host.state.areas[2].id]),
                    ..game_projects::Knowledge::default()
                },
            )?;
        }
        self.host.state.invocation = Some(invocation::World {
            control: game_invocation::ControlState::new(config.invocation.clone())?,
        });
        self.host.state.force = Some(force::World {
            yielded: BTreeMap::new(),
            rules: force::Rules::default(),
            escapes: BTreeMap::new(),
            restraints: BTreeMap::new(),
            last_event: self.host.next_event() - 1,
        });
        self.host.state.missions = Some(missions::Runtime {
            last_event: self.host.next_event() - 1,
            last_review: self.host.clock().time,
        });
        invocation::validate(&self.canonical_state())?;
        Ok(())
    }

    fn seed_playable_workshop(
        &mut self,
        established: bool,
        settlement: &population::Settlement,
    ) -> Result<(), Error> {
        if !established {
            return Ok(());
        }
        let definitions = self
            .host
            .state
            .openings
            .as_ref()
            .unwrap()
            .catalog
            .settlements[&settlement.profile]
            .workshops
            .clone();
        let epoch = self.host.clock().time;
        let construction = self
            .host
            .state
            .projects
            .as_ref()
            .unwrap()
            .rules
            .construction_labor
            .0;
        let history = definitions.iter().try_fold(0_u64, |total, definition| {
            let recipe = self
                .host
                .state
                .workshops
                .recipe(&game_workshop::RecipeId(definition.recipe.clone()))?;
            let production = recipe
                .duration
                .0
                .checked_mul(definition.historical_batches)
                .ok_or(Error::Overflow)?;
            total
                .checked_add(construction)
                .and_then(|value| value.checked_add(production))
                .ok_or(Error::Overflow)
        })?;
        self.host.publish_clock(game_sim::Clock {
            time: SimTime(epoch.0.checked_sub(history).ok_or(Error::Overflow)?),
            ..self.host.clock()
        });
        for definition in &definitions {
            self.seed_authored_workshop(definition, settlement)?;
        }
        debug_assert_eq!(self.host.clock().time, epoch);
        Ok(())
    }
    fn seed_authored_workshop(
        &mut self,
        definition: &game_openings::WorkshopDefinition,
        settlement: &population::Settlement,
    ) -> Result<(), Error> {
        let residents: BTreeSet<_> = settlement.residents.iter().copied().collect();
        let area = settlement.areas[&definition.area];
        let ruler = settlement.residents[definition.sponsor];
        let recipe_id = game_workshop::RecipeId(definition.recipe.clone());
        let recipe = self.host.state.workshops.recipe(&recipe_id)?.clone();
        let timber = settlement.stocks[&definition.input];
        let tool = settlement.stocks[&definition.tool];
        let output = settlement.stocks[&definition.output];
        // The workshop and existing tools predate character control. Its
        // material and labor history are explicit initial content.
        let craftsperson = settlement.residents[definition.craftsperson];
        let initial_materials = definition.historical_materials.as_ref().ok_or_else(|| {
            Error::InvalidConfiguration(
                "established workshop has no authored construction stock".into(),
            )
        })?;
        let materials = self.add_inventory(
            Good::Timber,
            InventoryLocation::Area(area),
            initial_materials.quantity,
            initial_materials.capacity,
            residents.clone(),
        )?;
        game_property::declare_custody(&mut self.host.storage, materials, ruler)
            .expect("declared custody");
        let project = ProjectId(self.host.allocate_id()?.0);
        let enterprise = EnterpriseId(self.host.allocate_id()?.0);
        let rules = self.host.state.projects.as_ref().unwrap().rules.clone();
        let beneficiaries: BTreeSet<_> = definition
            .beneficiaries
            .iter()
            .map(|i| settlement.residents[*i])
            .collect();
        let workers: BTreeSet<_> = definition
            .workers
            .iter()
            .map(|i| settlement.residents[*i])
            .collect();
        let site = self.bind_authored_workshop(definition, settlement)?;
        let finish_by = SimTime(self.time().0 + 7 * 86_400_000);
        game_projects::insert_project(
            &mut self.host.storage,
            project,
            projects::Project {
                kind: ProjectKind::Workshop {
                    recipe: recipe_id.clone(),
                },
                id: project,
                sponsor: ruler,
                area,
                materials,
                tool,
                input: timber,
                output,
                beneficiaries: beneficiaries.clone(),
                workers: workers.clone(),
                support: BTreeSet::new(),
                status: ProjectStatus::Completed,
                blocker: None,
                progress_ms: rules.construction_labor.0,
                contributions: BTreeMap::from([(craftsperson, rules.construction_labor.0)]),
                materials_consumed: true,
                finish_by,
                enterprise: Some(enterprise),
            },
        )?;
        game_projects::insert_enterprise(
            &mut self.host.storage,
            enterprise,
            projects::Enterprise {
                inputs: BTreeMap::from([(
                    recipe.input_good,
                    recipe
                        .input_quantity
                        .0
                        .checked_mul(definition.historical_batches)
                        .ok_or(Error::Overflow)?,
                )]),
                kind: ProjectKind::Workshop {
                    recipe: recipe_id.clone(),
                },
                produced: BTreeMap::from([(
                    recipe.output_good,
                    recipe
                        .output_quantity
                        .0
                        .checked_mul(definition.historical_batches)
                        .ok_or(Error::Overflow)?,
                )]),
                id: enterprise,
                project,
                sponsor: ruler,
                area,
                site,
                beneficiaries: beneficiaries.clone(),
                workers,
                status: EnterpriseStatus::Ready,
                blocker: None,
                labor: BTreeMap::from([(
                    craftsperson,
                    definition
                        .historical_batches
                        .checked_mul(recipe.duration.0)
                        .ok_or(Error::Overflow)?,
                )]),
                operating_agreements: BTreeSet::new(),
            },
        )?;
        let history_events = definition
            .historical_batches
            .checked_mul(3)
            .and_then(|count| count.checked_add(3))
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(usize::try_from(history_events).map_err(|_| Error::Overflow)?)?;
        let construction_stock =
            game_cargo::inventory_mut(&mut self.host.storage, materials).unwrap();
        construction_stock.quantity = Quantity(
            construction_stock
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
            actor: craftsperson,
            source: materials,
            quantity: rules.construction_timber,
        });
        self.emit(EventKind::ConstructionStarted {
            project,
            actor: craftsperson,
        });
        self.host.publish_clock(game_sim::Clock {
            time: SimTime(
                self.host
                    .clock()
                    .time
                    .0
                    .checked_add(rules.construction_labor.0)
                    .ok_or(Error::Overflow)?,
            ),
            ..self.host.clock()
        });
        self.emit(EventKind::ProjectCompleted {
            project,
            enterprise,
            site,
        });
        for _ in 0..definition.historical_batches {
            self.host.publish_clock(game_sim::Clock {
                time: SimTime(
                    self.host
                        .clock()
                        .time
                        .0
                        .checked_add(recipe.duration.0)
                        .ok_or(Error::Overflow)?,
                ),
                ..self.host.clock()
            });
            let stock = game_cargo::inventory_mut(&mut self.host.storage, timber).unwrap();
            stock.quantity = Quantity(
                stock
                    .quantity
                    .0
                    .checked_sub(recipe.input_quantity.0)
                    .ok_or_else(|| {
                        Error::InvalidConfiguration(
                            "authored production inputs are insufficient".into(),
                        )
                    })?,
            );
            self.emit(EventKind::Consumed {
                actor: craftsperson,
                source: timber,
                quantity: recipe.input_quantity,
            });
            self.emit(EventKind::EnterpriseInputUsed {
                enterprise,
                site,
                quantity: recipe.input_quantity,
            });
            let stock = game_cargo::inventory_mut(&mut self.host.storage, output).unwrap();
            let quantity = stock
                .quantity
                .0
                .checked_add(recipe.output_quantity.0)
                .ok_or(Error::Overflow)?;
            if quantity > stock.capacity.0 {
                return Err(Error::InvalidConfiguration(
                    "authored production exceeds output capacity".into(),
                ));
            }
            stock.quantity = Quantity(quantity);
            self.emit(EventKind::Produced {
                actor: craftsperson,
                site,
                output,
                quantity: recipe.output_quantity,
            });
        }
        self.host.state.projects.as_mut().unwrap().last_event = self.host.next_event() - 1;
        let level = 10.min(self.host.state.skills.as_ref().unwrap().rules.max_level);
        let practice =
            self.host.state.skills.as_ref().unwrap().rules.per_level.0 * u64::from(level);
        game_skills::skills_mut(&mut self.host.storage, craftsperson)
            .unwrap()
            .records
            .insert(
                Skill::Craft,
                skills::Progress {
                    practice_ms: practice,
                    level,
                    progress_ms: 0,
                },
            );
        let rules = &self.host.state.skills.as_ref().unwrap().rules;
        let experiences: BTreeMap<_, _> = [
            (Skill::Craft, 5_u16),
            (Skill::Leadership, 5),
            (Skill::Negotiation, 2),
        ]
        .into_iter()
        .map(|(skill, level)| {
            let level = level.min(rules.max_level);
            (
                skill,
                skills::Progress {
                    practice_ms: rules.per_level.0 * u64::from(level),
                    level,
                    progress_ms: 0,
                },
            )
        })
        .collect();
        game_skills::skills_mut(&mut self.host.storage, ruler)
            .unwrap()
            .records
            .extend(experiences);
        let observed_at = self.time();
        for guardian in self
            .host
            .state
            .lineage
            .as_mut()
            .unwrap()
            .guardians
            .values_mut()
        {
            guardian.assessments.insert(ruler, lineage::Assessment {
                    independence: 500, competence: 500, judgment: 450, risks: 200,
                    observed_stage: Some(LifeStage::Adult), observed_at, ready: true,
                    explanation: "I witnessed this adult's earlier provisioning, craft work and council decisions; I still judge each request individually".into(),
                });
        }
        self.update_property_events()?;
        Ok(())
    }

    fn seed_playable_resources(
        &mut self,
        settlement: &population::Settlement,
    ) -> Result<(), Error> {
        let residents: BTreeSet<_> = self
            .host
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect();
        let day = self.host.state.environment.as_ref().unwrap().calendar.day.0;
        self.host.state.ecology = Some(ecology::Runtime {
            rules: game_ecology::EcologyRules::default(),
            day: self.host.clock().time.0 / day,
        });
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
        let woodkeeper = settlement.residents[5];
        let farmer = settlement.residents[3];
        self.debug_add_plot(
            self.host.state.areas[1].id,
            Some(farmer),
            residents.clone(),
            900,
        )?;
        self.debug_add_plot(
            self.host.state.areas[1].id,
            Some(woodkeeper),
            residents.clone(),
            850,
        )?;
        let healer = self.host.state.people[6].id;
        let water = settlement.stocks["drinking-water"];
        let amount = game_cargo::inventory(&self.host.storage, water)
            .unwrap()
            .quantity;
        self.collected_water_quality(water, amount, 0);
        let skill_rules = &self.host.state.skills.as_ref().unwrap().rules;
        let level = 10.min(skill_rules.max_level);
        let practice_ms = skill_rules.per_level.0 * u64::from(level);
        game_skills::skills_mut(&mut self.host.storage, healer)
            .unwrap()
            .records
            .insert(
                Skill::Medicine,
                skills::Progress {
                    practice_ms,
                    level,
                    progress_ms: 0,
                },
            );
        let mind = game_characters::decisions::mind_mut::<game_api::Command>(
            &mut self.host.storage,
            healer,
        )
        .unwrap();
        mind.personality.empathy = 900;
        mind.personality.patience = 900;
        mind.dream = decisions::Aspiration::SupportOthers;
        self.update_property_events()?;
        Ok(())
    }

    fn seed_playable_family_plans(&mut self) -> Result<(), Error> {
        let first = self.host.state.people[3].id;
        let second = self.host.state.people[4].id;
        // This particular couple discussed parenthood before control begins.
        // Affection alone never creates family intent for later generations.
        let effect = self
            .host
            .state
            .social
            .as_mut()
            .unwrap()
            .finish_conversation(first, second, SocialTopic::FamilyPlans)?;
        for (actor, partner) in [(first, second), (second, first)] {
            let relationship = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut self.host.storage,
                actor,
            )
            .unwrap()
            .relationships
            .get_mut(&partner)
            .unwrap();
            relationship.affection = (relationship.affection + effect.affection_gain).min(1_000);
            relationship.trust = (relationship.trust + effect.trust_gain).min(1_000);
        }
        self.emit(EventKind::Socialized {
            first,
            second,
            topic: SocialTopic::FamilyPlans,
        });
        Ok(())
    }

    fn establish_starting_custom(&mut self, ruling_office: bool) -> Result<(), Error> {
        let ruler = self.host.state.ruler;
        let steward = self.host.state.people[4].id;
        let village = self.host.state.areas[0].id;
        let food = game_cargo::inventories(&self.host.storage)
            .map(|(_, row)| row)
            .find(|stock| {
                stock.good == Good::Food && stock.location == InventoryLocation::Area(village)
            })
            .unwrap()
            .id;
        game_cargo::inventory_mut(&mut self.host.storage, food)
            .unwrap()
            .access = BTreeSet::from([ruler]);
        let fund = self.add_inventory(
            Good::Food,
            InventoryLocation::Area(village),
            Quantity(0),
            Quantity(100_000),
            BTreeSet::from([ruler, steward]),
        )?;
        game_property::declare_custody(&mut self.host.storage, fund, steward)
            .expect("declared custody");
        self.update_property_events()?;
        let founders = BTreeSet::from([
            ruler,
            steward,
            self.host.state.people[5].id,
            self.host.state.people[6].id,
            self.host.state.people[7].id,
        ]);
        self.debug_property_action(
            ruler,
            PropertyAction::Claim {
                inventory: food,
                rights: BTreeSet::from([
                    ClaimRight::Use,
                    ClaimRight::Consume,
                    ClaimRight::Transfer,
                    ClaimRight::GrantAccess,
                ]),
                evidence: Vec::new(),
            },
        )?;
        let claim = game_property::claims(&self.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap();
        for actor in founders.iter().copied().filter(|actor| *actor != ruler) {
            self.debug_property_action(actor, PropertyAction::Recognize { claim })?;
        }
        let holder = if ruling_office { ruler } else { steward };
        let charter = InstitutionCharter {
            purpose: if ruling_office {
                "Willowbank council and supported village office"
            } else {
                "Willowbank camp provisioning association"
            }
            .into(),
            jurisdiction: village,
            founders: founders.clone(),
            open_membership: true,
            procedure: DecisionProcedure {
                proposers: Subjects::members(),
                voters: Subjects::members(),
                minimum_support: 3,
            },
            offices: vec![OfficeMandate {
                slot: 0,
                holder,
                inventories: BTreeSet::from([food, fund]),
                sites: BTreeSet::new(),
                powers: BTreeSet::from([
                    OfficePower::Negotiate,
                    OfficePower::Access,
                    OfficePower::Delivery,
                    OfficePower::HearAccessCases,
                ]),
                minimum_support: 3,
            }],
            rules: vec![InstitutionalRule {
                subjects: Subjects::members(),
                when: RuleExpression::Fact(SocialCondition::Local),
                effect: SocialEffect::Access {
                    inventory: food,
                    allowed: true,
                },
                authority: 0,
            }],
            contributions: vec![AgreementTerm::Delivery {
                debtor: ruler,
                creditor: steward,
                source: food,
                destination: fund,
                quantity: Quantity(1_000),
                deadline: SimTime(self.host.clock().time.0 + 86_400_000),
            }],
            evidence: BTreeSet::new(),
        };
        self.debug_institution_action(
            ruler,
            InstitutionAction::Found {
                charter,
                origin: None,
            },
        )?;
        let institution = game_society::institutions(&self.host.storage)
            .map(|(id, _)| id)
            .next_back()
            .unwrap();
        for actor in founders.into_iter().filter(|actor| *actor != ruler) {
            self.debug_institution_action(actor, InstitutionAction::Join { institution })?;
        }
        for actor in self
            .host
            .state
            .people
            .iter()
            .map(|person| person.id)
            .filter(|person| *person != ruler)
            .collect::<Vec<_>>()
        {
            if !game_society::institution(&self.host.storage, institution)
                .expect("validated Society reference")
                .members
                .contains(&actor)
            {
                self.debug_institution_action(actor, InstitutionAction::Join { institution })?;
            }
        }
        self.reconcile_institutions()?;
        self.carry_good(ruler, food, Quantity(1_000))?;
        self.advance(MINUTE)?;
        self.debug_actor_command(
            ruler,
            Command::Load {
                source: self.host.state.people[self.person_index[&ruler]].inventory,
                destination: fund,
                quantity: Quantity(1_000),
            },
        )?;
        self.advance(MINUTE)?;
        self.reconcile_institutions()?;
        Ok(())
    }

    fn author_child_opening(&mut self) -> Result<(), Error> {
        let owner_seed = self.host.seed();
        let mother = self.host.state.people[3].id;
        let father = self.host.state.people[4].id;
        if !game_lifecycle::all_pregnancies(&self.host.storage)
            .map(|(_, row)| row)
            .any(|pregnancy| {
                pregnancy.gestator == mother
                    && matches!(
                        pregnancy.state,
                        development::PregnancyState::Gestating { .. }
                            | development::PregnancyState::Conceiving { .. }
                    )
            })
        {
            self.debug_actor_command(mother, Command::Conceive { partner: father })?;
        }
        let pending_conception = game_lifecycle::all_pregnancies(&self.host.storage)
            .map(|(_, row)| row)
            .find_map(|pregnancy| {
                (pregnancy.gestator == mother)
                    .then_some(&pregnancy.state)
                    .and_then(|state| match state {
                        development::PregnancyState::Conceiving { completes } => Some(*completes),
                        _ => None,
                    })
            });
        if let Some(completes) = pending_conception {
            self.advance(SimDuration(completes.0 - self.host.clock().time.0))?;
        }
        let birth = game_lifecycle::all_pregnancies(&self.host.storage)
            .map(|(_, row)| row)
            .find_map(|pregnancy| {
                (pregnancy.gestator == mother)
                    .then_some(&pregnancy.state)
                    .and_then(|state| match state {
                        development::PregnancyState::Gestating { due } => {
                            Some((pregnancy.id, *due))
                        }
                        _ => None,
                    })
            })
            .ok_or_else(|| {
                Error::InvalidConfiguration("the authored parents did not choose conception".into())
            })?;
        let lead = MINUTE.0.min(
            self.host
                .state
                .development
                .as_ref()
                .unwrap()
                .rules
                .gestation
                .0,
        );
        let death_at = SimTime(birth.1.0.checked_sub(lead).ok_or(Error::Overflow)?);
        // This is authored prenatal history before gameplay, not a fast-forward
        // command or a promise that the simulation will create another birth.
        self.host.publish_clock(game_sim::Clock {
            time: death_at,
            ..self.host.clock()
        });
        self.host.state.due.clear();
        let actors: Vec<_> = game_physiology::bodies(&self.host.storage)
            .map(|(actor, _)| actor)
            .collect();
        for actor in actors {
            *self.host.storage.activity_mut(actor).unwrap() = Activity::Idle;
            game_workshop::remove_job(&mut self.host.storage, actor).map(|_| ())?;
            let body = game_physiology::body_mut(&mut self.host.storage, actor).unwrap();
            body.vitals.hunger = 0;
            body.vitals.fatigue = 0;
        }
        for person in &mut self.host.state.people {
            game_transport::replace_route(&mut self.host.storage, person.id, Vec::new())
                .expect("actor route");
            *game_spatial::location_mut(
                &mut self.host.storage,
                game_ecs::StableId((person.id).0),
            )
            .expect("location exists") = Location::AtArea(self.host.state.areas[0].id);
        }
        let lifespan = self
            .host
            .state
            .development
            .as_ref()
            .unwrap()
            .rules
            .lifespan
            .0;
        let elderhood = self
            .host
            .state
            .development
            .as_ref()
            .unwrap()
            .rules
            .elderhood
            .0;
        let cultivators: Vec<_> = game_agriculture::cultivations(&self.host.storage)
            .map(|(actor, _)| actor)
            .collect();
        for actor in cultivators {
            game_agriculture::remove_cultivation(&mut self.host.storage, actor)?;
        }
        for actor in game_projects::constructions(&self.host.storage)
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            game_projects::remove_construction(&mut self.host.storage, actor)?;
        }
        let gatherers: Vec<_> = game_ecology::gatherings(&self.host.storage)
            .map(|(actor, _)| actor)
            .collect();
        for actor in gatherers {
            game_ecology::remove_gathering(&mut self.host.storage, actor)?;
        }
        // This authored interval ends all previously active social tasks. Its
        // scheduler epoch begins at this snapshot, as do the rebuilt physical
        // deadlines; it does not replay unobserved prenatal interactions.
        if let Some(traits) = &mut self.host.state.physical_traits {
            traits.reset_epoch(death_at)?;
        }
        if let Some(social) = &mut self.host.state.social {
            social.interactions.clear();
            social.participants.clear();
            social.last_review = death_at;
            for record in social.development.values_mut() {
                record.next_choice = death_at;
            }
        }

        for person in game_lifecycle::all_lives(&self.host.storage)
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            let life =
                game_lifecycle::life_mut(&mut self.host.storage, person).expect("life exists");
            let age = life
                .initial_age
                .0
                .checked_add(death_at.0 - life.born.0)
                .ok_or(Error::Overflow)?;
            if age >= lifespan {
                return Err(Error::InvalidConfiguration(
                    "biology cannot support the authored prenatal history".into(),
                ));
            }
            life.stage = if age < elderhood {
                LifeStage::Adult
            } else {
                LifeStage::Elder
            };
        }
        for person in self
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
            self.schedule_age(person);
        }
        self.host
            .state
            .due
            .entry(birth.1)
            .or_default()
            .push(Due::Birth(birth.0));
        let next_review = death_at
            .0
            .checked_div(MINUTE.0)
            .unwrap()
            .checked_add(1)
            .and_then(|minute| minute.checked_mul(MINUTE.0))
            .ok_or(Error::Overflow)?;
        self.host
            .state
            .due
            .entry(SimTime(next_review))
            .or_default()
            .push(Due::Review);
        let epoch = self.time();
        let environment = self.host.state.environment.as_mut().unwrap();
        let day = epoch.0 / environment.calendar.day.0;
        environment.reset_epoch(&mut self.host.storage, epoch, owner_seed)?;
        self.host.state.ecology.as_mut().unwrap().day = day;
        self.host.state.agriculture.as_mut().unwrap().day = day;
        let old = self.host.state.ruler;
        self.debug_injure(old, self.body_rules(old).max_health)?;
        self.observe_invocation_deaths()?;
        // The existing council chooses a living steward. The successor child
        // receives neither this office nor the previous character's assets.
        let institution = game_society::institutions(&self.host.storage)
            .map(|(id, _)| id)
            .next()
            .unwrap();
        self.debug_institution_action(
            father,
            InstitutionAction::Propose {
                institution,
                change: InstitutionChange::Appoint {
                    office: 0,
                    holder: father,
                },
            },
        )?;
        let proposal = game_society::proposals(&self.host.storage)
            .map(|(id, _)| id)
            .next_back()
            .unwrap();
        for actor in game_society::institution(&self.host.storage, institution)
            .expect("validated Society reference")
            .members
            .iter()
            .copied()
            .filter(|actor| *actor != old)
            .take(3)
            .collect::<Vec<_>>()
        {
            self.debug_institution_action(
                actor,
                InstitutionAction::Vote {
                    proposal,
                    support: true,
                },
            )?;
        }
        for actor in game_society::institution(&self.host.storage, institution)
            .expect("validated Society reference")
            .members
            .iter()
            .copied()
            .filter(|actor| *actor != old)
            .take(3)
            .collect::<Vec<_>>()
        {
            self.debug_institution_action(
                actor,
                InstitutionAction::SupportOffice {
                    institution,
                    office: 0,
                    support: true,
                },
            )?;
        }
        self.advance(SimDuration(lead))?;
        if self.host.state.ruler == old {
            return Err(Error::InvalidConfiguration(
                "the actual birth did not bind the successor".into(),
            ));
        }
        Ok(())
    }
    fn seed_playable_shelters(&mut self, rules: &HealthRules, houses: bool) -> Result<(), Error> {
        self.initialize_health(rules.clone())?;
        let custodian = self.host.state.people[4].id;
        let access: BTreeSet<_> = self
            .host
            .state
            .people
            .iter()
            .filter(|person| {
                game_physiology::body(&self.host.storage, person.id)
                    .expect("body exists")
                    .vitals
                    .alive
            })
            .map(|person| person.id)
            .collect();
        // Residents remember this maintained source's history; later arrivals
        // and newborns still learn water safety through observation or reports.
        let potable: Vec<_> = game_cargo::inventories(&self.host.storage)
            .map(|(_, row)| row)
            .filter(|stock| stock.good == Good::Water && stock.quantity.0 > 0)
            .map(|stock| stock.id)
            .collect();
        for observer in access.iter().copied() {
            if game_lifecycle::life(&self.host.storage, observer)
                .expect("life exists")
                .stage
                == LifeStage::Infant
            {
                continue;
            }
            for source in potable.iter().copied() {
                self.seed_water_safety(observer, source)?;
            }
        }
        for (area, capacity, insulation) in [
            (
                self.host.state.areas[0].id,
                18_u32,
                if houses { 750_u16 } else { 350 },
            ),
            (self.host.state.areas[1].id, 4_u32, 500_u16),
        ] {
            let materials = u128::from(capacity)
                * u128::from(rules.shelter_timber_per_place.0)
                * u128::from(500 + insulation);
            let quantity =
                Quantity(u64::try_from(materials.div_ceil(1_000)).map_err(|_| Error::Overflow)?);
            // This allocated construction lot is authored prior property. The
            // health constructor physically consumes it rather than awarding a
            // cost-free building or giving a custodian title to other goods.
            let source = self.add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                quantity,
                quantity,
                BTreeSet::from([custodian]),
            )?;
            game_property::declare_custody(&mut self.host.storage, source, custodian)
                .expect("declared custody");
            let built_at = SimTime(self.host.clock().time.0.saturating_sub(2 * 86_400_000));
            let shelter = self.seed_shelter(
                area,
                custodian,
                access.clone(),
                capacity,
                insulation,
                source,
                quantity,
                built_at,
            )?;
            let occupants: Vec<_> = self
                .host
                .state
                .people
                .iter()
                .filter(|person| {
                    access.contains(&person.id)
                        && *game_spatial::location(
                            &self.host.storage,
                            game_ecs::StableId((person.id).0),
                        )
                        .expect("location exists")
                            == Location::AtArea(area)
                })
                .take(capacity as usize)
                .map(|person| person.id)
                .collect();
            for actor in occupants {
                self.set_initial_shelter_occupancy(actor, shelter)?;
            }
        }
        self.update_property_events()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opening(id: &str, workers: usize) -> Simulation {
        Simulation::playable_scenario(
            id,
            7,
            Execution {
                workers,
                reverse_jobs: true,
            },
            Config::default(),
        )
        .unwrap()
    }

    #[test]
    fn all_openings_are_connected_human_worlds_and_restore_their_actual_content() {
        for id in PLAYABLE {
            let sim = opening(id, 1);
            let state = sim.canonical_state();
            assert_eq!(state.state.scenario.id, id);
            assert!(!state.state.scenario.debug_fixture);
            assert!(
                state.state.physical.is_some()
                    && state.state.development.is_some()
                    && state.state.decisions.is_some()
                    && state.state.combat.is_some()
                    && state.state.lineage.is_some()
                    && state.state.skills.is_some()
                    && state.state.agreements.is_some()
                    && state.state.property.is_some()
                    && state.state.institutions.is_some()
                    && state.state.projects.is_some()
                    && state.state.invocation.is_some()
                    && state.state.environment.is_some()
                    && state.state.ecology.is_some()
                    && state.state.health.is_some()
                    && state.state.force.is_some()
                    && state.state.missions.is_some()
                    && state.state.social.is_some()
            );
            assert_eq!(
                state.state.environment.as_ref().unwrap().calendar.day,
                SimDuration(86_400_000)
            );
            assert_eq!(
                state.state.development.as_ref().unwrap().rules,
                development::Rules::human()
            );
            assert_eq!(
                state.state.lineage.as_ref().unwrap().rules.lesson_duration,
                SimDuration(3_600_000)
            );
            assert_eq!(
                state.state.skills.as_ref().unwrap().rules.per_level,
                SimDuration(3_600_000)
            );
            assert_eq!(
                state
                    .state
                    .workshops
                    .recipe(&state.state.workshops.catalog.default_recipe)
                    .unwrap()
                    .duration,
                SimDuration(3_600_000)
            );
            let saved = sim.save().unwrap();
            let restored = Simulation::load(
                &saved,
                Execution {
                    workers: 4,
                    reverse_jobs: false,
                },
            )
            .unwrap_or_else(|error| panic!("{id}: {error}"));
            assert_eq!(sim.canonical_state(), restored.canonical_state());
        }
    }

    #[test]
    fn shelters_and_healing_supplies_have_actual_materials_and_personal_knowledge() {
        for id in PLAYABLE {
            let sim = opening(id, 1);
            let world = sim.host.state.health.as_ref().unwrap();
            assert_eq!(world.rules, HealthRules::human());
            assert_eq!(game_health::shelters(&sim.host.storage).count(), 2);
            assert!(
                game_health::shelters(&sim.host.storage)
                    .map(|(_, shelter)| shelter)
                    .all(|shelter| shelter.timber_used.0 > 0 && shelter.labor_ms > 0)
            );
            let consumed: u64 = sim
                .host
                .events()
                .iter()
                .filter_map(|event| match event.event {
                    EventKind::Consumed {
                        source, quantity, ..
                    } if game_cargo::inventory(&sim.host.storage, source)
                        .expect("inventory exists")
                        .good
                        == Good::Timber =>
                    {
                        Some(quantity.0)
                    }
                    _ => None,
                })
                .sum();
            assert!(
                consumed
                    >= game_health::shelters(&sim.host.storage)
                        .map(|(_, shelter)| shelter)
                        .map(|shelter| shelter.timber_used.0)
                        .sum::<u64>()
            );
            let healer = sim.host.state.people[6].id;
            assert!(sim.own_skill_level(healer, Skill::Medicine) > 0);
            let water = game_cargo::inventories(&sim.host.storage)
                .map(|(_, row)| row)
                .find(|stock| stock.good == Good::Water && stock.quantity.0 > 0)
                .unwrap()
                .id;
            assert!(
                game_health::knowledge(&sim.host.storage, healer)
                    .expect("health knowledge exists")
                    .water[&water]
                    .purified
            );
            assert!(
                game_cargo::inventories(&sim.host.storage)
                    .map(|(_, row)| row)
                    .any(|stock| stock.good == Good::Herb && stock.quantity.0 >= 20_000)
            );
            let ruler = sim.host.state.ruler;
            assert!(game_health::occupancy(&sim.host.storage, ruler).is_some());
            if id == "child-successor" {
                assert!(
                    game_health::knowledge(&sim.host.storage, ruler)
                        .is_none_or(|known| !known.water.contains_key(&water))
                );
            }
        }
    }

    #[test]
    fn the_founder_and_established_ruler_have_different_actual_supported_authority() {
        let founder = opening("settlement-founder", 1);
        let established = opening("established-ruler", 1);
        let founder_office = game_society::institutions(&founder.host.storage)
            .map(|(_, row)| row)
            .next()
            .unwrap();
        let ruler_office = game_society::institutions(&established.host.storage)
            .map(|(_, row)| row)
            .next()
            .unwrap();
        assert_ne!(
            founder_office.charter.offices[0].holder,
            founder.host.state.ruler
        );
        assert_eq!(
            ruler_office.charter.offices[0].holder,
            established.host.state.ruler
        );
        assert!(founder_office.funding.iter().all(|id| {
            game_agreements::agreement(&founder.host.storage, *id)
                .unwrap()
                .status
                == AgreementStatus::Fulfilled
        }));
        assert!(ruler_office.funding.iter().all(|id| {
            game_agreements::agreement(&established.host.storage, *id)
                .unwrap()
                .status
                == AgreementStatus::Fulfilled
        }));
        assert!(
            established
                .institution_view(established.host.state.ruler)
                .iter()
                .any(
                    |view| view.offices.iter().any(|office| office.mandate.holder
                        == established.host.state.ruler
                        && office.effective)
                )
        );
        assert!(
            game_projects::enterprises(&founder.host.storage)
                .next()
                .is_none()
        );
        assert!(
            game_projects::enterprises(&established.host.storage)
                .map(|(_, row)| row)
                .any(
                    |enterprise| enterprise.produced.get(&Good::Tool).copied().unwrap_or(0)
                        >= 5_000
                )
        );
    }

    #[test]
    fn child_control_follows_an_actual_human_birth_after_actual_death_and_inherits_no_office_or_mastery()
     {
        let sim = opening("child-successor", 1);
        let lineage = sim.host.state.lineage.as_ref().unwrap();
        assert_eq!(lineage.incarnations.len(), 2);
        let previous = &lineage.incarnations[0];
        let current = &lineage.incarnations[1];
        let born = game_lifecycle::life(&sim.host.storage, current.person)
            .expect("life exists")
            .born;
        assert!(previous.died_at.unwrap() < born);
        assert_eq!(born, sim.host.clock().time);
        assert_eq!(sim.observe().life_stage, Some(LifeStage::Infant));
        assert!(sim.host.events().iter().any(|event| event.at == born
            && matches!(event.event, EventKind::Born { child, .. } if child == current.person)));
        assert!(
            sim.host.events()
                .iter()
                .any(|event| event.at == previous.died_at.unwrap()
                    && matches!(event.event, EventKind::Died { actor } if actor == previous.person))
        );
        assert!(
            game_invocation::archive::<game_api::Command>(&sim.host.storage, previous.person)
                .is_some()
        );
        assert!(sim.own_skill_level(previous.person, Skill::Craft) > 0);
        assert!(
            sim.own_skill_view(current.person)
                .iter()
                .all(|skill| skill.level == 0 && skill.practice_ms == 0)
        );
        assert!(
            game_society::institutions(&sim.host.storage)
                .map(|(_, row)| row)
                .all(|institution| institution
                    .charter
                    .offices
                    .iter()
                    .all(|office| office.holder != current.person))
        );
        assert!(
            game_property::claims(&sim.host.storage)
                .all(|(_, claim)| claim.claimant != current.person)
        );
        assert!(
            sim.observe()
                .available_commands
                .iter()
                .all(|command| matches!(command, Command::Signal { .. } | Command::Cancel))
        );
    }

    #[test]
    fn explicit_config_and_worker_count_change_no_random_opening_or_gameplay_results() {
        for seed in 0..32 {
            assert!(PLAYABLE.contains(&choose(seed).unwrap().as_str()));
            assert_eq!(choose(seed), choose(seed));
        }
        let mut config = Config::default();
        config.environment.climate.seasonal_temperature = [9, 22, 7, -8];
        let a = Simulation::playable_scenario(
            "random",
            11,
            Execution {
                workers: 1,
                reverse_jobs: false,
            },
            config.clone(),
        )
        .unwrap();
        let b = Simulation::playable_scenario(
            "random",
            11,
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
            config.clone(),
        )
        .unwrap();
        assert_eq!(a.host.state.scenario.id, choose(11).unwrap());
        assert_eq!(a.canonical_state(), b.canonical_state());
        assert_eq!(
            a.host.state.environment.as_ref().unwrap().climate,
            config.environment.climate
        );
        let before = a.canonical_snapshot().unwrap();
        for _ in 0..100 {
            a.observe();
        }
        assert_eq!(a.canonical_snapshot().unwrap(), before);
    }
}

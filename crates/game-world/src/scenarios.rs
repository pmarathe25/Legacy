//! Playable openings compose the ordinary domains. Their people, assets and
//! starting customs are authored content, rather than global government modes.
use crate::*;

pub const PLAYABLE: [&str; 3] = ["settlement-founder", "established-ruler", "child-successor"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
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

/// Random selection never chooses a fixture, and is independent of wall time.
pub fn choose(seed: u64) -> &'static str {
    let mixed = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    PLAYABLE[((mixed >> 32) % PLAYABLE.len() as u64) as usize]
}

impl Simulation {
    pub fn playable_scenario(
        id: &str,
        seed: u64,
        execution: Execution,
        config: Config,
    ) -> Result<Self, Error> {
        let id = if id == "random" { choose(seed) } else { id };
        if !PLAYABLE.contains(&id) {
            return Err(Error::InvalidConfiguration(
                "unknown playable opening".into(),
            ));
        }
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
        let (mut sim, _) =
            Self::initial_population(seed, execution, &config, population::Plan::small(), false)?;
        sim.state.time = SimTime(
            config
                .environment
                .calendar
                .day
                .0
                .checked_mul(3)
                .ok_or(Error::Overflow)?
                .max(3 * 24 * 3_600_000),
        );
        let environment = sim.state.environment.as_mut().unwrap();
        environment.reset_epoch(&mut sim.state.components, sim.state.time, seed)?;
        sim.state.scenario = ScenarioInfo {
            id: id.into(),
            version: 1,
            debug_fixture: false,
        };
        sim.state.areas[0].name = "Willowbank".into();
        sim.state.areas[1].name = "Willowbank fields".into();
        sim.state.areas[2].name = "North wood".into();
        sim.prepare_playable_people(&config)?;
        sim.install_playable_domains(&config)?;
        sim.initialize_transport(config.transport.clone())?;
        sim.initialize_realm(realm::Rules::default())?;
        sim.initialize_armies(armies::Rules::default())?;
        sim.seed_playable_resources()?;
        sim.seed_playable_workshop(id != "settlement-founder")?;
        if let Some(rules) = config.genetics.clone() {
            sim.initialize_genetics(rules)?;
        }
        sim.initialize_social(config.social.clone())?;
        sim.seed_playable_family_plans()?;
        sim.establish_starting_custom(id != "settlement-founder")?;
        if id == "child-successor" {
            sim.author_child_opening()?;
        }
        sim.seed_playable_shelters(&config.health, id != "settlement-founder")?;
        sim.update_property_events()?;
        sim.observe_agreement_events();
        sim.update_projects()?;
        sim.reconcile_institutions()?;
        Ok(sim)
    }

    fn prepare_playable_people(&mut self, config: &Config) -> Result<(), Error> {
        let village = self.state.areas[0].id;
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
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect::<Vec<_>>()
        {
            self.interrupt_actor_activity(actor);
            self.state.components.set_job(actor, None)?;
        }
        self.state.development.as_mut().unwrap().rules = config.biology.clone();
        for person in &mut self.state.people {
            *self
                .state
                .components
                .location_mut(person.id)
                .expect("location exists") = Location::AtArea(village);
            person.route.clear();
        }
        for person in self
            .state
            .components
            .lives()
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            let life = self.state.components.life_mut(person).expect("life exists");
            life.initial_age = initial_age;
            life.stage = LifeStage::Adult;
        }
        self.state.due.clear();
        self.state
            .due
            .entry(SimTime(
                self.state
                    .time
                    .0
                    .checked_add(MINUTE.0)
                    .ok_or(Error::Overflow)?,
            ))
            .or_default()
            .push(Due::Review);
        for person in self.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
            self.schedule_age(person);
        }
        // These residents remember the existing fields, granaries and routes.
        // The reports predate control; newborns acquire their own knowledge later.
        let reporter = self.state.people[3].id;
        for person in &self.state.people {
            let mind = self.state.components.mind_mut(person.id).unwrap();
            mind.plan = None;
            mind.retry_after = self.state.time;
            for stock in self
                .state
                .inventories
                .values()
                .filter(|stock| matches!(stock.location, InventoryLocation::Area(_)))
            {
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
                        accessible: stock.access.contains(&person.id),
                        observed_at: self.state.time,
                        learned_at: self.state.time,
                        source: KnowledgeSource::Reported(reporter),
                    },
                );
            }
        }
        let parents = [self.state.people[3].id, self.state.people[4].id];
        for (actor, partner) in [(parents[0], parents[1]), (parents[1], parents[0])] {
            let mind = self.state.components.mind_mut(actor).unwrap();
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
        let residents: BTreeSet<_> = self.state.people.iter().map(|p| p.id).collect();
        let world = self.state.agriculture.as_mut().unwrap();
        let fields: Vec<_> = world
            .fields(&self.state.components)
            .map(|(id, _)| id)
            .collect();
        for id in fields {
            let field = world.field_mut(&mut self.state.components, id).unwrap();
            field.access = residents.clone();
            self.state.inventories.get_mut(&field.tool).unwrap().access = residents.clone();
        }
        Ok(())
    }

    pub(crate) fn install_playable_domains(&mut self, config: &Config) -> Result<(), Error> {
        let people: BTreeSet<_> = self.state.people.iter().map(|p| p.id).collect();
        let ruler = self.state.ruler;
        let village = self.state.areas[0].id;
        self.state.skills = Some(skills::Runtime::new(config.skills.clone())?);
        for person in &people {
            game_skills::insert_skills(
                &mut self.state.components,
                *person,
                game_skills::Skills::default(),
            )?;
        }
        skills::validate(&self.state)?;
        let cap = config
            .skills
            .per_level
            .0
            .checked_mul(u64::from(config.skills.max_level))
            .ok_or(Error::Overflow)?;
        let guardians = self
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
                        sensed_at: self.state.time,
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
            .state
            .people
            .iter()
            .filter(|p| p.role == Role::Guardian)
            .map(|p| p.id)
        {
            self.state
                .components
                .personal_skills_mut(actor)
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
        self.state.lineage = Some(lineage::World {
            incarnations: vec![lineage::Incarnation {
                person: ruler,
                bound_at: self.state.time,
                died_at: None,
            }],
            waiting: None,
            guardians,
            rules: config.mentorship.clone(),
            invocation_learning: BTreeMap::new(),
        });
        self.state.combat = Some(combat::World {
            rules: combat::Rules {
                encounter_limit: 128,
                round: SimDuration(6_000),
                movement: 5_000,
                reach: 2_000,
                damage: self.state.physical.as_ref().unwrap().rules.max_health / 4,
                defence: self.state.physical.as_ref().unwrap().rules.max_health / 6,
                incapacitation: self.state.physical.as_ref().unwrap().rules.max_health / 4,
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
        let custodians = self
            .state
            .inventories
            .values()
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
        self.state.agreements = Some(agreements::World {
            ledger: game_agreements::State {
                agreements: BTreeMap::new(),
                obligations: BTreeMap::new(),
                amendments: BTreeMap::new(),
                last_event: self.state.next_event - 1,
                next_search: BTreeMap::new(),
                pending_work: BTreeMap::new(),
                knowledge: BTreeMap::new(),
                last_observation_event: self.state.next_event - 1,
            },
            custodians,
        });
        self.state.property = Some(property::World {
            reports: BTreeMap::new(),
            claims: BTreeMap::new(),
            incidents: BTreeMap::new(),
            known_claims: BTreeMap::new(),
            knowledge: BTreeMap::new(),
            attitudes: people
                .iter()
                .map(|p| {
                    (
                        *p,
                        property::Attitude {
                            exclusive_claims: 500,
                            sharing: 600,
                            willingness_to_defend: 500,
                        },
                    )
                })
                .collect(),
            portions: self
                .state
                .inventories
                .values()
                .map(|stock| {
                    (
                        stock.id,
                        if stock.quantity.0 == 0 {
                            BTreeMap::new()
                        } else {
                            BTreeMap::from([(ClaimId(0), stock.quantity.0)])
                        },
                    )
                })
                .collect(),
            pending: BTreeMap::new(),
            last_event: self.state.next_event - 1,
            next_search: BTreeMap::new(),
        });
        self.state.institutions = Some(institutions::World {
            institutions: BTreeMap::new(),
            proposals: BTreeMap::new(),
            concepts: BTreeMap::new(),
            cases: BTreeMap::new(),
            memories: people
                .iter()
                .map(|p| (*p, institutions::SocialMemory::default()))
                .collect(),
            last_event: self.state.next_event - 1,
            exclusions: BTreeMap::new(),
            heard: BTreeMap::new(),
        });
        self.state.projects = Some(projects::World {
            rules: projects::Rules {
                construction_labor: SimDuration(16 * 3_600_000),
                recipe_time: SimDuration(3_600_000),
                ..projects::Rules::default()
            },
            projects: BTreeMap::new(),
            enterprises: BTreeMap::new(),
            jobs: BTreeMap::new(),
            known_projects: BTreeMap::new(),
            known_enterprises: BTreeMap::new(),
            timber_hints: people
                .iter()
                .map(|p| (*p, BTreeSet::from([self.state.areas[2].id])))
                .collect(),
            next_search: BTreeMap::new(),
            last_event: self.state.next_event - 1,
        });
        self.state.invocation = Some(invocation::World {
            control: game_invocation::ControlState::new(config.invocation.clone())?,
            history: BTreeMap::new(),
            records: BTreeMap::new(),
            actions: BTreeMap::new(),
        });
        self.state.force = Some(force::World {
            yielded: BTreeMap::new(),
            rules: force::Rules::default(),
            escapes: BTreeMap::new(),
            restraints: BTreeMap::new(),
            last_event: self.state.next_event - 1,
        });
        self.state.missions = Some(missions::World {
            missions: BTreeMap::new(),
            recruitments: BTreeMap::new(),
            known: BTreeMap::new(),
            talks: BTreeMap::new(),
            last_event: self.state.next_event - 1,
            last_review: self.state.time,
        });
        invocation::validate(&self.state)?;
        Ok(())
    }

    fn seed_playable_workshop(&mut self, established: bool) -> Result<(), Error> {
        let area = self.state.areas[0].id;
        let ruler = self.state.ruler;
        let residents: BTreeSet<_> = self.state.people.iter().map(|p| p.id).collect();
        let timber = self
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Timber && stock.location == InventoryLocation::Area(area)
            })
            .unwrap()
            .id;
        let tool = self
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Tool && stock.location == InventoryLocation::Area(area)
            })
            .unwrap()
            .id;
        self.state.inventories.get_mut(&tool).unwrap().quantity = Quantity(3_000);
        self.state
            .property
            .as_mut()
            .unwrap()
            .portions
            .insert(tool, BTreeMap::from([(ClaimId(0), 3_000)]));
        let output = self.add_inventory(
            Good::Tool,
            InventoryLocation::Area(area),
            Quantity(0),
            Quantity(100_000),
            residents.clone(),
        )?;
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .custodians
            .insert(output, ruler);
        let bindings = self.add_inventory(
            Good::Binding,
            InventoryLocation::Area(area),
            Quantity(10_000),
            Quantity(100_000),
            residents.clone(),
        )?;
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .custodians
            .insert(bindings, ruler);
        if established {
            // The workshop and existing tools predate character control. Its
            // material and labor history are explicit initial content.
            let craftsperson = self.state.people[6].id;
            let materials = self.add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                Quantity(5_000),
                Quantity(100_000),
                residents.clone(),
            )?;
            self.state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(materials, ruler);
            let project = ProjectId(allocate(&mut self.state)?);
            let enterprise = EnterpriseId(allocate(&mut self.state)?);
            let site = SiteId(allocate(&mut self.state)?);
            let rules = self.state.projects.as_ref().unwrap().rules.clone();
            let participants = BTreeSet::from([ruler, craftsperson]);
            self.state
                .components
                .insert_site(physical::Site {
                    id: site,
                    area,
                    input: timber,
                    input_quantity: rules.recipe_timber,
                    tool,
                    output,
                    output_quantity: rules.recipe_tools,
                    duration: rules.recipe_time,
                    access: participants.clone(),
                    last_granted: None,
                })
                .expect("validated site insertion");
            self.state.projects.as_mut().unwrap().projects.insert(
                project,
                projects::Project {
                    kind: ProjectKind::Workshop,
                    id: project,
                    sponsor: ruler,
                    area,
                    materials,
                    tool,
                    input: timber,
                    output,
                    beneficiaries: participants.clone(),
                    workers: participants.clone(),
                    support: BTreeSet::new(),
                    status: ProjectStatus::Completed,
                    blocker: None,
                    progress_ms: rules.construction_labor.0,
                    contributions: BTreeMap::from([(craftsperson, rules.construction_labor.0)]),
                    materials_consumed: true,
                    finish_by: SimTime(self.state.time.0 + 7 * 86_400_000),
                    enterprise: Some(enterprise),
                },
            );
            self.state.projects.as_mut().unwrap().enterprises.insert(
                enterprise,
                projects::Enterprise {
                    inputs: BTreeMap::from([(Good::Timber, 10_000)]),
                    kind: ProjectKind::Workshop,
                    produced: BTreeMap::from([(Good::Tool, 5_000)]),
                    id: enterprise,
                    project,
                    sponsor: ruler,
                    area,
                    site,
                    beneficiaries: participants.clone(),
                    workers: participants,
                    status: EnterpriseStatus::Ready,
                    blocker: None,
                    timber_consumed: 10_000,
                    tools_produced: 5_000,
                    labor: BTreeMap::from([(craftsperson, 5 * rules.recipe_time.0)]),
                    operating_agreements: BTreeSet::new(),
                },
            );
            let epoch = self.state.time;
            let production_labor = rules.recipe_time.0.checked_mul(5).ok_or(Error::Overflow)?;
            self.state.time = SimTime(
                epoch
                    .0
                    .checked_sub(
                        rules
                            .construction_labor
                            .0
                            .checked_add(production_labor)
                            .ok_or(Error::Overflow)?,
                    )
                    .ok_or(Error::Overflow)?,
            );
            self.ensure_event_capacity(18)?;
            self.state.inventories.get_mut(&materials).unwrap().quantity = Quantity(0);
            self.emit(EventKind::Consumed {
                actor: craftsperson,
                source: materials,
                quantity: rules.construction_timber,
            });
            self.emit(EventKind::ConstructionStarted {
                project,
                actor: craftsperson,
            });
            self.state.time.0 += rules.construction_labor.0;
            self.emit(EventKind::ProjectCompleted {
                project,
                enterprise,
                site,
            });
            for _ in 0..5 {
                self.state.time.0 += rules.recipe_time.0;
                self.state.inventories.get_mut(&timber).unwrap().quantity.0 -=
                    rules.recipe_timber.0;
                self.emit(EventKind::Consumed {
                    actor: craftsperson,
                    source: timber,
                    quantity: rules.recipe_timber,
                });
                self.emit(EventKind::EnterpriseInputUsed {
                    enterprise,
                    site,
                    quantity: rules.recipe_timber,
                });
                self.state.inventories.get_mut(&output).unwrap().quantity.0 += rules.recipe_tools.0;
                self.emit(EventKind::Produced {
                    actor: craftsperson,
                    site,
                    output,
                    quantity: rules.recipe_tools,
                });
            }
            self.state.time = epoch;
            self.state.projects.as_mut().unwrap().last_event = self.state.next_event - 1;
            let level = 10.min(self.state.skills.as_ref().unwrap().rules.max_level);
            let practice = self.state.skills.as_ref().unwrap().rules.per_level.0 * u64::from(level);
            self.state
                .components
                .personal_skills_mut(craftsperson)
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
            let rules = &self.state.skills.as_ref().unwrap().rules;
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
            self.state
                .components
                .personal_skills_mut(ruler)
                .unwrap()
                .records
                .extend(experiences);
            for guardian in self.state.lineage.as_mut().unwrap().guardians.values_mut() {
                guardian.assessments.insert(ruler, lineage::Assessment {
                    independence: 500, competence: 500, judgment: 450, risks: 200,
                    observed_stage: Some(LifeStage::Adult), observed_at: self.state.time, ready: true,
                    explanation: "I witnessed this adult's earlier provisioning, craft work and council decisions; I still judge each request individually".into(),
                });
            }
        }
        self.update_property_events()?;
        Ok(())
    }

    fn seed_playable_resources(&mut self) -> Result<(), Error> {
        let residents: BTreeSet<_> = self.state.people.iter().map(|person| person.id).collect();
        let day = self.state.environment.as_ref().unwrap().calendar.day.0;
        self.state.ecology = Some(ecology::Runtime {
            observed_areas: BTreeMap::new(),
            rules: game_ecology::EcologyRules::default(),
            jobs: BTreeMap::new(),
            known: BTreeMap::new(),
            known_plots: BTreeMap::new(),
            next_request: BTreeMap::new(),
            day: self.state.time.0 / day,
        });
        let woodkeeper = self.state.people[5].id;
        let farmer = self.state.people[3].id;
        for (kind, area, capacity, growth, contamination) in [
            (
                ResourceKind::Coppice,
                self.state.areas[2].id,
                200_000,
                500,
                0,
            ),
            (
                ResourceKind::Berries,
                self.state.areas[2].id,
                40_000,
                1_000,
                0,
            ),
            (ResourceKind::Herbs, self.state.areas[2].id, 30_000, 500, 0),
            (
                ResourceKind::Spring,
                self.state.areas[1].id,
                100_000,
                30_000,
                5,
            ),
        ] {
            let resource = self.debug_add_resource(
                kind,
                area,
                Quantity(capacity),
                Quantity(capacity),
                Quantity(growth),
                residents.clone(),
                Some(if area == self.state.areas[2].id {
                    woodkeeper
                } else {
                    farmer
                }),
                contamination,
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
        self.debug_add_plot(self.state.areas[1].id, Some(farmer), residents.clone(), 900)?;
        self.debug_add_plot(
            self.state.areas[1].id,
            Some(woodkeeper),
            residents.clone(),
            850,
        )?;
        let healer = self.state.people[6].id;
        let herbs = self.add_inventory(
            Good::Herb,
            InventoryLocation::Area(self.state.areas[0].id),
            Quantity(20_000),
            Quantity(100_000),
            BTreeSet::from([healer]),
        )?;
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .custodians
            .insert(herbs, healer);
        let water = self.add_inventory(
            Good::Water,
            InventoryLocation::Area(self.state.areas[0].id),
            Quantity(30_000),
            Quantity(100_000),
            residents,
        )?;
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .custodians
            .insert(water, healer);
        self.collected_water_quality(water, Quantity(30_000), 0);
        let skill_rules = &self.state.skills.as_ref().unwrap().rules;
        let level = 10.min(skill_rules.max_level);
        let practice_ms = skill_rules.per_level.0 * u64::from(level);
        self.state
            .components
            .personal_skills_mut(healer)
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
        let mind = self.state.components.mind_mut(healer).unwrap();
        mind.personality.empathy = 900;
        mind.personality.patience = 900;
        mind.dream = decisions::Aspiration::SupportOthers;
        self.update_property_events()?;
        Ok(())
    }

    fn seed_playable_family_plans(&mut self) -> Result<(), Error> {
        let first = self.state.people[3].id;
        let second = self.state.people[4].id;
        // This particular couple discussed parenthood before control begins.
        // Affection alone never creates family intent for later generations.
        let effect = self.state.social.as_mut().unwrap().finish_conversation(
            first,
            second,
            SocialTopic::FamilyPlans,
        )?;
        for (actor, partner) in [(first, second), (second, first)] {
            let relationship = self
                .state
                .components
                .mind_mut(actor)
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
        let ruler = self.state.ruler;
        let steward = self.state.people[4].id;
        let village = self.state.areas[0].id;
        let food = self
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Food && stock.location == InventoryLocation::Area(village)
            })
            .unwrap()
            .id;
        self.state.inventories.get_mut(&food).unwrap().access = BTreeSet::from([ruler]);
        let fund = self.add_inventory(
            Good::Food,
            InventoryLocation::Area(village),
            Quantity(0),
            Quantity(100_000),
            BTreeSet::from([ruler, steward]),
        )?;
        self.state
            .agreements
            .as_mut()
            .unwrap()
            .custodians
            .insert(fund, steward);
        self.update_property_events()?;
        let founders = BTreeSet::from([
            ruler,
            steward,
            self.state.people[5].id,
            self.state.people[6].id,
            self.state.people[7].id,
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
        let claim = *self
            .state
            .property
            .as_ref()
            .unwrap()
            .claims
            .keys()
            .next_back()
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
                deadline: SimTime(self.state.time.0 + 86_400_000),
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
        let institution = *self
            .state
            .institutions
            .as_ref()
            .unwrap()
            .institutions
            .keys()
            .next_back()
            .unwrap();
        for actor in founders.into_iter().filter(|actor| *actor != ruler) {
            self.debug_institution_action(actor, InstitutionAction::Join { institution })?;
        }
        for actor in self
            .state
            .people
            .iter()
            .map(|person| person.id)
            .filter(|person| *person != ruler)
            .collect::<Vec<_>>()
        {
            if !self.state.institutions.as_ref().unwrap().institutions[&institution]
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
                source: self.state.people[self.person_index[&ruler]].inventory,
                destination: fund,
                quantity: Quantity(1_000),
            },
        )?;
        self.advance(MINUTE)?;
        self.reconcile_institutions()?;
        Ok(())
    }

    fn author_child_opening(&mut self) -> Result<(), Error> {
        let mother = self.state.people[3].id;
        let father = self.state.people[4].id;
        if !self
            .state
            .components
            .pregnancies()
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
        let pending_conception = self
            .state
            .components
            .pregnancies()
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
            self.advance(SimDuration(completes.0 - self.state.time.0))?;
        }
        let birth = self
            .state
            .components
            .pregnancies()
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
        let lead = MINUTE
            .0
            .min(self.state.development.as_ref().unwrap().rules.gestation.0);
        let death_at = SimTime(birth.1.0.checked_sub(lead).ok_or(Error::Overflow)?);
        // This is authored prenatal history before gameplay, not a fast-forward
        // command or a promise that the simulation will create another birth.
        self.state.time = death_at;
        self.state.due.clear();
        let actors: Vec<_> = self
            .state
            .components
            .bodies()
            .map(|(actor, _)| actor)
            .collect();
        for actor in actors {
            *self.state.components.activity_mut(actor).unwrap() = Activity::Idle;
            self.state.components.set_job(actor, None)?;
            let body = self.state.components.body_mut(actor).unwrap();
            body.vitals.hunger = 0;
            body.vitals.fatigue = 0;
        }
        for person in &mut self.state.people {
            person.route.clear();
            *self
                .state
                .components
                .location_mut(person.id)
                .expect("location exists") = Location::AtArea(self.state.areas[0].id);
        }
        let lifespan = self.state.development.as_ref().unwrap().rules.lifespan.0;
        let elderhood = self.state.development.as_ref().unwrap().rules.elderhood.0;
        self.state.agriculture.as_mut().unwrap().jobs.clear();
        self.state.projects.as_mut().unwrap().jobs.clear();
        self.state.ecology.as_mut().unwrap().jobs.clear();
        // This authored interval ends all previously active social tasks. Its
        // scheduler epoch begins at this snapshot, as do the rebuilt physical
        // deadlines; it does not replay unobserved prenatal interactions.
        if let Some(traits) = &mut self.state.physical_traits {
            traits.reset_epoch(death_at)?;
        }
        if let Some(social) = &mut self.state.social {
            social.interactions.clear();
            social.participants.clear();
            social.last_review = death_at;
            for record in social.development.values_mut() {
                record.next_choice = death_at;
            }
        }

        for person in self
            .state
            .components
            .lives()
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            let life = self.state.components.life_mut(person).expect("life exists");
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
        for person in self.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
            self.schedule_age(person);
        }
        self.state
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
        self.state
            .due
            .entry(SimTime(next_review))
            .or_default()
            .push(Due::Review);
        let environment = self.state.environment.as_mut().unwrap();
        let day = self.state.time.0 / environment.calendar.day.0;
        environment.reset_epoch(&mut self.state.components, self.state.time, self.state.seed)?;
        self.state.ecology.as_mut().unwrap().day = day;
        self.state.agriculture.as_mut().unwrap().day = day;
        let old = self.state.ruler;
        self.debug_injure(old, self.body_rules(old).max_health)?;
        self.observe_invocation_deaths()?;
        // The existing council chooses a living steward. The successor child
        // receives neither this office nor the previous character's assets.
        let institution = *self
            .state
            .institutions
            .as_ref()
            .unwrap()
            .institutions
            .keys()
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
        let proposal = *self
            .state
            .institutions
            .as_ref()
            .unwrap()
            .proposals
            .keys()
            .next_back()
            .unwrap();
        for actor in self.state.institutions.as_ref().unwrap().institutions[&institution]
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
        for actor in self.state.institutions.as_ref().unwrap().institutions[&institution]
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
        if self.state.ruler == old {
            return Err(Error::InvalidConfiguration(
                "the actual birth did not bind the successor".into(),
            ));
        }
        Ok(())
    }
    fn seed_playable_shelters(&mut self, rules: &HealthRules, houses: bool) -> Result<(), Error> {
        self.initialize_health(rules.clone())?;
        let custodian = self.state.people[4].id;
        let access: BTreeSet<_> = self
            .state
            .people
            .iter()
            .filter(|person| {
                self.state
                    .components
                    .body(person.id)
                    .expect("body exists")
                    .vitals
                    .alive
            })
            .map(|person| person.id)
            .collect();
        // Residents remember this maintained source's history; later arrivals
        // and newborns still learn water safety through observation or reports.
        let potable: Vec<_> = self
            .state
            .inventories
            .values()
            .filter(|stock| stock.good == Good::Water && stock.quantity.0 > 0)
            .map(|stock| stock.id)
            .collect();
        for observer in access.iter().copied() {
            if self
                .state
                .components
                .life(observer)
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
                self.state.areas[0].id,
                18_u32,
                if houses { 750_u16 } else { 350 },
            ),
            (self.state.areas[1].id, 4_u32, 500_u16),
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
            self.state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(source, custodian);
            let built_at = SimTime(self.state.time.0.saturating_sub(2 * 86_400_000));
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
                .state
                .people
                .iter()
                .filter(|person| {
                    access.contains(&person.id)
                        && *self
                            .state
                            .components
                            .location(person.id)
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
            assert_eq!(state.scenario.id, id);
            assert!(!state.scenario.debug_fixture);
            assert!(
                state.physical.is_some()
                    && state.development.is_some()
                    && state.decisions.is_some()
                    && state.combat.is_some()
                    && state.lineage.is_some()
                    && state.skills.is_some()
                    && state.agreements.is_some()
                    && state.property.is_some()
                    && state.institutions.is_some()
                    && state.projects.is_some()
                    && state.invocation.is_some()
                    && state.environment.is_some()
                    && state.ecology.is_some()
                    && state.health.is_some()
                    && state.force.is_some()
                    && state.missions.is_some()
                    && state.social.is_some()
            );
            assert_eq!(
                state.environment.as_ref().unwrap().calendar.day,
                SimDuration(86_400_000)
            );
            assert_eq!(
                state.development.as_ref().unwrap().rules,
                development::Rules::human()
            );
            assert_eq!(
                state.lineage.as_ref().unwrap().rules.lesson_duration,
                SimDuration(3_600_000)
            );
            assert_eq!(
                state.skills.as_ref().unwrap().rules.per_level,
                SimDuration(3_600_000)
            );
            assert_eq!(
                state.projects.as_ref().unwrap().rules.recipe_time,
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
            let world = sim.state.health.as_ref().unwrap();
            assert_eq!(world.rules, HealthRules::human());
            assert_eq!(sim.state.components.shelters().count(), 2);
            assert!(
                sim.state
                    .components
                    .shelters()
                    .map(|(_, shelter)| shelter)
                    .all(|shelter| shelter.timber_used.0 > 0 && shelter.labor_ms > 0)
            );
            let consumed: u64 = sim
                .state
                .events
                .iter()
                .filter_map(|event| match event.kind {
                    EventKind::Consumed {
                        source, quantity, ..
                    } if sim.state.inventories[&source].good == Good::Timber => Some(quantity.0),
                    _ => None,
                })
                .sum();
            assert!(
                consumed
                    >= sim
                        .state
                        .components
                        .shelters()
                        .map(|(_, shelter)| shelter)
                        .map(|shelter| shelter.timber_used.0)
                        .sum::<u64>()
            );
            let healer = sim.state.people[6].id;
            assert!(sim.own_skill_level(healer, Skill::Medicine) > 0);
            let water = sim
                .state
                .inventories
                .values()
                .find(|stock| stock.good == Good::Water && stock.quantity.0 > 0)
                .unwrap()
                .id;
            assert!(
                sim.state
                    .components
                    .health_knowledge(healer)
                    .expect("health knowledge exists")
                    .water[&water]
                    .purified
            );
            assert!(
                sim.state
                    .inventories
                    .values()
                    .any(|stock| stock.good == Good::Herb && stock.quantity.0 >= 20_000)
            );
            let ruler = sim.state.ruler;
            assert!(sim.state.components.shelter_occupancy(ruler).is_some());
            if id == "child-successor" {
                assert!(
                    sim.state
                        .components
                        .health_knowledge(ruler)
                        .is_none_or(|known| !known.water.contains_key(&water))
                );
            }
        }
    }

    #[test]
    fn the_founder_and_established_ruler_have_different_actual_supported_authority() {
        let founder = opening("settlement-founder", 1);
        let established = opening("established-ruler", 1);
        let founder_office = founder
            .state
            .institutions
            .as_ref()
            .unwrap()
            .institutions
            .values()
            .next()
            .unwrap();
        let ruler_office = established
            .state
            .institutions
            .as_ref()
            .unwrap()
            .institutions
            .values()
            .next()
            .unwrap();
        assert_ne!(
            founder_office.charter.offices[0].holder,
            founder.state.ruler
        );
        assert_eq!(
            ruler_office.charter.offices[0].holder,
            established.state.ruler
        );
        assert!(founder_office.funding.iter().all(|id| {
            founder.state.agreements.as_ref().unwrap().agreements[id].status
                == AgreementStatus::Fulfilled
        }));
        assert!(ruler_office.funding.iter().all(|id| {
            established.state.agreements.as_ref().unwrap().agreements[id].status
                == AgreementStatus::Fulfilled
        }));
        assert!(
            established
                .institution_view(established.state.ruler)
                .iter()
                .any(|view| view
                    .offices
                    .iter()
                    .any(|office| office.mandate.holder == established.state.ruler
                        && office.effective))
        );
        assert!(
            founder
                .state
                .projects
                .as_ref()
                .unwrap()
                .enterprises
                .is_empty()
        );
        assert!(
            established
                .state
                .projects
                .as_ref()
                .unwrap()
                .enterprises
                .values()
                .any(|enterprise| enterprise.tools_produced >= 5_000)
        );
    }

    #[test]
    fn child_control_follows_an_actual_human_birth_after_actual_death_and_inherits_no_office_or_mastery()
     {
        let sim = opening("child-successor", 1);
        let lineage = sim.state.lineage.as_ref().unwrap();
        assert_eq!(lineage.incarnations.len(), 2);
        let previous = &lineage.incarnations[0];
        let current = &lineage.incarnations[1];
        let born = sim
            .state
            .components
            .life(current.person)
            .expect("life exists")
            .born;
        assert!(previous.died_at.unwrap() < born);
        assert_eq!(born, sim.state.time);
        assert_eq!(sim.observe().life_stage, Some(LifeStage::Infant));
        assert!(sim.state.events.iter().any(|event| event.time == born
            && matches!(event.kind, EventKind::Born { child, .. } if child == current.person)));
        assert!(
            sim.state
                .events
                .iter()
                .any(|event| event.time == previous.died_at.unwrap()
                    && matches!(event.kind, EventKind::Died { actor } if actor == previous.person))
        );
        assert!(
            sim.state
                .invocation
                .as_ref()
                .unwrap()
                .history
                .contains_key(&previous.person)
        );
        assert!(sim.own_skill_level(previous.person, Skill::Craft) > 0);
        assert!(
            sim.own_skill_view(current.person)
                .iter()
                .all(|skill| skill.level == 0 && skill.practice_ms == 0)
        );
        assert!(
            sim.state
                .institutions
                .as_ref()
                .unwrap()
                .institutions
                .values()
                .all(|institution| institution
                    .charter
                    .offices
                    .iter()
                    .all(|office| office.holder != current.person))
        );
        assert!(
            sim.state
                .property
                .as_ref()
                .unwrap()
                .claims
                .values()
                .all(|claim| claim.claimant != current.person)
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
            assert!(PLAYABLE.contains(&choose(seed)));
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
        assert_eq!(a.state.scenario.id, choose(11));
        assert_eq!(a.state, b.state);
        assert_eq!(
            a.state.environment.as_ref().unwrap().climate,
            config.environment.climate
        );
        let before = a.state.clone();
        for _ in 0..100 {
            a.observe();
        }
        assert_eq!(a.state, before);
    }
}

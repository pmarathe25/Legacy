//! Deterministic, content-configured founding populations for the parallel host.
use game_parallel::{
    CaregivingPolicy, Dream, Person, PersonalityPerson, PurposePerson, Region, Route, World,
};
use game_parallel_genetics::{Sex, random};
use rayon::prelude::*;

pub use game_parallel_worldgen_owners::Settings;

/// Construct a new world from an authored rules template. No hidden runtime state is shared.
pub fn generate(template: &World, settings: &Settings) -> Result<World, String> {
    settings.validate(template)?;
    let mut world = template.clone();
    world.tick = 0;
    world.seed = settings.seed;
    world.shipments.clear();
    world.travelers.clear();
    if let Some(nutrition) = &mut world.nutrition {
        nutrition.people.clear();
    }
    world.regions = (1..=settings.regions)
        .map(|id| Region {
            id: u64::from(id),
            food: settings.initial_food,
            people: Vec::new(),
        })
        .collect();
    for person in 1..=u64::from(settings.population) {
        let region = ((person - 1) % u64::from(settings.regions)) as usize;
        world.regions[region].people.push(Person {
            id: person,
            hunger: 0,
            health: world.rules.max_health,
        });
    }
    world.routes.clear();
    for id in 1..=u64::from(settings.regions) {
        let next = id % u64::from(settings.regions) + 1;
        world.routes.push(Route {
            source: id,
            destination: next,
            ticks: settings.route_ticks,
        });
        if settings.regions > 2 {
            world.routes.push(Route {
                source: next,
                destination: id,
                ticks: settings.route_ticks,
            });
        }
    }
    let family = world.families.as_mut().unwrap();
    let adult_age = family.rules.adult_age;
    let female = family
        .members
        .iter()
        .find(|row| row.genome.sex == Sex::Female)
        .unwrap()
        .clone();
    let male = family
        .members
        .iter()
        .find(|row| row.genome.sex == Sex::Male)
        .unwrap()
        .clone();
    family.members = (1..=u64::from(settings.population))
        .into_par_iter()
        .map(|person| {
            let mut member = if ((person - 1) / u64::from(settings.regions)).is_multiple_of(2) {
                female.clone()
            } else {
                male.clone()
            };
            member.id = person;
            member.age = adult_age;
            member.born = None;
            member.parents.clear();
            member.partner = None;
            member.pregnancy = None;
            member.next_conception = 0;
            member.lifespan_loss = 0;
            member.alive = true;
            member.reproduction_allowed = true;
            member.reconsider_partnership = random(settings.seed ^ 0x7265636f6e736964, person, 0)
                % 100
                < u64::from(settings.reconsider_partnership_percent);
            for (locus, alleles) in member.genome.alleles.iter_mut().enumerate() {
                for (side, allele) in alleles.iter_mut().enumerate() {
                    *allele = (400 + random(settings.seed, person, (locus * 2 + side) as u64) % 601)
                        as u16;
                }
            }
            member
        })
        .collect();
    family.next_id = u64::from(settings.population) + 1;
    family.last_tick = 0;
    family.totals = Default::default();
    family.events.clear();
    let ids: Vec<_> = (1..=u64::from(settings.population)).collect();
    if let Some(social) = &mut world.social {
        game_parallel_worldgen_owners::social(social, &ids);
        social.pressures = ids
            .par_iter()
            .filter(|&&person| {
                random(settings.seed ^ 0x736f6369616c, person, 0) % 100
                    < u64::from(settings.social_pressure_percent)
            })
            .map(|&person| game_parallel::SocialPressurePolicy {
                person,
                fear_at_least: settings.social_fear_threshold,
                obligation_at_least: settings.social_obligation_threshold,
                enabled: true,
            })
            .collect();
    }
    if let Some(knowledge) = &mut world.knowledge {
        let source = (
            settings.seed,
            settings.regions,
            settings.initial_food
                - if settings.food_stores_per_region > 0 {
                    settings.initial_food / 2
                } else {
                    0
                },
            settings.reporting_percent,
            world.lineage.is_some().then_some(1),
        );
        game_parallel_worldgen_owners::knowledge(knowledge, &ids, source);
    }
    if let Some(personality) = &mut world.personality {
        personality.people = ids
            .par_iter()
            .map(|&person| PersonalityPerson {
                person,
                sociability: (200 + random(settings.seed ^ 17, person, 0) % 801) as u16,
                diligence: (300 + random(settings.seed ^ 23, person, 0) % 701) as u16,
            })
            .collect();
    }
    configure_owners(&mut world, settings, &ids)?;
    world.validate()?;
    Ok(world)
}

fn configure_owners(world: &mut World, settings: &Settings, ids: &[u64]) -> Result<(), String> {
    if let (Some(skills), Some(production)) = (&mut world.skills, &world.production) {
        game_parallel_skill_catalog::configure(skills, &production.recipe_categories)?;
    }
    let controlled = world.lineage.is_some().then_some(1);
    let mut teaching_recipes = Vec::new();
    if let Some(skills) = &world.skills {
        teaching_recipes.extend(
            skills
                .teachers
                .iter()
                .filter(|policy| policy.enabled)
                .map(|policy| policy.recipe),
        );
    }
    teaching_recipes.sort_unstable();
    teaching_recipes.dedup();
    if let Some(skills) = &mut world.skills {
        skills.records.clear();
        skills.events.clear();
        skills.lessons.clear();
        skills.teachers.clear();
    }
    let workers: Vec<_> = ids
        .par_iter()
        .copied()
        .filter(|&person| {
            Some(person) != controlled
                && random(settings.seed ^ 29, person, 0) % 100 < u64::from(settings.working_percent)
        })
        .collect();
    if let Some(skills) = &mut world.skills {
        skills.teachers = workers
            .par_iter()
            .flat_map_iter(|&person| {
                teaching_recipes
                    .iter()
                    .map(move |&recipe| game_parallel::SkillTeacherPolicy {
                        person,
                        recipe,
                        enabled: true,
                        eligible_at: 0,
                    })
            })
            .collect();
    }
    if let Some(goals) = &mut world.goals {
        goals.people.clear();
        goals.events.clear();
    }
    if let Some(auto) = &mut world.autowork {
        auto.policies.clear();
        auto.requests = game_parallel_worldgen_owners::work_requests(ids, settings, controlled);
    }
    if let Some(dreams) = &mut world.dreams {
        dreams.people = workers
            .par_iter()
            .map(|&person| Dream {
                person,
                recipe: 1,
                priority: 10,
                batch: 1,
                review_interval: 5,
                next_review: 0,
                enabled: true,
            })
            .collect();
        dreams.events.clear();
    }
    if let Some(purpose) = &mut world.purpose {
        purpose.people = workers
            .par_iter()
            .map(|&person| PurposePerson {
                person,
                deprivation: 0,
                eligible_at: 0,
            })
            .collect();
    }
    if let Some(care) = &mut world.caregiving {
        care.policies = ids
            .par_iter()
            .map(|&person| CaregivingPolicy {
                person,
                willing: true,
            })
            .collect();
        care.events.clear();
    }
    if let Some(agriculture) = &mut world.agriculture {
        agriculture.generate_volunteers(ids, |person| {
            Some(person) != controlled
                && random(settings.seed ^ 37, person, 0) % 100 < u64::from(settings.farming_percent)
        });
        game_parallel_worldgen_owners::farms(
            agriculture,
            &world.regions,
            settings.fields_per_region,
        );
    }
    if let Some(environment) = &mut world.environment
        && let Some(hazard) = environment.regions.first().cloned()
    {
        environment.regions = world
            .regions
            .par_iter()
            .map(|region| {
                let mut hazard = hazard.clone();
                hazard.region = region.id;
                hazard
            })
            .collect();
    }
    let production = world.production.as_mut().unwrap();
    let inventory = production.inventories.first().unwrap().clone();
    if !inventory.stocks.iter().any(|stock| stock.good == 3) {
        return Err("world template needs a log stock in its first inventory".into());
    }
    production.inventories = world
        .regions
        .par_iter()
        .map(|region| {
            let mut inventory = inventory.clone();
            inventory.region = region.id;
            for stock in &mut inventory.stocks {
                stock.quantity = if stock.good == 3 {
                    settings.starting_logs
                } else {
                    0
                };
                stock.capacity = stock.capacity.max(stock.quantity);
            }
            inventory
        })
        .collect();
    production.jobs.clear();
    production.events.clear();
    if let Some(threshold) = settings.care_pressure_threshold {
        world
            .caregiving
            .as_mut()
            .expect("validated caregiving")
            .rules
            .pressure_threshold = Some(threshold);
    }
    if settings.dependent_food_care {
        let care = world.caregiving.as_mut().expect("validated caregiving");
        care.inherit_births = true;
        care.rules.feed_from_cargo = true;
        care.rules.fetch_local_food = true;
    }
    if let Some(cargo) = &mut world.cargo {
        game_parallel_worldgen_food::configure_food(
            cargo,
            settings,
            ids,
            controlled,
            world.rules.food_per_meal,
            &mut world.regions,
        );
        if settings.dependent_food_care {
            cargo.auto_seek_age = world
                .caregiving
                .as_ref()
                .map(|care| care.rules.dependent_until);
        }
    }
    let traders: Vec<_> = ids
        .par_iter()
        .copied()
        .filter(|&person| {
            Some(person) != controlled
                && random(settings.seed ^ 31, person, 0) % 100 < u64::from(settings.trading_percent)
        })
        .map(|person| {
            (
                person,
                if ((person - 1) / u64::from(settings.regions)).is_multiple_of(2) {
                    1
                } else {
                    2
                },
            )
        })
        .collect();
    if let Some(cargo) = &mut world.goods_cargo {
        cargo.holdings = traders
            .par_iter()
            .map(|&(person, good)| game_parallel::GoodsHolding {
                person,
                items: vec![game_parallel::Amount {
                    good,
                    quantity: settings.trader_items,
                }]
                .into(),
            })
            .collect();
    }
    if let Some(market) = &mut world.market {
        game_parallel_worldgen_owners::market(
            market,
            ids,
            &traders,
            (
                settings.seed,
                settings.requesting_percent,
                settings.helpful_percent,
                controlled,
            ),
        );
    }
    if let Some(agreements) = &mut world.agreements {
        agreements.reset_for_generation();
    }
    if let Some(opinions) = &mut world.opinions {
        opinions.reset_for_generation();
        game_parallel_worldgen_owners::claim_tolerance(opinions, ids, settings, controlled);
    }
    if settings.confrontation_percent > 0 {
        world.conflict = Some(game_parallel::Conflict {
            damage: settings.confrontation_damage,
            next_id: 1,
            encounters: Vec::new(),
            events: Vec::new(),
        });
    }
    if let Some(missions) = &mut world.missions {
        missions.entries.clear();
        missions.events.clear();
        missions.next_id = 1;
    }
    if let Some(disease) = &mut world.disease {
        disease.cases.clear();
        disease.events.clear();
        if settings.initial_infection {
            let strain = disease
                .strains
                .first()
                .ok_or("world template needs an infection strain")?
                .id;
            disease.cases.push(game_parallel::DiseaseCase {
                person: u64::from(settings.population.min(3)),
                strain,
                infected_at: 0,
                source: None,
            });
        }
    } else if settings.initial_infection {
        return Err("world template has no disease owner".into());
    }
    if let Some(autonomy) = &mut world.autonomy {
        autonomy.generate_actors(ids, controlled, settings.seed);
    }
    if let Some(lineage) = &mut world.lineage {
        lineage.current = Some(1);
        lineage.started = 0;
        lineage.waiting_since = None;
        lineage.history.clear();
        lineage.events.clear();
        if let Some(invocation) = &mut lineage.invocation {
            invocation.owner = Some(1);
            invocation.active.clear();
            invocation.burden = 0;
            invocation.exposure = 0;
            invocation.strain_accepted_until = None;
            invocation.lifespan_accepted_until = None;
            invocation.loss_approved = false;
            invocation.loss_triggered = false;
            invocation.control_until = None;
            invocation.pending = None;
            invocation.revision = 0;
        }
    }
    configure_institutions(world, settings);
    Ok(())
}

fn configure_institutions(world: &mut World, _settings: &Settings) {
    world.institutions = None;
    world.civic_auto = None;
}

pub fn load(path: &std::path::Path, seed: Option<u64>) -> Result<World, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut settings = Settings::from_toml(&text)?;
    if let Some(seed) = seed {
        settings.seed = seed;
    }
    let template_path = path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(&settings.template);
    let template = std::fs::read_to_string(template_path).map_err(|error| error.to_string())?;
    let mut template = World::from_toml(&template)?;
    if let (Some(objects), Some(recipes)) = (&settings.objects_catalog, &settings.recipes_catalog) {
        let base = path.parent().unwrap_or(std::path::Path::new("."));
        game_parallel_content_import::augment(
            template
                .production
                .as_mut()
                .ok_or("catalog requires production")?,
            &base.join(objects),
            &base.join(recipes),
            settings.catalog_tick_millis,
            settings.catalog_stock_capacity,
            settings.catalog_food_relief_divisor,
        )?;
    }
    generate(&template, &settings)
}

#[cfg(test)]
#[path = "../tests/support/generation.rs"]
mod tests;

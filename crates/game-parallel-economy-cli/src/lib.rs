//! Load new economic scenarios from authored content and retain lot ages in saves.
mod population;
use game_parallel_autowork::{Autowork, Policy};
use game_parallel_economy::{
    Agreements, Agriculture, CropPhase, DependentCare, Dream, Dreams, Economy, Families,
    FarmPolicy, Field, Goals, Person, Route, Rules, State, Travel,
};
use game_parallel_economy_agriculture::content as crop_content;
use game_parallel_lot_content::Catalog;
use game_parallel_lot_runtime::Snapshot;
use game_parallel_lots::{Container, Place};
use game_parallel_production::{Inventory, Production};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    seed: u64,
    tick_millis: u64,
    stock_capacity: u64,
    food_relief_divisor: u32,
    rules: Rules,
    regions: Vec<Region>,
    #[serde(default)]
    people: Vec<Resident>,
    #[serde(default)]
    population: Vec<population::Profile>,
    #[serde(default)]
    routes: Vec<Route>,
    #[serde(default)]
    family: Option<Families>,
    #[serde(default)]
    care: Option<DependentCare>,
    #[serde(default)]
    agreements: Option<Agreements>,
    #[serde(default)]
    promise_offering: Vec<PromiseOfferConfig>,
    #[serde(default)]
    work_requests: Option<game_parallel_autowork::requests::Requests>,
    #[serde(default)]
    work_tools: Vec<WorkToolConfig>,
    #[serde(default)]
    work_sites: Option<WorkSitesConfig>,
    #[serde(default)]
    construction: Option<ConstructionConfig>,
    #[serde(default)]
    opinions: Option<game_parallel_opinions::Opinions>,
    #[serde(default)]
    aspirations: Vec<Aspiration>,
    #[serde(default)]
    cultivation: Option<Cultivation>,
    #[serde(default)]
    dynamics: Option<game_parallel_economy::Dynamics>,
    #[serde(default)]
    remedies: Vec<game_parallel_economy_dynamics::treatment::Remedy>,
    #[serde(default)]
    missions: Option<game_parallel_economy::Missions>,
    #[serde(default)]
    campaign: Option<game_parallel_economy::Campaign>,
    #[serde(default)]
    skills: Option<game_parallel_economy::Skills>,
    #[serde(default)]
    knowledge: Option<game_parallel_economy::Knowledge>,
    #[serde(default)]
    combat: Option<game_parallel_economy_combat::Config>,
    #[serde(default)]
    fatigue: Option<game_parallel_economy::Fatigue>,
    #[serde(default)]
    environment: Option<EnvironmentConfig>,
    #[serde(default)]
    purpose: Option<game_parallel_economy::Purpose>,
    #[serde(default)]
    recovery: Option<game_parallel_economy::RecoveryRules>,
    #[serde(default)]
    gathering: Option<GatheringConfig>,
    #[serde(default)]
    food_search: Option<game_parallel_economy::FoodSearch>,
    #[serde(default)]
    giving: Option<GivingConfig>,
    #[serde(default)]
    medical: Option<MedicalConfig>,
    #[serde(default)]
    hauling: Option<HaulingConfig>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConstructionConfig {
    blueprints: Vec<BuildingBlueprintConfig>,
    #[serde(default)]
    policies: Vec<game_parallel_economy::ConstructionPolicy>,
    #[serde(default)]
    demolition: Vec<game_parallel_economy::DemolitionPolicy>,
    #[serde(default)]
    goal_builders: Vec<game_parallel_economy::GoalBuilder>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildingBlueprintConfig {
    id: u32,
    name: String,
    kind: String,
    capacity: u64,
    kit: String,
    quantity: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkSitesConfig {
    #[serde(default)]
    sites: Vec<game_parallel_economy::WorkSite>,
    #[serde(default)]
    requirements: Vec<WorkSiteRequirementConfig>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkSiteRequirementConfig {
    recipe: String,
    kind: String,
    slots: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkToolConfig {
    recipe: String,
    good: String,
    quantity: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PromiseOfferConfig {
    person: u64,
    recipient: u64,
    good: String,
    quantity: u64,
    due_after: u64,
    retry_ticks: u64,
    willing: bool,
    #[serde(default)]
    recipient_hunger_at_least: u32,
    #[serde(default)]
    minimum_trust: Option<i32>,
    #[serde(default)]
    pressure_threshold: Option<u64>,
    #[serde(default)]
    after_work_recipe: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HaulingConfig {
    #[serde(default)]
    policies: Vec<HaulingPolicyConfig>,
    #[serde(default)]
    asking: Vec<HaulingAskingConfig>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HaulingAskingConfig {
    person: u64,
    carrier: u64,
    source: u64,
    destination: u64,
    good: String,
    enabled: bool,
    retry_ticks: u64,
    request_lifetime: u64,
    #[serde(default)]
    next_review: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HaulingPolicyConfig {
    person: u64,
    source: u64,
    destination: u64,
    good: String,
    quantity: u64,
    willing: bool,
    #[serde(default)]
    requires_request: bool,
    #[serde(default)]
    request_pressure_threshold: Option<u64>,
    #[serde(default)]
    pressured_by: Option<u64>,
    #[serde(default)]
    pressure_threshold: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentConfig {
    regions: Vec<HazardConfig>,
    #[serde(default)]
    seasons: Vec<game_parallel_economy::Season>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HazardConfig {
    region: u64,
    damage: u32,
    shelter: u32,
    shelter_reduction: u32,
    #[serde(default)]
    shelter_good: Option<GoodReference>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum GoodReference {
    Name(String),
    Id(u32),
}
fn resolve_environment(
    config: EnvironmentConfig,
    goods: &BTreeMap<&str, u32>,
) -> Result<game_parallel_economy::Environment, String> {
    let regions = config
        .regions
        .into_iter()
        .map(|hazard| {
            let shelter_good = hazard
                .shelter_good
                .map(|reference| match reference {
                    GoodReference::Id(id) => Ok(id),
                    GoodReference::Name(name) => goods
                        .get(name.as_str())
                        .copied()
                        .ok_or_else(|| format!("unknown shelter good {name}")),
                })
                .transpose()?;
            Ok(game_parallel_economy::Hazard {
                region: hazard.region,
                damage: hazard.damage,
                shelter: hazard.shelter,
                shelter_reduction: hazard.shelter_reduction,
                shelter_good,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(game_parallel_economy::Environment {
        regions,
        seasons: config.seasons,
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GatheringConfig {
    max_hunger: u32,
    #[serde(default)]
    inherit_births: bool,
    nodes: Vec<ResourceConfig>,
    #[serde(default)]
    policies: Vec<game_parallel_economy::GatherPolicy>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceConfig {
    id: u64,
    region: u64,
    good: String,
    available: u64,
    capacity: u64,
    regrowth: u64,
    yield_per_action: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GivingConfig {
    #[serde(default)]
    inherit_births: bool,
    #[serde(default)]
    policies: Vec<GivingPolicyConfig>,
    #[serde(default)]
    responses: Vec<FoodResponseConfig>,
    #[serde(default)]
    learning: Vec<game_parallel_economy::SharingLearning>,
    #[serde(default)]
    asking: Vec<game_parallel_economy::FoodAsking>,
    #[serde(default)]
    acquaintance_asking: Vec<game_parallel_economy::AcquaintanceAsking>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FoodResponseConfig {
    person: u64,
    good: String,
    quantity: u64,
    recipient_hunger_at_least: u32,
    willing: bool,
    #[serde(default)]
    minimum_trust: Option<i32>,
    #[serde(default)]
    affinity_threshold: Option<i32>,
    #[serde(default)]
    pressure_threshold: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GivingPolicyConfig {
    person: u64,
    recipient: u64,
    good: String,
    quantity: u64,
    recipient_hunger_at_least: u32,
    willing: bool,
    #[serde(default)]
    requires_request: bool,
    #[serde(default)]
    pressured_by: Option<u64>,
    #[serde(default)]
    pressure_threshold: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MedicalConfig {
    policies: Vec<MedicalPolicyConfig>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MedicalPolicyConfig {
    person: u64,
    patient: u64,
    medicine: String,
    quantity: u64,
    willing: bool,
    #[serde(default)]
    pressured_by: Option<u64>,
    #[serde(default)]
    pressure_threshold: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cultivation {
    max_hunger: u32,
    #[serde(default)]
    inherit_births: bool,
    fields: Vec<CultivatedField>,
    policies: Vec<FarmPolicy>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CultivatedField {
    id: u64,
    region: u64,
    crop: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Aspiration {
    person: u64,
    recipe: String,
    priority: i32,
    batch: u64,
    review_interval: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Region {
    id: u64,
    #[serde(default)]
    stock: BTreeMap<String, u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resident {
    id: u64,
    region: u64,
    health: u32,
    hunger: u32,
    willing_to_work: bool,
    seek_meals: bool,
    #[serde(default)]
    controlled: bool,
    #[serde(default)]
    recipe: Option<String>,
    #[serde(default)]
    goods: BTreeMap<String, u64>,
}

/// Construct fresh custody only for a new scenario, never for a saved world.
pub fn scenario(path: &Path, content: &Path) -> Result<Economy, String> {
    scenario_with_seed(path, content, None)
}

/// Override the scenario seed for both generation and subsequent simulation choices.
pub fn scenario_with_seed(
    path: &Path,
    content: &Path,
    seed: Option<u64>,
) -> Result<Economy, String> {
    scenario_with_options(path, content, seed, None)
}

/// Add authored population profiles to a scenario without changing its other content.
pub fn scenario_with_options(
    path: &Path,
    content: &Path,
    seed: Option<u64>,
    population_path: Option<&Path>,
) -> Result<Economy, String> {
    let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut scenario: Scenario = toml::from_str(&source).map_err(|error| error.to_string())?;
    if let Some(path) = population_path {
        let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        let overlay: population::Overlay =
            toml::from_str(&source).map_err(|error| error.to_string())?;
        scenario.population.extend(overlay.population);
    }
    if let Some(seed) = seed {
        scenario.seed = seed;
    }
    let mut regions = HashSet::new();
    if scenario
        .regions
        .iter()
        .any(|r| r.id == 0 || !regions.insert(r.id))
        || scenario.people.iter().any(|p| !regions.contains(&p.region))
    {
        return Err("invalid scenario regions".into());
    }
    let generated = population::generate(
        scenario.seed,
        &regions,
        &scenario.people,
        &scenario.population,
    )?;
    scenario.people.extend(generated);
    let mut social_rows = population::social(scenario.seed, &scenario.population)?;
    if !social_rows.participants.is_empty() {
        let dynamics = scenario
            .dynamics
            .as_mut()
            .ok_or("generated social preferences require dynamics")?;
        let personality = dynamics
            .personality
            .as_mut()
            .ok_or("generated social preferences require personality")?;
        let authored: HashSet<_> = personality.people.iter().map(|row| row.person).collect();
        social_rows
            .personalities
            .retain(|row| !authored.contains(&row.person));
        personality.people.extend(social_rows.personalities);
        let social = dynamics
            .social
            .as_mut()
            .ok_or("generated social preferences require social rules")?;
        if let Some(needs) = &mut social.needs {
            let authored: HashSet<_> = needs.people.iter().map(|row| row.person).collect();
            needs.people.extend(
                social_rows
                    .participants
                    .iter()
                    .filter(|row| !authored.contains(&row.person))
                    .map(|row| game_parallel_social::Need {
                        person: row.person,
                        isolation: 0,
                    }),
            );
        }
        let authored: HashSet<_> = social.participants.iter().map(|row| row.person).collect();
        social_rows
            .participants
            .retain(|row| !authored.contains(&row.person));
        social.participants.extend(social_rows.participants);
        let authored: HashSet<_> = social.pressures.iter().map(|row| row.person).collect();
        social_rows
            .pressures
            .retain(|row| !authored.contains(&row.person));
        social.pressures.extend(social_rows.pressures);
    }
    let mut preferences = population::claims(scenario.seed, &scenario.population)?;
    if !preferences.is_empty() {
        let claims = scenario
            .dynamics
            .as_mut()
            .and_then(|owner| owner.claims.as_mut())
            .ok_or("generated claim preferences require personal claims")?;
        let authored: HashSet<_> = claims.preferences.iter().map(|row| row.person).collect();
        preferences.retain(|row| !authored.contains(&row.person));
        claims.preferences.extend(preferences);
    }
    let mut help = population::help(scenario.seed, &scenario.population)?;
    if !help.responses.is_empty() {
        let giving = scenario
            .giving
            .as_mut()
            .ok_or("generated help preferences require giving")?;
        let authored: HashSet<_> = giving
            .policies
            .iter()
            .map(|p| p.person)
            .chain(giving.responses.iter().map(|p| p.person))
            .collect();
        help.responses.retain(|p| !authored.contains(&p.person));
        giving.responses.extend(help.responses);
        let authored: HashSet<_> = giving.learning.iter().map(|p| p.person).collect();
        help.learning.retain(|p| !authored.contains(&p.person));
        giving.learning.extend(help.learning);
        let authored: HashSet<_> = giving
            .acquaintance_asking
            .iter()
            .map(|p| p.person)
            .collect();
        help.acquaintance_asking
            .retain(|p| !authored.contains(&p.person));
        giving.acquaintance_asking.extend(help.acquaintance_asking);
    }
    let mut work_support = population::work_support(scenario.seed, &scenario.population)?;
    if !work_support.preferences.is_empty() {
        let requests = scenario
            .work_requests
            .as_mut()
            .ok_or("generated work support requires work requests")?;
        let authored: HashSet<_> = requests.preferences.iter().map(|p| p.person).collect();
        work_support
            .preferences
            .retain(|p| !authored.contains(&p.person));
        requests.preferences.extend(work_support.preferences);
        let authored: HashSet<_> = requests.askers.iter().map(|p| p.person).collect();
        work_support
            .askers
            .retain(|p| !authored.contains(&p.person));
        requests.askers.extend(work_support.askers);
        let authored: HashSet<_> = scenario
            .aspirations
            .iter()
            .map(|a| (a.person, a.recipe.as_str()))
            .collect();
        work_support
            .aspirations
            .retain(|a| !authored.contains(&(a.person, a.recipe.as_str())));
        scenario.aspirations.extend(work_support.aspirations);
    }
    let mut farming = population::farming(scenario.seed, &scenario.population);
    if !farming.is_empty() {
        let cultivation = scenario
            .cultivation
            .as_mut()
            .ok_or("generated farming preferences require cultivation")?;
        let authored: HashSet<_> = cultivation.policies.iter().map(|row| row.person).collect();
        farming.retain(|row| !authored.contains(&row.person));
        cultivation.policies.extend(farming);
    }
    let mut food_search = population::food_search(scenario.seed, &scenario.population);
    if !food_search.is_empty() {
        let search = scenario
            .food_search
            .as_mut()
            .ok_or("generated food-search preferences require food search")?;
        let authored: HashSet<_> = search.people.iter().map(|row| row.person).collect();
        food_search.retain(|row| !authored.contains(&row.person));
        search.people.extend(food_search);
    }
    if let Some(family) = scenario.family.as_mut() {
        let mut founders = population::founders(scenario.seed, &scenario.population)?;
        let explicitly_authored: HashSet<_> =
            family.members.iter().map(|member| member.id).collect();
        founders.retain(|member| !explicitly_authored.contains(&member.id));
        if let Some(maximum) = founders.iter().map(|member| member.id).max() {
            family.next_id = family
                .next_id
                .max(maximum.checked_add(1).ok_or("founder identity overflow")?);
        }
        family.members.extend(founders);
    } else if population::has_founders(&scenario.population) {
        return Err("generated founder profiles require family rules".into());
    }
    let mut dispositions = population::sensitivities(&scenario.population);
    if !dispositions.is_empty() {
        let opinions = scenario
            .opinions
            .as_mut()
            .ok_or("generated sensitivities require opinions")?;
        let individually_authored: HashSet<_> = opinions
            .sensitivities
            .iter()
            .map(|row| row.person)
            .collect();
        dispositions.retain(|row| !individually_authored.contains(&row.person));
        opinions.sensitivities.extend(dispositions);
    }
    let mut production = Production {
        max_work_hunger: scenario.rules.max_work_hunger,
        goods: vec![],
        recipes: vec![],
        recipe_categories: vec![],
        inventories: scenario
            .regions
            .iter()
            .map(|r| Inventory {
                region: r.id,
                stocks: vec![],
            })
            .collect(),
        jobs: vec![],
        events: vec![],
    };
    game_parallel_content_import::augment(
        &mut production,
        &content.join("objects"),
        &content.join("workshop"),
        scenario.tick_millis,
        scenario.stock_capacity,
        scenario.food_relief_divisor,
    )?;
    let mut skills = scenario.skills;
    if let Some(skills) = &mut skills {
        game_parallel_skill_catalog::configure(skills, &production.recipe_categories)?;
    }
    let goods: BTreeMap<_, _> = production
        .goods
        .iter()
        .map(|good| (good.name.as_str(), good.id))
        .collect();
    let recipes: BTreeMap<_, _> = production
        .recipes
        .iter()
        .map(|recipe| (recipe.name.as_str(), recipe.id))
        .collect();
    let work_tools = scenario
        .work_tools
        .into_iter()
        .map(|row| {
            Ok(game_parallel_economy::WorkTool {
                recipe: *recipes
                    .get(row.recipe.as_str())
                    .ok_or_else(|| format!("unknown work tool recipe {}", row.recipe))?,
                good: *goods
                    .get(row.good.as_str())
                    .ok_or_else(|| format!("unknown work tool good {}", row.good))?,
                quantity: row.quantity,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let work_sites = scenario
        .work_sites
        .map(|config| {
            let requirements = config
                .requirements
                .into_iter()
                .map(|r| {
                    Ok(game_parallel_economy::WorkSiteRequirement {
                        recipe: *recipes
                            .get(r.recipe.as_str())
                            .ok_or_else(|| format!("unknown work site recipe {}", r.recipe))?,
                        kind: r.kind,
                        slots: r.slots,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok::<_, String>(game_parallel_economy::WorkSites {
                next_id: 0,
                sites: config.sites,
                requirements,
                bindings: vec![],
            })
        })
        .transpose()?;
    let construction = scenario
        .construction
        .map(|config| {
            let blueprints = config
                .blueprints
                .into_iter()
                .map(|b| {
                    Ok(game_parallel_economy::BuildingBlueprint {
                        id: b.id,
                        name: b.name,
                        kind: b.kind,
                        capacity: b.capacity,
                        kit: *goods
                            .get(b.kit.as_str())
                            .ok_or_else(|| format!("unknown construction kit {}", b.kit))?,
                        quantity: b.quantity,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok::<_, String>(game_parallel_economy::Construction {
                blueprints,
                policies: config.policies,
                events: vec![],
                demolition: config.demolition,
                demolition_events: vec![],
                goal_builders: config.goal_builders,
            })
        })
        .transpose()?;
    let mut agreements = scenario.agreements;
    if !scenario.promise_offering.is_empty() {
        let owner = agreements
            .as_mut()
            .ok_or("promise offering requires agreements")?;
        let offering = scenario
            .promise_offering
            .into_iter()
            .map(|p| {
                Ok(game_parallel_economy::PromiseOfferPolicy {
                    person: p.person,
                    recipient: p.recipient,
                    good: *goods
                        .get(p.good.as_str())
                        .ok_or_else(|| format!("unknown promise good {}", p.good))?,
                    quantity: p.quantity,
                    due_after: p.due_after,
                    retry_ticks: p.retry_ticks,
                    willing: p.willing,
                    recipient_hunger_at_least: p.recipient_hunger_at_least,
                    minimum_trust: p.minimum_trust,
                    pressure_threshold: p.pressure_threshold,
                    next_review: 0,
                    after_work_recipe: p
                        .after_work_recipe
                        .as_deref()
                        .map(|name| {
                            recipes
                                .get(name)
                                .copied()
                                .ok_or_else(|| format!("unknown promise work recipe {name}"))
                        })
                        .transpose()?,
                    last_work_offer_at: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        owner.offering.extend(offering);
    }
    let dynamics = game_parallel_economy_dynamics::configure_remedies!(scenario, goods);
    for (inventory, region) in production.inventories.iter_mut().zip(&scenario.regions) {
        for (name, quantity) in &region.stock {
            let good = *goods
                .get(name.as_str())
                .ok_or_else(|| format!("unknown good {name}"))?;
            let stock = inventory
                .stocks
                .iter_mut()
                .find(|row| row.good == good)
                .ok_or("missing stock")?;
            stock.quantity = *quantity;
        }
    }
    let catalog = Catalog::from_tick_millis(&production.goods, scenario.tick_millis)?;
    let environment = scenario
        .environment
        .map(|config| resolve_environment(config, &goods))
        .transpose()?;
    let giving = scenario
        .giving
        .map(|config| -> Result<_, String> {
            let policies = config
                .policies
                .into_iter()
                .map(|p| {
                    Ok(game_parallel_economy::GivingPolicy {
                        person: p.person,
                        recipient: p.recipient,
                        good: *goods
                            .get(p.good.as_str())
                            .ok_or_else(|| format!("unknown giving good {}", p.good))?,
                        quantity: p.quantity,
                        recipient_hunger_at_least: p.recipient_hunger_at_least,
                        willing: p.willing,
                        requires_request: p.requires_request,
                        pressured_by: p.pressured_by,
                        pressure_threshold: p.pressure_threshold,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let responses = config
                .responses
                .into_iter()
                .map(|p| {
                    Ok(game_parallel_economy::FoodResponse {
                        person: p.person,
                        good: *goods
                            .get(p.good.as_str())
                            .ok_or_else(|| format!("unknown giving good {}", p.good))?,
                        quantity: p.quantity,
                        recipient_hunger_at_least: p.recipient_hunger_at_least,
                        willing: p.willing,
                        minimum_trust: p.minimum_trust,
                        affinity_threshold: p.affinity_threshold,
                        pressure_threshold: p.pressure_threshold,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(game_parallel_economy::Giving {
                inherit_births: config.inherit_births,
                policies,
                responses,
                learning: config.learning,
                asking: config.asking,
                requests: vec![],
                acquaintance_asking: config.acquaintance_asking,
            })
        })
        .transpose()?;
    let hauling = scenario
        .hauling
        .map(|config| -> Result<_, String> {
            let policies = config
                .policies
                .into_iter()
                .map(|p| {
                    Ok(game_parallel_economy::HaulingPolicy {
                        person: p.person,
                        source: p.source,
                        destination: p.destination,
                        good: *goods
                            .get(p.good.as_str())
                            .ok_or_else(|| format!("unknown hauling good {}", p.good))?,
                        quantity: p.quantity,
                        willing: p.willing,
                        requires_request: p.requires_request,
                        request_pressure_threshold: p.request_pressure_threshold,
                        pressured_by: p.pressured_by,
                        pressure_threshold: p.pressure_threshold,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let asking = config
                .asking
                .into_iter()
                .map(|p| {
                    Ok(game_parallel_economy::HaulingAsking {
                        person: p.person,
                        carrier: p.carrier,
                        source: p.source,
                        destination: p.destination,
                        good: *goods
                            .get(p.good.as_str())
                            .ok_or_else(|| format!("unknown hauling good {}", p.good))?,
                        enabled: p.enabled,
                        retry_ticks: p.retry_ticks,
                        request_lifetime: p.request_lifetime,
                        next_review: p.next_review,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(game_parallel_economy::Hauling {
                policies,
                asking,
                requests: vec![],
            })
        })
        .transpose()?;
    let medical = scenario
        .medical
        .map(|config| -> Result<_, String> {
            let policies = config
                .policies
                .into_iter()
                .map(|p| {
                    Ok(game_parallel_economy::MedicalPolicy {
                        person: p.person,
                        patient: p.patient,
                        medicine: *goods
                            .get(p.medicine.as_str())
                            .ok_or_else(|| format!("unknown medicine {}", p.medicine))?,
                        quantity: p.quantity,
                        willing: p.willing,
                        pressured_by: p.pressured_by,
                        pressure_threshold: p.pressure_threshold,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(game_parallel_economy::Medical { policies })
        })
        .transpose()?;
    let gathering = scenario
        .gathering
        .map(|config| -> Result<_, String> {
            let nodes = config
                .nodes
                .into_iter()
                .map(|node| {
                    Ok(game_parallel_economy::ResourceNode {
                        id: node.id,
                        region: node.region,
                        good: *goods
                            .get(node.good.as_str())
                            .ok_or_else(|| format!("unknown resource good {}", node.good))?,
                        available: node.available,
                        capacity: node.capacity,
                        regrowth: node.regrowth,
                        yield_per_action: node.yield_per_action,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(game_parallel_economy::Gathering {
                max_hunger: config.max_hunger,
                inherit_births: config.inherit_births,
                nodes,
                policies: config.policies,
            })
        })
        .transpose()?;
    let combat = scenario
        .combat
        .map(|config| {
            config.build(
                &scenario.regions.iter().map(|r| r.id).collect::<Vec<_>>(),
                scenario.seed,
                |name| goods.get(name).copied(),
            )
        })
        .transpose()?;
    let agriculture = scenario
        .cultivation
        .map(|state| -> Result<_, String> {
            let crops = crop_content::load(
                &content.join("agriculture"),
                &production.goods,
                scenario.tick_millis,
            )?;
            let fields = state
                .fields
                .into_iter()
                .map(|field| {
                    let crop = crops
                        .get(&field.crop)
                        .ok_or_else(|| format!("unknown crop {}", field.crop))?
                        .id;
                    Ok(Field {
                        id: field.id,
                        region: field.region,
                        crop,
                        phase: CropPhase::Fallow,
                        remaining: 0,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Agriculture {
                inherit_births: state.inherit_births,
                crops: crops.into_values().collect(),
                fields,
                policies: state.policies,
                max_hunger: state.max_hunger,
            })
        })
        .transpose()?;
    let mut custody = catalog.fresh_world(&production.inventories, 0)?;
    let mut people = Vec::new();
    let mut policies = Vec::new();
    let dreams: Vec<_> = scenario
        .aspirations
        .iter()
        .map(|dream| {
            let recipe = *recipes
                .get(dream.recipe.as_str())
                .ok_or_else(|| format!("unknown recipe {}", dream.recipe))?;
            Ok(Dream {
                person: dream.person,
                recipe,
                priority: dream.priority,
                batch: dream.batch,
                review_interval: dream.review_interval,
                next_review: 0,
                enabled: true,
            })
        })
        .collect::<Result<_, String>>()?;
    for resident in scenario.people {
        let mut lots = Vec::new();
        for (name, quantity) in resident.goods {
            let good = *goods
                .get(name.as_str())
                .ok_or_else(|| format!("unknown good {name}"))?;
            lots.push(catalog.fresh(good, quantity, 0)?);
        }
        custody.containers.push(Container::new(
            Place::Person(resident.id),
            scenario.rules.carrying_capacity,
            lots,
        )?);
        if let Some(name) = resident.recipe {
            let recipe = *recipes
                .get(name.as_str())
                .ok_or_else(|| format!("unknown recipe {name}"))?;
            policies.push(Policy {
                person: resident.id,
                recipe,
                enabled: true,
                disregard_claims: false,
            });
        }
        people.push(Person {
            id: resident.id,
            region: Some(resident.region),
            health: resident.health,
            hunger: resident.hunger,
            willing_to_work: resident.willing_to_work,
            seek_meals: resident.seek_meals,
            controlled: resident.controlled,
        });
    }
    Economy::compile(State {
        seed: scenario.seed,
        physical: Snapshot {
            custody,
            goods: production.goods,
            recipes: production.recipes,
            tick_millis: scenario.tick_millis,
            work: vec![],
        },
        people,
        rules: scenario.rules,
        market: None,
        agreements,
        opinions: scenario.opinions,
        autowork: Some(Autowork {
            next_id: 1,
            policies,
            requests: scenario.work_requests,
        }),
        work_tools,
        work_sites,
        construction,
        goals: (!dreams.is_empty()).then_some(Goals {
            retry_interval: 3,
            plan_inputs: true,
            people: vec![],
            events: vec![],
        }),
        dreams: (!dreams.is_empty()).then_some(Dreams {
            people: dreams,
            events: vec![],
        }),
        nutrition: None,
        pending_food: vec![],
        travel: (!scenario.routes.is_empty()).then_some(Travel {
            routes: scenario.routes,
            journeys: vec![],
        }),
        family: scenario.family,
        care: scenario.care,
        agriculture,
        dynamics,
        missions: scenario.missions,
        campaign: scenario.campaign,
        skills,
        knowledge: scenario.knowledge,
        combat,
        fatigue: scenario.fatigue,
        environment,
        purpose: scenario.purpose,
        recovery: scenario.recovery,
        gathering,
        food_search: scenario.food_search,
        giving,
        medical,
        hauling,
    })
}

pub fn save(world: &Economy, path: &Path) -> Result<(), String> {
    let data = serde_json::to_vec(&world.snapshot()).map_err(|error| error.to_string())?;
    game_parallel_persistence::save(path, &data)
}
pub fn load(path: &Path) -> Result<Economy, String> {
    let data = std::fs::read(path).map_err(|error| error.to_string())?;
    let state = serde_json::from_slice(&data).map_err(|error| error.to_string())?;
    Economy::compile(state)
}

#[cfg(test)]
#[path = "../tests/support/scenario.rs"]
mod tests;

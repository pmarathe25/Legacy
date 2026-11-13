//! Load new economic scenarios from authored content and retain lot ages in saves.
mod population;
mod seed_batches;
use game_autowork::{Autowork, Policy};
use game_economy::{
    Agreements, Agriculture, CropPhase, DependentCare, Dream, Dreams, Economy, Families,
    FarmPolicy, Field, Goals, Person, Route, RouteClosure, RouteObservation, RouteSharing, Rules,
    State, Travel,
};
use game_economy_agriculture::content as crop_content;
use game_lot_content::Catalog;
use game_lot_runtime::Snapshot;
use game_lots::{Container, Place};
use game_production::{Inventory, Production};
use rayon::prelude::*;
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
    route_closures: Vec<RouteClosure>,
    #[serde(default)]
    personal_route_planning: bool,
    #[serde(default)]
    route_observations: Vec<RouteObservation>,
    #[serde(default)]
    route_sharing: Vec<RouteSharing>,
    #[serde(default)]
    family: Option<Families>,
    #[serde(default)]
    care: Option<DependentCare>,
    #[serde(default)]
    agreements: Option<Agreements>,
    #[serde(default)]
    market: Option<AuthoredMarket>,
    #[serde(default)]
    promise_offering: Vec<PromiseOfferConfig>,
    #[serde(default)]
    work_promise_offering: Vec<WorkPromiseOfferConfig>,
    #[serde(default)]
    work_requests: Option<game_autowork::requests::Requests>,
    #[serde(default)]
    shared_work: Option<SharedWorkConfig>,
    #[serde(default)]
    work_offer_values: Vec<WorkOfferValueConfig>,
    #[serde(default)]
    cultivation_offer_values: Vec<WorkOfferValueConfig>,
    #[serde(default)]
    work_tools: Vec<WorkToolConfig>,
    #[serde(default)]
    tool_wear: Option<ToolWearConfig>,
    #[serde(default)]
    work_sites: Option<WorkSitesConfig>,
    #[serde(default)]
    construction: Option<ConstructionConfig>,
    #[serde(default)]
    confinement: Option<game_economy::Confinement>,
    #[serde(default)]
    opinions: Option<game_opinions::Opinions>,
    #[serde(default)]
    aspirations: Vec<Aspiration>,
    #[serde(default)]
    goals: Option<AuthoredGoals>,
    #[serde(default)]
    aspiration_learning: Vec<game_economy::Learning>,
    #[serde(default)]
    inherit_aspiration_learning: bool,
    #[serde(default)]
    cultivation: Option<Cultivation>,
    #[serde(default)]
    soil_amendment: Option<SoilAmendmentConfig>,
    #[serde(default)]
    irrigation: Option<IrrigationConfig>,
    #[serde(default)]
    dynamics: Option<game_economy::Dynamics>,
    #[serde(default)]
    language: Option<game_language::Language>,
    #[serde(default)]
    language_topic_profiles: Vec<LanguageTopicProfile>,
    #[serde(default)]
    signals: Option<SignalsConfig>,
    #[serde(default)]
    work_capabilities: Option<WorkCapabilitiesConfig>,
    #[serde(default)]
    mechanics: Option<MechanicsConfig>,
    #[serde(default)]
    personal_choices: Vec<game_personal_choice::Person>,
    #[serde(default)]
    birth_choices: Option<game_personal_choice::BirthProfile>,
    #[serde(default)]
    meal_preferences: Vec<MealPreference>,
    #[serde(default)]
    remedies: Vec<game_economy_dynamics::treatment::Remedy>,
    #[serde(default)]
    missions: Option<game_economy::Missions>,
    #[serde(default)]
    campaign: Option<game_economy::Campaign>,
    #[serde(default)]
    skills: Option<game_economy::Skills>,
    #[serde(default)]
    knowledge: Option<game_economy::Knowledge>,
    #[serde(default)]
    writing: Option<WritingConfig>,
    #[serde(default)]
    combat: Option<game_economy_combat::Config>,
    #[serde(default)]
    fatigue: Option<game_economy::Fatigue>,
    #[serde(default)]
    environment: Option<EnvironmentConfig>,
    #[serde(default)]
    purpose: Option<game_economy::Purpose>,
    #[serde(default)]
    recovery: Option<game_economy::RecoveryRules>,
    #[serde(default)]
    gathering: Option<GatheringConfig>,
    #[serde(default)]
    fauna: Option<game_fauna::content::Config>,
    #[serde(default)]
    animal_care: Option<game_economy::AnimalCareConfig>,
    #[serde(default)]
    animal_products: Option<game_economy::AnimalProductsConfig>,
    #[serde(default)]
    animal_handling: Option<game_economy::AnimalHandling>,
    #[serde(default)]
    animal_restraints: Option<game_economy::AnimalRestraints>,
    #[serde(default)]
    animal_feelings: Option<game_economy::AnimalFeelings>,
    #[serde(default)]
    taking: Option<game_economy_taking::content::Config>,
    #[serde(default)]
    hunting: Option<game_economy::HuntingConfig>,
    #[serde(default)]
    food_search: Option<game_economy::FoodSearch>,
    #[serde(default)]
    giving: Option<GivingConfig>,
    #[serde(default)]
    medical: Option<MedicalConfig>,
    #[serde(default)]
    hauling: Option<HaulingConfig>,
    #[serde(default)]
    local_transport: Option<LocalTransportConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MechanicsConfig {
    catalog: String,
    #[serde(default)]
    apparatus: Vec<game_mechanics::Apparatus>,
    #[serde(default)]
    materials: Vec<MechanicsMaterialConfig>,
    #[serde(default)]
    load_materials: Vec<MechanicsLoadMaterialConfig>,
    #[serde(default)]
    load_policies: Vec<MechanicsLoadPolicyConfig>,
    #[serde(default)]
    capabilities: Vec<game_economy::core_interval::MechanicalCapability>,
    #[serde(default)]
    policies: Vec<game_economy::core_interval::MechanicalPolicy>,
    #[serde(default)]
    memories: Vec<game_economy::core_interval::MechanicalMemory>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MechanicsMaterialConfig {
    component: u32,
    good: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MechanicsLoadMaterialConfig {
    good: String,
    force_per_unit: u64,
}

fn default_mechanical_load_value() -> i64 {
    1
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MechanicsLoadPolicyConfig {
    id: u64,
    person: u64,
    apparatus: u64,
    output_part: u64,
    input_force: u64,
    input_distance: u64,
    required_travel: u64,
    region: u64,
    good: String,
    quantity: u64,
    source: game_economy::LocalEndpoint,
    destination: game_economy::LocalEndpoint,
    #[serde(default)]
    item: Option<u64>,
    #[serde(default)]
    batch: Option<u64>,
    #[serde(default)]
    willing: bool,
    #[serde(default = "default_mechanical_load_value")]
    value: i64,
}

fn goal_retry_interval() -> u64 {
    3
}
fn goal_enabled() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredGoals {
    #[serde(default = "goal_retry_interval")]
    retry_interval: u64,
    #[serde(default)]
    plan_inputs: bool,
    people: Vec<AuthoredGoal>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredGoal {
    person: u64,
    recipe: String,
    priority: i32,
    target: u64,
    #[serde(default)]
    progress: u64,
    #[serde(default)]
    next_review: u64,
    #[serde(default = "goal_enabled")]
    enabled: bool,
    #[serde(default)]
    recognize_help: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredMarket {
    #[serde(default)]
    policies: Vec<AuthoredMarketPolicy>,
    #[serde(default)]
    offers: Vec<AuthoredMarketOffer>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredMarketPolicy {
    person: u64,
    #[serde(default = "goal_enabled")]
    enabled: bool,
    #[serde(default)]
    propose: bool,
    #[serde(default)]
    request_help: bool,
    #[serde(default)]
    min_gain: u64,
    #[serde(default)]
    help_value: u64,
    #[serde(default)]
    values: BTreeMap<String, u64>,
    #[serde(default)]
    influences: Vec<game_market::Influence>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredMarketOffer {
    proposer: u64,
    recipient: u64,
    #[serde(default)]
    give: BTreeMap<String, u64>,
    #[serde(default)]
    receive: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WritingConfig {
    paper: String,
    ink: String,
    page: String,
    paper_quantity: u64,
    ink_quantity: u64,
    next_item: u64,
    #[serde(default)]
    literacy: Vec<game_economy::Literacy>,
    #[serde(default)]
    pages: Vec<game_economy::WrittenPage>,
    #[serde(default)]
    reads: Vec<(u64, u64)>,
    #[serde(default)]
    authors: Vec<game_economy::WritingAuthor>,
    #[serde(default)]
    study: Vec<game_economy::WritingStudy>,
    #[serde(default)]
    practice: Vec<game_economy::WritingPractice>,
    #[serde(default)]
    text_templates: Vec<game_economy::WritingTextTemplate>,
    #[serde(default)]
    text_authors: Vec<game_economy::WritingTextAuthor>,
    #[serde(default)]
    text_retention: Vec<game_economy::TextRetention>,
    #[serde(default)]
    text_memories: Vec<game_economy::TextMemory>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalTransportConfig {
    stores: Vec<LocalStoreConfig>,
    #[serde(default)]
    policies: Vec<LocalTransportPolicyConfig>,
    #[serde(default)]
    goal_fetchers: Vec<game_economy::GoalFetchPreference>,
    #[serde(default)]
    walking: Option<game_economy::LocalWalking>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalStoreConfig {
    id: u64,
    region: u64,
    name: String,
    capacity: u64,
    #[serde(default)]
    goods: BTreeMap<String, u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalTransportPolicyConfig {
    person: u64,
    region: u64,
    source: game_economy::LocalEndpoint,
    destination: game_economy::LocalEndpoint,
    good: String,
    quantity: u64,
    #[serde(default)]
    willing: bool,
    #[serde(default)]
    pressured_by: Option<u64>,
    #[serde(default)]
    pressure_threshold: Option<u64>,
    #[serde(default)]
    disregard_claims: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConstructionConfig {
    blueprints: Vec<BuildingBlueprintConfig>,
    #[serde(default)]
    policies: Vec<game_economy::ConstructionPolicy>,
    #[serde(default)]
    demolition: Vec<game_economy::DemolitionPolicy>,
    #[serde(default)]
    goal_builders: Vec<game_economy::GoalBuilder>,
    #[serde(default)]
    salvaging: Vec<game_economy::SalvagePolicy>,
    #[serde(default)]
    maintenance: Option<MaintenanceConfig>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaintenanceConfig {
    rules: Vec<MaintenanceRuleConfig>,
    #[serde(default)]
    policies: Vec<game_economy::MaintenancePolicy>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaintenanceRuleConfig {
    kind: String,
    wear_limit: u64,
    wear_per_slot: u64,
    repair_good: String,
    repair_quantity: u64,
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
    #[serde(default)]
    salvage: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkSitesConfig {
    #[serde(default)]
    sites: Vec<game_economy::WorkSite>,
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
struct ToolWearConfig {
    rules: Vec<ToolWearRuleConfig>,
    conditions: Vec<ToolConditionConfig>,
    #[serde(default)]
    repairs: Vec<game_economy::ToolRepairChoice>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolWearRuleConfig {
    good: String,
    durability: u64,
    wear: u64,
    repair_good: String,
    repair_quantity: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolConditionConfig {
    item: u64,
    good: String,
    damage: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SharedWorkConfig {
    rules: Vec<SharedCrewConfig>,
    #[serde(default)]
    preferences: Vec<SharedPreferenceConfig>,
    #[serde(default)]
    succession: Option<game_economy::WorkSuccession>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SharedCrewConfig {
    recipe: String,
    max_helpers: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SharedPreferenceConfig {
    person: u64,
    lead: u64,
    recipe: String,
    #[serde(default)]
    willing: bool,
    #[serde(default)]
    fear_at_least: Option<u64>,
    #[serde(default)]
    obligation_at_least: Option<u64>,
    #[serde(default)]
    disregard_claims: bool,
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
    #[serde(default)]
    after_help: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkOfferValueConfig {
    person: u64,
    good: String,
    value_per_unit: i64,
    max_quantity: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkPromiseOfferConfig {
    person: u64,
    good: String,
    quantity: u64,
    due_after: u64,
    retry_ticks: u64,
    willing: bool,
    #[serde(default)]
    after_refusal: bool,
    #[serde(default)]
    minimum_trust: Option<i32>,
    #[serde(default)]
    pressure_threshold: Option<u64>,
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
    empty_backoff: Option<u64>,
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
    seasons: Vec<game_economy::Season>,
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
) -> Result<game_economy::Environment, String> {
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
            Ok(game_economy::Hazard {
                region: hazard.region,
                damage: hazard.damage,
                shelter: hazard.shelter,
                shelter_reduction: hazard.shelter_reduction,
                shelter_good,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(game_economy::Environment {
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
    policies: Vec<game_economy::GatherPolicy>,
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
    #[serde(default)]
    seasonal: bool,
    yield_per_action: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GivingConfig {
    #[serde(default)]
    practices: Option<toml::Value>,
    #[serde(default)]
    inherit_births: bool,
    #[serde(default)]
    policies: Vec<GivingPolicyConfig>,
    #[serde(default)]
    responses: Vec<FoodResponseConfig>,
    #[serde(default)]
    learning: Vec<game_economy::SharingLearning>,
    #[serde(default)]
    asking: Vec<game_economy::FoodAsking>,
    #[serde(default)]
    acquaintance_asking: Vec<game_economy::AcquaintanceAsking>,
}
fn resolve_practice_names(
    value: &mut toml::Value,
    goods: &BTreeMap<&str, u32>,
    recipes: &BTreeMap<&str, u32>,
) -> Result<(), String> {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table.iter_mut() {
                if key == "good"
                    && let Some(name) = value.as_str()
                {
                    *value = toml::Value::Integer(i64::from(
                        *goods
                            .get(name)
                            .ok_or_else(|| format!("unknown practice good {name}"))?,
                    ));
                } else if key == "recipe"
                    && let Some(name) = value.as_str()
                {
                    *value = toml::Value::Integer(i64::from(
                        *recipes
                            .get(name)
                            .ok_or_else(|| format!("unknown practice recipe {name}"))?,
                    ));
                } else {
                    resolve_practice_names(value, goods, recipes)?;
                }
            }
        }
        toml::Value::Array(rows) => {
            for row in rows {
                resolve_practice_names(row, goods, recipes)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FoodResponseConfig {
    person: u64,
    good: String,
    #[serde(default)]
    alternatives: Vec<String>,
    quantity: u64,
    recipient_hunger_at_least: u32,
    willing: bool,
    #[serde(default)]
    minimum_trust: Option<i32>,
    #[serde(default)]
    affinity_threshold: Option<i32>,
    #[serde(default)]
    affinity_at_most: Option<i32>,
    #[serde(default)]
    affinity_weight: Option<i32>,
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
    #[serde(default)]
    policies: Vec<MedicalPolicyConfig>,
    #[serde(default)]
    acquaintances: Vec<MedicalAcquaintanceConfig>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MedicalAcquaintanceConfig {
    person: u64,
    medicine: String,
    quantity: u64,
    patient_health_at_most: u32,
    willing: bool,
    #[serde(default)]
    pressure_threshold: Option<u64>,
    #[serde(default)]
    urgency_weight: i32,
    #[serde(default)]
    affinity_weight: i32,
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
    patient_health_at_most: Option<u32>,
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
    track_seed_batches: bool,
    #[serde(default)]
    seed_batches: Vec<seed_batches::SeedBatch>,
    #[serde(default)]
    seed_profiles: Vec<game_economy_agriculture::provenance::SeedProfile>,
    #[serde(default)]
    seed_genetics: Option<game_economy_agriculture::genetics::Genetics>,
    #[serde(default)]
    soil: Option<game_economy_agriculture::soil::Soil>,
    #[serde(default)]
    inherit_births: bool,
    #[serde(default)]
    requests: Option<game_economy::cultivation_requests::Requests>,
    fields: Vec<CultivatedField>,
    policies: Vec<FarmPolicy>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SoilAmendmentConfig {
    good: String,
    quantity: u64,
    fertility_gain: u64,
    choices: Vec<game_economy_agriculture::amendment::Choice>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IrrigationConfig {
    water: String,
    capacity: u64,
    minimum_growth: u64,
    evaporation: u64,
    moisture: Vec<game_economy::FieldMoisture>,
    #[serde(default)]
    choices: Vec<game_economy::WaterChoice>,
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
    #[serde(default)]
    recognize_help: bool,
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
    #[serde(default)]
    immortal: Option<game_economy::Immortal>,
    willing_to_work: bool,
    seek_meals: bool,
    #[serde(default)]
    controlled: bool,
    #[serde(default)]
    recipe: Option<String>,
    #[serde(default)]
    goods: BTreeMap<String, u64>,
    #[serde(default)]
    items: Vec<ResidentItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResidentItem {
    id: u64,
    good: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MealPreference {
    person: u64,
    good: String,
    #[serde(default = "default_food_preference_domain")]
    domain: u32,
    enabled: bool,
    bias: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LanguageTopicProfile {
    person: u64,
    #[serde(default)]
    idle: i64,
    #[serde(default)]
    act_on_tie: bool,
    #[serde(default)]
    preferences: Vec<LanguageTopic>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LanguageTopic {
    #[serde(default)]
    object: Option<String>,
    #[serde(default)]
    place: Option<u64>,
    #[serde(default)]
    person: Option<u64>,
    #[serde(default)]
    signal: Option<u32>,
    enabled: bool,
    value: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignalsConfig {
    catalog: String,
    #[serde(default)]
    birth_profile: Option<game_signals::BirthProfile>,
    #[serde(default)]
    minds: Vec<game_signals::Mind>,
    #[serde(default)]
    gifts: Vec<SignalGift>,
    #[serde(default)]
    learning: Vec<SignalLearning>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignalGift {
    person: u64,
    meaning: String,
    good: String,
    quantity: u64,
    value: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignalLearning {
    person: u64,
    good: String,
    meaning: String,
    enabled: bool,
    value: i64,
    #[serde(default)]
    replace: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkCapabilitiesConfig {
    growth: Vec<game_work_capability::GrowthPoint>,
    vigor_base: u32,
    vigor_weight: u32,
    default_strength: u32,
    #[serde(default)]
    missing_strength: Option<u32>,
    #[serde(default)]
    requirements: Vec<NamedWorkRequirement>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NamedWorkRequirement {
    recipe: String,
    strength: u32,
}

fn default_food_preference_domain() -> u32 {
    7
}

/// Construct fresh custody only for a new scenario, never for a saved world.
pub fn scenario(path: &Path, content: &Path) -> Result<Economy, String> {
    scenario_with_seed(path, content, None)
}

/// Load a fresh authored scenario into the interval host and its default schedule.
/// Unsupported owners are reported rather than silently discarded.
pub fn core_scenario(
    path: &Path,
    content: &Path,
) -> Result<game_economy::core_interval::CoreHost, String> {
    core_scenario_with_options(path, content, None, None)
}

pub fn core_scenario_with_options(
    path: &Path,
    content: &Path,
    seed: Option<u64>,
    population_path: Option<&Path>,
) -> Result<game_economy::core_interval::CoreHost, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let authored: Scenario = toml::from_str(&text).map_err(|error| error.to_string())?;
    let language = authored.language;
    let language_topic_profiles = authored.language_topic_profiles;
    let signals = authored.signals;
    let work_capabilities = authored.work_capabilities;
    let regions: HashSet<_> = authored.regions.iter().map(|region| region.id).collect();
    let mechanics = authored.mechanics;
    let mut personal_choices = authored.personal_choices;
    let birth_choices = authored.birth_choices;
    let meal_preferences = authored.meal_preferences;
    let scenario = scenario_with_options_inner(path, content, seed, population_path, true)?;
    let mut snapshot =
        game_economy::core_interval::CoreHostSnapshot::from_scenario(scenario.snapshot())?;
    if let Some(config) = work_capabilities {
        let mut requirements = config
            .requirements
            .into_iter()
            .map(|row| {
                let recipe = snapshot
                    .core
                    .physical
                    .recipes
                    .iter()
                    .find(|recipe| recipe.name == row.recipe)
                    .ok_or_else(|| format!("unknown capability recipe {}", row.recipe))?
                    .id;
                Ok(game_work_capability::Requirement {
                    recipe,
                    strength: row.strength,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        requirements.sort_unstable_by_key(|row| row.recipe);
        snapshot.work_capabilities = Some(game_work_capability::Capabilities {
            growth: config.growth,
            vigor_base: config.vigor_base,
            vigor_weight: config.vigor_weight,
            default_strength: config.default_strength,
            missing_strength: config.missing_strength,
            requirements,
        });
    }
    for preference in meal_preferences {
        if !matches!(preference.domain, 7 | 9) {
            return Err("food preference requires the meal or pickup domain".into());
        }
        let good = snapshot
            .core
            .physical
            .goods
            .iter()
            .find(|good| good.name == preference.good && good.food.is_some())
            .ok_or_else(|| format!("unknown edible good {}", preference.good))?;
        let person = personal_choices
            .iter_mut()
            .find(|person| person.id == preference.person)
            .ok_or("meal preference requires a personal choice profile")?;
        if !person
            .preferences
            .iter()
            .any(|row| row.domain == preference.domain)
        {
            return Err("food preference requires its personal domain preference".into());
        }
        person.actions.push(game_personal_choice::ActionPreference {
            key: game_personal_choice::Key {
                domain: preference.domain,
                action: u64::from(good.id),
            },
            enabled: preference.enabled,
            bias: preference.bias,
        });
    }
    for person in &mut personal_choices {
        person.actions.sort_unstable_by_key(|row| row.key);
    }
    if let Some(language) = language {
        snapshot
            .social
            .as_mut()
            .ok_or("language requires social encounters")?
            .language = Some(language);
    }
    if !language_topic_profiles.is_empty() {
        let goods: std::collections::HashMap<_, _> = snapshot
            .core
            .physical
            .goods
            .iter()
            .map(|good| (good.name.as_str(), good.id))
            .collect();
        let mut profiles = Vec::with_capacity(language_topic_profiles.len());
        let people: HashSet<_> = snapshot
            .core
            .people
            .iter()
            .map(|person| person.id)
            .collect();
        for profile in language_topic_profiles {
            let mut preferences = Vec::with_capacity(profile.preferences.len());
            for topic in profile.preferences {
                let meaning = match (
                    topic.object.as_deref(),
                    topic.place,
                    topic.person,
                    topic.signal,
                ) {
                    (Some(name), None, None, None) => game_language::Meaning::Object(
                        *goods
                            .get(name)
                            .ok_or_else(|| format!("unknown language topic object {name}"))?,
                    ),
                    (None, Some(region), None, None) if regions.contains(&region) => {
                        game_language::Meaning::Place(region)
                    }
                    (None, None, Some(person), None) if people.contains(&person) => {
                        game_language::Meaning::Person(person)
                    }
                    (None, None, None, Some(form)) if form != 0 => {
                        game_language::Meaning::Signal(form)
                    }
                    _ => {
                        return Err(
                            "language topic requires one known object, place, person or signal form".into()
                        );
                    }
                };
                preferences.push(game_language::TopicPreference {
                    meaning,
                    enabled: topic.enabled,
                    value: topic.value,
                });
            }
            preferences.sort_unstable_by_key(|row| row.meaning);
            profiles.push((
                profile.person,
                game_language::TopicPolicy {
                    idle: profile.idle,
                    act_on_tie: profile.act_on_tie,
                    preferences,
                },
            ));
        }
        snapshot
            .social
            .as_mut()
            .and_then(|social| social.language.as_mut())
            .ok_or("language topic profiles require language and social encounters")?
            .configure_topics(&profiles)
            .map_err(str::to_owned)?;
    }
    if let Some(mut signals) = signals {
        use std::path::Component;
        let path = Path::new(&signals.catalog);
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err("signals catalog must be a relative content path".into());
        }
        let source =
            std::fs::read_to_string(content.join(path)).map_err(|error| error.to_string())?;
        let mut catalog: game_signals::Catalog =
            toml::from_str(&source).map_err(|error| error.to_string())?;
        catalog.forms.sort_unstable_by_key(|row| row.id);
        catalog.meanings.sort_unstable_by_key(|row| row.id);
        signals.minds.sort_unstable_by_key(|mind| mind.person);
        for gift in signals.gifts {
            let meaning = catalog
                .meanings
                .iter()
                .find(|row| row.name == gift.meaning)
                .ok_or_else(|| format!("unknown signal meaning {}", gift.meaning))?
                .id;
            let good = snapshot
                .core
                .physical
                .goods
                .iter()
                .find(|row| row.name == gift.good)
                .ok_or_else(|| format!("unknown signal gift good {}", gift.good))?
                .id;
            let index = signals
                .minds
                .binary_search_by_key(&gift.person, |mind| mind.person)
                .map_err(|_| "signal gift requires a personal signal mind")?;
            signals.minds[index]
                .gift_preferences
                .push(game_signals::GiftPreference {
                    meaning,
                    good,
                    quantity: gift.quantity,
                    value: gift.value,
                });
        }
        for learning in signals.learning {
            let meaning = catalog
                .meanings
                .iter()
                .find(|row| row.name == learning.meaning)
                .ok_or_else(|| format!("unknown signal meaning {}", learning.meaning))?
                .id;
            let good = snapshot
                .core
                .physical
                .goods
                .iter()
                .find(|row| row.name == learning.good)
                .ok_or_else(|| format!("unknown signal learning good {}", learning.good))?
                .id;
            let index = signals
                .minds
                .binary_search_by_key(&learning.person, |mind| mind.person)
                .map_err(|_| "signal learning requires a personal signal mind")?;
            signals.minds[index]
                .learning_rules
                .push(game_signals::LearningRule {
                    good,
                    meaning,
                    enabled: learning.enabled,
                    value: learning.value,
                    replace: learning.replace,
                });
        }
        for mind in &mut signals.minds {
            mind.expressions
                .sort_unstable_by_key(|row| (row.form, row.recipient));
            mind.interpretations.sort_unstable_by_key(|row| row.form);
            mind.reactions.sort_unstable_by_key(|row| row.meaning);
            mind.learning_rules
                .sort_unstable_by_key(|row| (row.good, row.meaning));
        }
        snapshot
            .social
            .as_mut()
            .ok_or("signals require social encounters")?
            .signals = Some(game_signals::Signals {
            catalog,
            birth_profile: signals.birth_profile,
            minds: signals.minds,
            events: vec![],
            learning_events: vec![],
        });
    }
    if let Some(mut mechanics) = mechanics {
        use std::path::Component;
        let path = Path::new(&mechanics.catalog);
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err("mechanics catalog must be a relative content path".into());
        }
        let source =
            std::fs::read_to_string(content.join(path)).map_err(|error| error.to_string())?;
        let catalog: game_mechanics::Catalog =
            toml::from_str(&source).map_err(|error| error.to_string())?;
        let mut materials: Vec<_> = mechanics
            .materials
            .into_iter()
            .map(|row| {
                snapshot
                    .core
                    .physical
                    .goods
                    .iter()
                    .find(|good| good.name == row.good)
                    .map(|good| game_economy::core_interval::MechanicalMaterial {
                        component: row.component,
                        good: good.id,
                    })
                    .ok_or_else(|| format!("unknown mechanics good {}", row.good))
            })
            .collect::<Result<_, _>>()?;
        materials.sort_unstable_by_key(|row| row.component);
        let mut load_materials: Vec<_> = mechanics
            .load_materials
            .into_iter()
            .map(|row| {
                snapshot
                    .core
                    .physical
                    .goods
                    .iter()
                    .find(|good| good.name == row.good)
                    .map(|good| game_economy::core_interval::MechanicalLoadMaterial {
                        good: good.id,
                        force_per_unit: row.force_per_unit,
                    })
                    .ok_or_else(|| format!("unknown mechanical load good {}", row.good))
            })
            .collect::<Result<_, _>>()?;
        load_materials.sort_unstable_by_key(|row| row.good);
        let mut load_policies: Vec<_> = mechanics
            .load_policies
            .into_iter()
            .map(|row| {
                let good = snapshot
                    .core
                    .physical
                    .goods
                    .iter()
                    .find(|good| good.name == row.good)
                    .map(|good| good.id)
                    .ok_or_else(|| format!("unknown mechanical load good {}", row.good))?;
                let place = |endpoint| match endpoint {
                    game_economy::LocalEndpoint::Stock => game_lots::Place::Stock {
                        region: row.region,
                        good,
                    },
                    game_economy::LocalEndpoint::Store { id } => game_lots::Place::Inventory(id),
                };
                Ok(game_economy::core_interval::MechanicalLoadPolicy {
                    id: row.id,
                    value: row.value,
                    willing: row.willing,
                    request: game_economy::core_interval::MechanicalLoadRequest {
                        actor: row.person,
                        apparatus: row.apparatus,
                        output_part: row.output_part,
                        input_force: row.input_force,
                        input_distance: row.input_distance,
                        required_travel: row.required_travel,
                        movement: game_lots::Movement {
                            id: 0,
                            from: place(row.source),
                            to: Some(place(row.destination)),
                            good,
                            quantity: row.quantity,
                            item: row.item,
                            batch: row.batch,
                        },
                    },
                })
            })
            .collect::<Result<_, String>>()?;
        load_policies.sort_unstable_by_key(|row| (row.request.actor, row.id));
        mechanics
            .capabilities
            .sort_unstable_by_key(|row| row.person);
        mechanics
            .policies
            .sort_unstable_by_key(|row| (row.experiment.actor, row.id));
        mechanics.memories.sort_unstable_by_key(|row| row.person);
        mechanics.apparatus.sort_unstable_by_key(|row| row.id);
        for machine in &mut mechanics.apparatus {
            std::sync::Arc::make_mut(&mut machine.parts).sort_unstable_by_key(|part| part.id);
        }
        snapshot.mechanics = Some(game_economy::core_interval::CoreMechanics {
            world: game_mechanics::Snapshot {
                catalog,
                apparatus: mechanics.apparatus,
                revision: 0,
                last_tick: None,
            },
            materials,
            load_materials,
            load_policies,
            capabilities: mechanics.capabilities,
            policies: mechanics.policies,
            memories: mechanics.memories,
            observations: vec![],
        });
    }
    let period = snapshot.core.physical.tick_millis;
    let mut host = game_economy::core_interval::CoreHost::begin(snapshot, &[])?;
    host.enable_schedule(period, 1)?;
    if !personal_choices.is_empty() || birth_choices.is_some() {
        host.enable_personal_choices(personal_choices)?;
    }
    if let Some(profile) = birth_choices {
        host.enable_birth_choices(profile)?;
    }
    Ok(host)
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
    scenario_with_options_inner(path, content, seed, population_path, false)
}

fn scenario_with_options_inner(
    path: &Path,
    content: &Path,
    seed: Option<u64>,
    population_path: Option<&Path>,
    interval_features: bool,
) -> Result<Economy, String> {
    let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut scenario: Scenario = toml::from_str(&source).map_err(|error| error.to_string())?;
    if (scenario.language.is_some() || !scenario.language_topic_profiles.is_empty())
        && !interval_features
    {
        return Err("language requires the interval simulation host".into());
    }
    if scenario.signals.is_some() && !interval_features {
        return Err("signals require the interval simulation host".into());
    }
    if scenario.work_capabilities.is_some() && !interval_features {
        return Err("work capabilities require the interval simulation host".into());
    }
    if scenario.mechanics.is_some() && !interval_features {
        return Err("mechanics requires the interval simulation host".into());
    }
    if (!scenario.personal_choices.is_empty()
        || !scenario.meal_preferences.is_empty()
        || scenario.birth_choices.is_some())
        && !interval_features
    {
        return Err("personal choices require the interval simulation host".into());
    }
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
                    .map(|row| game_social::Need {
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
    let authored: HashSet<_> = scenario
        .aspiration_learning
        .iter()
        .map(|row| row.person)
        .collect();
    work_support
        .learning
        .retain(|row| !authored.contains(&row.person));
    scenario.aspiration_learning.extend(work_support.learning);
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
        if !dispositions.is_empty() {
            opinions.sensitivities_mut().extend(dispositions);
        }
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
    game_content_import::augment(
        &mut production,
        &content.join("objects"),
        &content.join("workshop"),
        scenario.tick_millis,
        scenario.stock_capacity,
        scenario.food_relief_divisor,
    )?;
    let mut skills = scenario.skills;
    if let Some(skills) = &mut skills {
        game_skill_catalog::configure(skills, &production.recipe_categories)?;
    }
    let goods: BTreeMap<_, _> = production
        .goods
        .iter()
        .map(|good| (good.name.as_str(), good.id))
        .collect();
    let writing = scenario
        .writing
        .map(|config| {
            let resolve = |name: &str| {
                goods
                    .get(name)
                    .copied()
                    .ok_or_else(|| format!("unknown writing good {name}"))
            };
            Ok::<_, String>(game_economy::Writing {
                paper: resolve(&config.paper)?,
                ink: resolve(&config.ink)?,
                page: resolve(&config.page)?,
                paper_quantity: config.paper_quantity,
                ink_quantity: config.ink_quantity,
                next_item: config.next_item,
                literacy: config.literacy.into(),
                pages: config.pages.into(),
                reads: config.reads.into(),
                authors: config.authors.into(),
                study: config.study.into(),
                practice: config.practice.into(),
                text_templates: config.text_templates.into(),
                text_authors: config.text_authors.into(),
                text_retention: config.text_retention.into(),
                text_memories: config.text_memories.into(),
            })
        })
        .transpose()?;
    let recipes: BTreeMap<_, _> = production
        .recipes
        .iter()
        .map(|recipe| (recipe.name.as_str(), recipe.id))
        .collect();
    if !scenario.cultivation_offer_values.is_empty() {
        let requests = scenario
            .cultivation
            .as_mut()
            .and_then(|farm| farm.requests.as_mut())
            .ok_or("cultivation offer values require cultivation requests")?;
        for value in scenario.cultivation_offer_values {
            if requests
                .offer_values
                .iter()
                .any(|row| row.person == value.person)
            {
                return Err("duplicate cultivation offer value".into());
            }
            if !requests
                .preferences
                .iter()
                .any(|row| row.person == value.person)
            {
                return Err(
                    "cultivation offer value requires a personal cultivation preference".into(),
                );
            }
            requests
                .offer_values
                .push(game_economy::cultivation_requests::OfferValue {
                    person: value.person,
                    good: *goods
                        .get(value.good.as_str())
                        .ok_or_else(|| format!("unknown cultivation offer good {}", value.good))?,
                    value_per_unit: value.value_per_unit,
                    max_quantity: value.max_quantity,
                });
        }
    }
    if let Some(config) = scenario.soil_amendment {
        let soil = scenario
            .cultivation
            .as_mut()
            .and_then(|farm| farm.soil.as_mut())
            .ok_or("soil amendment requires cultivation soil")?;
        if soil.amendment.is_some() {
            return Err("duplicate soil amendment".into());
        }
        soil.amendment = Some(game_economy_agriculture::amendment::Amendment {
            good: *goods
                .get(config.good.as_str())
                .ok_or_else(|| format!("unknown soil amendment good {}", config.good))?,
            quantity: config.quantity,
            fertility_gain: config.fertility_gain,
            choices: config.choices,
            events: vec![],
        });
    }
    if !scenario.work_offer_values.is_empty() {
        let requests = scenario
            .work_requests
            .as_mut()
            .ok_or("work offer values require work requests")?;
        let mut seen = HashSet::new();
        for value in scenario.work_offer_values {
            if !seen.insert(value.person) {
                return Err("duplicate work offer value".into());
            }
            let preference = requests
                .preferences
                .iter_mut()
                .find(|p| p.person == value.person)
                .ok_or("work offer value requires a personal work preference")?;
            preference.offer_value = Some(game_economy::WorkOfferValue {
                good: *goods
                    .get(value.good.as_str())
                    .ok_or_else(|| format!("unknown work offer good {}", value.good))?,
                value_per_unit: value.value_per_unit,
                max_quantity: value.max_quantity,
            });
        }
    }
    let shared_work = scenario
        .shared_work
        .map(|config| {
            let lookup = |name: &str| {
                recipes
                    .get(name)
                    .copied()
                    .ok_or_else(|| format!("unknown shared work recipe {name}"))
            };
            Ok::<_, String>(game_economy::SharedWork {
                rules: config
                    .rules
                    .into_iter()
                    .map(|rule| {
                        Ok(game_economy::SharedCrewRule {
                            recipe: lookup(&rule.recipe)?,
                            max_helpers: rule.max_helpers,
                        })
                    })
                    .collect::<Result<_, String>>()?,
                preferences: config
                    .preferences
                    .into_iter()
                    .map(|policy| {
                        Ok(game_economy::SharedWorkPreference {
                            person: policy.person,
                            lead: policy.lead,
                            recipe: lookup(&policy.recipe)?,
                            willing: policy.willing,
                            fear_at_least: policy.fear_at_least,
                            obligation_at_least: policy.obligation_at_least,
                            disregard_claims: policy.disregard_claims,
                        })
                    })
                    .collect::<Result<_, String>>()?,
                events: vec![],
                succession: config.succession,
            })
        })
        .transpose()?;
    let work_tools = scenario
        .work_tools
        .into_iter()
        .map(|row| {
            Ok(game_economy::WorkTool {
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
    let tool_wear = scenario
        .tool_wear
        .map(|config| {
            let resolve = |name: &str| {
                goods
                    .get(name)
                    .copied()
                    .ok_or_else(|| format!("unknown tool wear good {name}"))
            };
            let mut rules = config
                .rules
                .into_iter()
                .map(|rule| {
                    Ok(game_economy::ToolWearRule {
                        good: resolve(&rule.good)?,
                        durability: rule.durability,
                        wear: rule.wear,
                        repair_good: resolve(&rule.repair_good)?,
                        repair_quantity: rule.repair_quantity,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            rules.sort_unstable_by_key(|rule| rule.good);
            let mut conditions = config
                .conditions
                .into_iter()
                .map(|condition| {
                    Ok(game_economy::ToolCondition {
                        item: condition.item,
                        good: resolve(&condition.good)?,
                        damage: condition.damage,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            conditions.sort_unstable_by_key(|condition| condition.item);
            let mut repairs = config.repairs;
            repairs.sort_unstable_by_key(|choice| choice.person);
            Ok::<_, String>(game_economy::ToolWear {
                next_item: 1,
                rules,
                conditions,
                repairs,
                events: vec![],
            })
        })
        .transpose()?;
    let work_sites = scenario
        .work_sites
        .map(|config| {
            let requirements = config
                .requirements
                .into_iter()
                .map(|r| {
                    Ok(game_economy::WorkSiteRequirement {
                        recipe: *recipes
                            .get(r.recipe.as_str())
                            .ok_or_else(|| format!("unknown work site recipe {}", r.recipe))?,
                        kind: r.kind,
                        slots: r.slots,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok::<_, String>(game_economy::WorkSites {
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
                    Ok(game_economy::BuildingBlueprint {
                        id: b.id,
                        name: b.name,
                        kind: b.kind,
                        capacity: b.capacity,
                        kit: *goods
                            .get(b.kit.as_str())
                            .ok_or_else(|| format!("unknown construction kit {}", b.kit))?,
                        quantity: b.quantity,
                        salvage: b.salvage,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok::<_, String>(game_economy::Construction {
                blueprints,
                policies: config.policies,
                events: vec![],
                demolition: config.demolition,
                demolition_events: vec![],
                goal_builders: config.goal_builders,
                materials: vec![],
                salvaging: config.salvaging,
                salvage_events: vec![],
                maintenance: config
                    .maintenance
                    .map(|m| {
                        let rules = m
                            .rules
                            .into_iter()
                            .map(|r| {
                                Ok(game_economy::MaintenanceRule {
                                    kind: r.kind,
                                    wear_limit: r.wear_limit,
                                    wear_per_slot: r.wear_per_slot,
                                    repair_good: *goods.get(r.repair_good.as_str()).ok_or_else(
                                        || format!("unknown repair good {}", r.repair_good),
                                    )?,
                                    repair_quantity: r.repair_quantity,
                                })
                            })
                            .collect::<Result<Vec<_>, String>>()?;
                        Ok::<_, String>(game_economy::Maintenance {
                            rules,
                            conditions: vec![],
                            policies: m.policies,
                            events: vec![],
                        })
                    })
                    .transpose()?,
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
                Ok(game_economy::PromiseOfferPolicy {
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
                    after_help: p.after_help,
                    last_help_offer_at: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        owner.offering.extend(offering);
    }
    if !scenario.work_promise_offering.is_empty() {
        let owner = agreements
            .as_mut()
            .ok_or("work promise offering requires agreements")?;
        let offering = scenario
            .work_promise_offering
            .into_iter()
            .map(|p| {
                Ok(game_economy::WorkPromiseOfferPolicy {
                    person: p.person,
                    good: *goods
                        .get(p.good.as_str())
                        .ok_or_else(|| format!("unknown promise good {}", p.good))?,
                    quantity: p.quantity,
                    due_after: p.due_after,
                    retry_ticks: p.retry_ticks,
                    willing: p.willing,
                    after_refusal: p.after_refusal,
                    minimum_trust: p.minimum_trust,
                    pressure_threshold: p.pressure_threshold,
                    next_review: 0,
                    last_work_offer_at: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        owner.work_offering.extend(offering);
    }
    let dynamics = game_economy_dynamics::configure_remedies!(scenario, goods);
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
            let practices = config
                .practices
                .map(|mut value| {
                    resolve_practice_names(&mut value, &goods, &recipes)?;
                    value
                        .try_into()
                        .map_err(|error: toml::de::Error| error.to_string())
                })
                .transpose()?;
            let policies = config
                .policies
                .into_iter()
                .map(|p| {
                    Ok(game_economy::GivingPolicy {
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
                    Ok(game_economy::FoodResponse {
                        person: p.person,
                        good: *goods
                            .get(p.good.as_str())
                            .ok_or_else(|| format!("unknown giving good {}", p.good))?,
                        alternatives: p
                            .alternatives
                            .iter()
                            .map(|name| {
                                goods
                                    .get(name.as_str())
                                    .copied()
                                    .ok_or_else(|| format!("unknown giving good {name}"))
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                        quantity: p.quantity,
                        recipient_hunger_at_least: p.recipient_hunger_at_least,
                        willing: p.willing,
                        minimum_trust: p.minimum_trust,
                        affinity_threshold: p.affinity_threshold,
                        affinity_at_most: p.affinity_at_most,
                        affinity_weight: p.affinity_weight,
                        pressure_threshold: p.pressure_threshold,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(game_economy::Giving {
                practices,
                inherit_births: config.inherit_births,
                policies: policies.into(),
                responses: responses.into(),
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
                    Ok(game_economy::HaulingPolicy {
                        person: p.person,
                        source: p.source,
                        destination: p.destination,
                        good: *goods
                            .get(p.good.as_str())
                            .ok_or_else(|| format!("unknown hauling good {}", p.good))?,
                        quantity: p.quantity,
                        willing: p.willing,
                        empty_backoff: p.empty_backoff,
                        retry_at: 0,
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
                    Ok(game_economy::HaulingAsking {
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
            Ok(game_economy::Hauling {
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
                    Ok(game_economy::MedicalPolicy {
                        person: p.person,
                        patient: p.patient,
                        medicine: *goods
                            .get(p.medicine.as_str())
                            .ok_or_else(|| format!("unknown medicine {}", p.medicine))?,
                        quantity: p.quantity,
                        willing: p.willing,
                        patient_health_at_most: p.patient_health_at_most,
                        pressured_by: p.pressured_by,
                        pressure_threshold: p.pressure_threshold,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let acquaintances = config
                .acquaintances
                .into_iter()
                .map(|p| {
                    Ok(game_economy::MedicalAcquaintancePolicy {
                        person: p.person,
                        medicine: *goods
                            .get(p.medicine.as_str())
                            .ok_or_else(|| format!("unknown medicine {}", p.medicine))?,
                        quantity: p.quantity,
                        patient_health_at_most: p.patient_health_at_most,
                        willing: p.willing,
                        pressure_threshold: p.pressure_threshold,
                        urgency_weight: p.urgency_weight,
                        affinity_weight: p.affinity_weight,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(game_economy::Medical {
                policies,
                acquaintances,
            })
        })
        .transpose()?;
    let gathering = scenario
        .gathering
        .map(|config| -> Result<_, String> {
            let nodes = config
                .nodes
                .into_iter()
                .map(|node| {
                    Ok(game_economy::ResourceNode {
                        id: node.id,
                        region: node.region,
                        good: *goods
                            .get(node.good.as_str())
                            .ok_or_else(|| format!("unknown resource good {}", node.good))?,
                        available: node.available,
                        capacity: node.capacity,
                        regrowth: node.regrowth,
                        seasonal: node.seasonal,
                        yield_per_action: node.yield_per_action,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(game_economy::Gathering {
                max_hunger: config.max_hunger,
                inherit_births: config.inherit_births,
                nodes,
                policies: config.policies,
            })
        })
        .transpose()?;
    let fauna = scenario
        .fauna
        .map(|config| config.build(|name| goods.get(name).copied()))
        .transpose()?;
    let animal_care = scenario
        .animal_care
        .map(|config| config.build(|name| goods.get(name).copied()))
        .transpose()?;
    let animal_products = scenario
        .animal_products
        .map(|config| config.build(|name| goods.get(name).copied()))
        .transpose()?;
    let hunting = scenario
        .hunting
        .map(|config| config.build(|name| goods.get(name).copied()))
        .transpose()?;
    let taking = scenario
        .taking
        .map(|config| config.build(|name| goods.get(name).copied()))
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
    let authored_seed_batches = scenario
        .cultivation
        .as_ref()
        .map_or_else(Vec::new, |state| state.seed_batches.clone());
    let agriculture = scenario
        .cultivation
        .map(|state| -> Result<_, String> {
            if !state.track_seed_batches
                && (!state.seed_profiles.is_empty() || state.seed_genetics.is_some())
            {
                return Err("seed preferences and genetics require seed tracking".into());
            }
            let mut seed_profiles = state.seed_profiles;
            seed_profiles.sort_unstable_by_key(|row| row.person);
            for profile in &mut seed_profiles {
                profile.preferences.sort_unstable_by_key(|row| row.batch);
            }
            let genetics = state
                .seed_genetics
                .map(|mut genetics| {
                    std::sync::Arc::make_mut(&mut genetics.strains)
                        .sort_unstable_by_key(|row| row.batch);
                    genetics.validate().map_err(str::to_owned)?;
                    Ok::<_, String>(std::sync::Arc::new(genetics))
                })
                .transpose()?;
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
                provenance: state.track_seed_batches.then(|| {
                    game_economy_agriculture::provenance::Provenance {
                        profiles: std::sync::Arc::new(seed_profiles),
                        genetics,
                        ..Default::default()
                    }
                }),
                inherit_births: state.inherit_births,
                requests: state.requests,
                soil: state.soil,
                crops: crops.into_values().collect(),
                fields,
                policies: state.policies,
                max_hunger: state.max_hunger,
            })
        })
        .transpose()?;
    let irrigation = scenario
        .irrigation
        .map(|config| {
            let mut moisture = config.moisture;
            moisture.sort_unstable_by_key(|row| row.field);
            let mut choices = config.choices;
            choices.sort_unstable_by_key(|row| row.person);
            Ok::<_, String>(game_economy::Irrigation {
                water: *goods
                    .get(config.water.as_str())
                    .ok_or_else(|| format!("unknown water good {}", config.water))?,
                capacity: config.capacity,
                minimum_growth: config.minimum_growth,
                evaporation: config.evaporation,
                moisture,
                choices,
                events: vec![],
            })
        })
        .transpose()?;
    let mut custody = catalog.fresh_world(&production.inventories, 0)?;
    if !authored_seed_batches.is_empty() {
        let farm = agriculture
            .as_ref()
            .ok_or("seed batches require cultivation")?;
        seed_batches::assign(&mut custody, &authored_seed_batches, &farm.crops, |name| {
            goods.get(name).copied()
        })?;
    }
    let local_transport = scenario
        .local_transport
        .map(|config| {
            let stores = config
                .stores
                .into_iter()
                .map(|store| {
                    let lots = store
                        .goods
                        .into_iter()
                        .map(|(name, quantity)| {
                            let good = *goods
                                .get(name.as_str())
                                .ok_or_else(|| format!("unknown stored good {name}"))?;
                            catalog.fresh(good, quantity, 0)
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    custody.containers.push(Container::new(
                        Place::Inventory(store.id),
                        store.capacity,
                        lots,
                    )?);
                    Ok(game_economy::LocalStore {
                        id: store.id,
                        region: store.region,
                        name: store.name,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let policies = config
                .policies
                .into_iter()
                .map(|policy| {
                    Ok(game_economy::LocalTransportPolicy {
                        person: policy.person,
                        region: policy.region,
                        source: policy.source,
                        destination: policy.destination,
                        good: *goods.get(policy.good.as_str()).ok_or_else(|| {
                            format!("unknown local transport good {}", policy.good)
                        })?,
                        quantity: policy.quantity,
                        willing: policy.willing,
                        pressured_by: policy.pressured_by,
                        pressure_threshold: policy.pressure_threshold,
                        disregard_claims: policy.disregard_claims,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok::<_, String>(game_economy::LocalTransport {
                stores,
                policies,
                goal_fetchers: config.goal_fetchers,
                events: vec![],
                walking: config.walking,
            })
        })
        .transpose()?;
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
                recognize_help: dream.recognize_help,
            })
        })
        .collect::<Result<_, String>>()?;
    let has_dreams = !dreams.is_empty() || !scenario.aspiration_learning.is_empty();
    let authored_goals = scenario
        .goals
        .take()
        .map(|owner| {
            let people = owner
                .people
                .into_iter()
                .map(|goal| {
                    let recipe = *recipes
                        .get(goal.recipe.as_str())
                        .ok_or_else(|| format!("unknown recipe {}", goal.recipe))?;
                    Ok(game_economy::Goal {
                        person: goal.person,
                        recipe,
                        priority: goal.priority,
                        target: goal.target,
                        progress: goal.progress,
                        next_review: goal.next_review,
                        enabled: goal.enabled,
                        recognize_help: goal.recognize_help,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok::<_, String>(Goals {
                retry_interval: owner.retry_interval,
                plan_inputs: owner.plan_inputs,
                people,
                events: vec![],
            })
        })
        .transpose()?;
    let market = scenario
        .market
        .take()
        .map(|authored| -> Result<game_market::Market, String> {
            let resolve = |name: &str| {
                goods
                    .get(name)
                    .copied()
                    .ok_or_else(|| format!("unknown good {name}"))
            };
            let policies = authored
                .policies
                .into_iter()
                .map(|policy| -> Result<_, String> {
                    let values = policy
                        .values
                        .into_iter()
                        .map(|(name, value)| {
                            Ok(game_market::Value {
                                good: resolve(&name)?,
                                value,
                            })
                        })
                        .collect::<Result<_, String>>()?;
                    Ok(game_market::Policy {
                        person: policy.person,
                        enabled: policy.enabled,
                        propose: policy.propose,
                        request_help: policy.request_help,
                        min_gain: policy.min_gain,
                        help_value: policy.help_value,
                        values,
                        influences: policy.influences,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let proposals = authored
                .offers
                .into_iter()
                .map(|offer| -> Result<_, String> {
                    let amounts = |items: BTreeMap<String, u64>| {
                        items
                            .into_iter()
                            .map(|(name, quantity)| {
                                Ok(game_production::Amount {
                                    good: resolve(&name)?,
                                    quantity,
                                })
                            })
                            .collect::<Result<Vec<_>, String>>()
                    };
                    Ok(game_market::Proposal {
                        proposer: offer.proposer,
                        recipient: offer.recipient,
                        give: amounts(offer.give)?,
                        receive: amounts(offer.receive)?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let owner = game_market::Market {
                next_id: 1,
                policies,
                offers: vec![],
                events: vec![],
            };
            owner.propose_batch(&proposals, 0).map(|(owner, _)| owner)
        })
        .transpose()?;
    for resident in scenario.people {
        let mut lots = Vec::new();
        for (name, quantity) in resident.goods {
            let good = *goods
                .get(name.as_str())
                .ok_or_else(|| format!("unknown good {name}"))?;
            lots.push(catalog.fresh(good, quantity, 0)?);
        }
        for item in resident.items {
            let good = *goods
                .get(item.good.as_str())
                .ok_or_else(|| format!("unknown identified good {}", item.good))?;
            if catalog.fresh(good, 1, 0)?.expires_at.is_some() {
                return Err("identified items must not be perishable".into());
            }
            lots.push(game_lots::Lot {
                batch: None,
                good,
                quantity: 1,
                item: Some(item.id),
                expires_at: None,
            });
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
            hunger_remainder: 0,
            immortal: resident.immortal,
            willing_to_work: resident.willing_to_work,
            seek_meals: resident.seek_meals,
            controlled: resident.controlled,
        });
    }
    if local_transport.is_some() {
        custody.containers.par_sort_unstable_by_key(|row| row.place);
    }
    if let Some(genetics) = agriculture
        .as_ref()
        .and_then(|farm| farm.provenance.as_ref())
        .and_then(|owner| owner.genetics.as_ref())
    {
        let highest = custody
            .containers
            .par_iter()
            .flat_map_iter(|container| container.lots.iter().filter_map(|lot| lot.batch))
            .chain(genetics.strains.par_iter().map(|strain| strain.batch))
            .chain(
                genetics
                    .descendants
                    .par_iter()
                    .flat_map_iter(|chunk| chunk.iter().map(|row| row.batch)),
            )
            .max();
        custody.next_batch = Some(highest.map_or(Ok(1), |id| {
            id.checked_add(1).ok_or("batch identity overflow")
        })?);
    }
    Economy::compile(State {
        seed: scenario.seed,
        physical: Snapshot {
            custody,
            goods: production.goods,
            recipes: production.recipes,
            tick_millis: scenario.tick_millis,
            work: vec![],
            work_interval: None,
        },
        people,
        rules: scenario.rules,
        market,
        agreements,
        opinions: scenario.opinions,
        autowork: Some(Autowork {
            next_id: 1,
            policies,
            requests: scenario.work_requests,
        }),
        shared_work,
        work_tools,
        tool_wear,
        work_sites,
        construction,
        confinement: scenario.confinement,
        goals: authored_goals.or_else(|| {
            has_dreams.then_some(Goals {
                retry_interval: 3,
                plan_inputs: true,
                people: vec![],
                events: vec![],
            })
        }),
        dreams: has_dreams.then_some(Dreams {
            inherit_births: scenario.inherit_aspiration_learning,
            learning: scenario.aspiration_learning,
            people: dreams,
            events: vec![],
        }),
        nutrition: None,
        pending_food: vec![],
        travel: (!scenario.routes.is_empty()).then_some(Travel {
            closures: scenario.route_closures,
            observations: scenario.route_observations,
            observation_limit: 8,
            personal_routing: scenario.personal_route_planning,
            sharing: scenario.route_sharing,
            destinations: vec![],
            routes: scenario.routes,
            journeys: vec![],
        }),
        family: scenario.family,
        care: scenario.care,
        agriculture,
        irrigation,
        dynamics,
        missions: scenario.missions,
        campaign: scenario.campaign,
        skills,
        knowledge: scenario.knowledge,
        writing,
        combat,
        fatigue: scenario.fatigue,
        environment,
        purpose: scenario.purpose,
        recovery: scenario.recovery,
        gathering,
        fauna,
        animal_care,
        animal_products,
        animal_handling: scenario.animal_handling,
        animal_restraints: scenario.animal_restraints,
        animal_feelings: scenario.animal_feelings,
        taking,
        hunting,
        food_search: scenario.food_search,
        giving,
        medical,
        hauling,
        local_transport,
    })
}

pub fn save(world: &Economy, path: &Path) -> Result<(), String> {
    let data = serde_json::to_vec(&world.snapshot()).map_err(|error| error.to_string())?;
    game_persistence::save(path, &data)
}
pub fn load(path: &Path) -> Result<Economy, String> {
    let data = std::fs::read(path).map_err(|error| error.to_string())?;
    let state = serde_json::from_slice(&data).map_err(|error| error.to_string())?;
    Economy::compile(state)
}

#[cfg(test)]
#[path = "../tests/support/scenario.rs"]
mod tests;

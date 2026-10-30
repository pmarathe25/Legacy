use game_parallel::{FoodStoreBlueprint, Sex, World};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Path to the authored world template, relative to this settings file.
    pub template: String,
    /// Optional legacy data catalogs, resolved relative to the settings file.
    #[serde(default)]
    pub objects_catalog: Option<String>,
    #[serde(default)]
    pub recipes_catalog: Option<String>,
    #[serde(default = "default_catalog_tick")]
    pub catalog_tick_millis: u64,
    #[serde(default = "default_catalog_capacity")]
    pub catalog_stock_capacity: u64,
    #[serde(default = "default_food_divisor")]
    pub catalog_food_relief_divisor: u32,
    pub seed: u64,
    pub regions: u32,
    pub population: u32,
    pub initial_food: u32,
    pub route_ticks: u32,
    pub fields_per_region: u32,
    pub working_percent: u32,
    /// Use individual food acquisition instead of automatic regional meals.
    #[serde(default)]
    pub personal_food_access: bool,
    /// Physically distinct starting food stores in each region.
    #[serde(default)]
    pub food_stores_per_region: u32,
    #[serde(default)]
    pub food_store_users_percent: u32,
    #[serde(default)]
    pub food_depositors_percent: u32,
    /// Fraction of depositors inclined to claim a store after depositing.
    #[serde(default)]
    pub deposit_claim_percent: u32,
    /// Independent starting intentions and authored material requirements.
    #[serde(default)]
    pub store_builders_percent: u32,
    #[serde(default = "default_work_interval")]
    pub store_build_interval: u64,
    #[serde(default)]
    pub store_builder_blueprint: Option<FoodStoreBlueprint>,
    #[serde(default)]
    pub food_store_claim_percent: u32,
    #[serde(default)]
    pub food_store_objection_percent: u32,
    /// Individual inclination to confront someone after personally losing trust.
    #[serde(default)]
    pub confrontation_percent: u32,
    #[serde(default = "default_confrontation_threshold")]
    pub confrontation_threshold: i32,
    #[serde(default)]
    pub confrontation_fear_limit: u64,
    #[serde(default = "default_work_interval")]
    pub confrontation_interval: u64,
    #[serde(default = "default_confrontation_damage")]
    pub confrontation_damage: u32,
    /// Let selected caregivers use held or locally fetched food for dependents.
    #[serde(default)]
    pub dependent_food_care: bool,
    #[serde(default)]
    pub care_pressure_threshold: Option<u64>,
    #[serde(default)]
    pub food_asking_percent: u32,
    #[serde(default)]
    pub food_helpful_percent: u32,
    /// Fraction inclined to share with people they feel indebted to or fear.
    #[serde(default)]
    pub food_sharing_percent: u32,
    /// Fraction inclined to reconsider an incompatible reproductive partnership.
    #[serde(default)]
    pub reconsider_partnership_percent: u32,
    #[serde(default = "default_work_interval")]
    pub food_request_interval: u64,
    #[serde(default = "default_work_lifetime")]
    pub food_request_lifetime: u64,
    #[serde(default)]
    pub farming_percent: u32,
    #[serde(default)]
    pub reporting_percent: u32,
    #[serde(default)]
    pub social_pressure_percent: u32,
    #[serde(default)]
    pub social_fear_threshold: Option<u64>,
    #[serde(default)]
    pub social_obligation_threshold: Option<u64>,
    pub starting_logs: u64,
    #[serde(default)]
    pub trading_percent: u32,
    #[serde(default)]
    pub requesting_percent: u32,
    #[serde(default)]
    pub helpful_percent: u32,
    /// Individuals inclined to ask local people for help with their goals.
    #[serde(default)]
    pub work_requesting_percent: u32,
    /// Maximum remembered work outcomes per person.
    #[serde(default)]
    pub work_memory_limit: usize,
    /// Individuals who personally value helping with work.
    #[serde(default)]
    pub work_helpful_percent: u32,
    /// Fraction willing to use claimed inputs when responding to a work request.
    #[serde(default)]
    pub work_disregard_percent: u32,
    /// Fraction who do not personally lose trust over witnessed use of their claim.
    #[serde(default)]
    pub claim_tolerance_percent: u32,
    #[serde(default = "default_work_interval")]
    pub work_request_interval: u64,
    #[serde(default = "default_work_lifetime")]
    pub work_request_lifetime: u64,
    #[serde(default = "default_work_effort")]
    pub work_effort: u64,
    #[serde(default)]
    pub trader_items: u64,
    #[serde(default)]
    pub initial_institutions: bool,
    #[serde(default)]
    pub initial_infection: bool,
    /// Fraction of adults who may voluntarily found a local institution.
    #[serde(default)]
    pub organizing_percent: u32,
    /// Fraction of adults who seek membership in compatible local institutions.
    #[serde(default)]
    pub joining_percent: u32,
    #[serde(default = "default_founding_threshold")]
    pub founding_minimum: u32,
    #[serde(default = "default_founding_threshold")]
    pub founding_quorum: u32,
}
fn default_food_divisor() -> u32 {
    10
}
fn default_catalog_tick() -> u64 {
    600_000
}
fn default_catalog_capacity() -> u64 {
    1_000
}
fn default_confrontation_threshold() -> i32 {
    -2
}
fn default_confrontation_damage() -> u32 {
    5
}
fn default_founding_threshold() -> u32 {
    2
}
fn default_work_interval() -> u64 {
    5
}
fn default_work_lifetime() -> u64 {
    5
}
fn default_work_effort() -> u64 {
    1
}
impl Settings {
    pub fn from_toml(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|error| format!("invalid world generation settings: {error}"))
    }
    pub fn validate(&self, template: &World) -> Result<(), String> {
        if self.initial_institutions || self.organizing_percent > 0 || self.joining_percent > 0 {
            return Err("preset institutions are not supported by the parallel simulation".into());
        }
        if self.objects_catalog.is_some() != self.recipes_catalog.is_some()
            || self
                .objects_catalog
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
            || self
                .recipes_catalog
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
            || self.catalog_tick_millis == 0
            || self.catalog_stock_capacity == 0
            || self.catalog_food_relief_divisor == 0
            || self.template.trim().is_empty()
            || self.regions < 2
            || self.regions > 10_000
            || self.population < self.regions
            || self.population > 1_000_000
            || self.route_ticks == 0
            || self.fields_per_region > 1000
            || self.working_percent > 100
            || self.social_pressure_percent > 100
            || self.social_fear_threshold == Some(0)
            || self.social_obligation_threshold == Some(0)
            || (self.social_pressure_percent > 0
                && (template.social.is_none()
                    || template.opinions.is_none()
                    || (self.social_fear_threshold.is_none()
                        && self.social_obligation_threshold.is_none())))
            || (self.dependent_food_care
                && (template.caregiving.is_none()
                    || !(self.personal_food_access
                        || template
                            .cargo
                            .as_ref()
                            .is_some_and(|cargo| cargo.personal_food_access))))
            || self.care_pressure_threshold == Some(0)
            || (self.care_pressure_threshold.is_some() && template.caregiving.is_none())
            || self.food_stores_per_region > 16
            || self.food_store_users_percent > 100
            || self.food_depositors_percent > 100
            || self.deposit_claim_percent > 100
            || self.store_builders_percent > 100
            || self.store_build_interval == 0
            || (self.store_builders_percent > 0
                && (self
                    .store_builder_blueprint
                    .as_ref()
                    .is_none_or(|blueprint| {
                        blueprint.id == 0
                            || blueprint.good == 0
                            || blueprint.quantity == 0
                            || blueprint.capacity == 0
                            || template.production.as_ref().is_none_or(|production| {
                                !production
                                    .goods
                                    .iter()
                                    .any(|good| good.id == blueprint.good)
                            })
                            || template
                                .goods_cargo
                                .as_ref()
                                .is_none_or(|cargo| cargo.capacity < blueprint.quantity)
                    })
                    || template.cargo.is_none()))
            || self.food_store_claim_percent > 100
            || self.food_store_objection_percent > 100
            || self.confrontation_percent > 100
            || self.confrontation_interval == 0
            || self.confrontation_damage == 0
            || (self.confrontation_percent > 0
                && (template.tactics.is_some()
                    || template.opinions.as_ref().is_none_or(|owner| {
                        self.confrontation_threshold < owner.rules.minimum
                            || self.confrontation_threshold > owner.rules.maximum
                    })))
            || ((self.food_store_claim_percent > 0 || self.food_store_objection_percent > 0)
                && self.food_stores_per_region == 0)
            || ((self.food_store_users_percent > 0
                || self.food_depositors_percent > 0
                || self.deposit_claim_percent > 0)
                && self.food_stores_per_region == 0)
            || (self.food_stores_per_region > 0
                && !(self.personal_food_access
                    || template
                        .cargo
                        .as_ref()
                        .is_some_and(|cargo| cargo.personal_food_access)))
            || self.food_asking_percent > 100
            || self.food_helpful_percent > 100
            || self.food_sharing_percent > 100
            || self.reconsider_partnership_percent > 100
            || (self.food_sharing_percent > 0 && template.opinions.is_none())
            || self.food_request_interval == 0
            || self.food_request_lifetime == 0
            || (self.store_builders_percent > 0
                && !(self.personal_food_access
                    || template
                        .cargo
                        .as_ref()
                        .is_some_and(|cargo| cargo.personal_food_access)))
            || ((self.personal_food_access
                || self.food_asking_percent > 0
                || self.food_helpful_percent > 0
                || self.food_sharing_percent > 0)
                && template.cargo.as_ref().is_none_or(|cargo| {
                    !cargo.auto_eat_local || cargo.capacity < template.rules.food_per_meal
                }))
            || ((self.food_asking_percent > 0
                || self.food_helpful_percent > 0
                || self.food_sharing_percent > 0)
                && !(self.personal_food_access
                    || template
                        .cargo
                        .as_ref()
                        .is_some_and(|cargo| cargo.personal_food_access)))
            || self.farming_percent > 100
            || self.reporting_percent > 100
            || (self.reporting_percent > 0 && template.knowledge.is_none())
            || (self.farming_percent > 0
                && template
                    .agriculture
                    .as_ref()
                    .is_none_or(|owner| owner.volunteers.is_none()))
            || self.trading_percent > 100
            || self.requesting_percent > 100
            || self.helpful_percent > 100
            || self.work_memory_limit > 32
            || self.work_requesting_percent > 100
            || self.work_helpful_percent > 100
            || self.work_disregard_percent > 100
            || self.claim_tolerance_percent > 100
            || (self.claim_tolerance_percent > 0 && template.opinions.is_none())
            || self.work_request_interval == 0
            || self.work_request_lifetime == 0
            || ((self.work_requesting_percent > 0
                || self.work_helpful_percent > 0
                || self.work_disregard_percent > 0)
                && (template.autowork.is_none() || template.goals.is_none()))
            || self.organizing_percent > 100
            || self.joining_percent > 100
            || (self.joining_percent > 0
                && (template.institutions.is_none() || template.civic_auto.is_none()))
            || (self.organizing_percent > 0
                && (self.founding_minimum < 2
                    || self.founding_quorum == 0
                    || self.founding_quorum > self.founding_minimum
                    || template.institutions.is_none()
                    || template.civic_auto.is_none()))
            || (self.trading_percent > 0 && self.trader_items == 0)
            || self.initial_food > template.rules.food_capacity
        {
            return Err("invalid world generation bounds".into());
        }
        if template.families.as_ref().is_none_or(|family| {
            !family
                .members
                .iter()
                .any(|member| member.genome.sex == Sex::Female)
                || !family
                    .members
                    .iter()
                    .any(|member| member.genome.sex == Sex::Male)
        }) || template.production.as_ref().is_none_or(|production| {
            production.inventories.is_empty()
                || !production.recipes.iter().any(|recipe| recipe.id == 1)
                || !production.goods.iter().any(|good| good.id == 3)
        }) {
            return Err("world template lacks founding families or the configured economy".into());
        }
        if (self.trading_percent > 0 || self.requesting_percent > 0)
            && (template.market.is_none()
                || template
                    .goods_cargo
                    .as_ref()
                    .is_none_or(|cargo| cargo.capacity < self.trader_items)
                || template.production.as_ref().is_none_or(|production| {
                    ![1, 2]
                        .iter()
                        .all(|id| production.goods.iter().any(|good| good.id == *id))
                }))
        {
            return Err("world template lacks trader goods, capacity, or market".into());
        }
        Ok(())
    }
}

//! Shared physical identity and route validation for the parallel host.
use game_parallel_logistics::Shipment;
use game_parallel_people::{Region, Rules};
use game_parallel_transport::{Route, Traveler};
use rayon::prelude::*;
use std::collections::HashSet;
pub struct Context<'a> {
    pub version: u32,
    pub rules: &'a Rules,
    pub regions: &'a [Region],
    pub routes: &'a [Route],
    pub travelers: &'a [Traveler],
    pub shipments: &'a [Shipment],
    pub institutions: Option<&'a game_parallel_institutions::Institutions>,
    pub families: Option<&'a game_parallel_families::Families>,
    pub lineage: Option<&'a game_parallel_lineage::Lineage>,
    pub guardians: Option<&'a game_parallel_guardians::Guardians>,
    pub tick: u64,
}
impl Context<'_> {
    pub fn validate(&self) -> Result<(HashSet<u64>, HashSet<u64>), String> {
        if self.version != 1 {
            return Err(format!("unsupported world version {}", self.version));
        }
        self.rules.validate()?;
        let mut regions = HashSet::new();
        let mut people = HashSet::new();
        for region in self.regions {
            if !regions.insert(region.id) {
                return Err("duplicate region id".into());
            }
            if region.food > self.rules.food_capacity {
                return Err("food exceeds capacity".into());
            }
            for person in &region.people {
                if !people.insert(person.id) {
                    return Err("duplicate person id".into());
                }
                if person.health > self.rules.max_health {
                    return Err("health exceeds maximum".into());
                }
            }
        }
        let mut routes = HashSet::new();
        for route in self.routes {
            if route.ticks == 0
                || route.source == route.destination
                || !regions.contains(&route.source)
                || !regions.contains(&route.destination)
                || !routes.insert((route.source, route.destination))
            {
                return Err("invalid route".into());
            }
        }
        for traveler in self.travelers {
            if traveler.ticks_remaining == 0
                || !routes.contains(&(traveler.source, traveler.destination))
                || !people.insert(traveler.person.id)
                || traveler.person.health > self.rules.max_health
            {
                return Err("invalid traveler".into());
            }
        }
        let mut shipments = HashSet::new();
        for shipment in self.shipments {
            if !shipments.insert(shipment.id) {
                return Err("duplicate shipment id".into());
            }
            if shipment.food == 0
                || shipment.source == shipment.destination
                || !regions.contains(&shipment.source)
                || !regions.contains(&shipment.destination)
                || !routes.contains(&(shipment.source, shipment.destination))
            {
                return Err("invalid shipment".into());
            }
        }
        if let Some(institutions) = self.institutions {
            institutions.validate(&people, &regions, self.tick)?;
        }
        game_parallel_family_host::validate(
            self.families,
            self.regions,
            self.travelers,
            self.tick,
            self.rules.max_health,
        )?;
        game_parallel_lineage::Lineage::validate_optional(self.lineage, self.families, self.tick)?;
        if let Some(guardians) = self.guardians {
            guardians.validate_world(self.lineage.is_some(), self.regions, self.routes)?;
        }
        Ok((people, regions))
    }
}

/// Validate carried-food references before automatic meals in transit.
pub fn validate_auto_cargo(
    cargo: Option<&game_parallel_cargo::Cargo>,
    rules: &Rules,
    regions: &[Region],
    travelers: &[Traveler],
) -> Result<(), &'static str> {
    use rayon::prelude::*;
    let Some(cargo) = cargo.filter(|cargo| cargo.auto_eat_travel && !travelers.is_empty()) else {
        return Ok(());
    };
    rules.validate().map_err(|_| "invalid rules")?;
    let mut ids: Vec<_> = regions
        .par_iter()
        .flat_map(|region| region.people.par_iter().map(|person| person.id))
        .chain(travelers.par_iter().map(|traveler| traveler.person.id))
        .collect();
    ids.par_sort_unstable();
    if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("duplicate person id");
    }
    cargo.validate_sorted(&ids)
}

/// Validate independent physical and decision owners against shared identity sets.
pub struct Owners<'a> {
    pub agreements: Option<&'a game_parallel_agreements::Agreements>,
    pub opinions: Option<&'a game_parallel_opinions::Opinions>,
    pub disease: Option<&'a game_parallel_disease::Disease>,
    pub fatigue: Option<&'a game_parallel_fatigue::Fatigue>,
    pub skills: Option<&'a game_parallel_skills::Skills>,
    pub tactics: Option<&'a game_parallel_tactics::Tactics>,
    pub conflict: Option<&'a game_parallel_conflict::Conflict>,
    pub care: Option<&'a game_parallel_care::Care>,
    pub environment: Option<&'a game_parallel_environment::Environment>,
    pub production: Option<&'a game_parallel_production::Production>,
    pub agriculture: Option<&'a game_parallel_agriculture::Agriculture>,
    pub cargo: Option<&'a game_parallel_cargo::Cargo>,
    pub social: Option<&'a game_parallel_social::Social>,
    pub knowledge: Option<&'a game_parallel_knowledge::Knowledge>,
    pub autonomy: Option<&'a game_parallel_autonomy::Autonomy>,
    pub region_data: &'a [Region],
    pub people: &'a HashSet<u64>,
    pub regions: &'a HashSet<u64>,
    pub tick: u64,
}
impl Owners<'_> {
    pub fn validate(&self) -> Result<(), String> {
        if self.cargo.is_some_and(|cargo| {
            cargo
                .food_stores
                .par_iter()
                .any(|store| !self.regions.contains(&store.region))
        }) {
            return Err("unknown food store region".into());
        }
        if let Some(cargo) = self
            .cargo
            .filter(|cargo| !cargo.food_store_blueprints.is_empty())
        {
            let production = self
                .production
                .ok_or("food store construction requires production")?;
            if cargo.food_store_blueprints.par_iter().any(|blueprint| {
                !production
                    .goods
                    .iter()
                    .any(|good| good.id == blueprint.good)
            }) {
                return Err("unknown food store material".into());
            }
        }
        if let Some(opinions) = self.opinions {
            if opinions
                .sensitivities
                .par_iter()
                .any(|row| row.confrontation.is_some())
                && (self.conflict.is_none() || self.tactics.is_some())
            {
                return Err("personal confrontation requires non-tactical conflict".into());
            }
            opinions.validate(self.people, self.tick)?;
        }
        if let Some(agreements) = self.agreements {
            if agreements.policies.iter().any(|policy| {
                policy.minimum_trust.is_some()
                    || policy.minimum_honor_trust.is_some()
                    || policy.fear_honor_threshold.is_some()
                    || policy.obligation_honor_threshold.is_some()
                    || policy.accept_pressure_threshold.is_some()
            }) && self.opinions.is_none()
            {
                return Err("trust-based agreement policy requires opinions".into());
            }
            let production = self.production.ok_or("agreements require production")?;
            let goods = production.goods.iter().map(|good| good.id).collect();
            agreements.validate(self.people, &goods, self.tick)?;
        }
        if let Some(disease) = self.disease {
            disease.validate(self.people, self.tick)?;
            if !disease.treatments.is_empty() {
                let production = self
                    .production
                    .ok_or("disease treatment requires production")?;
                if disease.treatments.iter().any(|treatment| {
                    !production
                        .goods
                        .iter()
                        .any(|good| good.id == treatment.medicine)
                }) {
                    return Err("unknown disease treatment medicine".into());
                }
            }
        }
        if let Some(fatigue) = self.fatigue {
            fatigue.validate(self.people)?;
        }
        if let Some(skills) = self.skills {
            let production = self.production.ok_or("skills require production")?;
            let recipes = production.recipes.iter().map(|recipe| recipe.id).collect();
            skills.validate(self.people, &recipes, self.tick)?;
        }
        if let Some(tactics) = self.tactics {
            tactics.validate_world(
                self.conflict.ok_or("tactics require conflict")?,
                self.regions,
                self.tick,
            )?;
        }
        if let Some(environment) = self.environment {
            environment.validate(self.regions)?;
            for hazard in &environment.regions {
                if let Some(good) = hazard.shelter_good {
                    let production = self.production.ok_or("shelter goods require production")?;
                    if !production.goods.iter().any(|item| item.id == good)
                        || !production.inventories.iter().any(|inventory| {
                            inventory.region == hazard.region
                                && inventory.stocks.iter().any(|stock| stock.good == good)
                        })
                    {
                        return Err("missing shelter stock".into());
                    }
                }
            }
        }
        if let Some(conflict) = self.conflict {
            conflict.validate(self.people, self.regions, self.tick)?;
            let jobs = self
                .production
                .map_or(&[][..], |production| production.jobs.as_slice());
            game_parallel_conflict::validate_physical(conflict, self.region_data, jobs)?;
        }
        if let Some(care) = self.care {
            let production = self.production.ok_or("care requires production")?;
            let goods = production.goods.iter().map(|good| good.id).collect();
            care.validate(self.people, self.regions, &goods, self.tick)?;
            let actors = care.actors();
            if production.jobs.iter().any(|job| {
                job.phase == game_parallel_production::JobPhase::Working
                    && actors.binary_search(&job.worker).is_ok()
            }) || self.conflict.is_some_and(|conflict| {
                conflict
                    .actors()
                    .iter()
                    .any(|person| actors.binary_search(person).is_ok())
            }) {
                return Err("caregiver has conflicting activity".into());
            }
        }
        if let Some(owner) = self.agriculture {
            owner.validate(self.region_data)?;
            owner.validate_volunteers(self.people)?;
        }
        if let Some(owner) = self.cargo {
            owner.validate(self.people)?;
        }
        if let Some(owner) = self.social {
            owner.validate(self.people, self.tick)?;
        }
        if let Some(owner) = self.knowledge {
            owner.validate(self.people, self.regions, self.tick)?;
        }
        if let Some(owner) = self.autonomy {
            if self.knowledge.is_none() {
                return Err("autonomy requires knowledge".into());
            }
            owner.validate(self.people, self.regions, self.tick)?;
        }
        Ok(())
    }
}

/// Check the physical owners required by an editable-world tick.
pub fn validate_step(
    regions: &[Region],
    travelers: &[Traveler],
    rules: &Rules,
    cargo: Option<&game_parallel_cargo::Cargo>,
    agriculture: Option<&game_parallel_agriculture::Agriculture>,
) -> Result<(), &'static str> {
    if !game_parallel_transport::valid_destinations(regions, travelers) {
        return Err("invalid traveler destination or countdown");
    }
    if let Some(agriculture) = agriculture {
        agriculture
            .validate(regions)
            .map_err(|_| "invalid agriculture state")?;
    }
    validate_auto_cargo(cargo, rules, regions, travelers)
}

/// Validate a composed World without moving owner-specific rules into the host.
#[macro_export]
macro_rules! validate_world {
    ($world:expr) => {{
        let world = $world;
        let (people, regions) = game_parallel_validation::Context {
            version: world.version,
            rules: &world.rules,
            regions: &world.regions,
            routes: &world.routes,
            travelers: &world.travelers,
            shipments: &world.shipments,
            institutions: world.institutions.as_ref(),
            families: world.families.as_ref(),
            lineage: world.lineage.as_ref(),
            guardians: world.guardians.as_ref(),
            tick: world.tick,
        }
        .validate()?;
        game_parallel_validation::Owners {
            agreements: world.agreements.as_ref(),
            opinions: world.opinions.as_ref(),
            disease: world.disease.as_ref(),
            fatigue: world.fatigue.as_ref(),
            skills: world.skills.as_ref(),
            tactics: world.tactics.as_ref(),
            conflict: world.conflict.as_ref(),
            care: world.care.as_ref(),
            environment: world.environment.as_ref(),
            production: world.production.as_ref(),
            agriculture: world.agriculture.as_ref(),
            cargo: world.cargo.as_ref(),
            social: world.social.as_ref(),
            knowledge: world.knowledge.as_ref(),
            autonomy: world.autonomy.as_ref(),
            region_data: &world.regions,
            people: &people,
            regions: &regions,
            tick: world.tick,
        }
        .validate()?;
        if let Some(effects) = &world.food_effects {
            effects.validate(&people, world.tick)?;
            if world.nutrition.is_none()
                && effects.pending.iter().any(|row| row.nutrients != [0; 6])
            {
                return Err("nutrient effects require nutrition".into());
            }
        }
        if let Some(nutrition) = &world.nutrition {
            nutrition.validate(&people)?;
        }
        if let (Some(families), Some(production)) = (&world.families, &world.production) {
            if families.rules.min_work_age > 0 {
                let too_young: std::collections::HashSet<_> = families
                    .members
                    .iter()
                    .filter(|member| member.age < families.rules.min_work_age)
                    .map(|member| member.id)
                    .collect();
                if production.jobs.iter().any(|job| {
                    job.phase == game_parallel_production::JobPhase::Working
                        && too_young.contains(&job.worker)
                }) {
                    return Err("active worker is younger than the work age".into());
                }
            }
        }
        if world
            .property
            .as_ref()
            .is_some_and(|property| property.inheritance.is_some())
            && world.families.is_none()
        {
            return Err("property inheritance requires families".into());
        }
        if let Some(missions) = &world.missions {
            missions.validate(&people, &regions, &world.routes, world.tick)?;
            if let Some(families) = &world.families {
                let adults = families
                    .members
                    .iter()
                    .filter(|member| member.age >= families.rules.adult_age)
                    .map(|member| member.id)
                    .collect();
                missions.validate_adults(&adults)?;
            }
        }
        if let Some(personality) = &world.personality {
            personality.validate(&people)?;
        }
        if let Some(civic) = &world.civic_auto {
            world
                .institutions
                .as_ref()
                .ok_or("civic preferences require institutions")?;
            civic.validate(&people)?;
        }
        if let Some(dreams) = &world.dreams {
            let production = world
                .production
                .as_ref()
                .ok_or("aspirations require production")?;
            world.goals.as_ref().ok_or("aspirations require goals")?;
            let recipes = production.recipes.iter().map(|recipe| recipe.id).collect();
            dreams.validate(&people, &recipes, world.tick)?;
        }
        if let Some(caregiving) = &world.caregiving {
            let families = world
                .families
                .as_ref()
                .ok_or("caregiving requires families")?;
            caregiving.validate(&people, families.rules.adult_age, world.tick)?;
        }
        if let Some(purpose) = &world.purpose {
            world.goals.as_ref().ok_or("purpose requires goals")?;
            purpose.validate(&people)?;
        }
        if let Some(goals) = &world.goals {
            let production = world
                .production
                .as_ref()
                .ok_or("goals require production")?;
            world.autowork.as_ref().ok_or("goals require autowork")?;
            let recipes = production.recipes.iter().map(|recipe| recipe.id).collect();
            goals.validate(&people, &recipes, world.tick)?;
        }
        if world
            .autowork
            .as_ref()
            .and_then(|auto| auto.requests.as_ref())
            .is_some_and(|requests| {
                requests
                    .pending
                    .iter()
                    .any(|request| request.created_at > world.tick)
                    || requests.events.iter().any(|event| event.tick > world.tick)
            })
        {
            return Err("work request event is from the future".into());
        }
        if world
            .agreements
            .as_ref()
            .is_some_and(|agreements| agreements.policies.iter().any(|policy| policy.automatic()))
            && world.goods_cargo.is_none()
        {
            return Err("automatic agreements require goods cargo".into());
        }
        game_parallel_market_auto::validation::Owners {
            production: world.production.as_ref(),
            cargo: world.goods_cargo.as_ref(),
            market: world.market.as_ref(),
            autowork: world.autowork.as_ref(),
            property: world.property.as_ref(),
            regions: &world.regions,
            travelers: &world.travelers,
            tick: world.tick,
        }
        .validate()?;
        Ok(())
    }};
}
/// Add validated content and save constructors to the public world host.
#[macro_export]
macro_rules! world_codec_methods {
    () => {
        pub fn from_toml(input: &str) -> Result<Self, String> {
            let world: Self = toml::from_str(input).map_err(|e| format!("invalid world: {e}"))?;
            world.validate()?;
            Ok(world)
        }
        pub fn from_json(input: &str) -> Result<Self, String> {
            let world: Self =
                serde_json::from_str(input).map_err(|e| format!("invalid save: {e}"))?;
            world.validate()?;
            Ok(world)
        }
        pub fn validate(&self) -> Result<(), String> {
            game_parallel_validation::validate_world!(self)
        }
    };
}

/// Reject features that the editable comparison host cannot execute.
#[macro_export]
macro_rules! ensure_reference_supported {
    ($world:expr) => {{
        if $world
            .families
            .as_ref()
            .is_some_and(|families| families.rules.min_work_age > 0)
            || $world.opinions.as_ref().is_some_and(|opinions| {
                opinions.rules.minimum_report_trust.is_some()
                    || opinions.rules.fear_decay_per_tick > 0
                    || opinions
                        .sensitivities
                        .iter()
                        .any(|row| row.confrontation.is_some())
            })
            || $world
                .property
                .as_ref()
                .is_some_and(|property| property.inheritance.is_some())
            || $world
                .agriculture
                .as_ref()
                .is_some_and(|owner| owner.recruitment.is_some())
            || $world
                .food_effects
                .as_ref()
                .is_some_and(|effects| !effects.pending.is_empty())
            || $world.personality.is_some()
            || $world
                .social
                .as_ref()
                .is_some_and(|social| !social.pressures.is_empty())
            || $world.nutrition.is_some()
            || $world.missions.is_some()
            || $world.civic_auto.is_some()
            || $world.dreams.is_some()
            || $world.caregiving.is_some()
            || $world.purpose.is_some()
            || $world.goals.is_some()
            || $world.cargo.as_ref().is_some_and(|cargo| {
                cargo.auto_eat_local
                    || cargo.personal_food_access
                    || !cargo.food_aid.is_empty()
                    || !cargo.food_sharing.is_empty()
            })
            || $world.autowork.as_ref().is_some_and(|auto| {
                auto.requests.is_some() || auto.policies.iter().any(|p| p.disregard_claims)
            })
            || $world.agreements.as_ref().is_some_and(|agreements| {
                agreements.policies.iter().any(|policy| policy.automatic())
            })
            || $world.conflict.is_some()
            || $world.care.is_some()
            || $world.environment.is_some()
            || $world.skills.is_some()
            || $world.fatigue.is_some()
            || $world.tactics.is_some()
            || $world.disease.is_some()
            || game_parallel_lineage::has_physical_strain($world.lineage.as_ref())
        {
            return Err("configured owners require fused runtime");
        }
    }};
}

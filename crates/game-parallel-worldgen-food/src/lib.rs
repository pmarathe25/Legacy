//! Seed personal, local food choices without creating social authority.
use game_parallel::{
    Cargo, FoodAidPolicy, FoodAsker, FoodDepositPolicy, FoodSharingPolicy, FoodStore,
    FoodStoreBuilder, FoodStoreChoice, Region,
};
use game_parallel_genetics::random;
use game_parallel_worldgen_owners::Settings;
use rayon::prelude::*;
pub fn configure_food(
    cargo: &mut Cargo,
    settings: &Settings,
    ids: &[u64],
    controlled: Option<u64>,
    meal: u32,
    regions: &mut [Region],
) {
    cargo.holdings.clear();
    cargo.food_stores.clear();
    cargo.food_store_claims.clear();
    cargo.food_store_builders.clear();
    cargo.inherit_store_building = settings.store_builders_percent > 0;
    cargo.food_store_choices.clear();
    cargo.food_depositors.clear();
    cargo.food_requests.clear();
    cargo.food_enrollments.clear();
    cargo.next_food_request = 0;
    cargo.personal_food_access |= settings.personal_food_access;
    cargo.acquire_when_hungry = if cargo.personal_food_access {
        ids.par_iter()
            .copied()
            .filter(|&id| Some(id) != controlled)
            .collect()
    } else {
        Vec::new()
    };
    if settings.store_builders_percent > 0 {
        let blueprint = settings
            .store_builder_blueprint
            .as_ref()
            .expect("validated store blueprint");
        cargo
            .food_store_blueprints
            .retain(|row| row.id != blueprint.id);
        cargo.food_store_blueprints.push(blueprint.clone());
        cargo.food_store_builders = ids
            .par_iter()
            .copied()
            .filter(|&person| {
                Some(person) != controlled
                    && random(settings.seed ^ 0x6275696c64657273, person, 0) % 100
                        < u64::from(settings.store_builders_percent)
            })
            .map(|person| FoodStoreBuilder {
                person,
                blueprint: blueprint.id,
                remaining: 1,
                interval: settings.store_build_interval,
                object_to_taking: true,
                use_after_build: true,
                deposit_after_build: true,
                enabled: true,
            })
            .collect();
    }
    if settings.food_stores_per_region > 0 {
        let count = settings.food_stores_per_region;
        cargo.food_stores = regions
            .par_iter_mut()
            .flat_map_iter(|region| {
                let amount = region.food / 2;
                region.food -= amount;
                let id = region.id;
                (0..count).map(move |index| FoodStore {
                    id: (id - 1) * u64::from(count) + u64::from(index) + 1,
                    region: id,
                    food: amount / count + u32::from(index < amount % count),
                    capacity: settings.initial_food.max(meal),
                    claimant: {
                        let candidates =
                            (u64::from(settings.population) - id) / u64::from(settings.regions) + 1;
                        let first = id
                            + (random(settings.seed ^ 0x636c61696d73746f, id, u64::from(index))
                                % candidates)
                                * u64::from(settings.regions);
                        let chosen = if Some(first) == controlled && candidates > 1 {
                            id + ((first - id) / u64::from(settings.regions) + 1) % candidates
                                * u64::from(settings.regions)
                        } else {
                            first
                        };
                        (Some(chosen) != controlled
                            && random(settings.seed ^ 0x73746f7265636c61, id, u64::from(index))
                                % 100
                                < u64::from(settings.food_store_claim_percent))
                        .then_some(chosen)
                    },
                    object_to_taking: random(
                        settings.seed ^ 0x73746f72656f626a,
                        id,
                        u64::from(index),
                    ) % 100
                        < u64::from(settings.food_store_objection_percent),
                })
            })
            .collect();
        let source = |person: u64| {
            let region = (person - 1) % u64::from(settings.regions);
            let store = random(settings.seed ^ 0x73746f726563686f, person, 0) % u64::from(count);
            region * u64::from(count) + store + 1
        };
        cargo.food_store_choices = ids
            .par_iter()
            .copied()
            .filter(|&person| {
                Some(person) != controlled
                    && random(settings.seed ^ 0x73746f7265757365, person, 0) % 100
                        < u64::from(settings.food_store_users_percent)
            })
            .map(|person| FoodStoreChoice {
                person,
                store: source(person),
                reconsider_on_failure: true,
            })
            .collect();
        cargo.food_depositors = ids
            .par_iter()
            .copied()
            .filter(|&person| {
                Some(person) != controlled
                    && random(settings.seed ^ 0x73746f7265646570, person, 0) % 100
                        < u64::from(settings.food_depositors_percent)
            })
            .map(|person| FoodDepositPolicy {
                person,
                store: source(person),
                retain: 0,
                claim_after_deposit: (random(settings.seed ^ 0x6465706f73636c61, person, 0) % 100
                    < u64::from(settings.deposit_claim_percent))
                .then_some(true),
                interval: 5,
                enabled: true,
            })
            .collect();
    }
    let contact = |person: u64| -> Option<u64> {
        let regions = u64::from(settings.regions);
        let population = u64::from(settings.population);
        let region = (person - 1) % regions;
        let position = (person - 1) / regions;
        let count = (population - 1 - region) / regions + 1;
        (count > 1).then_some(1 + region + ((position + 1) % count) * regions)
    };
    cargo.inherit_food_sharing = settings.food_sharing_percent > 0;
    cargo.food_sharing = ids
        .par_iter()
        .copied()
        .filter(|&person| {
            Some(person) != controlled
                && random(settings.seed ^ 0x666f6f6473686172, person, 0) % 100
                    < u64::from(settings.food_sharing_percent)
        })
        .map(|person| FoodSharingPolicy {
            person,
            amount: meal,
            retain: meal,
            threshold: 1,
            interval: 1,
            enabled: true,
        })
        .collect();
    cargo.food_askers = ids
        .par_iter()
        .copied()
        .filter_map(|person| {
            let giver = contact(person)?;
            (Some(person) != controlled
                && random(settings.seed ^ 0x666f6f6461736b, person, 0) % 100
                    < u64::from(settings.food_asking_percent))
            .then_some(FoodAsker {
                person,
                giver,
                choose_known: true,
                amount: meal,
                interval: settings.food_request_interval,
                lifetime: settings.food_request_lifetime,
                enabled: true,
            })
        })
        .collect();
    cargo.food_aid = ids
        .par_iter()
        .copied()
        .filter_map(|recipient| {
            let giver = contact(recipient)?;
            (Some(giver) != controlled
                && random(settings.seed ^ 0x666f6f64616964, giver, recipient) % 100
                    < u64::from(settings.food_helpful_percent))
            .then_some(FoodAidPolicy {
                giver,
                recipient,
                amount: meal,
                retain: 0,
                help_value: 1,
                threshold: 1,
                interval: 1,
                requires_request: true,
                enabled: true,
            })
        })
        .collect();
}

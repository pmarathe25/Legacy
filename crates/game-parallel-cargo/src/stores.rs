//! Physical stores and individual source choices, independent of ownership enforcement.
use super::Cargo;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodStore {
    pub id: u64,
    pub region: u64,
    pub food: u32,
    pub capacity: u32,
    /// A personal claim; it does not physically prevent another person from taking.
    #[serde(default)]
    pub claimant: Option<u64>,
    /// The claimant personally objects to other people taking from this store.
    #[serde(default)]
    pub object_to_taking: bool,
}
/// One person's independent assertion about a store. It does not grant exclusive access.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodStoreClaim {
    pub person: u64,
    pub store: u64,
    #[serde(default)]
    pub object_to_taking: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodDepositPolicy {
    pub person: u64,
    pub store: u64,
    pub retain: u32,
    /// If set, assert a personal claim after a successful deposit; value is the initial objection.
    #[serde(default)]
    pub claim_after_deposit: Option<bool>,
    pub interval: u64,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
fn enabled() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodStoreChoice {
    pub person: u64,
    pub store: u64,
    /// Abandon this intention after a failed attempt, then reconsider the regional source.
    #[serde(default)]
    pub reconsider_on_failure: bool,
}
pub(super) fn validate<F>(cargo: &Cargo, exists: F) -> Result<(), &'static str>
where
    F: Fn(u64) -> bool + Sync,
{
    if (!cargo.food_stores.is_empty()
        || !cargo.food_store_claims.is_empty()
        || !cargo.food_store_choices.is_empty()
        || !cargo.food_depositors.is_empty()
        || !cargo.food_store_blueprints.is_empty()
        || !cargo.food_store_builders.is_empty()
        || cargo.inherit_store_building)
        && !cargo.personal_food_access
    {
        return Err("food stores require personal access");
    }
    let mut blueprints: Vec<_> = cargo
        .food_store_blueprints
        .par_iter()
        .map(|row| row.id)
        .collect();
    blueprints.par_sort_unstable();
    let mut stores: Vec<_> = cargo.food_stores.par_iter().map(|store| store.id).collect();
    stores.par_sort_unstable();
    let mut primary: Vec<_> = cargo
        .food_stores
        .par_iter()
        .map(|row| (row.id, row.claimant))
        .collect();
    primary.par_sort_unstable_by_key(|row| row.0);
    let mut claims: Vec<_> = cargo
        .food_store_claims
        .par_iter()
        .map(|row| (row.store, row.person))
        .collect();
    claims.par_sort_unstable();
    let mut choices: Vec<_> = cargo
        .food_store_choices
        .par_iter()
        .map(|choice| choice.person)
        .collect();
    choices.par_sort_unstable();
    let mut depositors: Vec<_> = cargo
        .food_depositors
        .par_iter()
        .map(|row| row.person)
        .collect();
    depositors.par_sort_unstable();
    let mut builders: Vec<_> = cargo
        .food_store_builders
        .par_iter()
        .map(|row| row.person)
        .collect();
    builders.par_sort_unstable();
    if builders.par_windows(2).any(|p| p[0] == p[1])
        || cargo.food_store_builders.par_iter().any(|row| {
            row.interval == 0
                || !exists(row.person)
                || blueprints.binary_search(&row.blueprint).is_err()
        })
        || blueprints.par_windows(2).any(|p| p[0] == p[1])
        || cargo
            .food_store_blueprints
            .par_iter()
            .any(|row| row.id == 0 || row.good == 0 || row.quantity == 0 || row.capacity == 0)
        || stores.par_windows(2).any(|p| p[0] == p[1])
        || claims.par_windows(2).any(|p| p[0] == p[1])
        || cargo.food_store_claims.par_iter().any(|claim| {
            !exists(claim.person)
                || stores.binary_search(&claim.store).is_err()
                || primary
                    .binary_search_by_key(&claim.store, |row| row.0)
                    .ok()
                    .is_some_and(|index| primary[index].1 == Some(claim.person))
        })
        || choices.par_windows(2).any(|p| p[0] == p[1])
        || depositors.par_windows(2).any(|p| p[0] == p[1])
        || cargo.food_depositors.par_iter().any(|row| {
            row.interval == 0 || !exists(row.person) || stores.binary_search(&row.store).is_err()
        })
        || cargo.food_stores.par_iter().any(|store| {
            store.id == 0
                || store.capacity == 0
                || store.food > store.capacity
                || store.claimant.is_some_and(|id| !exists(id))
        })
        || cargo
            .food_store_choices
            .par_iter()
            .any(|choice| !exists(choice.person) || stores.binary_search(&choice.store).is_err())
    {
        return Err("invalid food stores");
    }
    Ok(())
}

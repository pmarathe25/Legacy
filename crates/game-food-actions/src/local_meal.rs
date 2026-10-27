use super::*;

/// The physical stock demand of the next, local self-feeding stage.
///
/// This is an advisory description, not a reservation or permission to consume.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMeal {
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}

/// Describes an explicit meal whose next physical stage can be consumption.
///
/// Requires an idle actor at an area, a personally known local destination, an
/// actually local inventory, and an edible good. Uses the authored portion that
/// `step` passes to `ConsumeEdible`. Own carried stores follow the actor, matching
/// `step` even when their remembered area is stale.
///
/// Does not inspect stock, permissions, health or motor capacity, invoke hooks,
/// or grant knowledge. The ordinary execution request must still run all of its
/// checks and hooks; callers must recheck after any intervening world changes.
/// Unspecified meals, travel, exploration, and forage stages are not described.
pub fn local_meal(
    tx: &Transaction<'_>,
    person: PersonId,
    plan: &Plan,
) -> Result<Option<LocalMeal>> {
    let Plan::Meal {
        inventory,
        good: Some(good),
    } = *plan
    else {
        return Ok(None);
    };
    if tx.get::<Actions>()?.0.contains_key(&person) {
        return Ok(None);
    }
    let positions = tx.get::<game_transport::Positions>()?;
    let Some(game_transport::Location::At(area)) = positions.0.get(&person) else {
        return Ok(None);
    };
    let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&person) else {
        return Ok(None);
    };
    let Some(remembered) = known::<StoreLocation>(facts, &location_key(inventory)) else {
        return Ok(None);
    };
    if remembered.carrier != Some(person) && remembered.area != *area {
        return Ok(None);
    }
    let Some(store) = tx.get::<game_cargo::Cargo>()?.0.get(&inventory) else {
        return Ok(None);
    };
    if store.area(positions) != Some(*area) {
        return Ok(None);
    }
    let Some(food) = tx.get::<game_objects::Objects>()?.food_rules().get(&good) else {
        return Ok(None);
    };
    Ok(Some(LocalMeal {
        inventory,
        good,
        quantity: food.portion,
    }))
}

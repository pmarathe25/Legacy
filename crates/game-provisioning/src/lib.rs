//! Food acquisition templates use learned stock, patches, and roads.
use game_food_memory::assessment as diet;
mod reserves;
mod scoring;
use game_core::*;
use game_food_actions::next_step;
pub use game_food_actions::{FoodCare, Plan, TendFood};
pub use game_food_memory::remembered_meal;
use game_knowledge::{Facts, StoreLocation, known, location_key, with_prefix};
use game_sim::*;
pub use reserves::{Enable, Reserves, Source, Support, plugin, reserve_plan};
pub use scoring::FoodScores;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opportunity {
    pub id: u64,
    pub travel: SimDuration,
    pub risk: i64,
    pub appetite: u64,
    pub value: i64,
    pub self_supplied: bool,
    pub plan: Plan,
}
impl Opportunity {
    pub fn reasons(&self) -> Vec<String> {
        vec![
            if matches!(self.plan, Plan::Explore { .. }) {
                "search beyond familiar food sources"
            } else if matches!(self.plan, Plan::Forage { .. }) {
                "obtain own food"
            } else {
                "known meal"
            }
            .into(),
            format!("known travel {} ms", self.travel.0),
        ]
    }
}
pub fn options(tx: &Transaction<'_>, person: PersonId, meals: u64) -> Result<Vec<Opportunity>> {
    options_for(tx, person, meals, false)
}
pub fn care_options(
    tx: &Transaction<'_>,
    person: PersonId,
    meals: u64,
) -> Result<Vec<Opportunity>> {
    options_for(tx, person, meals, true)
}
fn options_for(
    tx: &Transaction<'_>,
    person: PersonId,
    meals: u64,
    partial: bool,
) -> Result<Vec<Opportunity>> {
    require(meals > 0, "gather a positive number of meals")?;
    let _measurement = game_diagnostics::span("food options");
    let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&person) else {
        return Ok(vec![]);
    };
    game_food_memory::with_stores(
        person,
        facts,
        tx.get::<game_objects::Objects>()?,
        |stores| {
            let claims = game_property::ClaimCosts::new(tx, person);
            let position = tx
                .get::<game_transport::Positions>()
                .ok()
                .and_then(|p| p.0.get(&person));
            // One immutable fact view per query: reuse routes to a destination across
            // stores and crops. Unknown or unreachable destinations remain unavailable.
            let mut costs = std::collections::BTreeMap::new();
            let mut duration = |target: AreaId| {
                let position = position?;
                if matches!(position, game_transport::Location::At(here) if *here == target) {
                    return Some(SimDuration(0));
                }
                *costs.entry(target).or_insert_with(|| {
                    next_step(tx, person, facts, position, target)
                        .ok()
                        .map(|(_, duration)| duration)
                })
            };
            let can_cache = tx.registered::<game_stockpiles::StoreSurplus>();
            let mut options = vec![];
            let mut dietary = diet::Cache::new();
            let ranks: std::collections::BTreeMap<_, _> = tx
                .get::<game_objects::Objects>()?
                .food_rules()
                .keys()
                .enumerate()
                .map(|(n, g)| (*g, n as u64))
                .collect();
            let stride = ranks.len() as u64;
            let mut sources = diet::sources(tx, person, stores);
            if partial {
                for own in stores.carried(person) {
                    if !sources.iter().any(|s| s.id == own.id) {
                        sources.push(own);
                    }
                }
            }
            for inventory in sources {
                let Some(location) = &inventory.location else {
                    continue;
                };
                let target = if location.carrier == Some(person) {
                    let Ok(area) = game_transport::area(tx.get()?, person) else {
                        continue;
                    };
                    area
                } else {
                    location.area
                };
                let Some(travel) = duration(target) else {
                    continue;
                };
                for (good, appetite, value) in diet::store_foods_partial(
                    tx,
                    person,
                    facts,
                    inventory.id,
                    &mut dietary,
                    partial,
                ) {
                    let id = inventory
                        .id
                        .0
                        .checked_mul(stride)
                        .and_then(|n| n.checked_add(ranks[&good]))
                        .and_then(|n| n.checked_mul(2))
                        .and_then(|n| n.checked_add(10))
                        .filter(|n| *n < 1 << 63)
                        .ok_or(Error::Overflow)?
                        | 1 << 63;
                    options.push(Opportunity {
                        id,
                        appetite,
                        value,
                        travel,
                        self_supplied: diet::self_supplied(
                            tx,
                            person,
                            inventory.id,
                            location.carrier,
                        ),
                        risk: claims.cost(inventory.id),
                        plan: Plan::Meal {
                            inventory: inventory.id,
                            good: Some(good),
                        },
                    });
                }
            }
            let batch = reserves::batch(tx, person, meals);
            for (key, _) in with_prefix(facts, "patch:") {
                let Some(patch) = key
                    .strip_prefix("patch:")
                    .and_then(|id| id.parse::<u64>().ok())
                else {
                    continue;
                };
                let Some(p) = known::<game_ecology::Patch>(facts, key) else {
                    continue;
                };
                let Some(food) = tx
                    .get::<game_objects::Objects>()?
                    .by_id(p.good)?
                    .food
                    .as_ref()
                else {
                    continue;
                };
                let desired = food.portion.checked_mul(batch).ok_or(Error::Overflow)?;
                let available = if partial {
                    p.stock
                } else {
                    p.stock / food.portion * food.portion
                };
                if available == 0 {
                    continue;
                }
                let Some(travel) = duration(p.area) else {
                    continue;
                };
                let portion_mass = tx
                    .get::<game_objects::Objects>()?
                    .mass(p.good, food.portion)?;
                // A desired reserve is a target. A smaller feasible harvest remains an option.
                let fits = |store: &game_food_memory::Store| {
                    let l = store.location.as_ref()?;
                    if l.carrier != Some(person) && l.area != p.area {
                        return None;
                    }
                    let free = if l.carrier == Some(person) && can_cache {
                        l.capacity
                    } else {
                        l.capacity.saturating_sub(l.mass)
                    };
                    let capacity = if partial {
                        free / tx
                            .get::<game_objects::Objects>()
                            .ok()?
                            .mass(p.good, 1)
                            .ok()?
                    } else {
                        free.checked_div(portion_mass)
                            .map_or(desired, |portions| portions.saturating_mul(food.portion))
                    };
                    let quantity = desired.min(available).min(capacity);
                    (quantity >= if partial { 1 } else { food.portion })
                        .then_some((store.id, quantity))
                };
                let chosen = stores
                    .carried(person)
                    .filter_map(fits)
                    .min_by_key(|(id, _)| *id)
                    .or_else(|| stores.all().filter_map(fits).min_by_key(|(id, _)| *id));
                let Some((inventory, quantity)) = chosen else {
                    continue;
                };
                let (appetite, value) = diet::food(tx, person, p.good, food, &mut dietary);
                let carrier =
                    known::<StoreLocation>(facts, &location_key(inventory)).and_then(|l| l.carrier);
                options.push(Opportunity {
                    appetite,
                    value,
                    self_supplied: diet::self_supplied(tx, person, inventory, carrier),
                    id: patch
                        .checked_mul(2)
                        .and_then(|n| n.checked_add(11))
                        .ok_or(Error::Overflow)?,
                    travel: SimDuration(
                        travel
                            .0
                            .saturating_add(reserves::reliability(tx, person, patch)),
                    ),
                    risk: claims.cost(inventory),
                    plan: Plan::Forage {
                        patch,
                        inventory,
                        quantity,
                        gathering: false,
                    },
                });
            }
            if options.is_empty() {
                for area in frontiers(facts) {
                    if let Some(travel) = duration(area) {
                        options.push(Opportunity {
                            appetite: 0,
                            value: 0,
                            self_supplied: true,
                            id: area
                                .0
                                .checked_mul(2)
                                .and_then(|n| n.checked_add(10))
                                .ok_or(Error::Overflow)?,
                            travel,
                            risk: 0,
                            plan: Plan::Explore { area },
                        });
                    }
                }
            }
            Ok(options)
        },
    )
}
/// Endpoints of remembered roads whose outgoing surroundings remain unknown.
pub fn frontiers(facts: &Facts) -> Vec<AreaId> {
    let mut areas = std::collections::BTreeSet::new();
    for (_, fact) in with_prefix(facts, "roads:") {
        if let Some(area) = fact.value.decode::<game_land::Area>() {
            for next in area.links.keys() {
                if !facts.contains_key(game_knowledge::roads_key(*next).as_str()) {
                    areas.insert(*next);
                }
            }
        }
    }
    areas.into_iter().collect()
}
pub fn step(tx: &mut Transaction<'_>, person: PersonId, plan: &mut Plan) -> Result<bool> {
    let done = game_food_actions::step(tx, person, plan)?;
    if done {
        reserves::experience(tx, person, plan, true)?;
    }
    Ok(done)
}
pub fn acquire(tx: &mut Transaction<'_>, person: PersonId, plan: &mut Plan) -> Result<bool> {
    let done = game_food_actions::acquire(tx, person, plan)?;
    if done {
        reserves::experience(tx, person, plan, true)?;
    }
    Ok(done)
}
pub fn learn_failure(tx: &mut Transaction<'_>, person: PersonId, plan: &Plan) {
    let _ = reserves::experience(tx, person, plan, false);
    let inventory = match *plan {
        Plan::Meal { inventory, .. } | Plan::Forage { inventory, .. } => inventory,
        Plan::Explore { .. } => {
            let _ = tx.attempt(game_knowledge::ObserveSurroundings { person });
            return;
        }
    };
    let _ = tx.attempt(game_knowledge::ObserveStore { person, inventory });
    if let Plan::Forage { patch, .. } = *plan {
        let _ = tx.attempt(game_knowledge::ObservePatch { person, patch });
    }
}

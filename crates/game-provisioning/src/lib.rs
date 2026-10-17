//! Food acquisition templates use learned stock, patches, and roads.
use game_food_memory::assessment as diet;
mod reserves;
use game_core::*;
pub use game_food_memory::remembered_meal;
use game_knowledge::{Facts, StoreLocation, known, location_key, patch_key, with_prefix};
use game_sim::*;
pub use reserves::{Enable, Reserves, Source, Support, plugin, reserve_plan};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Plan {
    Explore {
        area: AreaId,
    },
    Meal {
        inventory: InventoryId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        good: Option<GoodId>,
    },
    Forage {
        patch: u64,
        inventory: InventoryId,
        quantity: u64,
        gathering: bool,
    },
}
fn location(
    tx: &Transaction<'_>,
    person: PersonId,
    facts: &Facts,
    inventory: InventoryId,
) -> Result<AreaId> {
    let location: StoreLocation = known(facts, &location_key(inventory))
        .ok_or_else(|| Error::Missing("known inventory location".into()))?;
    if location.carrier == Some(person) {
        game_transport::area(tx.get()?, person)
    } else {
        Ok(location.area)
    }
}
fn travel(
    tx: &Transaction<'_>,
    person: PersonId,
    facts: &Facts,
    target: AreaId,
) -> Result<(Option<AreaId>, SimDuration)> {
    let location = tx
        .get::<game_transport::Positions>()?
        .0
        .get(&person)
        .ok_or_else(|| Error::Missing("position".into()))?;
    next_step(tx, person, facts, location, target)
}
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
            for inventory in diet::sources(tx, person, stores) {
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
                for (good, appetite, value) in
                    diet::store_foods(tx, person, facts, inventory.id, &mut dietary)
                {
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
                let available = p.stock / food.portion * food.portion;
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
                    let capacity = free
                        .checked_div(portion_mass)
                        .map_or(desired, |portions| portions.saturating_mul(food.portion));
                    let quantity = desired.min(available).min(capacity);
                    (quantity >= food.portion).then_some((store.id, quantity))
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
    let facts = tx
        .get::<game_knowledge::Knowledge>()?
        .0
        .get(&person)
        .ok_or_else(|| Error::Missing("personal knowledge".into()))?;
    if let Plan::Explore { area } = *plan {
        if let (Some(next), _) = travel(tx, person, facts, area)? {
            tx.attempt(game_transport::Move {
                person,
                destination: next,
            })?;
            return Ok(false);
        }
        tx.call(game_knowledge::ObserveSurroundings { person })?;
        return Ok(true);
    }
    let inventory = match *plan {
        Plan::Meal { inventory, .. } | Plan::Forage { inventory, .. } => inventory,
        Plan::Explore { .. } => unreachable!(),
    };
    let (target, chosen) = match *plan {
        Plan::Meal { good, .. } => (location(tx, person, facts, inventory)?, good),
        Plan::Forage { patch, .. } => {
            let patch = known::<game_ecology::Patch>(facts, &patch_key(patch))
                .ok_or_else(|| Error::Missing("known patch".into()))?;
            (patch.area, Some(patch.good))
        }
        Plan::Explore { .. } => unreachable!(),
    };
    if let (Some(next), _) = travel(tx, person, facts, target)? {
        tx.attempt(game_transport::Move {
            person,
            destination: next,
        })?;
        return Ok(false);
    }
    if let Plan::Forage {
        patch,
        quantity,
        gathering,
        ..
    } = plan
        && !*gathering
    {
        let good = tx.get::<game_ecology::Ecology>()?.patches[patch].good;
        let needed = tx.get::<game_objects::Objects>()?.mass(good, *quantity)?;
        let held = &tx.get::<game_cargo::Cargo>()?.0[&inventory];
        if held.capacity.saturating_sub(held.used()?) < needed
            && tx.registered::<game_stockpiles::StoreSurplus>()
        {
            tx.attempt(game_stockpiles::StoreSurplus {
                person,
                inventory,
                needed_mass: needed,
            })?;
        }
        tx.attempt(game_ecology::Gather {
            person,
            patch: *patch,
            quantity: *quantity,
            destination: inventory,
        })?;
        *gathering = true;
        return Ok(false);
    }
    if let Some(good) = chosen {
        let quantity = tx
            .get::<game_objects::Objects>()?
            .by_id(good)?
            .food
            .as_ref()
            .ok_or_else(|| Error::Rejected("chosen good is not food".into()))?
            .portion;
        tx.attempt(game_physiology::ConsumeEdible {
            person,
            inventory,
            good,
            quantity,
        })?;
    } else {
        tx.attempt(game_physiology::Eat { person, inventory })?;
    }
    reserves::experience(tx, person, plan, true)?;
    tx.call(game_knowledge::ObserveStore { person, inventory })?;
    Ok(true)
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
/// Choose the next known road, including resuming an interrupted journey.
fn next_step(
    tx: &Transaction<'_>,
    person: PersonId,
    facts: &Facts,
    location: &game_transport::Location,
    target: AreaId,
) -> Result<(Option<AreaId>, SimDuration)> {
    use game_transport::Location;
    match location {
        Location::At(area) if *area == target => Ok((None, SimDuration(0))),
        Location::At(area) => {
            let route = game_knowledge::route(facts, *area, target)?;
            Ok((
                route.areas.get(1).copied(),
                game_transport::travel_time(tx, person, route.duration),
            ))
        }
        Location::Road {
            from,
            to,
            elapsed,
            duration,
        } => [
            (*from, elapsed.0),
            (
                *to,
                duration
                    .0
                    .checked_sub(elapsed.0)
                    .ok_or_else(|| Error::Rejected("invalid road progress".into()))?,
            ),
        ]
        .into_iter()
        .filter_map(|(area, offset)| {
            let route = game_knowledge::route(facts, area, target).ok()?;
            Some((
                Some(area),
                SimDuration(
                    offset
                        .checked_add(game_transport::travel_time(tx, person, route.duration).0)?,
                ),
            ))
        })
        .min_by_key(|(area, cost)| (cost.0, *area))
        .ok_or_else(|| Error::Rejected("no known route from road".into())),
        _ => Err(Error::Rejected("already travelling".into())),
    }
}

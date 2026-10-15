//! Food acquisition templates use learned stock, patches, and roads.
use game_core::*;
use game_knowledge::{
    Facts, StoreLocation, known, location_key, patch_key, store_key, with_prefix,
};
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Plan {
    Meal {
        inventory: InventoryId,
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
    next_step(facts, location, target)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opportunity {
    pub id: u64,
    pub travel: SimDuration,
    pub plan: Plan,
}
impl Opportunity {
    pub fn reasons(&self) -> Vec<String> {
        vec![
            if matches!(self.plan, Plan::Forage { .. }) {
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
    let empty = Facts::default();
    let facts = tx
        .get::<game_knowledge::Knowledge>()?
        .0
        .get(&person)
        .unwrap_or(&empty);
    let inventories: Vec<_> = with_prefix(facts, "store:")
        .filter_map(|(key, _)| {
            key.strip_prefix("store:")?
                .parse::<u64>()
                .ok()
                .map(InventoryId)
        })
        .collect();
    let mut options = vec![];
    for inventory in &inventories {
        let stock = known::<game_knowledge::Stock>(facts, &store_key(*inventory));
        if !stock.is_some_and(|s| {
            tx.get::<game_objects::Objects>()
                .is_ok_and(|o| o.meal(&s).is_some())
        }) {
            continue;
        }
        let Ok((_, duration)) =
            location(tx, person, facts, *inventory).and_then(|a| travel(tx, person, facts, a))
        else {
            continue;
        };
        options.push(Opportunity {
            id: inventory
                .0
                .checked_mul(2)
                .and_then(|n| n.checked_add(10))
                .ok_or(Error::Overflow)?,
            travel: duration,
            plan: Plan::Meal {
                inventory: *inventory,
            },
        });
    }
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
        let quantity = food.portion.checked_mul(meals).ok_or(Error::Overflow)?;
        if p.stock < quantity {
            continue;
        }
        let Ok((_, duration)) = travel(tx, person, facts, p.area) else {
            continue;
        };
        let mass = tx.get::<game_objects::Objects>()?.mass(p.good, quantity)?;
        // Prefer one's own carried container, then a known container at the patch.
        let inventory = inventories
            .iter()
            .filter_map(|id| {
                let l: StoreLocation = known(facts, &location_key(*id))?;
                ((l.carrier == Some(person) || l.area == p.area)
                    && l.mass
                        .checked_add(mass)
                        .is_some_and(|used| used <= l.capacity))
                .then_some((l.carrier != Some(person), *id))
            })
            .min()
            .map(|(_, id)| id);
        let Some(inventory) = inventory else { continue };
        options.push(Opportunity {
            id: patch
                .checked_mul(2)
                .and_then(|n| n.checked_add(11))
                .ok_or(Error::Overflow)?,
            travel: duration,
            plan: Plan::Forage {
                patch,
                inventory,
                quantity,
                gathering: false,
            },
        });
    }
    Ok(options)
}
pub fn step(tx: &mut Transaction<'_>, person: PersonId, plan: &mut Plan) -> Result<bool> {
    let facts = tx
        .get::<game_knowledge::Knowledge>()?
        .0
        .get(&person)
        .ok_or_else(|| Error::Missing("personal knowledge".into()))?;
    let inventory = match *plan {
        Plan::Meal { inventory } | Plan::Forage { inventory, .. } => inventory,
    };
    let target = match *plan {
        Plan::Meal { .. } => location(tx, person, facts, inventory)?,
        Plan::Forage { patch, .. } => {
            known::<game_ecology::Patch>(facts, &patch_key(patch))
                .ok_or_else(|| Error::Missing("known patch".into()))?
                .area
        }
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
        tx.attempt(game_ecology::Gather {
            person,
            patch: *patch,
            quantity: *quantity,
            destination: inventory,
        })?;
        *gathering = true;
        return Ok(false);
    }
    tx.attempt(game_physiology::Eat { person, inventory })?;
    tx.call(game_knowledge::ObserveStore { person, inventory })?;
    Ok(true)
}
pub fn learn_failure(tx: &mut Transaction<'_>, person: PersonId, plan: &Plan) {
    let inventory = match *plan {
        Plan::Meal { inventory } | Plan::Forage { inventory, .. } => inventory,
    };
    let _ = tx.attempt(game_knowledge::ObserveStore { person, inventory });
    if let Plan::Forage { patch, .. } = *plan {
        let _ = tx.attempt(game_knowledge::ObservePatch { person, patch });
    }
}

/// Choose the next known road, including resuming an interrupted journey.
fn next_step(
    facts: &Facts,
    location: &game_transport::Location,
    target: AreaId,
) -> Result<(Option<AreaId>, SimDuration)> {
    use game_transport::Location;
    match location {
        Location::At(area) => {
            let route = game_knowledge::route(facts, *area, target)?;
            Ok((route.areas.get(1).copied(), route.duration))
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
                SimDuration(offset.checked_add(route.duration.0)?),
            ))
        })
        .min_by_key(|(area, cost)| (cost.0, *area))
        .ok_or_else(|| Error::Rejected("no known route from road".into())),
        _ => Err(Error::Rejected("already travelling".into())),
    }
}

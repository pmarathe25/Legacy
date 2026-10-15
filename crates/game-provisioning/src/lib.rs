//! Food acquisition templates use learned stock, patches, and roads.
use game_core::*;
use game_knowledge::{Fact, StoreLocation, known, location_key, patch_key, store_key};
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    facts: &BTreeMap<String, Fact>,
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
    facts: &BTreeMap<String, Fact>,
    target: AreaId,
) -> Result<(Option<AreaId>, SimDuration)> {
    let location = tx
        .get::<game_transport::Positions>()?
        .0
        .get(&person)
        .ok_or_else(|| Error::Missing("position".into()))?;
    game_knowledge::next_step(facts, location, target)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opportunity {
    pub id: u64,
    pub travel: SimDuration,
    pub plan: Plan,
}
pub fn options(tx: &Transaction<'_>, person: PersonId, meals: u64) -> Result<Vec<Opportunity>> {
    require(meals > 0, "gather a positive number of meals")?;
    let empty = BTreeMap::new();
    let facts = tx
        .get::<game_knowledge::Knowledge>()?
        .0
        .get(&person)
        .unwrap_or(&empty);
    let inventories: Vec<_> = facts
        .keys()
        .filter_map(|key| {
            key.strip_prefix("store:")?
                .parse::<u64>()
                .ok()
                .map(InventoryId)
        })
        .collect();
    let mut options = vec![];
    for inventory in &inventories {
        let stock = known::<BTreeMap<GoodId, u64>>(facts, &store_key(*inventory));
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
    for key in facts.keys() {
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
        .cloned()
        .unwrap_or_default();
    let inventory = match *plan {
        Plan::Meal { inventory } | Plan::Forage { inventory, .. } => inventory,
    };
    let target = match *plan {
        Plan::Meal { .. } => location(tx, person, &facts, inventory)?,
        Plan::Forage { patch, .. } => {
            known::<game_ecology::Patch>(&facts, &patch_key(patch))
                .ok_or_else(|| Error::Missing("known patch".into()))?
                .area
        }
    };
    if let (Some(next), _) = travel(tx, person, &facts, target)? {
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

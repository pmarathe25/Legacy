//! A finite, personally owned ground cache frees carried capacity without deleting goods.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stockpiles {
    pub stores: Table<PersonId, Table<AreaId, InventoryId>>,
    pub stored_mass: u64,
}
impl Resource for Stockpiles {
    const KEY: &'static str = "stockpiles.caches";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct StoreSurplus {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub needed_mass: u64,
}
impl Request for StoreSurplus {
    const KEY: &'static str = "stockpiles.store-surplus";
    type Response = bool;
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stored {
    pub person: PersonId,
    pub area: AreaId,
    pub source: InventoryId,
    pub destination: InventoryId,
    pub mass: u64,
}
impl Event for Stored {
    const KEY: &'static str = "stockpiles.stored";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("stockpiles", 1)?;
    b.resource::<Stockpiles>()?;
    b.request::<StoreSurplus>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let area = game_transport::area(tx.get()?, r.person)?;
        let source = tx
            .get::<game_cargo::Cargo>()?
            .0
            .get(&r.inventory)
            .ok_or_else(|| Error::Missing("carried stock".into()))?;
        require(
            matches!(source.custody,game_cargo::Custody::Carried(p) if p==r.person),
            "caching requires physical carried custody",
        )?;
        if source.capacity.saturating_sub(source.used()?) >= r.needed_mass {
            return Ok(false);
        }
        require(
            r.needed_mass <= source.capacity,
            "load exceeds carried capacity",
        )?;
        let required = r.needed_mass - (source.capacity - source.used()?);
        let capacity = source.capacity.checked_mul(4).ok_or(Error::Overflow)?;
        let objects = tx.get::<game_objects::Objects>()?;
        let mut goods: Vec<_> = source
            .goods
            .iter()
            .filter_map(|(id, n)| {
                let object = objects.by_id(*id).ok()?;
                // Preserve eight carried meals of each food; leave the person provisions.
                let retain = object
                    .food
                    .as_ref()
                    .map_or(0, |f| f.portion.saturating_mul(8));
                let movable = n.saturating_sub(retain);
                (movable > 0).then_some((
                    object.food.is_some(),
                    std::cmp::Reverse(object.mass.saturating_mul(movable)),
                    *id,
                    movable,
                    object.mass,
                ))
            })
            .collect();
        goods.sort();
        let existing = tx
            .get::<Stockpiles>()?
            .stores
            .get(&r.person)
            .and_then(|s| s.get(&area))
            .copied();
        let room = existing
            .map(|id| {
                tx.get::<game_cargo::Cargo>().unwrap().0[&id].capacity
                    - tx.get::<game_cargo::Cargo>().unwrap().0[&id]
                        .used()
                        .unwrap()
            })
            .unwrap_or(capacity);
        let mut remaining = required;
        let mut free = room;
        let mut moves = vec![];
        for (_, _, good, available, mass) in goods {
            let quantity = available.min(remaining.div_ceil(mass)).min(free / mass);
            if quantity == 0 {
                continue;
            }
            let moved = quantity.checked_mul(mass).ok_or(Error::Overflow)?;
            moves.push((good, quantity));
            remaining = remaining.saturating_sub(moved);
            free -= moved;
            if remaining == 0 {
                break;
            }
        }
        require(remaining == 0, "no room in a finite local cache")?;
        let destination = if let Some(id) = existing {
            tx.call(game_property::Use {
                actor: r.person,
                inventory: id,
                purpose: "caching".into(),
            })?;
            id
        } else {
            let id = tx.call(game_cargo::Create {
                custody: game_cargo::Custody::Area(area),
                capacity,
            })?;
            tx.call(game_property::ClaimInventory {
                claimant: r.person,
                inventory: id,
                recognized_by: [r.person].into(),
            })?;
            tx.edit::<Stockpiles>()?
                .stores
                .entry(r.person)
                .or_default()
                .insert(area, id);
            id
        };
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.inventory,
            purpose: "caching".into(),
        })?;
        let mut total = 0;
        for (good, quantity) in moves {
            total += tx.get::<game_objects::Objects>()?.mass(good, quantity)?;
            tx.call(game_cargo::Transfer {
                from: r.inventory,
                to: destination,
                good,
                quantity,
            })?;
        }
        tx.call(game_knowledge::ObserveStore {
            person: r.person,
            inventory: r.inventory,
        })?;
        tx.call(game_knowledge::ObserveStore {
            person: r.person,
            inventory: destination,
        })?;
        let old = tx.get::<Stockpiles>()?.stored_mass;
        tx.edit::<Stockpiles>()?.stored_mass = old.checked_add(total).ok_or(Error::Overflow)?;
        tx.emit(
            Stored {
                person: r.person,
                area,
                source: r.inventory,
                destination,
                mass: total,
            },
            vec![r.person],
        )?;
        Ok(true)
    })?;
    b.validate_save(|h| {
        for (person, stores) in &h.get::<Stockpiles>()?.stores {
            for (area, id) in stores {
                require(
                    h.get::<game_cargo::Cargo>()?
                        .0
                        .get(id)
                        .is_some_and(|i| i.custody == game_cargo::Custody::Area(*area))
                        && h.get::<game_property::Property>()?
                            .claims
                            .get(id)
                            .is_some_and(|c| c.iter().any(|c| c.claimant == *person)),
                    "invalid ground cache",
                )?;
            }
        }
        Ok(())
    });
    Ok(())
}

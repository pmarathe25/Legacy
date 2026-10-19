use super::*;
use game_equipment::{Equip, Equipment, Remove, Repair, condition, insulation, protection};
pub(super) fn options(tx: &Transaction<'_>, p: PersonId) -> Result<Vec<Proposal>> {
    let mut offers = vec![];
    if !tx.get::<Equipment>()?.enabled {
        return Ok(offers);
    }
    let Ok(area) = game_transport::area(tx.get()?, p) else {
        return Ok(offers);
    };
    let warmth = game_environment::at(tx, area)?.warmth;
    let hardy = game_vitality::ability(tx, p, "hardiness").min(16000) as i32 / 200;
    let discomfort = |insulation: i32| {
        i64::from((10 - hardy - warmth - insulation).max(0)).pow(2)
            + i64::from((warmth + insulation - 28 - hardy).max(0)).pow(2)
    };
    let present = protection(tx, p);
    let old = discomfort(present);
    if tx
        .get::<Equipment>()?
        .worn
        .get(&p)
        .is_some_and(|w| w.equipped)
    {
        offers.push(Proposal::new(
            "clothing:remove",
            motive(old - discomfort(0), 0, 0, 0, 0),
            Remove { person: p },
        )?);
    }
    for inventory in tx.get::<game_cargo::Cargo>()?.carried(p) {
        for (good, q) in &tx.get::<game_cargo::Cargo>()?.0[&inventory].goods {
            if *q == 0 {
                continue;
            }
            let object = tx.get::<game_objects::Objects>()?.by_id(*good)?;
            let warm = insulation(object);
            if warm > 0
                && tx
                    .get::<Equipment>()?
                    .worn
                    .get(&p)
                    .is_none_or(|w| !w.equipped || w.good != *good || w.inventory != inventory)
            {
                let remaining = tx
                    .get::<Equipment>()?
                    .conditions
                    .get(&p)
                    .and_then(|r| r.get(good))
                    .copied()
                    .unwrap_or(100);
                let comfort = discomfort(warm * remaining as i32 / 100);
                offers.push(Proposal::new(
                    format!("clothing:wear:{}:{}", good.0, inventory.0),
                    motive(
                        old - comfort,
                        0,
                        0,
                        1,
                        game_property::reluctance(tx, p, inventory),
                    ),
                    Equip {
                        person: p,
                        inventory,
                        good: *good,
                    },
                )?);
            }
            if object.tags.contains("repair-cloth")
                && let Some(w) = tx.get::<Equipment>()?.worn.get(&p)
            {
                if w.inventory != inventory {
                    continue;
                }
                let worn = condition(tx, p, w);
                if worn < 100 {
                    let full = insulation(tx.get::<game_objects::Objects>()?.by_id(w.good)?);
                    let benefit = old - discomfort(full);
                    offers.push(Proposal::new(
                        format!("clothing:repair:{}:{}", good.0, inventory.0),
                        motive(benefit, (100 - worn) as i64 / 10, 0, 2, 0),
                        Repair {
                            person: p,
                            inventory: w.inventory,
                            material: *good,
                        },
                    )?);
                }
            }
        }
    }
    Ok(offers)
}

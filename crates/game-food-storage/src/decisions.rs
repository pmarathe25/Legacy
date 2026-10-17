use super::*;
pub(super) fn consider(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if !tx.get::<Stores>()?.enabled
        || Some(person) == tx.get::<game_lineage::Lineage>()?.current
        || tx.get::<Actions>()?.0.contains_key(&person)
    {
        return Ok(());
    }
    let Ok(area) = game_transport::area(tx.get()?, person) else {
        return Ok(());
    };
    let sites: Vec<_> = tx
        .get::<Stores>()?
        .sites
        .iter()
        .filter(|(_, s)| s.owner == person && s.area == area)
        .map(|(id, _)| *id)
        .collect();
    let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(person).collect();
    if sites.is_empty() {
        let foresight = game_vitality::ability(tx, person, "foresight");
        for supplies in packs {
            let kit = tx.get::<game_cargo::Cargo>()?.0[&supplies]
                .goods
                .iter()
                .filter(|(_, q)| **q > 0)
                .find(|(g, _)| {
                    tx.get::<game_objects::Objects>()
                        .unwrap()
                        .by_id(**g)
                        .is_ok_and(|o| o.tags.iter().any(|t| t.starts_with("storage:")))
                })
                .map(|(g, _)| *g);
            if let Some(kit) = kit
                && foresight / 5 > 200
                && tx
                    .attempt(Build {
                        person,
                        supplies,
                        kit,
                    })
                    .is_ok()
            {
                return Ok(());
            }
        }
        return Ok(());
    }
    for from in packs {
        let offers: Vec<_> = tx.get::<game_cargo::Cargo>()?.0[&from]
            .goods
            .iter()
            .filter_map(|(good, q)| {
                let food = tx
                    .get::<game_objects::Objects>()
                    .ok()?
                    .food_rules()
                    .get(good)?;
                let surplus = q.saturating_sub(food.portion.saturating_mul(2));
                (surplus > 0).then_some((*good, surplus))
            })
            .collect();
        for (good, q) in offers {
            for to in &sites {
                let inv = &tx.get::<game_cargo::Cargo>()?.0[to];
                let mass = tx.get::<game_objects::Objects>()?.mass(good, 1)?;
                let quantity = q.min(inv.capacity.saturating_sub(inv.used()?) / mass);
                if quantity > 0
                    && tx
                        .attempt(PutAway {
                            person,
                            transfer: game_cargo::Transfer {
                                from,
                                to: *to,
                                good,
                                quantity,
                            },
                        })
                        .is_ok()
                {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

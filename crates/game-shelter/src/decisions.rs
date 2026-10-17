use super::*;
fn carried_fuel(
    tx: &Transaction<'_>,
    person: PersonId,
) -> Result<Option<(InventoryId, GoodId, u64)>> {
    for inventory in tx.get::<game_cargo::Cargo>()?.carried(person) {
        for (good, q) in &tx.get::<game_cargo::Cargo>()?.0[&inventory].goods {
            if *q > 0
                && tx
                    .get::<game_objects::Objects>()?
                    .by_id(*good)?
                    .tags
                    .iter()
                    .any(|t| t.starts_with("fuel-hours:"))
            {
                return Ok(Some((inventory, *good, (*q).min(12))));
            }
        }
    }
    Ok(None)
}
pub(super) fn consider(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if !tx.get::<Shelters>()?.enabled
        || Some(person) == tx.get::<game_lineage::Lineage>()?.current
        || tx.get::<Actions>()?.0.contains_key(&person)
    {
        return Ok(());
    }
    let Ok(area) = game_transport::area(tx.get()?, person) else {
        return Ok(());
    };
    let warmth = game_environment::at(tx, area)?.warmth
        + protection(tx, area)
        + game_equipment::protection(tx, person);
    let resistance = game_vitality::ability(tx, person, "hardiness") / 200;
    let need = (5 - i64::from(warmth) - resistance as i64).max(0);
    if let Some(target) = tx.get::<Shelters>()?.fuel_for.get(&person).copied() {
        if area == target {
            tx.edit::<Shelters>()?.fuel_for.remove(&person);
        } else if carried_fuel(tx, person)?.is_some() {
            let _ = tx.attempt(game_transport::Move {
                person,
                destination: target,
            });
            return Ok(());
        } else {
            let pack = tx.get::<game_cargo::Cargo>()?.carried(person).next();
            let patches: Vec<_> = tx
                .get::<game_knowledge::Knowledge>()?
                .0
                .get(&person)
                .into_iter()
                .flat_map(|f| game_knowledge::with_prefix(f, "patch:"))
                .filter_map(|(key, _)| key.strip_prefix("patch:")?.parse::<u64>().ok())
                .collect();
            if let Some(pack) = pack {
                for patch in patches {
                    let p = &tx.get::<game_ecology::Ecology>()?.patches[&patch];
                    if p.area == area
                        && tx
                            .get::<game_objects::Objects>()?
                            .by_id(p.good)?
                            .tags
                            .iter()
                            .any(|t| t.starts_with("fuel-hours:"))
                    {
                        let inv = &tx.get::<game_cargo::Cargo>()?.0[&pack];
                        let mass = tx.get::<game_objects::Objects>()?.mass(p.good, 1)?;
                        let quantity = 16
                            .min(p.stock)
                            .min(inv.capacity.saturating_sub(inv.used()?) / mass);
                        if quantity > 0
                            && tx
                                .attempt(game_ecology::Gather {
                                    person,
                                    patch,
                                    quantity,
                                    destination: pack,
                                })
                                .is_ok()
                        {
                            return Ok(());
                        }
                    }
                }
            }
            return Ok(());
        }
    }
    if need == 0 {
        return Ok(());
    }
    if let Some(room) = tx.get::<Shelters>()?.rooms.get(&area).cloned() {
        if room.burning_until <= tx.now() {
            if let Some((inventory, fuel, quantity)) = carried_fuel(tx, person)? {
                let _ = tx.attempt(Stoke {
                    person,
                    room: area,
                    inventory,
                    fuel,
                    quantity,
                });
            } else {
                tx.edit::<Shelters>()?.fuel_for.insert(person, area);
                let _ = tx.attempt(game_transport::Move {
                    person,
                    destination: room.outside,
                });
            }
        }
        return Ok(());
    }
    let room = tx
        .get::<Shelters>()?
        .rooms
        .iter()
        .filter(|(_, r)| r.outside == area)
        .max_by_key(|(_, r)| {
            r.insulation
                + if r.burning_until > tx.now() {
                    r.heat
                } else {
                    0
                }
        })
        .map(|(id, _)| *id);
    if let Some(room) = room {
        let _ = tx.attempt(game_transport::Move {
            person,
            destination: room,
        });
        return Ok(());
    }
    let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(person).collect();
    for inventory in packs {
        let kit = tx.get::<game_cargo::Cargo>()?.0[&inventory]
            .goods
            .iter()
            .find(|(g, q)| {
                **q > 0
                    && tx
                        .get::<game_objects::Objects>()
                        .unwrap()
                        .by_id(**g)
                        .is_ok_and(|o| o.tags.iter().any(|t| t.starts_with("shelter-insulation:")))
            })
            .map(|(g, _)| *g);
        let caution = game_vitality::disposition(tx, person, "risk-tolerance");
        if let Some(kit) = kit
            && need * 20 > caution / 10
            && tx
                .attempt(Build {
                    person,
                    inventory,
                    kit,
                })
                .is_ok()
        {
            break;
        }
    }
    Ok(())
}

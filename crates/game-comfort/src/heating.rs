use super::*;
use game_choice::{Proposal, motive};
use game_shelter::{Build, Shelters, Stoke, protection};
#[derive(Clone, Serialize, Deserialize)]
struct FuelRun {
    person: PersonId,
    room: AreaId,
    action: Pending,
}
impl Request for FuelRun {
    const KEY: &'static str = "shelter.fuel-run";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<FuelRun>(|tx, r| {
        tx.edit::<Plans>()?.fuel_for.insert(r.person, r.room);
        tx.invoke(r.action)?;
        Ok(())
    })
}
fn fuel<R: Request>(
    offers: &mut Vec<Proposal>,
    p: PersonId,
    room: AreaId,
    score: i64,
    action: R,
) -> Result<()> {
    offers.push(Proposal::new(
        format!("heat:fuel:{}:{}", room.0, R::KEY),
        motive(score, 0, 0, 2, 0),
        FuelRun {
            person: p,
            room,
            action: Pending::new(action)?,
        },
    )?);
    Ok(())
}
pub(super) fn options(tx: &Transaction<'_>, p: PersonId) -> Result<Vec<Proposal>> {
    let mut offers = vec![];
    if !tx.get::<Shelters>()?.enabled {
        return Ok(offers);
    }
    let Ok(area) = game_transport::area(tx.get()?, p) else {
        return Ok(offers);
    };
    let resistance = game_vitality::ability(tx, p, "hardiness").min(16000) as i64 / 200;
    let warmth = i64::from(
        game_environment::at(tx, area)?.warmth
            + protection(tx, area)
            + game_equipment::protection(tx, p),
    );
    let need = (10 - warmth - resistance).max(0);
    let cold = need * need;
    let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(p).collect();
    let rooms: Vec<_> = tx
        .get::<Shelters>()?
        .rooms
        .iter()
        .filter(|(id, r)| **id == area || r.outside == area)
        .map(|(id, r)| (*id, r.clone()))
        .collect();
    for (id, room) in &rooms {
        if *id != area {
            let benefit = need.min(i64::from(
                room.insulation
                    + if room.burning_until > tx.now() {
                        room.heat
                    } else {
                        0
                    },
            ));
            offers.push(Proposal::new(
                format!("heat:enter:{}", id.0),
                motive(benefit * benefit, 0, 0, 1, 0),
                game_transport::Move {
                    person: p,
                    destination: *id,
                },
            )?);
            if tx.get::<Plans>()?.fuel_for.get(&p) == Some(id) && cold > 0 {
                let has = packs.iter().any(|i| {
                    tx.get::<game_cargo::Cargo>().unwrap().0[i]
                        .goods
                        .iter()
                        .any(|(g, q)| {
                            *q > 0
                                && tx
                                    .get::<game_objects::Objects>()
                                    .unwrap()
                                    .by_id(*g)
                                    .is_ok_and(|o| {
                                        o.tags.iter().any(|t| t.starts_with("fuel-hours:"))
                                    })
                        })
                });
                if has {
                    fuel(
                        &mut offers,
                        p,
                        *id,
                        cold + 5,
                        game_transport::Move {
                            person: p,
                            destination: *id,
                        },
                    )?;
                }
            }
        } else {
            let remaining = room.burning_until.0.saturating_sub(tx.now().0) / HOUR.0;
            for inventory in &packs {
                for (good, q) in &tx.get::<game_cargo::Cargo>()?.0[inventory].goods {
                    if *q == 0
                        || !tx
                            .get::<game_objects::Objects>()?
                            .by_id(*good)?
                            .tags
                            .iter()
                            .any(|t| t.starts_with("fuel-hours:"))
                    {
                        continue;
                    }
                    for quantity in [1, (*q).min(12)]
                        .into_iter()
                        .collect::<std::collections::BTreeSet<_>>()
                    {
                        offers.push(Proposal::new(
                            format!(
                                "heat:stoke:{}:{}:{}:{}",
                                id.0, inventory.0, good.0, quantity
                            ),
                            motive(
                                cold,
                                game_vitality::ability(tx, p, "foresight").min(16000) as i64 / 500,
                                0,
                                quantity.min(100) as i64 + remaining.min(100) as i64 * 5,
                                0,
                            ),
                            Stoke {
                                person: p,
                                room: *id,
                                inventory: *inventory,
                                fuel: *good,
                                quantity,
                            },
                        )?);
                    }
                }
            }
            if cold > 0 {
                fuel(
                    &mut offers,
                    p,
                    *id,
                    cold,
                    game_transport::Move {
                        person: p,
                        destination: room.outside,
                    },
                )?;
            }
        }
    }
    if cold > 0
        && let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&p)
        && let Some(room) = tx
            .get::<Plans>()?
            .fuel_for
            .get(&p)
            .copied()
            .filter(|id| rooms.iter().any(|(r, _)| r == id))
    {
        for (key, fact) in game_knowledge::with_prefix(facts, "patch:").take(64) {
            let Some(patch) = fact.value.decode::<game_ecology::Patch>() else {
                continue;
            };
            let Some(id) = key
                .strip_prefix("patch:")
                .and_then(|n| n.parse::<u64>().ok())
            else {
                continue;
            };
            if patch.area != area
                || !tx
                    .get::<game_objects::Objects>()?
                    .by_id(patch.good)?
                    .tags
                    .iter()
                    .any(|t| t.starts_with("fuel-hours:"))
            {
                continue;
            }
            if let Some(pack) = packs.first() {
                let inv = &tx.get::<game_cargo::Cargo>()?.0[pack];
                let mass = tx.get::<game_objects::Objects>()?.mass(patch.good, 1)?;
                let quantity = 16
                    .min(patch.stock)
                    .min(inv.capacity.saturating_sub(inv.used()?) / mass);
                if quantity > 0 {
                    fuel(
                        &mut offers,
                        p,
                        room,
                        cold,
                        game_ecology::Gather {
                            person: p,
                            patch: id,
                            quantity,
                            destination: *pack,
                        },
                    )?;
                    break;
                }
            }
        }
    }
    for inventory in packs {
        for (good, q) in &tx.get::<game_cargo::Cargo>()?.0[&inventory].goods {
            if *q > 0
                && tx
                    .get::<game_objects::Objects>()?
                    .by_id(*good)?
                    .tags
                    .iter()
                    .any(|t| t.starts_with("shelter-insulation:"))
            {
                offers.push(Proposal::new(
                    format!("heat:build:{}:{}", inventory.0, good.0),
                    motive(
                        cold,
                        game_vitality::ability(tx, p, "foresight").min(16000) as i64 / 250,
                        0,
                        10 + rooms.len() as i64 * 20,
                        0,
                    ),
                    Build {
                        person: p,
                        inventory,
                        kit: *good,
                    },
                )?);
            }
        }
    }
    Ok(offers)
}

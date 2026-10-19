use super::*;
use game_choice::{Proposal, motive};
fn options(tx: &Transaction<'_>, p: PersonId) -> Result<Vec<Proposal>> {
    let mut offers = vec![];
    if !tx.get::<Stores>()?.enabled {
        return Ok(offers);
    }
    let Ok(area) = game_transport::area(tx.get()?, p) else {
        return Ok(offers);
    };
    let foresight = game_vitality::ability(tx, p, "foresight").min(16000);
    let horizon = 1 + foresight / 1000;
    let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(p).collect();
    let facts = tx.get::<game_knowledge::Knowledge>()?.0.get(&p);
    let sites: Vec<_> = tx
        .get::<Stores>()?
        .sites
        .iter()
        .filter(|(id, s)| {
            s.area == area
                && (s.owner == p
                    || facts
                        .is_some_and(|f| f.contains_key(game_knowledge::store_key(**id).as_str())))
        })
        .map(|(id, _)| *id)
        .collect();
    for inventory in packs {
        for (good, q) in &tx.get::<game_cargo::Cargo>()?.0[&inventory].goods {
            if *q == 0 {
                continue;
            }
            let object = tx.get::<game_objects::Objects>()?.by_id(*good)?;
            if object.tags.iter().any(|t| t.starts_with("storage:")) {
                offers.push(Proposal::new(
                    format!("storage:build:{}:{}", good.0, inventory.0),
                    motive(0, foresight as i64 / 25, 0, 5 + sites.len() as i64 * 40, 0),
                    Build {
                        person: p,
                        supplies: inventory,
                        kit: *good,
                    },
                )?);
            }
            let Some(food) = &object.food else {
                continue;
            };
            let retain = food.portion.saturating_mul(horizon);
            let surplus = q.saturating_sub(retain);
            if surplus == 0 {
                continue;
            }
            for to in &sites {
                let location = facts.and_then(|f| {
                    game_knowledge::known::<game_knowledge::StoreLocation>(
                        f,
                        &game_knowledge::location_key(*to),
                    )
                });
                let capacity = tx.get::<game_cargo::Cargo>()?.0[to].capacity;
                let free = location.map_or(capacity / 2, |l| capacity.saturating_sub(l.mass));
                let mass = tx.get::<game_objects::Objects>()?.mass(*good, 1)?;
                let quantity = surplus.min(free / mass);
                if quantity == 0 {
                    continue;
                }
                let mut benefit = 0i64;
                let temperature = i64::from(super::temperature(tx, *to)? / 5 * 5);
                let current = i64::from(super::temperature(tx, inventory)? / 5 * 5);
                let storage = if tx.get::<Stores>()?.sites[to].kind == Kind::Cellar {
                    2
                } else {
                    1
                };
                if let Some(facts) = facts {
                    for (_, f) in
                        game_knowledge::with_prefix(facts, &format!("theory:good:{}:", good.0))
                            .take(32)
                    {
                        if let Some(claim) = f.value.decode::<game_reasoning::Claim>() {
                            let source = std::collections::BTreeMap::from([
                                ("temperature".into(), current),
                                ("storage".into(), 0),
                            ]);
                            let target = std::collections::BTreeMap::from([
                                ("temperature".into(), temperature),
                                ("storage".into(), storage),
                            ]);
                            if matches!(claim.outcome.as_str(), "spoilage" | "stock-loss")
                                && claim.predicts(&source)
                                && claim.predicts(&target)
                            {
                                let delta = match claim.factor.as_str() {
                                    "temperature" => current - temperature,
                                    "storage" => -storage,
                                    _ => 0,
                                };
                                benefit = benefit.saturating_add(
                                    (i128::from(delta)
                                        * i128::from(claim.effect)
                                        * i128::from(f.confidence)
                                        / 10000)
                                        .clamp(-1000, 1000)
                                        as i64,
                                );
                            }
                        }
                    }
                }
                let need = quantity.min(100) as i64 * (1 + foresight as i64 / 1000) + benefit;
                let reluctance = game_property::reluctance(tx, p, *to);
                offers.push(Proposal::new(
                    format!("storage:put:{}:{}:{}", inventory.0, to.0, good.0),
                    motive(need, 0, 0, 2, reluctance),
                    PutAway {
                        person: p,
                        transfer: game_cargo::Transfer {
                            from: inventory,
                            to: *to,
                            good: *good,
                            quantity,
                        },
                    },
                )?);
            }
        }
    }
    Ok(offers)
}
pub(super) fn consider(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    if Some(p) == tx.get::<game_lineage::Lineage>()?.current {
        return Ok(());
    }
    let offers = options(tx, p)?;
    if game_choice::collecting(tx, p) {
        for o in offers {
            game_choice::submit(tx, p, o)?;
        }
    } else if let Some(o) = offers
        .into_iter()
        .filter(|o| o.motive.score() > 0)
        .max_by_key(|o| o.motive.score())
    {
        tx.invoke(o.request)?;
    }
    Ok(())
}

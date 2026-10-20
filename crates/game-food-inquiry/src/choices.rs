use super::*;
pub(super) fn register(b: &mut Builder) -> Result<()> {
    game_choice::provider(b, "observations.opportunities", |tx, e| {
        if !game_choice::collecting(tx, e.person) {
            return Ok(());
        }
        let p = e.person;
        food::propose_trials(tx, p)?;
        let curiosity = game_vitality::disposition(tx, p, "openness") / 10
            + game_vitality::disposition(tx, p, "contemplation") / 10;
        let Ok(area) = game_transport::area(tx.get()?, p) else {
            return Ok(());
        };
        let mut inventories: std::collections::BTreeSet<_> =
            tx.get::<game_cargo::Cargo>()?.carried(p).collect();
        if let Some(probes) = tx.get::<Samples>()?.probes.get(&p) {
            inventories.extend(
                probes
                    .values()
                    .filter(|s| {
                        tx.get::<game_knowledge::Knowledge>()
                            .ok()
                            .and_then(|k| k.0.get(&p))
                            .and_then(|f| {
                                game_knowledge::known::<game_knowledge::StoreLocation>(
                                    f,
                                    &game_knowledge::location_key(s.inventory),
                                )
                            })
                            .is_some_and(|l| l.area == area)
                    })
                    .map(|s| s.inventory),
            );
        }
        let facts = tx.get::<game_knowledge::Knowledge>()?.0.get(&p);
        inventories.extend(
            tx.get::<game_food_storage::Stores>()?
                .sites
                .keys()
                .copied()
                .filter(|id| {
                    facts
                        .and_then(|f| {
                            game_knowledge::known::<game_knowledge::StoreLocation>(
                                f,
                                &game_knowledge::location_key(*id),
                            )
                        })
                        .is_some_and(|l| l.area == area)
                }),
        );
        for inventory in inventories {
            let facts = tx.get::<game_knowledge::Knowledge>()?.0.get(&p);
            let key = game_knowledge::store_key(inventory);
            let stock = facts.and_then(|f| game_knowledge::known::<game_knowledge::Stock>(f, &key));
            if facts.is_some_and(|f| f.contains_key(key.as_str())) && stock.is_none() {
                continue;
            }
            let probes: Vec<_> = tx
                .get::<Samples>()?
                .probes
                .get(&p)
                .into_iter()
                .flat_map(|r| r.values())
                .filter(|s| s.inventory == inventory && s.initial > 0)
                .collect();
            let foods: Vec<_> = stock
                .as_ref()
                .into_iter()
                .flat_map(|r| r.iter())
                .filter(|(g, q)| {
                    **q > 0
                        && tx
                            .get::<game_objects::Objects>()
                            .unwrap()
                            .by_id(**g)
                            .is_ok_and(|o| o.food.is_some())
                })
                .map(|(g, _)| *g)
                .collect();
            let own = facts
                .and_then(|f| {
                    game_knowledge::known::<game_knowledge::StoreLocation>(
                        f,
                        &game_knowledge::location_key(inventory),
                    )
                })
                .is_some_and(|l| l.carrier == Some(p));
            if own && stock.is_some() && foods.is_empty() && probes.is_empty() {
                continue;
            }
            let novelty = if stock.is_none() {
                1
            } else {
                foods
                    .iter()
                    .filter(|g| !probes.iter().any(|s| s.good == **g))
                    .count()
            };
            let age = probes
                .iter()
                .map(|s| tx.now().0.saturating_sub(s.at.0) / HOUR.0)
                .max()
                .or_else(|| {
                    tx.get::<Samples>()
                        .ok()?
                        .inspections
                        .get(&p)?
                        .get(&inventory)
                        .map(|(at, _)| tx.now().0.saturating_sub(at.0) / HOUR.0)
                })
                .unwrap_or(24);
            game_choice::offer(
                tx,
                p,
                format!("inquiry:inspect:{}", inventory.0),
                game_choice::motive(
                    0,
                    0,
                    curiosity * (age.min(48) as i64 + novelty.min(4) as i64 * 12) / 48,
                    5,
                    0,
                ),
                game_choice::Undertake {
                    person: p,
                    kind: "inspect-food".into(),
                    duration: SimDuration(5 * MINUTE.0),
                    request: Pending::immediate(food::Inspect {
                        person: p,
                        inventory,
                    }),
                },
            )?;
        }
        Ok(())
    })
}

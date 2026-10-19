use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Probe {
    pub(super) inventory: InventoryId,
    pub(super) good: GoodId,
    pub(super) initial: u64,
    pub(super) at: SimTime,
    temperature: i64,
    method: i64,
    trial: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Split {
    from: InventoryId,
    to: InventoryId,
    good: GoodId,
    quantity: u64,
    trial: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Inspect {
    pub person: PersonId,
    pub inventory: InventoryId,
}
impl Request for Inspect {
    const KEY: &'static str = "inquiry.inspect-food";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub person: PersonId,
    pub from: InventoryId,
    pub to: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for Experiment {
    const KEY: &'static str = "inquiry.split-trial";
    type Response = SimTime;
}
fn inspect(
    tx: &mut Transaction<'_>,
    p: PersonId,
    inventory: InventoryId,
    trial: u64,
    witnessed_decay: bool,
) -> Result<()> {
    let area = game_transport::area(tx.get()?, p)?;
    tx.call(game_knowledge::ObserveStore {
        person: p,
        inventory,
    })?;
    if game_cargo::locality(tx, inventory).ok() != Some(area) {
        let keys: Vec<_> = tx
            .get::<Samples>()?
            .probes
            .get(&p)
            .into_iter()
            .flat_map(|r| r.iter())
            .filter(|(_, s)| s.inventory == inventory)
            .map(|(k, _)| k.clone())
            .collect();
        if let Some(row) = tx.edit::<Samples>()?.probes.get_mut(&p) {
            for key in keys {
                row.remove(&key);
            }
        }
        let now = tx.now();
        tx.edit::<Samples>()?
            .inspections
            .entry(p)
            .or_default()
            .insert(inventory, (now, GoodId(0)));
        return Ok(());
    }
    let temperature = i64::from(game_food_storage::temperature(tx, inventory)? / 5 * 5);
    let method = tx
        .get::<game_food_storage::Stores>()?
        .sites
        .get(&inventory)
        .map_or(0, |s| {
            if s.kind == game_food_storage::Kind::Cellar {
                2
            } else {
                1
            }
        });
    let mut goods: std::collections::BTreeSet<_> = tx.get::<game_cargo::Cargo>()?.0[&inventory]
        .goods
        .keys()
        .copied()
        .collect();
    goods.extend(
        tx.get::<Samples>()?
            .probes
            .get(&p)
            .into_iter()
            .flat_map(|r| r.values())
            .filter(|s| s.inventory == inventory)
            .map(|s| s.good),
    );
    let after = tx
        .get::<Samples>()?
        .inspections
        .get(&p)
        .and_then(|r| r.get(&inventory))
        .map_or(GoodId(0), |(_, g)| *g);
    let budget = (4 + game_vitality::ability(tx, p, "intelligence") / 250).clamp(4, 64) as usize;
    let selected: Vec<_> = goods
        .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
        .chain(goods.range(..=after))
        .copied()
        .filter(|good| {
            tx.get::<game_objects::Objects>()
                .unwrap()
                .by_id(*good)
                .is_ok_and(|o| o.food.is_some())
                && (tx.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(*good) > 0
                    || tx
                        .get::<Samples>()
                        .unwrap()
                        .probes
                        .get(&p)
                        .into_iter()
                        .flat_map(|r| r.values())
                        .any(|s| s.inventory == inventory && s.good == *good && s.initial > 0))
        })
        .take(budget)
        .collect();
    let cursor = selected.last().copied().unwrap_or(after);
    for good in selected {
        let quantity = tx.get::<game_cargo::Cargo>()?.0[&inventory].amount(good);
        let key = format!("{}:{}", inventory.0, good.0);
        let old = tx
            .get::<Samples>()?
            .probes
            .get(&p)
            .and_then(|r| r.get(&key))
            .cloned();
        if let Some(old) = old {
            let elapsed = tx.now().0.saturating_sub(old.at.0);
            if elapsed >= HOUR.0 && old.initial > 0 {
                let loss = old.initial.saturating_sub(quantity);
                let value = (u128::from(loss) * 1000 * u128::from(DAY.0)
                    / u128::from(old.initial)
                    / u128::from(elapsed))
                .min(1000000) as i64;
                tx.call(Witness {
                    observer: p,
                    observation: Observation {
                        subject: format!("good:{}", good.0),
                        context: BTreeMap::from([
                            ("temperature".into(), (old.temperature + temperature) / 2),
                            ("storage".into(), old.method),
                        ]),
                        outcome: if witnessed_decay {
                            "spoilage"
                        } else {
                            "stock-loss"
                        }
                        .into(),
                        value,
                        trial: if witnessed_decay { old.trial } else { 0 },
                    },
                })?;
            }
        }
        let now = tx.now();
        if quantity > 0 {
            tx.edit::<Samples>()?.probes.entry(p).or_default().insert(
                key,
                Probe {
                    inventory,
                    good,
                    initial: quantity,
                    at: now,
                    temperature,
                    method,
                    trial,
                },
            );
        } else if let Some(row) = tx.edit::<Samples>()?.probes.get_mut(&p) {
            row.remove(&key);
        }
    }
    let now = tx.now();
    tx.edit::<Samples>()?
        .inspections
        .entry(p)
        .or_default()
        .insert(inventory, (now, cursor));
    Ok(())
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Inspect>(|tx, r| inspect(tx, r.person, r.inventory, 0, false))?;
    b.request::<Experiment>(|tx, r| {
        require(
            r.quantity > 0
                && tx.get::<game_cargo::Cargo>()?.0[&r.from].amount(r.good)
                    == r.quantity.saturating_mul(2)
                && tx.get::<game_cargo::Cargo>()?.0[&r.to].amount(r.good) == 0,
            "split trial needs comparable separate samples",
        )?;
        let until = tx.call(game_food_storage::PutAway {
            person: r.person,
            transfer: game_cargo::Transfer {
                from: r.from,
                to: r.to,
                good: r.good,
                quantity: r.quantity,
            },
        })?;
        let trial = tx.allocate()?;
        tx.edit::<Samples>()?.trials.insert(
            r.person,
            Split {
                from: r.from,
                to: r.to,
                good: r.good,
                quantity: r.quantity,
                trial,
            },
        );
        Ok(until)
    })?;
    b.subscribe::<ActionFinished>("inquiry.split-samples", |tx, e| {
        if let Some(split) = tx.edit::<Samples>()?.trials.remove(&e.person)
            && tx.get::<game_cargo::Cargo>()?.0[&split.to].amount(split.good) == split.quantity
            && tx.get::<game_cargo::Cargo>()?.0[&split.from].amount(split.good) == split.quantity
        {
            inspect(tx, e.person, split.from, split.trial, false)?;
            inspect(tx, e.person, split.to, split.trial, false)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_preservation::Spoiled>("inquiry.witnessed-spoilage", |tx, e| {
        let observers: Vec<_> = tx
            .get::<Samples>()?
            .probes
            .iter()
            .filter(|(p, r)| {
                r.values().any(|s| s.inventory == e.inventory)
                    && game_transport::area(tx.get().unwrap(), **p)
                        .ok()
                        .is_some_and(|a| game_cargo::locality(tx, e.inventory).ok() == Some(a))
            })
            .map(|(p, _)| *p)
            .collect();
        for p in observers {
            inspect(tx, p, e.inventory, 0, true)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Fed>("inquiry.sample-consumption", |tx, e| {
        let Some(inventory) = e.inventory else {
            return Ok(());
        };
        let observers: Vec<_> = tx
            .get::<Samples>()?
            .probes
            .iter()
            .filter(|(p, r)| {
                r.values()
                    .any(|s| s.inventory == inventory && s.good == e.good)
                    && (e.caregiver == **p
                        || game_transport::area(tx.get().unwrap(), **p)
                            .ok()
                            .is_some_and(|a| game_cargo::locality(tx, inventory).ok() == Some(a)))
            })
            .map(|(p, _)| *p)
            .collect();
        let now = tx.now();
        for person in observers {
            let probes = tx.edit::<Samples>()?.probes.get_mut(&person).unwrap();
            let keys: Vec<_> = probes
                .iter()
                .filter(|(_, s)| s.inventory == inventory && s.good == e.good)
                .map(|(k, _)| k.clone())
                .collect();
            for key in keys {
                let s = probes.get_mut(&key).unwrap();
                s.trial = 0;
                s.initial = s.initial.saturating_sub(e.quantity);
                s.at = now;
                if s.initial == 0 {
                    probes.remove(&key);
                }
            }
        }
        Ok(())
    })
}

pub(super) fn propose_trials(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    let Ok(area) = game_transport::area(tx.get()?, p) else {
        return Ok(());
    };
    let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(p).collect();
    let facts = tx.get::<game_knowledge::Knowledge>()?.0.get(&p);
    let stores: Vec<_> = tx
        .get::<game_food_storage::Stores>()?
        .sites
        .iter()
        .filter(|(id, s)| {
            s.area == area
                && facts.is_some_and(|f| f.contains_key(game_knowledge::store_key(**id).as_str()))
        })
        .map(|(id, _)| *id)
        .take(8)
        .collect();
    let curiosity = game_vitality::disposition(tx, p, "openness") / 10
        + game_vitality::disposition(tx, p, "contemplation") / 10;
    let hunger = tx.get::<game_physiology::Bodies>()?.people[&p]
        .hunger
        .min(1000) as i64;
    for from in packs {
        let goods: Vec<_> = tx.get::<game_cargo::Cargo>()?.0[&from]
            .goods
            .iter()
            .filter(|(_, q)| **q >= 2 && **q % 2 == 0)
            .filter(|(g, _)| {
                tx.get::<game_objects::Objects>()
                    .unwrap()
                    .by_id(**g)
                    .is_ok_and(|o| o.food.is_some())
            })
            .map(|(g, q)| (*g, *q / 2))
            .take(4)
            .collect();
        for (good, quantity) in goods {
            for to in &stores {
                let facts = tx.get::<game_knowledge::Knowledge>()?.0.get(&p).unwrap();
                let empty = game_knowledge::known::<game_knowledge::Stock>(
                    facts,
                    &game_knowledge::store_key(*to),
                )
                .is_some_and(|s| s.get(&good).copied().unwrap_or(0) == 0);
                if empty {
                    game_choice::offer(
                        tx,
                        p,
                        format!("inquiry:trial:{}:{}:{}", from.0, to.0, good.0),
                        game_choice::motive(0, 0, curiosity, 20 + hunger / 3, 0),
                        Experiment {
                            person: p,
                            from,
                            to: *to,
                            good,
                            quantity,
                        },
                    )?;
                }
            }
        }
    }
    Ok(())
}

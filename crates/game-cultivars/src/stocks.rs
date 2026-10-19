use super::*;
pub(super) fn put(
    tx: &mut Transaction<'_>,
    inventory: InventoryId,
    good: GoodId,
    lot: Lot,
) -> Result<()> {
    let rows = tx
        .edit::<Cultivars>()?
        .stocks
        .entry(inventory)
        .or_default()
        .entry(good)
        .or_default();
    if let Some(last) = rows.back_mut()
        && last.strain.id == lot.strain.id
    {
        last.quantity = last
            .quantity
            .checked_add(lot.quantity)
            .ok_or(Error::Overflow)?;
    } else {
        rows.push_back(lot);
    }
    Ok(())
}
fn take(
    tx: &mut Transaction<'_>,
    inventory: InventoryId,
    good: GoodId,
    quantity: u64,
) -> Result<Vec<Lot>> {
    let s = tx.edit::<Cultivars>()?;
    let mut left = quantity;
    let mut taken = vec![];
    let rows = s
        .stocks
        .get_mut(&inventory)
        .and_then(|r| r.get_mut(&good))
        .ok_or_else(|| Error::Missing("plant lot".into()))?;
    while left > 0 {
        let mut lot = rows
            .pop_front()
            .ok_or_else(|| Error::Rejected("plant quantity mismatch".into()))?;
        let quantity = left.min(lot.quantity);
        left -= quantity;
        lot.quantity -= quantity;
        taken.push(Lot {
            quantity,
            strain: lot.strain.clone(),
        });
        if lot.quantity > 0 {
            rows.push_front(lot);
        }
    }
    if rows.is_empty() {
        s.stocks.get_mut(&inventory).unwrap().remove(&good);
        if s.stocks[&inventory].is_empty() {
            s.stocks.remove(&inventory);
        }
    }
    Ok(taken)
}
pub fn representative(
    tx: &Transaction<'_>,
    inventory: InventoryId,
    good: GoodId,
) -> Option<Strain> {
    tx.get::<Cultivars>()
        .ok()?
        .stocks
        .get(&inventory)?
        .get(&good)?
        .front()
        .map(|l| l.strain.clone())
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_cargo::Changed>("cultivars.stock", |tx, e| {
        let s = tx.get::<Cultivars>()?;
        if !s.enabled {
            return Ok(());
        }
        let Some(species) = s.goods.get(&e.good).copied() else {
            return Ok(());
        };
        if e.added {
            let expected = tx
                .edit::<Cultivars>()?
                .incoming
                .remove(&(e.inventory, e.good));
            let lots = if let Some(lots) = expected {
                require(
                    lots.iter()
                        .try_fold(0u64, |n, l| n.checked_add(l.quantity))
                        .is_some_and(|q| q == e.quantity),
                    "plant provenance quantity mismatch",
                )?;
                lots
            } else {
                let strain = tx.call(Make {
                    species,
                    mother: None,
                    father: None,
                    clonal: false,
                })?;
                vec![Lot {
                    quantity: e.quantity,
                    strain,
                }]
            };
            for lot in lots {
                put(tx, e.inventory, e.good, lot)?;
            }
        } else {
            let lots = take(tx, e.inventory, e.good, e.quantity)?;
            if let Some(to) = e.destination {
                for lot in lots {
                    put(tx, to, e.good, lot)?;
                }
            }
        }
        Ok(())
    })?;
    b.request::<Select>(|tx, r| {
        require(
            game_cargo::locality(tx, r.inventory)? == game_transport::area(tx.get()?, r.person)?,
            "seed selection is not local",
        )?;
        let s = tx.get::<Cultivars>()?;
        let Some(rows) = s
            .stocks
            .get(&r.inventory)
            .and_then(|rows| rows.get(&r.good))
        else {
            return Ok(None);
        };
        let budget =
            (1 + game_vitality::ability(tx, r.person, "intelligence") / 250).min(64) as usize;
        let knowledge = s.observed.get(&r.person);
        let patience = game_vitality::disposition(tx, r.person, "patience");
        let openness = game_vitality::disposition(tx, r.person, "openness");
        let foresight = game_vitality::ability(tx, r.person, "foresight").min(16000) as i64;
        let trials = s.trials.get(&r.person);
        let best = rows
            .iter()
            .enumerate()
            .take(budget)
            .max_by_key(|(index, lot)| {
                let inherited = lot.strain.mother.unwrap_or(lot.strain.id);
                let trial =
                    trials.and_then(|t| t.get(&lot.strain.id).or_else(|| t.get(&inherited)));
                let experienced = knowledge
                    .and_then(|k| k.get(&lot.strain.id).or_else(|| k.get(&inherited)))
                    .copied();
                let performance = trial.map_or(experienced.unwrap_or(1), |t| t.mean);
                let reliability = trial.map_or(0, |t| {
                    ((t.count - t.failures) * 100 / t.count.max(1)) as i64
                });
                let familiarity = if experienced.is_some() {
                    patience / 10
                } else {
                    openness / 5
                };
                let score = performance.saturating_mul(20)
                    + reliability.saturating_mul(foresight / 100)
                    + familiarity;
                (score, std::cmp::Reverse(*index))
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        let rows = tx
            .edit::<Cultivars>()?
            .stocks
            .get_mut(&r.inventory)
            .unwrap()
            .get_mut(&r.good)
            .unwrap();
        let lot = rows.remove(best).unwrap();
        let strain = lot.strain.clone();
        rows.push_front(lot);
        Ok(Some(strain))
    })?;
    Ok(())
}

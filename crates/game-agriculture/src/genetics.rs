use super::*;
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_environment::Changed>("agriculture.climate", |tx, e| {
        let plots: Vec<_> = tx
            .get::<Farms>()?
            .plots
            .iter()
            .filter(|(_, p)| p.area == e.area && matches!(p.phase, Phase::Growing { .. }))
            .map(|(id, _)| *id)
            .collect();
        if let Some(previous) = &e.previous {
            for plot in plots {
                update(tx, plot, Some(previous))?;
            }
        }
        Ok(())
    })?;
    b.before::<InstallCrops>("agriculture.catalog-unused", |tx, _| {
        require(
            tx.get::<Farms>()?.plots.is_empty(),
            "crop definitions are in use",
        )
    })?;
    b.before::<game_crops::Natural>("agriculture.natural-unused", |tx, _| {
        require(
            tx.get::<Farms>()?.plots.is_empty(),
            "enable seasonal farming before planting",
        )
    })?;
    Ok(())
}
pub(super) fn crop(tx: &Transaction<'_>, plot: PlotId) -> Result<Crop> {
    let p = tx
        .get::<Farms>()?
        .plots
        .get(&plot)
        .ok_or_else(|| Error::Missing("plot".into()))?;
    let mut crop = tx.get::<Crops>()?.expressed(&p.crop, p.strain.as_ref())?;
    crop.growth = game_crop_cohorts::duration(tx.get()?, &p.crop, &p.cohorts, crop.growth)?;
    Ok(crop)
}

pub(super) fn select(
    tx: &mut Transaction<'_>,
    person: PersonId,
    plot: PlotId,
    inventory: InventoryId,
    good: GoodId,
    quantity: u64,
) -> Result<()> {
    let strain = if tx.get::<Crops>()?.natural {
        tx.call(game_cultivars::Select {
            person,
            inventory,
            good,
        })?
    } else {
        None
    };
    let cohorts = game_crop_cohorts::sown(
        tx.get::<game_cultivars::Cultivars>().ok(),
        inventory,
        good,
        quantity,
        tx.now(),
    );
    let at = tx.now();
    let p = tx.edit::<Farms>()?.plots.get_mut(&plot).unwrap();
    p.strain = strain;
    p.cohorts = cohorts;
    p.established = Some(at);
    p.harvests = 0;
    Ok(())
}
pub(super) fn harvest(
    tx: &mut Transaction<'_>,
    person: PersonId,
    plot: PlotId,
    inventory: InventoryId,
    quantity: u64,
) -> Result<()> {
    let p = tx.get::<Farms>()?.plots[&plot].clone();
    let crop = crop(tx, plot)?;
    let parents = if p.cohorts.is_empty() {
        p.strain
            .clone()
            .map(|s| vec![(s, quantity)])
            .unwrap_or_default()
    } else {
        game_crop_cohorts::weights(tx.get()?, &p.crop, &p.cohorts)?
    };
    if parents.is_empty() {
        return Ok(());
    }
    let mates: Vec<_> = tx
        .get::<Farms>()?
        .plots
        .values()
        .filter(|other| {
            other.area == p.area
                && matches!(other.phase, Phase::Ripe { .. } | Phase::Harvesting { .. })
        })
        .flat_map(|other| game_crop_cohorts::strains(&other.cohorts, other.strain.as_ref()))
        .collect();
    game_crop_cohorts::offspring(tx, person, &parents, &mates, quantity, &crop, inventory)
}
pub(super) fn after_harvest(tx: &mut Transaction<'_>, plot: PlotId, remaining: u64) -> Result<()> {
    if remaining > 0 {
        tx.edit::<Farms>()?.plots.get_mut(&plot).unwrap().phase = Phase::Ripe {
            yield_units: remaining,
        };
        return Ok(());
    }
    let c = crop(tx, plot)?;
    let now = tx.now();
    let p = &tx.get::<Farms>()?.plots[&plot];
    let regrowth = c.regrowth_at(now, p.established, tx.get::<Crops>()?.natural);
    let phase = if let Some(span) = regrowth {
        let ready = now.after(span)?;
        tx.schedule(ready, 25, Ripe { plot })?;
        Phase::Growing { ready }
    } else {
        Phase::Fallow
    };
    let p = tx.edit::<Farms>()?.plots.get_mut(&plot).unwrap();
    p.phase = phase;
    p.progress = regrowth.map(|_| game_crops::Growth::new(now));
    game_crop_cohorts::reset(&mut p.cohorts, &game_crops::Growth::new(now));
    p.harvests = p.harvests.saturating_add(1);
    Ok(())
}
pub(super) fn update(
    tx: &mut Transaction<'_>,
    plot: PlotId,
    previous: Option<&game_environment::Climate>,
) -> Result<()> {
    if !tx.get::<Crops>()?.natural {
        tx.edit::<Farms>()?.plots.get_mut(&plot).unwrap().progress = None;
        return Ok(());
    }
    let c = crop(tx, plot)?;
    let p = &tx.get::<Farms>()?.plots[&plot];
    let mut growth = p
        .progress
        .clone()
        .unwrap_or_else(|| game_crops::Growth::new(tx.now()));
    growth.advance(
        previous.unwrap_or(game_environment::climate(tx, p.area)?),
        &c,
        tx.now(),
    )?;
    let cohorts = game_crop_cohorts::advance(
        tx.get()?,
        &p.crop,
        &p.cohorts,
        previous.unwrap_or(game_environment::climate(tx, p.area)?),
        tx.now(),
    )?;
    let p = tx.edit::<Farms>()?.plots.get_mut(&plot).unwrap();
    p.progress = Some(growth);
    p.cohorts = cohorts;
    Ok(())
}

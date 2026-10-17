use super::*;
pub(super) fn advance(
    tx: &mut Transaction<'_>,
    r: game_ecology::PlantGrowth,
) -> Result<Option<game_ecology::RenewalOutcome>> {
    let _measurement = game_diagnostics::span("wild growth");
    if !tx.get::<Botany>()?.enabled || !stand::ensure(tx, &r)? {
        return Ok(None);
    }
    let at = tx.now();
    let season = game_environment::at(tx, r.plant.area)?.clone();
    let mut s = tx.get::<Botany>()?.stands[&r.patch].clone();
    let elapsed = at.0.saturating_sub(s.last.0);
    let before = s.plants.len();
    if elapsed > 0 {
        s.plants.retain(|p| {
            let age = u128::from(at.0.saturating_sub(p.born.0).saturating_add(p.age_offset)) * 1000
                / u128::from(p.expression.lifespan_factor.max(1));
            let extreme = (s.minimum - p.expression.cold - 10 - season.warmth).max(0)
                + (season.warmth - (s.maximum + p.expression.heat + 10)).max(0);
            let risk = (extreme as u64).saturating_mul(20).saturating_mul(elapsed) / DAY.0;
            age < u128::from(s.lifespan.0)
                && random(tx.seed(), r.patch.wrapping_add(p.id), at.0 / DAY.0) % 1000
                    >= risk.min(1000)
        });
    }
    while s.seedbank.front().is_some_and(|l| l.expires <= at) {
        s.seedbank.pop_front();
    }
    if at >= s.flowering {
        for p in &mut s.plants {
            p.family = None;
        }
        s.flowering = at.after(SimDuration(30 * DAY.0))?;
    }
    let productive: Vec<_> = s
        .plants
        .iter()
        .enumerate()
        .filter(|(_, p)| s.productive(p, at, season.warmth, season.rain))
        .map(|(i, p)| (i, p.expression.yield_factor))
        .collect();
    for (i, _) in &productive {
        if s.plants[*i].family.is_some() {
            continue;
        }
        let mother = s.plants[*i].strain.clone();
        let father = if productive.is_empty() {
            mother.clone()
        } else {
            let j = (random(tx.seed(), mother.id, at.0) % productive.len() as u64) as usize;
            s.plants[productive[j].0].strain.clone()
        };
        let family = tx.call(game_cultivars::Make {
            species: mother.species,
            mother: Some(mother),
            father: Some(father),
            clonal: false,
        })?;
        s.plants[*i].family = Some(family.clone());
        s.seedbank.push_back(Seed {
            strain: family,
            quantity: (s.plants[*i].expression.yield_factor / 500).max(1),
            expires: at.after(SimDuration(730 * DAY.0))?,
        });
    }
    let mut bank: u64 = s.seedbank.iter().map(|l| l.quantity).sum();
    while bank > 256 {
        if let Some(old) = s.seedbank.pop_front() {
            bank -= old.quantity;
        } else {
            break;
        }
    }
    let mut born = 0;
    for _ in 0..8 {
        if s.plants.len() >= 32 {
            break;
        }
        let candidates: Vec<_> = s
            .seedbank
            .iter()
            .enumerate()
            .filter(|(_, l)| s.suitable(l.strain.expression(), season.warmth, season.rain))
            .map(|(i, l)| (i, l.quantity))
            .collect();
        if candidates.is_empty() {
            break;
        }
        let total: u64 = candidates.iter().map(|(_, q)| *q).sum();
        let mut draw = random(tx.seed(), r.patch, at.0 + born) % total;
        let choice = candidates
            .into_iter()
            .find_map(|(i, q)| {
                if draw < q {
                    Some(i)
                } else {
                    draw -= q;
                    None
                }
            })
            .unwrap();
        let seed = &mut s.seedbank[choice];
        let strain = seed.strain.clone();
        seed.quantity -= 1;
        if seed.quantity == 0 {
            s.seedbank.remove(choice);
        }
        s.sequence = s.sequence.checked_add(1).ok_or(Error::Overflow)?;
        s.plants.push(Plant {
            id: s.sequence,
            expression: strain.expression(),
            strain,
            born: at,
            age_offset: 0,
            family: None,
        });
        born += 1;
    }
    let decay = s.shelf.map_or(0, |days| {
        let life = u128::from(days)
            * u128::from(DAY.0)
            * u128::from(game_preservation::temperature_factor(season.warmth));
        (u128::from(r.plant.stock) * u128::from(elapsed) * 1000)
            .div_ceil(life.max(1))
            .min(u128::from(r.plant.stock.saturating_sub(r.reserved))) as u64
    });
    if decay > 0 {
        s.take(decay)?;
    }
    let weight: u64 = productive.iter().map(|(_, w)| *w).sum();
    let potential = (u128::from(r.plant.renew) * u128::from(weight) * u128::from(elapsed)
        / 32_000
        / u128::from(DAY.0))
    .min(u128::from(u64::MAX)) as u64;
    let growth = potential.min(r.plant.capacity.saturating_sub(r.plant.stock - decay));
    let mut remaining = growth;
    for (n, (i, w)) in productive.iter().enumerate() {
        let quantity = if n + 1 == productive.len() {
            remaining
        } else {
            (u128::from(growth) * u128::from(*w) / u128::from(weight.max(1))) as u64
        };
        remaining -= quantity;
        if quantity > 0 {
            s.add(Lot {
                quantity,
                strain: s.plants[*i].family.clone().unwrap(),
            })?;
        }
    }
    s.last = at;
    let deaths = before.saturating_sub(s.plants.len().saturating_sub(born as usize)) as u64;
    let snapshot = Changed {
        patch: r.patch,
        living: s.plants.len() as u64,
        seeds: s.seedbank.iter().map(|l| l.quantity).sum(),
        births: born,
        deaths,
        generation: s
            .plants
            .iter()
            .map(|p| p.strain.generation)
            .max()
            .unwrap_or(0),
        mean_cold: s
            .plants
            .iter()
            .map(|p| i64::from(p.expression.cold))
            .sum::<i64>()
            / s.plants.len().max(1) as i64,
    };
    let state = tx.edit::<Botany>()?;
    state.births = state.births.saturating_add(born);
    state.deaths = state.deaths.saturating_add(deaths);
    state.stands.insert(r.patch, s);
    if born > 0 || deaths > 0 {
        tx.emit(snapshot, vec![])?;
    }
    Ok(Some(game_ecology::RenewalOutcome { growth, decay }))
}

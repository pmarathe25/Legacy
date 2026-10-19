//! Physical mixed plant cohorts retain distinct ancestry and responses to climate.
use game_core::*;
use game_crops::{Crop, Crops, Growth};
use game_cultivars::{Lot, Strain};
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cohort {
    pub lot: Lot,
    pub progress: Growth,
}
pub fn weights(crops: &Crops, key: &str, cohorts: &[Cohort]) -> Result<Vec<(Strain, u64)>> {
    cohorts
        .iter()
        .map(|c| {
            let crop = crops.expressed(key, Some(&c.lot.strain))?;
            let weight = c
                .progress
                .yield_units(crop.yield_units)
                .saturating_mul(c.lot.quantity);
            Ok((c.lot.strain.clone(), weight))
        })
        .collect()
}
pub fn yield_units(crops: &Crops, key: &str, cohorts: &[Cohort]) -> Result<u64> {
    let seed: u64 = cohorts.iter().map(|c| c.lot.quantity).sum();
    let weighted = weights(crops, key, cohorts)?
        .iter()
        .map(|(_, w)| u128::from(*w))
        .sum::<u128>();
    Ok((weighted / u128::from(seed.max(1))).min(u128::from(u64::MAX)) as u64)
}
pub fn offspring(
    tx: &mut Transaction<'_>,
    person: PersonId,
    parents: &[(Strain, u64)],
    mates: &[Strain],
    quantity: u64,
    crop: &Crop,
    inventory: InventoryId,
) -> Result<()> {
    let clonal = tx
        .get::<game_objects::Objects>()?
        .by_id(crop.seed)?
        .tags
        .contains("clonal");
    let total: u128 = parents.iter().map(|(_, q)| u128::from(*q)).sum();
    if total == 0 || quantity == 0 {
        return Ok(());
    }
    let mut remaining = quantity;
    let mut lots = vec![];
    let mut allocations: Vec<_> = parents
        .iter()
        .filter(|(_, q)| *q > 0)
        .map(|(s, w)| {
            (
                s,
                (u128::from(quantity) * u128::from(*w) / total) as u64,
                u128::from(quantity) * u128::from(*w) % total,
            )
        })
        .collect();
    for (_, q, _) in &allocations {
        remaining -= *q;
    }
    allocations.sort_by_key(|(s, _, remainder)| (std::cmp::Reverse(*remainder), s.id));
    for (_, q, _) in allocations.iter_mut().take(remaining as usize) {
        *q += 1;
    }
    for (mother, q, _) in allocations {
        if q == 0 {
            continue;
        }
        tx.call(game_cultivars::Observe {
            person,
            strain: mother.id,
            yield_units: q,
            duration: crop.growth,
        })?;
        let father = mates
            .get((random(tx.seed(), mother.id, tx.now().0) % mates.len().max(1) as u64) as usize)
            .filter(|s| s.species == mother.species)
            .unwrap_or(mother);
        let strain = tx.call(game_cultivars::Make {
            species: mother.species,
            mother: Some(mother.clone()),
            father: Some(father.clone()),
            clonal,
        })?;
        lots.push(Lot {
            quantity: q,
            strain,
        });
    }
    tx.call(game_cultivars::ExpectLots {
        inventory,
        good: crop.output,
        lots,
    })
}

pub fn sown(
    cultivars: Option<&game_cultivars::Cultivars>,
    inventory: InventoryId,
    good: GoodId,
    quantity: u64,
    at: SimTime,
) -> Vec<Cohort> {
    let mut left = quantity;
    cultivars
        .into_iter()
        .flat_map(|s| s.stocks.get(&inventory))
        .flat_map(|r| r.get(&good))
        .flatten()
        .filter_map(|l| {
            let quantity = left.min(l.quantity);
            left -= quantity;
            (quantity > 0).then(|| Cohort {
                lot: Lot {
                    quantity,
                    strain: l.strain.clone(),
                },
                progress: Growth::new(at),
            })
        })
        .collect()
}
pub fn strains(cohorts: &[Cohort], representative: Option<&Strain>) -> Vec<Strain> {
    if cohorts.is_empty() {
        representative.cloned().into_iter().collect()
    } else {
        cohorts.iter().map(|c| c.lot.strain.clone()).collect()
    }
}

/// A shared harvest waits for the slowest constituent; a representative cannot mature its neighbors.
pub fn duration(
    crops: &Crops,
    key: &str,
    cohorts: &[Cohort],
    fallback: SimDuration,
) -> Result<SimDuration> {
    cohorts.iter().try_fold(fallback, |span, c| {
        Ok(span.max(crops.expressed(key, Some(&c.lot.strain))?.growth))
    })
}
pub fn advance(
    crops: &Crops,
    key: &str,
    cohorts: &[Cohort],
    climate: &game_environment::Climate,
    now: SimTime,
) -> Result<Vec<Cohort>> {
    let mut cohorts = cohorts.to_vec();
    for c in &mut cohorts {
        c.progress
            .advance(climate, &crops.expressed(key, Some(&c.lot.strain))?, now)?;
    }
    Ok(cohorts)
}

pub fn reset(cohorts: &mut [Cohort], progress: &Growth) {
    for c in cohorts {
        c.progress = progress.clone();
    }
}

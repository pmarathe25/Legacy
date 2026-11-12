//! Assign authored seed batch identities to quantities already in physical stock.
use game_lots::{Container, Lot, Place, State};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SeedBatch {
    pub region: u64,
    pub good: String,
    pub batch: u64,
    pub quantity: u64,
}

pub(crate) fn assign(
    custody: &mut State,
    authored: &[SeedBatch],
    crops: &[game_economy_agriculture::Crop],
    resolve: impl Fn(&str) -> Option<u32>,
) -> Result<(), String> {
    let mut rows = authored
        .iter()
        .map(|row| {
            let good =
                resolve(&row.good).ok_or_else(|| format!("unknown seed good {}", row.good))?;
            if row.region == 0
                || row.batch == 0
                || row.quantity == 0
                || !crops.iter().any(|crop| crop.seed == good)
            {
                return Err("invalid authored seed batch".into());
            }
            Ok((row.region, good, row.batch, row.quantity))
        })
        .collect::<Result<Vec<_>, String>>()?;
    rows.sort_unstable_by_key(|row| (row.0, row.1, row.2));
    if rows
        .windows(2)
        .any(|pair| (pair[0].0, pair[0].1, pair[0].2) == (pair[1].0, pair[1].1, pair[1].2))
    {
        return Err("duplicate authored seed batch".into());
    }
    let mut first = 0;
    let mut updates = Vec::new();
    while first < rows.len() {
        let region = rows[first].0;
        let good = rows[first].1;
        let end = first + rows[first..].partition_point(|row| (row.0, row.1) == (region, good));
        let place = Place::Stock { region, good };
        let index = custody
            .containers
            .binary_search_by_key(&place, |row| row.place)
            .map_err(|_| "seed batch requires existing regional stock")?;
        let source = &custody.containers[index];
        let available: u128 = source
            .lots
            .iter()
            .filter(|lot| lot.batch.is_none() && lot.item.is_none())
            .map(|lot| u128::from(lot.quantity))
            .sum();
        let requested: u128 = rows[first..end].iter().map(|row| u128::from(row.3)).sum();
        if requested > available {
            return Err("seed batches exceed existing stock".into());
        }
        let mut lots = Vec::new();
        let mut request = first;
        let mut request_remaining = rows[first].3;
        for original in &source.lots {
            if original.batch.is_some() || original.item.is_some() {
                lots.push(original.clone());
                continue;
            }
            let mut quantity = original.quantity;
            while quantity > 0 && request < end {
                let taken = quantity.min(request_remaining);
                let mut part: Lot = original.clone();
                part.quantity = taken;
                part.batch = Some(rows[request].2);
                lots.push(part);
                quantity -= taken;
                request_remaining -= taken;
                if request_remaining == 0 {
                    request += 1;
                    if request < end {
                        request_remaining = rows[request].3;
                    }
                }
            }
            if quantity > 0 {
                let mut remainder = original.clone();
                remainder.quantity = quantity;
                lots.push(remainder);
            }
        }
        updates.push((index, Container::new(place, source.capacity, lots)?));
        first = end;
    }
    for (index, container) in updates {
        custody.containers[index] = container;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_seed_batches_split_existing_stock_and_fail_atomically() {
        let crop = game_economy_agriculture::Crop {
            id: 1,
            seed: 2,
            seed_quantity: 1,
            output: 3,
            output_quantity: 1,
            growth_ticks: 1,
        };
        let lot = Lot {
            good: 2,
            quantity: 10,
            batch: None,
            item: None,
            expires_at: Some(4),
        };
        let mut state = State {
            next_batch: None,
            tick: 0,
            revision: 0,
            containers: vec![
                Container::new(Place::Stock { region: 1, good: 2 }, 20, vec![lot]).unwrap(),
            ],
        };
        let original = state.clone();
        let invalid = vec![
            SeedBatch {
                region: 1,
                good: "seed".into(),
                batch: 1,
                quantity: 6,
            },
            SeedBatch {
                region: 2,
                good: "seed".into(),
                batch: 2,
                quantity: 1,
            },
        ];
        assert!(
            assign(&mut state, &invalid, std::slice::from_ref(&crop), |name| {
                (name == "seed").then_some(2)
            })
            .is_err()
        );
        assert_eq!(state, original);
        let rows = vec![
            SeedBatch {
                region: 1,
                good: "seed".into(),
                batch: 11,
                quantity: 4,
            },
            SeedBatch {
                region: 1,
                good: "seed".into(),
                batch: 12,
                quantity: 5,
            },
        ];
        assign(&mut state, &rows, &[crop], |name| {
            (name == "seed").then_some(2)
        })
        .unwrap();
        assert_eq!(
            state.containers[0]
                .lots
                .iter()
                .map(|lot| (lot.batch, lot.quantity, lot.expires_at))
                .collect::<Vec<_>>(),
            vec![
                (None, 1, Some(4)),
                (Some(11), 4, Some(4)),
                (Some(12), 5, Some(4))
            ]
        );
    }
}

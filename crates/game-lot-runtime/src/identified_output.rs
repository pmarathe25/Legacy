//! Expand selected production output into stable physical identities.
use game_lots::{Creation, Lot, Store, Transaction};
use rayon::prelude::*;

/// Goods and next never-reused identity are owned by the feature requesting them.
pub struct IdentifiedProduction<'a> {
    pub goods: &'a [u32],
    pub next_item: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProducedItem {
    pub job: u64,
    pub item: u64,
    pub good: u32,
}
pub struct Plan {
    pub items: Vec<ProducedItem>,
    pub next_item: u64,
}

pub fn prepare(
    store: &Store,
    transactions: &mut [Transaction],
    external: &[Transaction],
    config: IdentifiedProduction<'_>,
) -> Result<Plan, String> {
    if config.next_item == 0 || config.goods.par_windows(2).any(|p| p[0] >= p[1]) {
        return Err("invalid identified production configuration".into());
    }
    if config.goods.is_empty()
        || transactions.par_iter().all(|transaction| {
            transaction
                .creations
                .iter()
                .all(|creation| config.goods.binary_search(&creation.lot.good).is_err())
        })
    {
        return Ok(Plan {
            items: Vec::new(),
            next_item: config.next_item,
        });
    }
    if transactions
        .par_windows(2)
        .any(|pair| pair[0].id > pair[1].id)
    {
        transactions.par_sort_unstable_by_key(|row| row.id);
    }
    let counts: Vec<u64> = transactions
        .par_iter()
        .map(|transaction| -> Result<u64, String> {
            transaction
                .creations
                .iter()
                .filter(|creation| config.goods.binary_search(&creation.lot.good).is_ok())
                .try_fold(0u64, |sum, creation| {
                    sum.checked_add(creation.lot.quantity)
                        .ok_or_else(|| "identified output count overflow".to_string())
                })
        })
        .collect::<Result<_, _>>()?;
    let offsets = game_lot_allocation::prefixes(&counts);
    let total = offsets
        .last()
        .zip(counts.last())
        .map_or(0, |(&offset, &count)| offset + u128::from(count));
    if total == 0 {
        return Ok(Plan {
            items: Vec::new(),
            next_item: config.next_item,
        });
    }
    let count = usize::try_from(total).map_err(|_| "too many identified outputs")?;
    let mut allocation_check = Vec::<u64>::new();
    allocation_check
        .try_reserve_exact(count)
        .map_err(|_| "too many identified outputs")?;
    drop(allocation_check);
    let offsets: Vec<usize> = offsets
        .into_par_iter()
        .map(|offset| {
            usize::try_from(offset).map_err(|_| "too many identified outputs".to_string())
        })
        .collect::<Result<_, _>>()?;
    let highest_external = external
        .par_iter()
        .flat_map_iter(|transaction| transaction.creations.iter())
        .filter_map(|creation| creation.lot.item)
        .max();
    let start = match highest_external {
        Some(id) => config
            .next_item
            .max(id.checked_add(1).ok_or("item identity overflow")?),
        None => config.next_item,
    };
    let items = store
        .next_unused_items(start, count)
        .ok_or("item identity overflow")?;
    let next_item = items
        .last()
        .and_then(|id| id.checked_add(1))
        .ok_or("item identity overflow")?;
    let expanded: Vec<_> = transactions
        .par_iter()
        .enumerate()
        .map(|(index, transaction)| -> Result<_, String> {
            let count =
                usize::try_from(counts[index]).map_err(|_| "too many identified outputs")?;
            let mut creations = Vec::new();
            creations
                .try_reserve(
                    transaction
                        .creations
                        .len()
                        .checked_add(count)
                        .ok_or("too many identified outputs")?,
                )
                .map_err(|_| "too many identified outputs")?;
            let mut records = Vec::new();
            records
                .try_reserve(count)
                .map_err(|_| "too many identified outputs")?;
            let mut at = offsets[index];
            for creation in &transaction.creations {
                if config.goods.binary_search(&creation.lot.good).is_err() {
                    creations.push(creation.clone());
                    continue;
                }
                if creation.lot.item.is_some() || creation.lot.expires_at.is_some() {
                    return Err(
                        "identified tool output must be nonperishable and unassigned".into(),
                    );
                }
                let quantity = usize::try_from(creation.lot.quantity)
                    .map_err(|_| "too many identified outputs")?;
                let end = at
                    .checked_add(quantity)
                    .ok_or("too many identified outputs")?;
                let (units, produced): (Vec<_>, Vec<_>) = items[at..end]
                    .par_iter()
                    .map(|&item| {
                        (
                            Creation {
                                id: creation.id,
                                to: creation.to,
                                lot: Lot {
                                    good: creation.lot.good,
                                    quantity: 1,
                                    item: Some(item),
                                    expires_at: None,
                                },
                            },
                            ProducedItem {
                                job: transaction.id,
                                item,
                                good: creation.lot.good,
                            },
                        )
                    })
                    .unzip();
                creations.extend(units);
                records.extend(produced);
                at = end;
            }
            Ok((creations, records))
        })
        .collect::<Result<_, _>>()?;
    let mut records: Vec<ProducedItem> =
        if expanded.len() < 64 && expanded.par_iter().any(|row| row.1.len() >= 2048) {
            expanded
                .par_iter()
                .flat_map(|row| row.1.par_iter().copied())
                .collect()
        } else {
            expanded
                .par_iter()
                .flat_map_iter(|row| row.1.iter().copied())
                .collect()
        };
    if records
        .par_windows(2)
        .any(|pair| pair[0].item > pair[1].item)
    {
        records.par_sort_unstable_by_key(|record| record.item);
    }
    transactions
        .par_iter_mut()
        .zip(expanded.into_par_iter())
        .for_each(|(transaction, (creations, _))| transaction.creations = creations);
    Ok(Plan {
        items: records,
        next_item,
    })
}

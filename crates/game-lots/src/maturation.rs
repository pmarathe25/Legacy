//! Publish completed preparations into the affected containers at an expiry deadline.
use crate::{Container, Creation, State};
use game_lot_model::key;
use game_lot_planning::Draft;
use rayon::prelude::*;

struct Outputs {
    index: usize,
    replacement: Option<Container>,
    receipts: Vec<(u64, bool)>,
}

pub(super) fn stage(
    state: &State,
    draft: &mut Draft,
    at: u64,
    creations: &[Creation],
) -> Result<Vec<(u64, bool)>, String> {
    let mut destinations: Vec<_> = creations
        .par_iter()
        .map(|output| {
            let lot = &output.lot;
            if output.id == 0
                || !output.to.accepts(lot.good)
                || lot.quantity == 0
                || lot.item.is_some()
                || lot.batch == Some(0)
                || lot.expires_at.is_some_and(|expires| expires <= at)
            {
                return Err("invalid matured lot");
            }
            let index = state
                .containers
                .binary_search_by_key(&output.to, |row| row.place)
                .map_err(|_| "unknown maturation destination")?;
            Ok((index, output))
        })
        .collect::<Result<_, &str>>()?;
    destinations.par_sort_unstable_by_key(|(index, output)| (*index, output.id));
    let changes = draft.changes.flat_mut();
    let patches: Vec<_> = destinations
        .par_chunk_by(|left, right| left.0 == right.0)
        .map(|destination| {
            let index = destination[0].0;
            let holder = changes
                .binary_search_by_key(&index, |row| row.0)
                .map_or(&state.containers[index], |position| &changes[position].1);
            let room = u128::from(holder.capacity - holder.used);
            let quantities: Vec<_> = destination.par_iter().map(|(_, output)| output.lot.quantity).collect();
            let prefixes = game_lot_allocation::prefixes(&quantities);
            let receipts: Vec<_> = destination
                .par_iter()
                .zip(prefixes.into_par_iter())
                .map(|(&(_, output), prefix)| (output.id, prefix + u128::from(output.lot.quantity) <= room))
                .collect();
            let mut lots: Vec<_> = destination
                .par_iter()
                .zip(receipts.par_iter())
                .filter(|(_, receipt)| receipt.1)
                .map(|((_, output), _)| output.lot.clone())
                .collect();
            let replacement = (!lots.is_empty()).then(|| {
                let used = holder.used + lots.par_iter().map(|lot| lot.quantity).sum::<u64>();
                lots.extend(holder.lots.iter().cloned());
                lots.par_sort_unstable_by_key(key);
                lots.dedup_by(|later, earlier| {
                    if key(earlier) != key(later) {
                        return false;
                    }
                    earlier.quantity += later.quantity;
                    true
                });
                Container {
                    place: holder.place,
                    capacity: holder.capacity,
                    used,
                    next_expiry: lots.iter().filter_map(|lot| lot.expires_at).min(),
                    lots,
                }
            });
            Outputs {
                index,
                replacement,
                receipts,
            }
        })
        .collect();

    let mut receipts: Vec<_> = patches
        .par_iter()
        .flat_map_iter(|row| row.receipts.iter().copied())
        .collect();
    receipts.par_sort_unstable_by_key(|row| row.0);
    if receipts.par_windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err("duplicate maturation output".into());
    }
    let mut added = patches
        .into_iter()
        .filter_map(|row| row.replacement.map(|holder| (row.index, holder)))
        .peekable();
    let mut base = std::mem::take(changes).into_iter().peekable();
    changes.reserve(base.len());
    while let (Some(old), Some(new)) = (base.peek(), added.peek()) {
        match old.0.cmp(&new.0) {
            std::cmp::Ordering::Less => changes.push(base.next().expect("peeked expiry")),
            std::cmp::Ordering::Equal => {
                base.next();
                changes.push(added.next().expect("peeked maturation"));
            }
            std::cmp::Ordering::Greater => changes.push(added.next().expect("peeked maturation")),
        }
    }
    changes.extend(base);
    changes.extend(added);
    Ok(receipts)
}

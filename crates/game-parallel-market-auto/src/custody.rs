//! Publish disjoint, physically prevalidated holding patches.
use game_parallel_goods_cargo::{GoodsCargo, GoodsHolding};
use rayon::prelude::*;
pub fn apply(
    cargo: &GoodsCargo,
    actors: &[u64],
    changes: Vec<[Option<GoodsHolding>; 2]>,
) -> GoodsCargo {
    let mut actors = actors.to_vec();
    if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
        actors.par_sort_unstable();
    }
    let dense = actors.first().and_then(|first| {
        actors.last().and_then(|last| {
            (last.checked_sub(*first) == Some((actors.len() - 1) as u64)).then_some(*first)
        })
    });
    let mut holdings: Vec<_> = cargo
        .holdings
        .par_iter()
        .filter(|row| {
            dense.map_or_else(
                || actors.binary_search(&row.person).is_err(),
                |first| row.person < first || row.person - first >= actors.len() as u64,
            )
        })
        .cloned()
        .collect();
    holdings.par_extend(
        changes
            .into_par_iter()
            .flat_map_iter(|rows| rows.into_iter().flatten()),
    );
    if holdings
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        holdings.par_sort_unstable_by_key(|row| row.person);
    }
    GoodsCargo {
        capacity: cargo.capacity,
        holdings,
    }
}

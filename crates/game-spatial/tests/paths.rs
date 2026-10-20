use game_core::*;
use game_land::Area;
use game_spatial::*;
use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

#[test]
fn cached_and_uncached_paths_match_an_independent_all_pairs_reference() {
    let mut cache = Routes::default();
    for seed in 0..48 {
        let token = Arc::new(());
        let mut distances = [[u64::MAX; 8]; 8];
        let mut areas = BTreeMap::new();
        for (i, row) in distances.iter_mut().enumerate() {
            row[i] = 0;
            let mut links = BTreeMap::new();
            for (j, cost) in row.iter_mut().enumerate() {
                if i != j && random(seed, i as u64, j as u64).is_multiple_of(3) {
                    *cost = random(seed + 1, i as u64, j as u64) % 20;
                    links.insert(AreaId(j as u64), SimDuration(*cost));
                }
            }
            areas.insert(
                AreaId(i as u64),
                Area {
                    name: i.to_string(),
                    links,
                },
            );
        }
        for k in 0..8 {
            for i in 0..8 {
                for j in 0..8 {
                    let via = distances[i][k].saturating_add(distances[k][j]);
                    distances[i][j] = distances[i][j].min(via);
                }
            }
        }
        for _ in 0..2 {
            for (i, row) in distances.iter().enumerate() {
                for (j, expected) in row.iter().enumerate() {
                    let from = AreaId(i as u64);
                    let to = AreaId(j as u64);
                    let cached = cache.route(Arc::downgrade(&token), from, to, |id| {
                        areas.get(&id).map(Cow::Borrowed)
                    });
                    assert_eq!(
                        cached,
                        route_with(from, to, |id| areas.get(&id).map(Cow::Borrowed))
                    );
                    if *expected == u64::MAX {
                        assert!(cached.is_err());
                        continue;
                    }
                    let route = cached.unwrap();
                    assert_eq!(route.duration.0, *expected);
                    assert_eq!(route.areas.first(), Some(&from));
                    assert_eq!(route.areas.last(), Some(&to));
                    let actual: u64 = route
                        .areas
                        .windows(2)
                        .map(|edge| areas[&edge[0]].links[&edge[1]].0)
                        .sum();
                    assert_eq!(actual, *expected);
                }
            }
        }
    }
}

#[test]
fn cached_failures_and_overflow_do_not_leak_to_new_topologies() {
    let mut cache = Routes::default();
    let from = AreaId(1);
    let to = AreaId(3);
    let old = Arc::new(());
    assert!(
        cache
            .route(Arc::downgrade(&old), from, to, |_| None)
            .is_err()
    );
    let graph = BTreeMap::from([
        (
            from,
            Area {
                name: "a".into(),
                links: [(AreaId(2), SimDuration(u64::MAX))].into(),
            },
        ),
        (
            AreaId(2),
            Area {
                name: "b".into(),
                links: [(to, SimDuration(1))].into(),
            },
        ),
    ]);
    let overflow = Arc::new(());
    assert_eq!(
        cache.route(Arc::downgrade(&overflow), from, to, |id| graph
            .get(&id)
            .map(Cow::Borrowed)),
        Err(Error::Overflow)
    );
    let new = Arc::new(());
    let direct = Area {
        name: "new".into(),
        links: [(to, SimDuration(9))].into(),
    };
    assert_eq!(
        cache
            .route(Arc::downgrade(&new), from, to, |id| (id == from)
                .then_some(Cow::Borrowed(&direct)))
            .unwrap()
            .duration,
        SimDuration(9)
    );
    assert!(
        cache
            .route(Arc::downgrade(&old), from, to, |_| panic!(
                "cached failure lost"
            ))
            .is_err()
    );
}

//! Parallel inheritance of regional stock titles after actual family deaths.
use game_parallel::{Property, PropertyEstate};
use game_parallel_family_lifecycle::Runtime;
use rayon::prelude::*;

pub fn publish(property: &mut Option<Property>, family: Option<&Runtime>, tick: u64) {
    let Some(property) = property else { return };
    let (Some(rules), Some(family)) = (&property.inheritance, family) else {
        return;
    };
    let mut owners: Vec<_> = property
        .claims
        .par_iter()
        .filter_map(|claim| {
            let slot = family.store.locate(claim.owner)?;
            (!family.store.state(slot, false).alive).then_some(claim.owner)
        })
        .collect();
    if owners.is_empty() {
        return;
    }
    owners.par_sort_unstable();
    owners.dedup();
    let mut deaths = family.deaths();
    deaths.par_sort_unstable();
    let owner_index = &owners;
    let mut children: Vec<_> = family
        .store
        .metadata
        .par_iter()
        .enumerate()
        .flat_map_iter(|(slot, row)| {
            let state = family.store.state(slot, false);
            let eligible = state.alive && state.age >= rules.minimum_age;
            row.parents[..row.parent_count]
                .iter()
                .filter_map(move |&parent| {
                    (eligible && owner_index.binary_search(&parent).is_ok())
                        .then_some((parent, row.id))
                })
        })
        .collect();
    children.par_sort_unstable();
    let mut heirs: Vec<_> = owners
        .par_iter()
        .filter_map(|&owner| {
            let partner = (rules.prefer_partner && deaths.binary_search(&owner).is_ok())
                .then(|| family.previous_partner(owner))
                .flatten()
                .filter(|&person| {
                    family.store.locate(person).is_some_and(|slot| {
                        let state = family.store.state(slot, false);
                        state.alive && state.age >= rules.minimum_age
                    })
                });
            let first = children.partition_point(|row| row.0 < owner);
            let child = children
                .get(first)
                .filter(|row| row.0 == owner)
                .map(|row| row.1);
            partner.or(child).map(|heir| (owner, heir))
        })
        .collect();
    if heirs.is_empty() {
        return;
    }
    if heirs.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        heirs.par_sort_unstable_by_key(|row| row.0);
    }
    let mut events: Vec<_> = property
        .claims
        .par_iter_mut()
        .filter_map(|claim| {
            let i = heirs.binary_search_by_key(&claim.owner, |row| row.0).ok()?;
            let event = PropertyEstate {
                tick,
                region: claim.region,
                good: claim.good,
                previous_owner: claim.owner,
                heir: heirs[i].1,
            };
            claim.owner = event.heir;
            claim.delegates.clear();
            Some(event)
        })
        .collect();
    events.par_sort_unstable_by_key(|event| (event.region, event.good));
    let keys: Vec<_> = events
        .par_iter()
        .map(|event| (event.region, event.good))
        .collect();
    property.offers = std::mem::take(&mut property.offers)
        .into_par_iter()
        .filter(|offer| keys.binary_search(&(offer.region, offer.good)).is_err())
        .collect();
    property.estates.par_extend(events);
}

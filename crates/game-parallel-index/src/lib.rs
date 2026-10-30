//! Shared projections of physical residents and travelers.
pub mod membership;
use game_parallel_people::Region;
use game_parallel_production_model::PersonSnapshot;
use game_parallel_transport::Traveler;
use rayon::prelude::*;
/// Build the sorted physical snapshot shared by owner adapters.
pub fn snapshot_people(regions: &[Region], travelers: &[Traveler]) -> Vec<PersonSnapshot> {
    let mut people: Vec<_> = regions
        .par_iter()
        .flat_map(|r| {
            r.people
                .par_iter()
                .map(move |p| (p.id, Some(r.id), p.health, p.hunger))
        })
        .chain(
            travelers
                .par_iter()
                .map(|t| (t.person.id, None, t.person.health, t.person.hunger)),
        )
        .collect();
    if people.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        people.par_sort_unstable_by_key(|person| person.0);
    }
    people
}

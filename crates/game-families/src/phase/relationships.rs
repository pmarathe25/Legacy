//! Elapsed partnership decisions over the opening personal and physical views.
use super::*;

#[derive(Default)]
pub(super) struct Draft {
    pub changes: Vec<(u64, Option<u64>)>,
    pub events: Vec<FamilyEvent>,
    pub formed: u64,
}

pub(super) fn review(
    family: &Families,
    physical: &impl game_family_model::PhysicalView,
    dependents: &(impl Fn(u64) -> u64 + Sync),
    seed: u64,
    tick: u64,
    now: u128,
) -> Draft {
    let members = &family.members;
    let index = OrderedMemberIndex::new(members);
    let mut separated_pairs: Vec<_> = members
        .par_iter()
        .filter_map(|person| {
            let partner = index.get(&person.partner?)?;
            if !person.alive
                || !partner.alive
                || physical.controlled(person.id)
                || physical.read(person.id).health == 0
                || physical.read(partner.id).health == 0
            {
                return None;
            }
            let fertile_until = family.rules.fertile_until;
            let reassess = person.reconsider_partnership
                && person.reproduction_allowed
                && family.age_at(person, now) < fertile_until
                && (!partner.reproduction_allowed || family.age_at(partner, now) >= fertile_until);
            (reassess || !accepts_mate(person, partner, physical, seed))
                .then_some((person.id.min(partner.id), person.id.max(partner.id)))
        })
        .collect();
    separated_pairs.par_sort_unstable();
    separated_pairs.dedup();
    let mut separated: Vec<_> = separated_pairs
        .par_iter()
        .flat_map_iter(|&(a, b)| [a, b])
        .collect();
    separated.par_sort_unstable();
    let mut local: Vec<_> = members
        .par_iter()
        .filter_map(|person| {
            if !person.alive
                || physical.controlled(person.id)
                || (person.partner.is_some() && separated.binary_search(&person.id).is_err())
                || family.age_at(person, now) < family.rules.adult_age
            {
                return None;
            }
            let state = physical.read(person.id);
            let region = state.region?;
            (state.health > 0).then_some((region, physical.contact_partition(person.id), person))
        })
        .collect();
    local.par_sort_unstable_by_key(|(region, partition, person)| (*region, *partition, person.id));
    let entropy = elapsed_entropy(now);
    let formed: Vec<_> = local
        .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
        .flat_map_iter(|group| {
            subjective_partnerships(
                group, &index, physical, dependents, seed, entropy, family.traits.as_deref(),
            )
        })
        .collect();
    let mut changes: Vec<_> = formed
        .par_iter()
        .flat_map_iter(|&(a, b, _)| [(a, Some(b)), (b, Some(a))])
        .collect();
    changes.par_sort_unstable_by_key(|row| row.0);
    let unmatched: Vec<_> = separated
        .par_iter()
        .filter_map(|&person| {
            changes
                .binary_search_by_key(&person, |row| row.0)
                .is_err()
                .then_some((person, None))
        })
        .collect();
    changes.par_extend(unmatched);
    changes.par_sort_unstable_by_key(|row| row.0);
    let mut events: Vec<_> = separated_pairs
        .par_iter()
        .map(|&(a, b)| event(tick, "separation", vec![a, b], physical.read(a).region))
        .collect();
    events.par_extend(
        formed
            .par_iter()
            .map(|&(a, b, region)| event(tick, "partnership", vec![a, b], Some(region))),
    );
    Draft {
        changes,
        events,
        formed: formed.len() as u64,
    }
}

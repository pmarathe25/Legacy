//! Build institutions from independent local proponents, grouped by region.
use game_parallel::{CivicAuto, InstitutionBody as Body, Institutions, Opinions};
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use std::collections::HashSet;

/// Each enabled proponent volunteers for at most one local body. A group's
/// proposed trade rule is chosen by majority, with ties favoring openness.
pub fn prepare(
    policy: &CivicAuto,
    institutions: &Institutions,
    opinions: Option<&Opinions>,
    people: &[PersonSnapshot],
    busy: &[u64],
) -> Result<Vec<Body>, &'static str> {
    let Some(rules) = &policy.founding else {
        return Ok(Vec::new());
    };
    let occupied: HashSet<_> = institutions
        .bodies
        .iter()
        .filter(|body| !body.members.is_empty())
        .map(|body| body.region)
        .collect();
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    let needs_trust = policy
        .policies
        .par_iter()
        .any(|policy| policy.trade_trust_target.is_some());
    let opinions = opinions.filter(|_| needs_trust).map(Opinions::index);
    let mut volunteers: Vec<_> = policy
        .policies
        .par_iter()
        .filter_map(|policy| {
            if !policy.enabled
                || !policy.propose_trade
                || busy.binary_search(&policy.person).is_ok()
            {
                return None;
            }
            let (_, region, health, _) = people[*person_index(people, policy.person).as_ref()?];
            let region = region.filter(|region| !occupied.contains(region) && health > 0)?;
            let open = policy
                .trade_trust_target
                .map_or(policy.prefer_open, |target| {
                    Some(
                        opinions
                            .as_ref()
                            .map_or(0, |index| index.trust(policy.person, target))
                            >= policy.trade_trust_threshold,
                    )
                })?;
            Some((region, policy.person, open))
        })
        .collect();
    volunteers.par_sort_unstable_by_key(|row| (row.0, row.1));
    let mut bodies: Vec<_> = volunteers
        .par_chunk_by(|a, b| a.0 == b.0)
        .filter_map(|group| {
            if group.len() < rules.minimum_founders as usize {
                return None;
            }
            let open = group.par_iter().filter(|row| row.2).count();
            Some(Body {
                id: 0,
                region: group[0].0,
                chair: Some(group[0].1),
                members: group.par_iter().map(|row| row.1).collect(),
                quorum: rules.quorum,
                trade_open: open >= group.len() - open,
            })
        })
        .collect();
    if bodies.is_empty() {
        return Ok(bodies);
    }
    bodies.par_sort_unstable_by_key(|body| body.region);
    let first = institutions
        .bodies
        .par_iter()
        .map(|body| body.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("institution ID overflow")?;
    first
        .checked_add((bodies.len() - 1) as u64)
        .ok_or("institution ID overflow")?;
    bodies
        .par_iter_mut()
        .enumerate()
        .for_each(|(index, body)| body.id = first + index as u64);
    Ok(bodies)
}

#[cfg(test)]
#[path = "../tests/support/founding.rs"]
mod tests;

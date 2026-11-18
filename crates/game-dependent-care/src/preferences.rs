//! Directed priorities for looking after a particular child.
use crate::Child;
use game_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub child: u64,
    pub value: i32,
    #[serde(default)]
    pub affinity_weight: i32,
    #[serde(default)]
    pub fear_weight: i32,
    #[serde(default)]
    pub obligation_weight: i32,
    #[serde(default)]
    pub hunger_weight: i32,
    #[serde(default)]
    pub disregard_claims: bool,
}

#[derive(Clone, Copy, Default)]
pub struct Feelings {
    pub affinity: i32,
    pub fear: u64,
    pub obligation: u64,
}

#[derive(Clone, Copy)]
pub(super) struct Assignment {
    pub child: u64,
    pub caregiver: usize,
    pub disregard_claims: bool,
}

pub(super) fn assignments(
    preferences: &[Preference],
    children: &[Child],
    people: &[PersonSnapshot],
    busy: &[u64],
    clock: (u64, u64),
    colocated: &(impl Fn(usize, usize) -> bool + Sync),
    feelings: &(impl Fn(u64, u64) -> Feelings + Sync),
) -> Vec<Assignment> {
    let (seed, tick) = clock;
    let mut offers: Vec<_> = preferences
        .par_iter()
        .filter_map(|preference| {
            let child = preference.child;
            let person = preference.person;
            let caregiver = person_index(people, person)?;
            let child_slot = person_index(people, child)?;
            let (_, _, health, hunger) = people[caregiver];
            if health == 0
                || busy.binary_search(&person).is_ok()
                || children.binary_search_by_key(&child, |row| row.id).is_err()
                || !colocated(child_slot, caregiver)
            {
                return None;
            }
            let feelings = feelings(person, child);
            let score = i128::from(preference.value)
                + i128::from(preference.affinity_weight) * i128::from(feelings.affinity)
                + i128::from(preference.fear_weight) * i128::from(feelings.fear)
                + i128::from(preference.obligation_weight) * i128::from(feelings.obligation)
                + i128::from(preference.hunger_weight) * i128::from(hunger);
            (score > 0).then_some((
                Reverse(score),
                Assignment {
                    child,
                    caregiver,
                    disregard_claims: preference.disregard_claims,
                },
            ))
        })
        .collect();
    // Each person offers the visit they value most. Resolve duplicate offers
    // for a child separately. No person can be assigned to a second visit.
    offers.par_sort_unstable_by_key(|&(value, row)| (row.caregiver, value, row.child));
    offers.dedup_by_key(|(_, row)| row.caregiver);
    offers.par_sort_unstable_by_key(|&(_, row)| {
        let person = people[row.caregiver].0;
        (
            row.child,
            game_genetics::random(seed ^ row.child, person, tick),
            person,
        )
    });
    offers.dedup_by_key(|(_, row)| row.child);
    offers.into_par_iter().map(|(_, row)| row).collect()
}

//! Personal food requests to people the requester has actually encountered.
use crate::{Giving, Person};
use game_parallel_economy_person::travel::Colocation;
use game_parallel_lots::{Place, View};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::Good;
use game_parallel_social::Relationship;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub enabled: bool,
    pub min_hunger: u32,
    pub retry_ticks: u64,
    pub request_lifetime: u64,
    #[serde(default)]
    pub min_familiarity: u32,
    #[serde(default)]
    pub min_affinity: Option<i32>,
    #[serde(default)]
    pub prefer_affinity: bool,
    #[serde(default)]
    pub next_review: u64,
    #[serde(default)]
    pub last_target: Option<u64>,
}

pub fn validate(rows: &[Policy], people: &HashSet<u64>) -> Result<(), String> {
    let mut ids: Vec<_> = rows.par_iter().map(|row| row.person).collect();
    ids.par_sort_unstable();
    if ids.par_windows(2).any(|pair| pair[0] == pair[1])
        || rows.par_iter().any(|row| {
            !people.contains(&row.person)
                || row.retry_ticks == 0
                || row.request_lifetime == 0
                || row
                    .last_target
                    .is_some_and(|target| target == row.person || !people.contains(&target))
        })
    {
        return Err("invalid acquaintance food request preference".into());
    }
    Ok(())
}

pub struct Context<'a> {
    pub people: &'a [Person],
    pub goods: &'a [Good],
    pub view: &'a View,
    pub busy: &'a [u64],
    pub relationships: &'a [Relationship],
    pub opinions: Option<&'a Opinions>,
    pub locations: &'a Colocation,
    pub seed: u64,
    pub tick: u64,
}

/// Each requester chooses one locally observed acquaintance; the recipient decides later.
pub fn prepare(owner: &mut Giving, context: Context<'_>) -> Result<Vec<u64>, String> {
    if owner.acquaintance_asking.is_empty() || context.relationships.is_empty() {
        return Ok(Vec::new());
    }
    let mut preferences: Vec<_> = owner.acquaintance_asking.par_iter().collect();
    preferences.par_sort_unstable_by_key(|row| row.person);
    let explicit: HashSet<_> = owner.asking.par_iter().map(|row| row.person).collect();
    let pending: HashSet<_> = owner.requests.par_iter().map(|row| row.requester).collect();
    let food: HashMap<_, _> = context
        .goods
        .iter()
        .filter_map(|good| good.food.as_ref().map(|food| (good.id, food.portion)))
        .collect();
    let opinions = context.opinions.map(Opinions::index);
    let mut choices: Vec<_> = context
        .relationships
        .par_iter()
        .flat_map_iter(|relationship| {
            [
                (relationship.a, relationship.b),
                (relationship.b, relationship.a),
            ]
            .into_iter()
            .filter_map(|(actor_id, target_id)| {
                let preference = preferences
                    .binary_search_by_key(&actor_id, |row| row.person)
                    .ok()
                    .map(|i| preferences[i])?;
                if !preference.enabled
                    || context.tick < preference.next_review
                    || relationship.familiarity < preference.min_familiarity
                    || explicit.contains(&actor_id)
                    || pending.contains(&actor_id)
                    || context.busy.binary_search(&actor_id).is_ok()
                {
                    return None;
                }
                let actor = context
                    .people
                    .binary_search_by_key(&actor_id, |row| row.id)
                    .ok()
                    .map(|i| &context.people[i])?;
                let target = context
                    .people
                    .binary_search_by_key(&target_id, |row| row.id)
                    .ok()
                    .map(|i| &context.people[i])?;
                if actor.controlled
                    || actor.health == 0
                    || actor.hunger < preference.min_hunger
                    || target.health == 0
                    || context
                        .locations
                        .region((actor.id, actor.region), (target.id, target.region))
                        .is_none()
                {
                    return None;
                }
                let holding = context.view.container(Place::Person(actor.id))?;
                if holding.goods.iter().any(|row| {
                    food.get(&row.good)
                        .is_some_and(|portion| row.quantity >= *portion)
                }) {
                    return None;
                }
                let affinity = opinions
                    .as_ref()
                    .map_or(0, |index| index.affinity(actor_id, target_id));
                if preference
                    .min_affinity
                    .is_some_and(|minimum| affinity < minimum)
                {
                    return None;
                }
                Some((
                    actor_id,
                    preference.last_target == Some(target_id),
                    if preference.prefer_affinity {
                        -i64::from(affinity)
                    } else {
                        0
                    },
                    game_parallel_genetics::random(
                        context.seed ^ actor_id,
                        target_id,
                        context.tick,
                    ),
                    target_id,
                    preference.retry_ticks,
                    preference.request_lifetime,
                ))
            })
        })
        .collect();
    choices.par_sort_unstable_by_key(|row| (row.0, row.1, row.2, row.3, row.4));
    let updates: Vec<_> = choices
        .par_chunk_by(|a, b| a.0 == b.0)
        .map(|group| {
            let row = group[0];
            Ok((
                row.0,
                row.4,
                context
                    .tick
                    .checked_add(row.5)
                    .ok_or("food request review overflow")?,
                context
                    .tick
                    .checked_add(row.6)
                    .ok_or("food request expiry overflow")?,
            ))
        })
        .collect::<Result<_, String>>()?;
    owner
        .acquaintance_asking
        .par_iter_mut()
        .for_each(|preference| {
            if let Ok(i) = updates.binary_search_by_key(&preference.person, |row| row.0) {
                preference.last_target = Some(updates[i].1);
                preference.next_review = updates[i].2;
            }
        });
    owner
        .requests
        .par_extend(updates.par_iter().map(|row| crate::FoodRequest {
            requester: row.0,
            target: row.1,
            expires_at: row.3,
        }));
    Ok(updates.into_par_iter().map(|row| row.0).collect())
}

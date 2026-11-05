//! Bounded personal decisions to initiate tactical conflict.
use super::{Battle, Draft, Encounter, Fighter, Order, Placement, Plan, Point, Response, Runtime};
use game_parallel_economy_person::Person;
use game_parallel_opinions::Opinions;
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Choice {
    actor: u64,
    target: u64,
    region: u64,
}

pub struct Selection {
    choices: Vec<Choice>,
    starts: Vec<(u64, [Point; 2])>,
    pub actors: Vec<u64>,
}

/// Select disjoint encounters from prior personal opinions. No target consents.
pub fn select(
    runtime: Option<&Runtime>,
    opinions: Option<&Opinions>,
    people: &[Person],
    unavailable: &[u64],
    unable_to_initiate: &[u64],
    tick: u64,
) -> Option<Selection> {
    let runtime = runtime?;
    let opinions = opinions.filter(|owner| {
        owner.sensitivities.par_iter().any(|row| {
            row.confrontation
                .as_ref()
                .is_some_and(|policy| policy.enabled && tick.is_multiple_of(policy.interval))
        })
    })?;
    let mut known: Vec<_> = opinions.records.par_iter().collect();
    known.par_sort_unstable_by_key(|row| (row.observer, row.trust, row.target));
    let mut starts: Vec<_> = runtime
        .state
        .tactics
        .maps
        .par_iter()
        .filter_map(|map| {
            let mut points = Vec::with_capacity(2);
            'outer: for x in 0..=map.rules.width.min(63) {
                for y in 0..=map.rules.height.min(63) {
                    let point = Point { x, y };
                    if map.open(point) {
                        points.push(point);
                    }
                    if points.len() == 2 {
                        break 'outer;
                    }
                }
            }
            (points.len() == 2).then(|| (map.region, [points[0], points[1]]))
        })
        .collect();
    starts.par_sort_unstable_by_key(|row| row.0);
    let find = |id| {
        people
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .map(|i| &people[i])
    };
    let mut choices: Vec<_> = opinions
        .sensitivities
        .par_iter()
        .filter_map(|preference| {
            let policy = preference.confrontation.as_ref()?;
            let actor = find(preference.person)?;
            if !policy.enabled
                || !tick.is_multiple_of(policy.interval)
                || actor.controlled
                || actor.health == 0
                || unavailable.binary_search(&actor.id).is_ok()
                || unable_to_initiate.binary_search(&actor.id).is_ok()
            {
                return None;
            }
            let region = actor.region?;
            if starts.binary_search_by_key(&region, |row| row.0).is_err() {
                return None;
            }
            let start = known.partition_point(|row| row.observer < actor.id);
            let end = known.partition_point(|row| row.observer <= actor.id);
            known[start..end].iter().take(32).find_map(|opinion| {
                if opinion.trust > policy.trust_at_most
                    || opinion.fear > policy.fear_at_most
                    || unavailable.binary_search(&opinion.target).is_ok()
                {
                    return None;
                }
                let target = find(opinion.target)?;
                (target.health > 0 && target.region == Some(region) && target.id != actor.id)
                    .then_some(Choice {
                        actor: actor.id,
                        target: target.id,
                        region,
                    })
            })
        })
        .collect();
    if choices.is_empty() {
        return None;
    }
    choices.par_sort_unstable_by_key(|row| (row.actor, row.target));
    let mut claims: Vec<_> = choices
        .par_iter()
        .enumerate()
        .flat_map_iter(|(i, row)| [(row.actor, i), (row.target, i)])
        .collect();
    claims.par_sort_unstable();
    let choices: Vec<_> = choices
        .into_par_iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let a = claims.partition_point(|claim| claim.0 < row.actor);
            let b = claims.partition_point(|claim| claim.0 < row.target);
            (claims[a].1 == i && claims[b].1 == i).then_some(row)
        })
        .collect();
    if choices.is_empty() {
        return None;
    }
    let mut actors: Vec<_> = choices
        .par_iter()
        .flat_map_iter(|row| [row.actor, row.target])
        .collect();
    actors.par_sort_unstable();
    Some(Selection {
        choices,
        starts,
        actors,
    })
}

impl Draft {
    /// Start surviving selected encounters after the old round, without applying damage yet.
    pub fn add_confrontations(
        &mut self,
        selection: &Selection,
        people: &[Person],
    ) -> Result<(), String> {
        let find = |id| {
            people
                .binary_search_by_key(&id, |p| p.id)
                .ok()
                .map(|i| &people[i])
        };
        let choices: Vec<_> = selection
            .choices
            .par_iter()
            .copied()
            .filter(|row| {
                find(row.actor).zip(find(row.target)).is_some_and(|(a, b)| {
                    a.health > 0
                        && b.health > 0
                        && a.region == Some(row.region)
                        && b.region == Some(row.region)
                })
            })
            .collect();
        if choices.is_empty() {
            return Ok(());
        }
        let first = self.state.conflict.next_id;
        let next = first
            .checked_add(choices.len() as u64)
            .ok_or("encounter identity overflow")?;
        let mut reactions: Vec<_> = self.state.reactions.par_iter().collect();
        reactions.par_sort_unstable_by_key(|row| row.person);
        let (encounters, battles): (Vec<_>, Vec<_>) = choices
            .par_iter()
            .enumerate()
            .map(|(i, row)| {
                let target = find(row.target).expect("selected target");
                let response = reactions
                    .binary_search_by_key(&row.target, |r| r.person)
                    .ok()
                    .map(|i| &reactions[i].response);
                let order = if target.controlled {
                    Order::Defend
                } else {
                    match response {
                        Some(Response::Fight) => Order::Attack { target: row.actor },
                        Some(Response::Retreat) => Order::Retreat,
                        _ => Order::Defend,
                    }
                };
                let mut fighters = vec![
                    Fighter {
                        person: row.actor,
                        side: 1,
                        order: Order::Attack { target: row.target },
                    },
                    Fighter {
                        person: row.target,
                        side: 2,
                        order,
                    },
                ];
                fighters.sort_unstable_by_key(|f| f.person);
                let points = selection.starts[selection
                    .starts
                    .binary_search_by_key(&row.region, |s| s.0)
                    .expect("selected map")]
                .1;
                let mut placements = vec![
                    Placement {
                        person: row.actor,
                        position: points[0],
                        last_move: None,
                    },
                    Placement {
                        person: row.target,
                        position: points[1],
                        last_move: None,
                    },
                ];
                placements.sort_unstable_by_key(|p| p.person);
                let id = first + i as u64;
                (
                    Encounter {
                        id,
                        region: row.region,
                        fighters,
                    },
                    Battle {
                        encounter: id,
                        region: row.region,
                        people: placements,
                    },
                )
            })
            .unzip();
        self.state.conflict.next_id = next;
        self.state.conflict.encounters.par_extend(encounters);
        self.state.tactics.battles.par_extend(battles);
        self.replacement = Some((
            Some(Plan::compile(&self.state.conflict).map_err(str::to_owned)?),
            self.state.tactics.index().map_err(str::to_owned)?,
            Some(self.state.conflict.actors()),
        ));
        Ok(())
    }
}

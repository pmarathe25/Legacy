//! Atomic departure admission over persistent blocks.
use game_parallel::{Move, Person, World};
use game_parallel_block_data::{Block, Journey};
use rayon::prelude::*;
use std::{
    collections::{BTreeMap, HashMap},
    mem,
};
enum Plan {
    Whole(Journey),
    Split(Vec<Block>),
}
pub fn check_player_move(world: &World, moves: &[Move]) -> Result<(), String> {
    if world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.suspended_person(world.tick))
        .is_some_and(|person| moves.par_iter().any(|request| request.person == person))
    {
        return Err("player control is suspended".into());
    }
    Ok(())
}
/// Validate a departure batch, then update or split affected blocks.
pub fn move_people(blocks: &mut Vec<Block>, world: &World, moves: &[Move]) -> Result<(), String> {
    if moves.is_empty() {
        return Ok(());
    }
    // One parallel scan detects ordering, adjacent duplicates and uniform routes.
    let flags = moves
        .par_windows(2)
        .map(|pair| {
            u8::from(pair[0].person > pair[1].person)
                | (u8::from(pair[0].person == pair[1].person) << 1)
                | (u8::from(pair[0].destination != pair[1].destination) << 2)
        })
        .reduce(|| 0, |a, b| a | b);
    if flags & 2 != 0 {
        return Err("duplicate departure person".into());
    }
    let mut ordered = None;
    if flags & 1 != 0 {
        let mut rows = moves.to_vec();
        rows.par_sort_unstable_by_key(|row| row.person);
        if rows
            .par_windows(2)
            .any(|pair| pair[0].person == pair[1].person)
        {
            return Err("duplicate departure person".into());
        }
        ordered = Some(rows);
    }
    let requests = ordered.as_deref().unwrap_or(moves);
    let uniform = flags & 4 == 0;
    let dense = requests.first().and_then(|first| {
        requests.last().and_then(|last| {
            (last.person.checked_sub(first.person) == Some((requests.len() - 1) as u64))
                .then_some(first.person)
        })
    });
    let lookup = |id: u64| -> Option<&Move> {
        dense
            .and_then(|first| id.checked_sub(first))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&i| i < requests.len())
            .or_else(|| {
                if dense.is_none() {
                    requests.binary_search_by_key(&id, |r| r.person).ok()
                } else {
                    None
                }
            })
            .map(|i| &requests[i])
    };
    let regions: HashMap<_, _> = world
        .regions
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id, i))
        .collect();
    let routes: HashMap<_, _> = world
        .routes
        .iter()
        .map(|r| {
            (
                (r.source, r.destination),
                (regions[&r.destination], r.ticks),
            )
        })
        .collect();
    let plans: Vec<Option<(Plan, usize)>> = blocks
        .par_iter()
        .map(|block| -> Result<_, String> {
            if block.journey.is_some() {
                return Ok(None);
            }
            // A dense whole-block departure can retain the columns in place.
            if let Some(first) = dense
                && let (Some(&head), Some(&tail)) = (block.ids.first(), block.ids.last())
                && tail.checked_sub(head) == Some((block.ids.len() - 1) as u64)
                && let Some(start) = head
                    .checked_sub(first)
                    .and_then(|v| usize::try_from(v).ok())
                && let Some(end) = start.checked_add(block.ids.len())
                && end <= requests.len()
            {
                if block.health.contains(&0) {
                    return Err("deceased person cannot travel".into());
                }
                let destination = requests[start].destination;
                if uniform
                    || requests[start..end]
                        .iter()
                        .all(|request| request.destination == destination)
                {
                    let source = world.regions[block.region].id;
                    let &(destination_index, remaining) =
                        routes.get(&(source, destination)).ok_or("no route")?;
                    return Ok(Some((
                        Plan::Whole(Journey {
                            source,
                            destination,
                            destination_index,
                            remaining,
                        }),
                        block.ids.len(),
                    )));
                }
            }
            let count = block.ids.iter().filter(|&&id| lookup(id).is_some()).count();
            if count == 0 {
                return Ok(None);
            }
            let source = world.regions[block.region].id;
            let mut same: Option<Journey> = None;
            for i in 0..block.ids.len() {
                if let Some(request) = lookup(block.ids[i]) {
                    if block.health[i] == 0 {
                        return Err("deceased person cannot travel".into());
                    }
                    let &(destination_index, remaining) = routes
                        .get(&(source, request.destination))
                        .ok_or("no route")?;
                    let next = Journey {
                        source,
                        destination: request.destination,
                        destination_index,
                        remaining,
                    };
                    if same.is_none() {
                        same = Some(next);
                    } else if same.is_some_and(|old| old.destination != next.destination) {
                        same = None;
                        break;
                    }
                }
            }
            if count == block.ids.len()
                && let Some(journey) = same
            {
                return Ok(Some((Plan::Whole(journey), count)));
            }
            let mut staying = Vec::with_capacity(block.ids.len() - count);
            let mut groups: BTreeMap<(u64, usize, u32), Vec<Person>> = BTreeMap::new();
            for i in 0..block.ids.len() {
                let person = Person {
                    id: block.ids[i],
                    health: block.health[i],
                    hunger: block.hunger[i],
                };
                if let Some(request) = lookup(person.id) {
                    if person.health == 0 {
                        return Err("deceased person cannot travel".into());
                    }
                    let &(destination_index, remaining) = routes
                        .get(&(source, request.destination))
                        .ok_or("no route")?;
                    groups
                        .entry((request.destination, destination_index, remaining))
                        .or_default()
                        .push(person);
                } else {
                    staying.push(person);
                }
            }
            let mut split = Vec::new();
            if !staying.is_empty() {
                split.push(Block::new(block.region, &staying));
            }
            for ((destination, destination_index, remaining), people) in groups {
                let mut travel = Block::new(block.region, &people);
                travel.journey = Some(Journey {
                    source,
                    destination,
                    destination_index,
                    remaining,
                });
                split.push(travel);
            }
            Ok(Some((Plan::Split(split), count)))
        })
        .collect::<Result<_, _>>()?;
    if plans
        .par_iter()
        .map(|plan| plan.as_ref().map_or(0, |(_, count)| *count))
        .sum::<usize>()
        != moves.len()
    {
        return Err("unknown or unavailable person".into());
    }
    *blocks = mem::take(blocks)
        .into_par_iter()
        .zip(plans.into_par_iter())
        .flat_map_iter(|(mut block, plan)| match plan {
            None => vec![block],
            Some((Plan::Whole(journey), _)) => {
                block.journey = Some(journey);
                vec![block]
            }
            Some((Plan::Split(blocks), _)) => blocks,
        })
        .collect();
    if blocks
        .par_windows(2)
        .any(|pair| (pair[0].region, pair[0].ids[0]) > (pair[1].region, pair[1].ids[0]))
    {
        blocks.par_sort_unstable_by_key(|block| (block.region, block.ids[0]));
    }
    Ok(())
}

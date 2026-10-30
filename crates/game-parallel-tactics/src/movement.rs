//! Sparse movement validation followed by infallible position/index publication.
use super::{Index, Move, Tactics};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
impl Index {
    /// Membership cleanup removes actors but never changes surviving positions.
    pub fn membership_changed(&mut self, state: &Tactics, changed: &[u64], removed: &[u64]) {
        for &id in removed {
            if let Some(slot) = self.locate(id) {
                self.actors[slot].active = false;
            }
        }
        let changed: HashSet<_> = changed.iter().copied().collect();
        let slots: HashMap<_, _> = state
            .battles
            .iter()
            .enumerate()
            .map(|(i, battle)| (battle.encounter, i))
            .collect();
        self.battles.retain(|id, battle| {
            let Some(&slot) = slots.get(id) else {
                return false;
            };
            battle.slot = slot;
            if changed.contains(id) {
                battle.slots = None;
            }
            true
        });
    }
    /// The index must belong to this committed tactical state. Failed batches
    /// may refresh disposable caches, but cannot change placements or budgets.
    pub fn move_batch(
        &mut self,
        state: &mut Tactics,
        moves: &[Move],
        tick: u64,
    ) -> Result<(), &'static str> {
        let mut requested = HashMap::with_capacity(moves.len());
        let mut destinations = HashSet::with_capacity(moves.len());
        for command in moves {
            if requested
                .insert((command.encounter, command.person), command.destination)
                .is_some()
            {
                return Err("duplicate tactical movement");
            }
            if !destinations.insert((
                command.encounter,
                command.destination.x,
                command.destination.y,
            )) {
                return Err("occupied tactical position");
            }
            let indexed = self
                .battles
                .get_mut(&command.encounter)
                .ok_or("unknown tactical encounter")?;
            if indexed.slots.is_none() {
                let battle = &state.battles[indexed.slot];
                indexed.occupied = battle
                    .people
                    .iter()
                    .map(|person| ((person.position.x, person.position.y), person.person))
                    .collect();
                indexed.slots = Some(
                    battle
                        .people
                        .iter()
                        .enumerate()
                        .map(|(i, person)| (person.person, i))
                        .collect(),
                );
            }
        }
        let dense = moves.len() >= 4096 && moves.len() > self.actors.len() / 4;
        let validate = |command: &Move| {
            let indexed = &self.battles[&command.encounter];
            let &slot = indexed
                .slots
                .as_ref()
                .unwrap()
                .get(&command.person)
                .ok_or("unknown tactical participant")?;
            let person = &state.battles[indexed.slot].people[slot];
            if person.last_move == Some(tick)
                || !self.maps[indexed.map].can_move(person.position, command.destination)
            {
                return Err("invalid tactical movement");
            }
            if let Some(&occupant) = indexed
                .occupied
                .get(&(command.destination.x, command.destination.y))
                && occupant != command.person
                && !requested.contains_key(&(command.encounter, occupant))
            {
                return Err("occupied tactical position");
            }
            Ok((indexed.slot, slot, person.position))
        };
        let edits: Vec<_> = if dense {
            moves.par_iter().map(validate).collect::<Result<_, _>>()?
        } else {
            moves.iter().map(validate).collect::<Result<_, _>>()?
        };
        if dense {
            let mut changes: Vec<_> = edits
                .iter()
                .zip(moves)
                .map(|(&(battle, slot, _), command)| (battle, slot, command.destination))
                .collect();
            changes.par_sort_unstable_by_key(|change| (change.0, change.1));
            for (battle, state) in state.battles.iter_mut().enumerate() {
                let start = changes.partition_point(|change| change.0 < battle);
                let end = changes.partition_point(|change| change.0 <= battle);
                let changes = &changes[start..end];
                if changes.is_empty() {
                    continue;
                }
                state
                    .people
                    .par_chunks_mut(2048)
                    .enumerate()
                    .for_each(|(chunk, people)| {
                        let offset = chunk * 2048;
                        let start = changes.partition_point(|change| change.1 < offset);
                        let end =
                            changes.partition_point(|change| change.1 < offset + people.len());
                        for &(_, slot, position) in &changes[start..end] {
                            people[slot - offset].position = position;
                            people[slot - offset].last_move = Some(tick);
                        }
                    });
            }
            self.battles.par_iter_mut().for_each(|(_, indexed)| {
                if changes
                    .binary_search_by_key(&indexed.slot, |change| change.0)
                    .is_err()
                {
                    return;
                }
                let people = &state.battles[indexed.slot].people;
                indexed.occupied = people
                    .iter()
                    .map(|person| ((person.position.x, person.position.y), person.person))
                    .collect();
            });
            let mut points: Vec<_> = moves
                .par_iter()
                .map(|command| (self.locate(command.person).unwrap(), command.destination))
                .collect();
            points.par_sort_unstable_by_key(|point| point.0);
            self.actors
                .par_chunks_mut(2048)
                .enumerate()
                .for_each(|(chunk, actors)| {
                    let offset = chunk * 2048;
                    let start = points.partition_point(|point| point.0 < offset);
                    let end = points.partition_point(|point| point.0 < offset + actors.len());
                    for &(slot, point) in &points[start..end] {
                        actors[slot - offset].point = point;
                    }
                });
            return Ok(());
        }
        // Remove every source before inserting destinations so swaps remain valid.
        for (command, &(_, _, from)) in moves.iter().zip(&edits) {
            self.battles
                .get_mut(&command.encounter)
                .unwrap()
                .occupied
                .remove(&(from.x, from.y));
        }
        for (command, &(battle, slot, _)) in moves.iter().zip(&edits) {
            let person = &mut state.battles[battle].people[slot];
            person.position = command.destination;
            person.last_move = Some(tick);
            let slot = self.locate(command.person).unwrap();
            self.actors[slot].point = command.destination;
            let indexed = self.battles.get_mut(&command.encounter).unwrap();

            indexed.occupied.insert(
                (command.destination.x, command.destination.y),
                command.person,
            );
        }
        Ok(())
    }
}

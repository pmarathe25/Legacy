//! Checked admission and simultaneous departure of travelers.
use game_parallel_cargo::Cargo;
use game_parallel_packing::Departure;
use game_parallel_people::{Region, Rules};
use game_parallel_transport::{Move, Route, Traveler};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
type DepartureRow = Result<Departure, &'static str>;

pub struct Context<'a> {
    pub regions: &'a mut [Region],
    pub travelers: &'a mut Vec<Traveler>,
    pub routes: &'a [Route],
    pub rules: &'a Rules,
    pub cargo: Option<&'a mut Cargo>,
    pub busy: Vec<u64>,
    pub suspended: Option<u64>,
}
impl Context<'_> {
    /// Launch all moves simultaneously, or reject the batch without mutation.
    pub fn move_people(&mut self, moves: &[Move]) -> Result<(), String> {
        if self
            .suspended
            .is_some_and(|person| moves.par_iter().any(|movement| movement.person == person))
        {
            return Err("player control is suspended".into());
        }
        self.busy.par_sort_unstable();
        if moves
            .par_iter()
            .any(|request| self.busy.binary_search(&request.person).is_ok())
        {
            return Err("worker has unfinished work".into());
        }
        let regions: HashSet<u64> = self.regions.iter().map(|r| r.id).collect();
        if regions.len() != self.regions.len() {
            return Err("duplicate region id".into());
        }
        let mut requested = moves.to_vec();
        requested.par_sort_unstable_by_key(|movement| movement.person);
        if requested
            .par_windows(2)
            .any(|pair| pair[0].person == pair[1].person)
        {
            return Err("person requested more than once".into());
        }
        if requested
            .par_iter()
            .any(|movement| !regions.contains(&movement.destination))
        {
            return Err("unknown destination".into());
        }
        let routes: HashMap<(u64, u64), u32> = self
            .routes
            .iter()
            .map(|r| ((r.source, r.destination), r.ticks))
            .collect();
        let rows: Vec<DepartureRow> = self
            .regions
            .par_iter()
            .flat_map(|region| {
                region.people.par_iter().filter_map(|person| {
                    let i = requested
                        .binary_search_by_key(&person.id, |m| m.person)
                        .ok()?;
                    let destination = requested[i].destination;
                    Some(if person.health == 0 {
                        Err("dead person cannot travel")
                    } else if let Some(&duration) = routes.get(&(region.id, destination)) {
                        if duration == 0 {
                            Err("invalid route")
                        } else {
                            Ok((person.id, region.id, destination, duration))
                        }
                    } else {
                        Err("no route")
                    })
                })
            })
            .collect();
        let mut departures: Vec<_> = rows.into_iter().collect::<Result<_, _>>()?;
        departures.par_sort_unstable_by_key(|row| row.0);
        if departures.par_windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("duplicate person id".into());
        }
        if departures.len() != moves.len() {
            return Err("unknown person".into());
        }
        let allocations = game_parallel_packing::prepare(
            self.regions,
            self.travelers,
            self.cargo.as_deref(),
            self.rules,
            &departures,
        )?;
        let outgoing: Vec<Vec<Traveler>> = self
            .regions
            .par_iter_mut()
            .zip(allocations.par_iter())
            .map(|(region, allocated)| {
                region.food -= allocated.par_iter().map(|h| h.food).sum::<u32>();
                let people = std::mem::take(&mut region.people);
                let sent = people
                    .par_iter()
                    .filter_map(|person| {
                        let i = departures
                            .binary_search_by_key(&person.id, |row| row.0)
                            .ok()?;
                        let (_, source, destination, ticks_remaining) = departures[i];
                        Some(Traveler {
                            person: person.clone(),
                            source,
                            destination,
                            ticks_remaining,
                        })
                    })
                    .collect();
                region.people = people
                    .into_par_iter()
                    .filter(|person| {
                        departures
                            .binary_search_by_key(&person.id, |row| row.0)
                            .is_err()
                    })
                    .collect();
                sent
            })
            .collect();
        if let Some(cargo) = self.cargo.as_deref_mut() {
            game_parallel_packing::commit(cargo, &allocations);
        }
        self.travelers.extend(outgoing.into_iter().flatten());
        self.travelers.par_sort_unstable_by_key(|t| t.person.id);
        Ok(())
    }
}

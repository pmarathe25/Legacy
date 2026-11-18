//! Within-region travel proceeds at elapsed deadlines after a personal choice.
use crate::{
    Endpoint,
    walking::{Event, Location, Outcome, Position, Walking},
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clock {
    pub unit_millis: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paused: Vec<u64>,
}

pub struct Progress {
    pub positions: Vec<Position>,
    pub arrivals: Vec<Event>,
}

impl Walking {
    pub fn next_arrival(&self, able_to_move: impl Fn(u64) -> bool + Sync) -> Option<u64> {
        let clock = self.elapsed.as_ref()?;
        self.positions
            .par_iter()
            .filter_map(|position| {
                let Location::Walking { remaining, .. } = position.location else {
                    return None;
                };
                (clock.paused.binary_search(&position.person).is_err()
                    && able_to_move(position.person))
                .then_some(remaining)
            })
            .min()
    }

    pub fn moving(&self, person: u64, region: u64) -> bool {
        matches!(self.location(person, region), Location::Walking { .. })
            && self
                .elapsed
                .as_ref()
                .is_some_and(|clock| clock.paused.binary_search(&person).is_err())
    }

    /// Stage already chosen legs without admitting a new decision or crossing
    /// an earlier arrival. An incapacitated person preserves their progress.
    pub fn stage_elapsed(
        &self,
        able_to_move: impl Fn(u64) -> bool + Sync,
        elapsed: u64,
        tick: u64,
    ) -> Option<Progress> {
        let clock = self.elapsed.as_ref()?;
        if elapsed == 0
            || !self.positions.par_iter().any(|position| {
                matches!(position.location, Location::Walking { .. })
                    && clock.paused.binary_search(&position.person).is_err()
                    && able_to_move(position.person)
            })
        {
            return None;
        }
        let rows: Vec<_> = self
            .positions
            .par_iter()
            .map(|opening| {
                let mut next = *opening;
                let Location::Walking {
                    region,
                    from,
                    to,
                    remaining,
                } = opening.location
                else {
                    return (next, None);
                };
                if clock.paused.binary_search(&opening.person).is_ok()
                    || !able_to_move(opening.person)
                {
                    return (next, None);
                }
                let remaining = remaining.saturating_sub(elapsed);
                if remaining == 0 {
                    next.location = Location::At {
                        region,
                        endpoint: to,
                    };
                    (
                        next,
                        Some(Event {
                            tick,
                            person: opening.person,
                            destination: to,
                            outcome: Outcome::Arrived,
                        }),
                    )
                } else {
                    next.location = Location::Walking {
                        region,
                        from,
                        to,
                        remaining,
                    };
                    (next, None)
                }
            })
            .collect();
        let (positions, arrivals): (Vec<_>, Vec<_>) = rows.into_par_iter().unzip();
        let arrivals = arrivals.into_par_iter().flatten().collect();
        Some(Progress {
            positions,
            arrivals,
        })
    }

    /// Store the person's choice to pause or resume without changing reach.
    pub fn pause(&mut self, person: u64, region: u64, paused: bool) -> bool {
        if !matches!(self.location(person, region), Location::Walking { .. }) {
            return false;
        }
        let Some(clock) = self.elapsed.as_mut() else {
            return false;
        };
        match (paused, clock.paused.binary_search(&person)) {
            (true, Err(index)) => clock.paused.insert(index, person),
            (false, Ok(index)) => {
                clock.paused.remove(index);
            }
            _ => return false,
        }
        true
    }

    pub fn stock_reachable(&self, person: u64, region: u64) -> bool {
        self.present(person, region, Endpoint::Stock)
    }
}

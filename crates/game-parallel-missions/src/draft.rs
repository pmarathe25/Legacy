//! Publish prevalidated sparse expedition changes.
use crate::{Departure, Event, EventKind, Missions, Phase, phase::Edit};
use rayon::prelude::*;

pub struct Draft {
    pub(crate) edits: Vec<Edit>,
    pub departures: Vec<Departure>,
    pub actors: Vec<u64>,
}
impl Draft {
    /// Publish only departures that survived post-physics revalidation and movement.
    pub fn publish(self, state: &mut Missions, available: &[bool], accepted: bool, tick: u64) {
        let mut updates: Vec<_> = self
            .edits
            .par_iter()
            .map(|edit| (edit.index, edit.phase, edit.remaining))
            .collect();
        let mut departures: Vec<_> = self
            .departures
            .par_iter()
            .copied()
            .zip(available.par_iter().copied())
            .collect();
        let starts: Vec<_> = departures
            .par_iter()
            .filter_map(|&(departure, available)| {
                (available
                    && accepted
                    && departure.outbound
                    && state.entries[departure.index].phase == Phase::Ready)
                    .then_some((departure.index, Phase::Outbound, 0))
            })
            .collect();
        updates.extend(starts);
        if updates.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            updates.par_sort_unstable_by_key(|row| row.0);
        }
        if !updates.is_empty() {
            state
                .entries
                .par_chunks_mut(1024)
                .enumerate()
                .for_each(|(chunk, entries)| {
                    let base = chunk * 1024;
                    let start = updates.partition_point(|row| row.0 < base);
                    let end = updates.partition_point(|row| row.0 < base + entries.len());
                    for &(index, phase, remaining) in &updates[start..end] {
                        entries[index - base].phase = phase;
                        entries[index - base].remaining = remaining;
                    }
                });
        }
        let mut events: Vec<_> = self
            .edits
            .into_par_iter()
            .filter_map(|edit| edit.event)
            .collect();
        events.par_extend(
            departures
                .par_drain(..)
                .map(|(departure, available)| Event {
                    tick,
                    mission: departure.mission,
                    person: departure.person,
                    kind: if !available {
                        EventKind::Unavailable
                    } else if !accepted {
                        EventKind::Rejected
                    } else if departure.outbound {
                        EventKind::Departure
                    } else {
                        EventKind::Return
                    },
                }),
        );
        if events
            .par_windows(2)
            .any(|pair| (pair[0].mission, pair[0].person) > (pair[1].mission, pair[1].person))
        {
            events.par_sort_unstable_by_key(|event| (event.mission, event.person));
        }
        state.events = events;
    }
    /// Convenience for a standalone owner that consumes its previous state.
    pub fn finish(
        self,
        mut state: Missions,
        available: &[bool],
        accepted: bool,
        tick: u64,
    ) -> Missions {
        self.publish(&mut state, available, accepted, tick);
        state
    }
}

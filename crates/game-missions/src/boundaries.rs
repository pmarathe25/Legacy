//! Deadlines and conservative record budgets belong to mission progression.
use crate::*;
use game_core::*;
use game_ecs::Store;
impl Runtime {
    pub fn boundary(&self, store: &Store, now: SimTime, limit: SimTime) -> SimTime {
        talks(store)
            .map(|(_, row)| row)
            .map(|talk| talk.completes)
            .chain(
                missions(store)
                    .map(|(_, row)| row)
                    .filter(|m| {
                        !matches!(
                            m.status,
                            MissionStatus::Completed
                                | MissionStatus::Failed
                                | MissionStatus::Cancelled
                        )
                    })
                    .map(|m| m.proposal.deadline),
            )
            .chain(
                missions(store)
                    .map(|(_, row)| row)
                    .filter_map(|m| match m.proposal.objective {
                        MissionObjective::Patrol { until, .. }
                        | MissionObjective::Confine { until, .. }
                            if !matches!(
                                m.status,
                                MissionStatus::Completed
                                    | MissionStatus::Failed
                                    | MissionStatus::Cancelled
                            ) =>
                        {
                            Some(until)
                        }
                        _ => None,
                    }),
            )
            .filter(|at| *at > now)
            .min()
            .map_or(limit, |next| next.min(limit))
    }
}
impl Runtime {
    pub fn allocation_bound(&self, store: &Store) -> u64 {
        missions(store)
            .flat_map(|(_, m)| m.members.values())
            .filter(|m| {
                matches!(
                    m.status,
                    CompanionStatus::Accepted | CompanionStatus::Offered
                )
            })
            .count() as u64
            * 12
    }
    pub fn event_bound(&self, store: &Store) -> usize {
        missions(store)
            .map(|(_, m)| m.members.len() * 32 + 4)
            .sum::<usize>()
            + store.count::<PendingTalk>() * 33
    }
    pub fn reconciliation_event_bound(&self, store: &Store) -> Result<usize, Error> {
        store
            .count::<PendingTalk>()
            .checked_add(
                store
                    .count::<Mission>()
                    .checked_mul(36)
                    .ok_or(Error::Overflow)?,
            )
            .ok_or(Error::Overflow)
    }
    pub fn preflight_budget(&self, store: &Store, next_id: u64) -> Result<usize, Error> {
        let people = missions(store)
            .filter(|(_, m)| {
                !matches!(
                    m.status,
                    MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
                )
            })
            .map(|(_, m)| m.members.len() as u64)
            .sum::<u64>();
        next_id
            .checked_add(people.checked_mul(12).ok_or(Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        people
            .checked_mul(32)
            .and_then(|n| {
                n.checked_add(
                    store.count::<PendingTalk>() as u64 * 2 + store.count::<Mission>() as u64 * 4,
                )
            })
            .ok_or(Error::Overflow)?
            .try_into()
            .map_err(|_| Error::Overflow)
    }
    pub fn order_event_bound(&self, store: &Store, id: MissionId) -> Result<usize, Error> {
        Ok(mission(store, id).ok_or(Error::Unavailable)?.members.len() + 1)
    }
    pub fn advance_receipts(&mut self, last_event: u64) {
        self.last_event = last_event;
    }
}

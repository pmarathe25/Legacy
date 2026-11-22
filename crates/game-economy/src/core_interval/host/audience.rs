//! Local performance discovery and voluntary attention.
use super::*;
use game_pastimes::{Attendance, Session, Venue};
use std::num::NonZeroU64;

impl CoreHost {
    pub fn current_performance_controlled(
        &self,
        person: u64,
    ) -> Result<Option<Attendance>, String> {
        self.controlled_person(person)?;
        let busy = self.choice_busy(&[]);
        Ok(self
            .pastimes
            .as_ref()
            .and_then(|owner| owner.culture.as_ref())
            .and_then(|culture| culture.attending(person))
            .filter(|row| {
                self.performance_venue(person) == Some(row.venue)
                    && self.performance_venue(row.performer) == Some(row.venue)
                    && busy.binary_search(&person).is_err()
                    && busy.binary_search(&row.performer).is_err()
            }))
    }

    pub fn local_performances_controlled(&self, person: u64) -> Result<Vec<Session>, String> {
        self.controlled_person(person)?;
        let Some(venue) = self.performance_venue(person) else {
            return Ok(Vec::new());
        };
        let Some(owner) = &self.pastimes else {
            return Ok(Vec::new());
        };
        let Some(culture) = &owner.culture else {
            return Ok(Vec::new());
        };
        let now = self.core.simulated_millis();
        let busy = self.choice_busy(&[]);
        Ok(owner
            .active
            .par_iter()
            .copied()
            .filter(|session| {
                session.person != person
                    && session.ends_at > now
                    && culture.presents(session.person, session.activity)
                    && self.performance_venue(session.person) == Some(venue)
                    && busy.binary_search(&session.person).is_err()
            })
            .collect())
    }

    pub fn join_performance_controlled(
        &mut self,
        person: u64,
        performer: u64,
        minimum: NonZeroU64,
    ) -> Result<bool, String> {
        self.controlled_person(person)?;
        let Some(venue) = self.performance_venue(person) else {
            return Ok(false);
        };
        let busy = self.choice_busy(&[]);
        if self.performance_venue(performer) != Some(venue)
            || busy.binary_search(&person).is_ok()
            || busy.binary_search(&performer).is_ok()
        {
            return Ok(false);
        }
        let now = self.core.simulated_millis();
        let Some(owner) = self.pastimes.as_mut() else {
            return Ok(false);
        };
        if owner.session(person).is_some() {
            return Ok(false);
        }
        let Some(session) = owner.session(performer).copied() else {
            return Ok(false);
        };
        Ok(owner
            .culture
            .as_mut()
            .is_some_and(|culture| culture.join_attendance(person, session, now, venue, minimum)))
    }

    pub fn leave_performance_controlled(&mut self, person: u64) -> Result<bool, String> {
        self.controlled_person(person)?;
        Ok(self
            .pastimes
            .as_mut()
            .and_then(|owner| owner.culture.as_mut())
            .is_some_and(|culture| culture.leave_attendance(person)))
    }

    pub(super) fn performance_venue(&self, person: u64) -> Option<Venue> {
        let actor = self.core.projected_person(person)?;
        let region = actor.region.filter(|_| actor.health > 0)?;
        let endpoint = self.local_partition(person);
        (!matches!(endpoint.0, 2 | 4)).then_some((region, endpoint))
    }

    pub(super) fn choose_audience(
        &mut self,
        blocked: &[u64],
        external: &[u64],
        new_performances: bool,
    ) {
        let Some(owner) = &self.pastimes else { return };
        let Some(culture) = &owner.culture else {
            return;
        };
        if owner.active.is_empty() || !culture.supports_attendance() {
            return;
        }
        self.enforce_audience_presence(external);
        let owner = self.pastimes.as_ref().expect("known pastime owner");
        let culture = owner.culture.as_ref().expect("known culture");
        let busy = self.choice_busy(blocked);
        let ongoing = self.choice_busy(external);
        let now = self.core.simulated_millis();
        let opened;
        let sessions = if new_performances {
            opened = owner
                .active
                .par_iter()
                .copied()
                .filter(|row| row.started_at == now)
                .collect::<Vec<_>>();
            &opened
        } else {
            &owner.active
        };
        let audience = culture.choose_attendance(
            now,
            sessions,
            |person| {
                ongoing
                    .binary_search(&person)
                    .is_err()
                    .then(|| self.performance_venue(person))
                    .flatten()
            },
            |person| busy.binary_search(&person).is_err() && owner.session(person).is_none(),
            |person| {
                self.core
                    .person(person)
                    .is_some_and(|person| !person.controlled)
            },
            |person| {
                owner.preferred_value(person, now, |activity| {
                    self.pastime_ready(person, activity, true)
                })
            },
        );
        self.pastimes
            .as_mut()
            .unwrap()
            .culture
            .as_mut()
            .unwrap()
            .attendance = audience;
    }

    pub(super) fn enforce_audience_presence(&mut self, blocked: &[u64]) {
        if self
            .pastimes
            .as_ref()
            .and_then(|owner| owner.culture.as_ref())
            .is_none_or(|culture| culture.attendance.is_empty())
        {
            return;
        }
        let busy = self.choice_busy(blocked);
        let mut owner = self.pastimes.take().expect("known pastime owner");
        let culture = owner.culture.as_mut().expect("known culture");
        culture.retain_attendance(|row| {
            [row.listener, row.performer].into_iter().all(|person| {
                busy.binary_search(&person).is_err()
                    && self.performance_venue(person) == Some(row.venue)
            })
        });
        self.pastimes = Some(owner);
    }
}

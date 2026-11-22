//! Continuous attention to an active performance at a shared location.
use super::{Attendance, Culture, Event, Heard, Outcome, Session, Venue};
use rayon::prelude::*;
use std::cmp::Reverse;
use std::num::NonZeroU64;

impl Culture {
    pub fn supports_attendance(&self) -> bool {
        self.listeners
            .iter()
            .any(|row| row.minimum_millis.is_some())
    }

    pub fn choose_attendance(
        &self,
        now: u128,
        sessions: &[Session],
        venue: impl Fn(u64) -> Option<Venue> + Sync,
        available: impl Fn(u64) -> bool + Sync,
        autonomous: impl Fn(u64) -> bool + Sync,
        competing_value: impl Fn(u64) -> Option<i64> + Sync,
    ) -> Vec<Attendance> {
        if sessions.is_empty() || self.presentations.is_empty() || self.listeners.is_empty() {
            return Vec::new();
        }
        let mut openings: Vec<_> = sessions
            .par_iter()
            .filter_map(|row| {
                self.presents(row.person, row.activity).then_some(())?;
                let venue = venue(row.person).filter(|venue| venue.0 == row.region)?;
                Some((
                    (venue, row.activity),
                    Reverse(row.ends_at),
                    row.person,
                    row.started_at,
                ))
            })
            .collect();
        openings.par_sort_unstable();
        self.listeners
            .par_iter()
            .filter_map(|listener| {
                let person = listener.person;
                let minimum = listener.minimum_millis?;
                let current = self.attending(person);
                if !autonomous(person) {
                    return current;
                }
                if !available(person) {
                    return None;
                }
                let venue = venue(person)?;
                let floor =
                    competing_value(person).map_or(listener.idle, |value| value.max(listener.idle));
                if let Some(current) = current
                    && listener.interest(current.activity).is_some_and(|interest| {
                        current.joined_at == now
                            || interest.value.saturating_add(listener.stay_value) > floor
                    })
                {
                    return Some(current);
                }
                listener
                    .interests
                    .iter()
                    .filter_map(|interest| {
                        if interest.value <= floor
                            || current.is_some_and(|row| row.activity == interest.activity)
                        {
                            return None;
                        }
                        let key = (venue, interest.activity);
                        let index = openings.partition_point(|row| row.0 < key);
                        // Only one session per performer. Either of the first two rows
                        // contains the longest available performance other than one's own.
                        let (_, Reverse(end), performer, started_at) = openings[index..]
                            .iter()
                            .take(2)
                            .find(|row| row.0 == key && row.2 != person)
                            .copied()?;
                        (end.saturating_sub(now) >= u128::from(minimum.get())).then_some((
                            (interest.value, Reverse(interest.activity)),
                            Attendance {
                                listener: person,
                                performer,
                                activity: interest.activity,
                                started_at,
                                joined_at: now,
                                venue,
                            },
                        ))
                    })
                    .max_by_key(|(rank, _)| *rank)
                    .map(|(_, attendance)| attendance)
            })
            .collect()
    }

    pub fn join_attendance(
        &mut self,
        person: u64,
        session: Session,
        now: u128,
        venue: Venue,
        minimum: NonZeroU64,
    ) -> bool {
        if person == session.person
            || venue.0 != session.region
            || session.ends_at.saturating_sub(now) < u128::from(minimum.get())
            || !self.presents(session.person, session.activity)
            || (self
                .listener(person)
                .and_then(|row| row.interest(session.activity))
                .is_none()
                && !self.set_interest(person, session.activity, 0))
        {
            return false;
        }
        let listener = self
            .listeners
            .binary_search_by_key(&person, |row| row.person)
            .expect("known listener");
        self.listeners[listener].minimum_millis = Some(minimum);
        let attendance = Attendance {
            listener: person,
            performer: session.person,
            activity: session.activity,
            started_at: session.started_at,
            joined_at: now,
            venue,
        };
        match self
            .attendance
            .binary_search_by_key(&person, |row| row.listener)
        {
            Ok(index) => self.attendance[index] = attendance,
            Err(index) => self.attendance.insert(index, attendance),
        }
        true
    }

    pub fn leave_attendance(&mut self, person: u64) -> bool {
        let Ok(index) = self
            .attendance
            .binary_search_by_key(&person, |row| row.listener)
        else {
            return false;
        };
        self.attendance.remove(index);
        true
    }

    pub fn retain_attendance(&mut self, valid: impl Fn(Attendance) -> bool + Sync) {
        self.attendance = self
            .attendance
            .par_iter()
            .copied()
            .filter(|row| valid(*row))
            .collect();
    }

    pub fn finish_attendance(&mut self, events: &[Event]) -> Vec<Heard> {
        if self.attendance.is_empty() || events.is_empty() {
            return Vec::new();
        }
        let heard = self
            .attendance
            .par_iter()
            .filter_map(|row| {
                let event = events.get(
                    events
                        .binary_search_by_key(&row.performer, |event| event.person)
                        .ok()?,
                )?;
                let minimum = self.listener(row.listener)?.minimum_millis?.get();
                let elapsed = u64::try_from(event.at_millis.checked_sub(row.joined_at)?).ok()?;
                (event.outcome == Outcome::Completed && elapsed >= minimum).then_some(Heard {
                    listener: row.listener,
                    performer: row.performer,
                    activity: row.activity,
                    at_millis: event.at_millis,
                    attended_millis: Some(elapsed),
                })
            })
            .collect();
        self.retain_attendance(|row| {
            events
                .binary_search_by_key(&row.performer, |event| event.person)
                .is_err()
        });
        heard
    }
}

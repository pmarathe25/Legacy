//! Simulated time used by discretionary cultivation and its requests.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldWork {
    pub person: u64,
    pub field: u64,
    pub resulting_phase: super::Phase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clock {
    pub period_millis: u64,
    pub request_unit_millis: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_review_millis: Option<u64>,
}

impl Clock {
    pub(crate) fn validate(self) -> Result<(), String> {
        if self.period_millis == 0 || self.request_unit_millis == 0 {
            return Err("nonpositive cultivation decision or request duration".into());
        }
        Ok(())
    }

    pub fn review(self, now: u64) -> Option<Review> {
        self.last_review_millis
            .is_none_or(|last| now / self.period_millis > last / self.period_millis)
            .then_some(Review { now, clock: self })
    }
}

#[derive(Clone, Copy)]
pub struct Review {
    pub now: u64,
    clock: Clock,
}

impl Review {
    pub(crate) fn duration(self, units: u64) -> Option<u64> {
        units.checked_mul(self.clock.request_unit_millis)
    }

    pub(crate) fn policy_due(self, units: u64) -> bool {
        self.duration(units).is_some_and(|period| {
            self.clock
                .last_review_millis
                .is_none_or(|last| self.now / period > last / period)
        })
    }

    pub(crate) fn committed_clock(self) -> Clock {
        Clock {
            last_review_millis: Some(self.now),
            ..self.clock
        }
    }
}

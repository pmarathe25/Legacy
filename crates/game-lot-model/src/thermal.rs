//! Fixed-point physical age for perishable lots moved between climates.
use serde::{Deserialize, Serialize};

pub const NORMAL_RATE: u16 = 1000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    #[default]
    Standard,
    Preparation,
    Thermal {
        /// The age budget left unused in the last projected millisecond.
        slack: u16,
    },
}

impl Freshness {
    pub fn is_standard(&self) -> bool {
        *self == Self::Standard
    }

    pub(super) fn slack(self) -> u16 {
        match self {
            Self::Thermal { slack } => slack,
            _ => 0,
        }
    }

    pub(super) fn valid(self, expiry: Option<u64>) -> bool {
        match self {
            Self::Standard => true,
            Self::Preparation => expiry.is_some(),
            Self::Thermal { slack } => expiry.is_some() && slack > 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgingRate {
    pub from_celsius: i16,
    /// Physical shelf life consumed per millisecond, relative to 1000 normally.
    pub per_thousand: u16,
}

pub(super) fn rescale(
    deadline: u64,
    slack: u16,
    at: u64,
    from: u16,
    to: u16,
) -> Result<(u64, Freshness), &'static str> {
    let left = deadline
        .checked_sub(at)
        .filter(|&left| left > 0)
        .ok_or("cannot move an expired lot")?;
    if from == 0 || to == 0 || slack >= from {
        return Err("invalid thermal aging rate or remainder");
    }
    let budget = u128::from(left) * u128::from(from) - u128::from(slack);
    let age_rate = u128::from(to);
    let delay = budget.div_ceil(age_rate);
    let slack = (delay * age_rate - budget) as u16;
    let deadline = u64::try_from(delay + u128::from(at)).map_err(|_| "thermal expiry overflow")?;
    let freshness = if slack == 0 {
        Freshness::Standard
    } else {
        Freshness::Thermal { slack }
    };
    Ok((deadline, freshness))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_temperature_changes_preserve_fractional_age_exactly() {
        let (at, rem) = rescale(20, 0, 7, NORMAL_RATE, 2100).unwrap();
        assert_eq!(at, 14);
        assert_eq!(rem, Freshness::Thermal { slack: 1700 });
        let (at, rem) = rescale(at, rem.slack(), 7, 2100, 425).unwrap();
        let (at, rem) = rescale(at, rem.slack(), 7, 425, NORMAL_RATE).unwrap();
        assert_eq!((at, rem), (20, Freshness::Standard));

        let (at, rem) = rescale(20, 0, 7, NORMAL_RATE, 2100).unwrap();
        let (at, rem) = rescale(at, rem.slack(), 10, 2100, 700).unwrap();
        assert_eq!(at, 20);
        assert_eq!(rem, Freshness::Thermal { slack: 300 });
    }
}

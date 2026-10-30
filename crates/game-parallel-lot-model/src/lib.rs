//! Physical goods cohorts and stable custody identities.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Place {
    Inventory(u64),
    /// A region's independently capacity-limited stock of one good.
    Stock {
        region: u64,
        good: u32,
    },
    Person(u64),
    JobInput(u64),
    JobOutput(u64),
}
impl Place {
    pub fn valid(self) -> bool {
        match self {
            Self::Stock { region, good } => region != 0 && good != 0,
            Self::Inventory(id) | Self::Person(id) | Self::JobInput(id) | Self::JobOutput(id) => {
                id != 0
            }
        }
    }
    pub fn accepts(self, good: u32) -> bool {
        self.valid()
            && good != 0
            && match self {
                Self::Stock { good: stock, .. } => good == stock,
                _ => true,
            }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lot {
    pub good: u32,
    pub quantity: u64,
    /// An expiry at or before a boundary is unusable at that boundary.
    pub expires_at: Option<u64>,
}
pub fn key(lot: &Lot) -> (u32, bool, u64) {
    (
        lot.good,
        lot.expires_at.is_none(),
        lot.expires_at.unwrap_or(0),
    )
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Container {
    pub place: Place,
    pub capacity: u64,
    pub lots: Vec<Lot>,
    /// Cached values are validated against lots when state is imported.
    pub used: u64,
    pub next_expiry: Option<u64>,
}
impl Container {
    pub fn new(place: Place, capacity: u64, mut lots: Vec<Lot>) -> Result<Self, String> {
        lots.sort_unstable_by_key(key);
        let used = lots
            .iter()
            .try_fold(0u64, |total, lot| total.checked_add(lot.quantity))
            .ok_or("lot quantity overflow")?;
        let next_expiry = lots.iter().filter_map(|lot| lot.expires_at).min();
        let result = Self {
            place,
            capacity,
            lots,
            used,
            next_expiry,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.place.valid()
            || self.capacity == 0
            || self.used > self.capacity
            || matches!(self.place, Place::Stock { good: 0, .. })
            || self
                .lots
                .iter()
                .any(|lot| !self.place.accepts(lot.good) || lot.quantity == 0)
            || self
                .lots
                .windows(2)
                .any(|pair| key(&pair[0]) >= key(&pair[1]))
            || self
                .lots
                .iter()
                .try_fold(0u64, |total, lot| total.checked_add(lot.quantity))
                != Some(self.used)
            || self.lots.iter().filter_map(|lot| lot.expires_at).min() != self.next_expiry
        {
            return Err("invalid physical lot container".into());
        }
        Ok(())
    }
}
/// Move a quantity from one actual custody to another, or consume it when `to` is absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Movement {
    pub id: u64,
    pub from: Place,
    pub to: Option<Place>,
    pub good: u32,
    pub quantity: u64,
}
/// Create one output cohort at its actual production boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Creation {
    pub id: u64,
    pub to: Place,
    pub lot: Lot,
}

/// The transaction's outcome is independent of how many physical legs it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionOutcome {
    pub id: u64,
    pub accepted: bool,
}

/// A physical action succeeds only if every input and output can be staged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub id: u64,
    pub movements: Vec<Movement>,
    pub creations: Vec<Creation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spoilage {
    pub place: Place,
    pub good: u32,
    pub quantity: u64,
    pub expires_at: u64,
}

/// An accepted movement returns the exact source cohorts; a rejected movement returns none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub id: u64,
    pub accepted: bool,
    pub lots: Vec<Lot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub tick: u64,
    pub revision: u64,
    pub containers: Vec<Container>,
}

/// Convert authored shelf life into a checked absolute tick deadline.
pub fn deadline(created_at: u64, shelf_days: u32, ticks_per_day: u64) -> Result<u64, &'static str> {
    if shelf_days == 0 || ticks_per_day == 0 {
        return Err("invalid shelf life");
    }
    created_at
        .checked_add(
            u64::from(shelf_days)
                .checked_mul(ticks_per_day)
                .ok_or("shelf life overflow")?,
        )
        .ok_or("shelf life overflow")
}

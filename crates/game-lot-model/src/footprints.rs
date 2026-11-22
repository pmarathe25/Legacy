use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Footprint {
    pub good: u32,
    pub units: u32,
}

/// Immutable capacity units per unit of goods. All unlisted goods occupy one unit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Footprints(Option<Arc<[Footprint]>>);

impl Footprints {
    pub fn new(mut entries: Vec<Footprint>) -> Result<Self, String> {
        entries.sort_unstable_by_key(|entry| entry.good);
        if entries
            .iter()
            .any(|entry| entry.good == 0 || entry.units == 0)
            || entries.windows(2).any(|pair| pair[0].good == pair[1].good)
        {
            return Err("invalid good capacity footprints".into());
        }
        entries.retain(|entry| entry.units > 1);
        Ok(Self((!entries.is_empty()).then(|| entries.into())))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self
            .entries()
            .iter()
            .any(|entry| entry.good == 0 || entry.units < 2)
            || self
                .entries()
                .windows(2)
                .any(|pair| pair[0].good >= pair[1].good)
        {
            return Err("invalid good capacity footprints".into());
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.entries().is_empty()
    }

    pub fn entries(&self) -> &[Footprint] {
        self.0.as_deref().unwrap_or(&[])
    }

    pub fn units(&self, good: u32) -> u64 {
        self.entries()
            .binary_search_by_key(&good, |entry| entry.good)
            .map_or(1, |index| u64::from(self.entries()[index].units))
    }

    /// A request can be larger than any possible holder, so hold the full amount
    /// until arbitration. Committed capacity units always fit in a `u64`.
    pub fn occupied(&self, good: u32, quantity: u64) -> u128 {
        u128::from(self.units(good)) * u128::from(quantity)
    }

    /// Calculate occupancy for a lot already admitted by physical custody.
    pub fn admitted(&self, good: u32, quantity: u64) -> u64 {
        u64::try_from(self.occupied(good, quantity)).expect("admitted lot fits a holder")
    }

    pub fn fit(&self, free: u64, good: u32) -> u64 {
        free / self.units(good)
    }
}

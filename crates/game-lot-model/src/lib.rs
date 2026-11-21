//! Physical goods cohorts and stable custody identities.
use serde::{Deserialize, Serialize};
mod lot_list;
mod outcome_batch;
mod place_index;
mod thermal;
pub use lot_list::LotList;
pub use outcome_batch::{OutcomeBatch, OutcomeChunk, OutcomeHeader, OutcomeRef};
pub use place_index::PlaceIndex;
pub use thermal::{AgingRate, Freshness};

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
    /// Recoverable material physically incorporated into a site or its debris.
    Site(u64),
    JobInput(u64),
    JobOutput(u64),
}
impl Place {
    pub fn valid(self) -> bool {
        match self {
            Self::Stock { region, good } => region != 0 && good != 0,
            Self::Inventory(id)
            | Self::Person(id)
            | Self::Site(id)
            | Self::JobInput(id)
            | Self::JobOutput(id) => id != 0,
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
    /// Opaque provenance for a divisible cohort; unrelated to unique items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<u64>,
    /// Stable identity of a unique, nonperishable physical item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<u64>,
    /// Unusable at or after this instant, in the time unit of the owning state.
    pub expires_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Freshness::is_standard")]
    pub freshness: Freshness,
}
pub fn key(
    lot: &Lot,
) -> (
    u32,
    bool,
    u64,
    std::cmp::Reverse<u16>,
    bool,
    Option<u64>,
    Option<u64>,
) {
    (
        lot.good,
        lot.expires_at.is_none(),
        lot.expires_at.unwrap_or(0),
        std::cmp::Reverse(lot.freshness.slack()),
        lot.freshness == Freshness::Preparation,
        lot.item,
        lot.batch,
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
            || self.lots.iter().any(|lot| {
                !self.place.accepts(lot.good)
                    || lot.quantity == 0
                    || !lot.freshness.valid(lot.expires_at)
                    || lot.batch == Some(0)
                    || (lot.batch.is_some() && lot.item.is_some())
                    || lot
                        .item
                        .is_some_and(|id| id == 0 || lot.quantity != 1 || lot.expires_at.is_some())
            })
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
    /// Request a particular divisible batch rather than generic FEFO stock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<u64>,
    /// Request a particular identified item instead of generic FEFO selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<u64>,
}
/// Create one output cohort at its actual production boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Creation {
    pub id: u64,
    pub to: Place,
    pub lot: Lot,
}

/// One independently atomic movement, stored without a per-action vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimpleTransfer {
    pub action: u64,
    pub movement: Movement,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<u64>,
}

/// An accepted movement returns the exact source cohorts; a rejected movement returns none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub id: u64,
    pub accepted: bool,
    pub lots: LotList,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub tick: u64,
    pub revision: u64,
    /// Persisted high-water mark when generated batch IDs are enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_batch: Option<u64>,
    /// Absolute elapsed milliseconds when dated lots use the elapsed clock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_climate: Option<StorageClimate>,
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

/// Storage temperatures measured in degrees Celsius.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageClimate {
    pub ambient_celsius: i16,
    #[serde(default)]
    pub places: Vec<StorageTemperature>,
    /// Stepwise temperature-dependent food aging; otherwise the standard rate applies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aging_rates: Vec<AgingRate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageTemperature {
    pub place: Place,
    pub celsius: i16,
}

impl StorageClimate {
    pub fn temperature(&self, place: Place) -> i16 {
        self.override_at(place).unwrap_or(self.ambient_celsius)
    }

    pub fn override_at(&self, place: Place) -> Option<i16> {
        self.places
            .binary_search_by_key(&place, |row| row.place)
            .ok()
            .map(|index| self.places[index].celsius)
    }

    fn rate(&self, place: Place) -> u16 {
        self.rate_at(self.temperature(place))
    }

    pub fn rate_at(&self, celsius: i16) -> u16 {
        let index = self
            .aging_rates
            .partition_point(|row| row.from_celsius <= celsius);
        self.aging_rates[..index]
            .last()
            .map_or(thermal::NORMAL_RATE, |row| row.per_thousand)
    }
}

impl State {
    pub fn ages_with_temperature(&self) -> bool {
        self.storage_climate
            .as_ref()
            .is_some_and(|climate| !climate.aging_rates.is_empty())
    }

    pub fn thermal_move(&self, movement: &Movement) -> bool {
        self.storage_climate.as_ref().is_some_and(|climate| {
            movement
                .to
                .is_some_and(|to| climate.rate(movement.from) != climate.rate(to))
        })
    }

    pub fn thermal_transaction(&self, action: &Transaction) -> bool {
        self.storage_climate.as_ref().is_some_and(|climate| {
            action
                .movements
                .iter()
                .any(|movement| self.thermal_move(movement))
                || action.creations.iter().any(|creation| {
                    creation.lot.expires_at.is_some()
                        && creation.lot.freshness != Freshness::Preparation
                        && climate.rate(creation.to) != thermal::NORMAL_RATE
                })
        })
    }

    pub fn valid_age(&self, place: Place, lot: &Lot) -> bool {
        match lot.freshness {
            Freshness::Thermal { slack } => self
                .storage_climate
                .as_ref()
                .is_some_and(|climate| slack < climate.rate(place)),
            Freshness::Preparation => self.expiry_millis.is_some(),
            Freshness::Standard => true,
        }
    }

    /// Reproject an already dated lot after a physical move, keeping its exact remaining age.
    /// A newly produced lot starts at the standard rate before entering its first holder.
    pub fn date_lot(
        &self,
        lot: &mut Lot,
        from: Option<Place>,
        to: Place,
        at: u64,
    ) -> Result<(), String> {
        let Some(climate) = &self.storage_climate else {
            return Ok(());
        };
        let Some(expiry) = lot.expires_at else {
            return Ok(());
        };
        if climate.aging_rates.is_empty() || lot.freshness == Freshness::Preparation {
            return Ok(());
        }
        let from = from.map_or(thermal::NORMAL_RATE, |place| climate.rate(place));
        let to = climate.rate(to);
        lot.rescale_aging(expiry, at, from, to)
    }
}

impl Lot {
    /// Preserve exact remaining shelf life when the physical storage rate changes.
    pub fn change_aging_rate(&mut self, at: u64, from: u16, to: u16) -> Result<(), String> {
        if let Some(expiry) = self.expires_at
            && self.freshness != Freshness::Preparation
        {
            self.rescale_aging(expiry, at, from, to)?;
        }
        Ok(())
    }

    fn rescale_aging(&mut self, expiry: u64, at: u64, from: u16, to: u16) -> Result<(), String> {
        if from != to {
            let (deadline, freshness) =
                thermal::rescale(expiry, self.freshness.slack(), at, from, to)?;
            self.expires_at = Some(deadline);
            self.freshness = freshness;
        }
        Ok(())
    }
}

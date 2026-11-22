use game_economy_knowledge::{Record, StockTime};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::num::NonZeroU64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TravelChoice {
    pub person: u64,
    pub min_hunger: Option<u32>,
    pub time_unit_millis: NonZeroU64,
    pub food_weight: i32,
    pub duration_cost: i32,
    #[serde(default)]
    pub age_cost: i32,
    #[serde(default)]
    pub testimony_cost: i32,
    #[serde(default)]
    pub continuation_value: i32,
    #[serde(default)]
    pub stay_value: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub destinations: Vec<DestinationValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DestinationValue {
    pub region: u64,
    pub value: i32,
}

impl TravelChoice {
    pub(super) fn valid(&self, routes: &HashSet<u64>) -> bool {
        let mut seen = HashSet::new();
        self.destinations
            .iter()
            .all(|row| routes.contains(&row.region) && seen.insert(row.region))
    }

    pub(super) fn score(
        &self,
        record: &Record,
        time: StockTime,
        duration_millis: u128,
        continuing: bool,
    ) -> i128 {
        let units = |millis| {
            let count = millis / u128::from(self.time_unit_millis.get());
            i128::from(u64::try_from(count).unwrap_or(u64::MAX))
        };
        let bias = self
            .destinations
            .iter()
            .find(|row| row.region == record.region)
            .map_or(0, |row| row.value);
        let age = units(record.age_millis(time).unwrap_or_default());
        let testimony = record.learned_from.is_some() || record.witness != self.person;
        i128::from(record.food) * i128::from(self.food_weight)
            - units(duration_millis) * i128::from(self.duration_cost)
            - age * i128::from(self.age_cost)
            - i128::from(self.testimony_cost) * i128::from(testimony)
            + i128::from(self.continuation_value) * i128::from(continuing)
            + i128::from(bias)
    }
}

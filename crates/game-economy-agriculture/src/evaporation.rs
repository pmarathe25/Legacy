//! Whole moisture units and fractional loss for elapsed evaporation.
use crate::irrigation::{Irrigation, Moisture};
use rayon::prelude::*;

impl Irrigation {
    /// The next time a currently wet field falls below its growth threshold.
    pub fn next_dry_after(&self) -> Option<u128> {
        let period = u128::from(
            self.evaporation_period_millis
                .filter(|&period| period > 0)?,
        );
        let rate = u128::from(self.evaporation);
        if rate == 0 {
            return None;
        }
        self.moisture
            .par_iter()
            .filter_map(|moisture| {
                let units = u128::from(moisture.amount.checked_sub(self.minimum_growth)?) + 1;
                let required = units * period - u128::from(moisture.evaporation_remainder);
                Some(required.div_ceil(rate))
            })
            .min()
    }

    /// Stage moisture loss from one immutable opening generation.
    pub fn stage_evaporation(&self, elapsed: u64) -> Option<Vec<Moisture>> {
        let period = u128::from(
            self.evaporation_period_millis
                .filter(|&period| period > 0)?,
        );
        if elapsed == 0 || self.evaporation == 0 {
            return None;
        }
        let loss = u128::from(self.evaporation) * u128::from(elapsed);
        Some(
            self.moisture
                .par_iter()
                .map(|opening| {
                    let accrued = loss + u128::from(opening.evaporation_remainder);
                    let whole = accrued / period;
                    let (amount, evaporation_remainder) = if whole >= u128::from(opening.amount) {
                        (0, 0)
                    } else {
                        (opening.amount - whole as u64, (accrued % period) as u64)
                    };
                    Moisture {
                        field: opening.field,
                        amount,
                        evaporation_remainder,
                    }
                })
                .collect(),
        )
    }
}

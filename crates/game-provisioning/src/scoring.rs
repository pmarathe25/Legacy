//! Compare food opportunities without owning character or bodily state.
use super::*;
use game_utility::Candidate;
pub struct FoodScores {
    pub hunger: u64,
    pub value: i32,
    pub independence: i32,
    pub risk: i64,
    pub travel_per_minute: u64,
    pub gathering_effort: u64,
}
impl FoodScores {
    pub fn candidates<'a>(
        &'a self,
        options: &'a [Opportunity],
    ) -> impl Iterator<Item = Candidate> + 'a {
        options.iter().map(move |o| {
            let foraging = matches!(o.plan, Plan::Forage { .. });
            let travel = o
                .travel
                .0
                .div_ceil(MINUTE.0)
                .saturating_mul(self.travel_per_minute);
            let hunger = self.hunger.max(o.appetite).min((i64::MAX / 4) as u64);
            Candidate {
                id: o.id,
                need: if foraging {
                    hunger as i64
                } else {
                    (hunger as i64).saturating_mul(2)
                },
                goal: if foraging { 5 } else { 0 },
                value: o
                    .value
                    .saturating_add(if foraging {
                        i64::from(self.value) - i64::from(self.independence)
                    } else {
                        0
                    })
                    .saturating_sub(if o.self_supplied {
                        0
                    } else {
                        i64::from(self.independence)
                    }),
                effort: travel
                    .saturating_add(if foraging { self.gathering_effort } else { 0 })
                    .min(i64::MAX as u64) as i64,
                risk: o.risk.saturating_add(if foraging { self.risk } else { 0 }),
                survival: true,
            }
        })
    }
}

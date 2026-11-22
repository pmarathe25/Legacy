//! Player choices about sharing and retaining personal food reports.
use super::*;
use game_economy_knowledge::{Mind, Misreport, Record, Reporting, StockTime};

impl CoreHost {
    pub(super) fn stock_time(&self) -> StockTime {
        StockTime::elapsed(
            self.core.physical.tick(),
            self.core.simulated_millis(),
            self.core.physical.tick_millis(),
        )
    }

    fn food_mind_controlled(&mut self, person: u64) -> Result<&mut Mind, String> {
        self.controlled_person(person)?;
        self.knowledge
            .as_mut()
            .and_then(|owner| owner.minds.iter_mut().find(|mind| mind.person == person))
            .ok_or_else(|| "this person has no food memory".into())
    }

    pub fn food_reports_controlled(&self, person: u64) -> Result<Vec<Record>, String> {
        self.controlled_person(person)?;
        self.knowledge
            .as_ref()
            .and_then(|owner| owner.recall_at_time(person, self.stock_time()))
            .ok_or_else(|| "this person has no food memory".into())
    }

    pub fn set_food_reporting_controlled(
        &mut self,
        person: u64,
        willing: bool,
    ) -> Result<(), String> {
        self.food_mind_controlled(person)?.reporting = Some(Reporting {
            willing,
            pressure_threshold: None,
        });
        Ok(())
    }

    pub fn set_food_misreport_controlled(
        &mut self,
        person: u64,
        delta: i64,
        target: Option<u64>,
    ) -> Result<(), String> {
        if target.is_some_and(|target| target == person || self.core.person(target).is_none()) {
            return Err("unknown or invalid listener".into());
        }
        self.food_mind_controlled(person)?.misreport = (delta != 0).then_some(Misreport {
            target,
            food_delta: delta,
        });
        Ok(())
    }
}

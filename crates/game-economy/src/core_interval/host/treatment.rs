//! Personal use of carried medicine, staged with physical item ownership.
use super::*;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreTreatmentResults {
    pub actors: Vec<u64>,
    pub outcomes: Vec<game_lots::TransactionOutcome>,
}

impl CoreHost {
    pub fn treat_autonomously(&mut self, occupied: &[u64]) -> Result<CoreTreatmentResults, String> {
        self.treat_people(occupied, None)
    }
    pub fn treat_controlled(&mut self, person: u64) -> Result<CoreTreatmentResults, String> {
        self.treat_people(&[], Some(person))
    }
    fn treat_people(
        &mut self,
        occupied: &[u64],
        manual: Option<u64>,
    ) -> Result<CoreTreatmentResults, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(health) = &self.health else {
            return if manual.is_some() {
                Err("treatment is disabled".into())
            } else {
                Ok(CoreTreatmentResults::default())
            };
        };
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before treatment".into());
        }
        let mut busy = occupied.to_vec();
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                .map(|row| row.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let view = self.core.physical.view(tick)?;
        let selected = manual.map(|person| {
            health
                .treatment
                .iter()
                .filter(|policy| policy.person == person)
                .cloned()
                .collect::<Vec<_>>()
        });
        let policies = selected.as_deref().unwrap_or(&health.treatment);
        let plan = game_economy_treatment::prepare_at(
            Some(&health.owner),
            policies,
            &self.core.people,
            &busy,
            &view,
            1,
            (
                game_economy_treatment::Choice {
                    manual,
                    max_health: self.core.rules.max_health,
                },
                tick,
            ),
        )?;
        self.commit_self_treatment(plan)
    }

    /// Commit an already selected treatment plan with all affected owners.
    pub(super) fn commit_self_treatment(
        &mut self,
        plan: game_economy_treatment::Plan,
    ) -> Result<CoreTreatmentResults, String> {
        if plan.actions.is_empty() {
            return Ok(CoreTreatmentResults::default());
        }
        let tick = self.core.physical.tick();
        let health = self.health.as_ref().ok_or("treatment is disabled")?;
        let physical = self
            .core
            .physical
            .prepare_interval_consumption(&plan.actions, self.core.seed)?;
        let tools = self.stage_tools(&[], None, physical.item_updates())?;
        let writing = self.stage_written_page_removals(physical.item_updates());
        let mut next = health.clone();
        game_economy_treatment::apply_outcomes(
            Some(&mut next.owner),
            &plan,
            physical.transactions(),
            tick,
        );
        self.core
            .physical
            .validate_interval_consumption(&physical)?;
        let outcomes = self.core.physical.commit_interval_consumption(physical)?;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(writing) = writing {
            self.writing = Some(writing);
        }
        self.health = Some(next);
        Ok(CoreTreatmentResults {
            actors: plan.actors,
            outcomes: outcomes.transactions,
        })
    }
}

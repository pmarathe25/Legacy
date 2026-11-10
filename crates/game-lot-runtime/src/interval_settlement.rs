//! Staged physical settlement for a completed work interval.
use crate::{
    AdvanceDraft, Events, Growth, IdentifiedProduction, ProducedItem, Runtime, WorkInterval,
};
use game_lots::Place;

/// A physical generation whose item-owner changes can be prepared before commit.
pub struct IntervalSettlementDraft {
    interval: WorkInterval,
    advance: AdvanceDraft,
}
impl IntervalSettlementDraft {
    pub fn events(&self) -> &Events {
        self.advance.events()
    }
    pub fn produced_items(&self) -> &[ProducedItem] {
        self.advance.produced_items()
    }
    pub fn next_produced_item(&self) -> Option<u64> {
        self.advance.next_produced_item()
    }
    pub fn item_updates(&self) -> &[(u64, Option<Place>)] {
        self.advance.item_updates()
    }
}
impl Runtime {
    /// Stage a completed interval without advancing work twice. The interval
    /// remains open on success or failure until the draft is committed.
    pub fn prepare_interval_settlement(
        &mut self,
        seed: u64,
        identified: Option<IdentifiedProduction<'_>>,
    ) -> Result<IntervalSettlementDraft, String> {
        self.prepare_interval_settlement_with_growth(&[], Growth::default(), seed, identified)
    }

    /// Stage boundary-owned actions and growth with the same physical generation.
    pub fn prepare_interval_settlement_with_growth(
        &mut self,
        external: &[game_lots::Transaction],
        growth: Growth<'_>,
        seed: u64,
        identified: Option<IdentifiedProduction<'_>>,
    ) -> Result<IntervalSettlementDraft, String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis != self.tick_millis
        {
            return Err("physical work interval is not complete".into());
        }
        let interval = self.work_interval.take().expect("checked interval");
        let result = self.prepare_with_identified_production(
            &[],
            external,
            &[],
            &[],
            (growth, &[]),
            seed,
            identified,
        );
        self.work_interval = Some(interval.clone());
        result.map(|advance| IntervalSettlementDraft { interval, advance })
    }

    /// Verify a staged boundary before publishing any other owner.
    pub fn validate_interval_settlement(
        &mut self,
        draft: &IntervalSettlementDraft,
    ) -> Result<(), String> {
        if self.work_interval.as_ref() != Some(&draft.interval)
            || draft.interval.tick != self.tick()
            || draft.interval.custody_revision != self.store.revision()
            || draft.interval.elapsed_millis != self.tick_millis
        {
            return Err("stale interval settlement draft".into());
        }
        let interval = self.work_interval.take().expect("checked interval");
        let result = self.validate_advance(&draft.advance);
        self.work_interval = Some(interval);
        result
    }

    /// Commit after all other owners have staged and validated their changes.
    pub fn commit_interval_settlement(
        &mut self,
        draft: IntervalSettlementDraft,
    ) -> Result<Events, String> {
        self.validate_interval_settlement(&draft)?;
        let interval = self.work_interval.take().expect("validated interval");
        match self.commit_advance(draft.advance) {
            Ok(events) => Ok(events),
            Err(error) => {
                self.work_interval = Some(interval);
                Err(error)
            }
        }
    }
}

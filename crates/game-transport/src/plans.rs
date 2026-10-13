//! Prepared owner variants and their narrowly typed factual inputs.
use crate::*;
use game_ecs::{Batch, Store};

pub enum MotionPlan {
    Direct(PreparedMotion),
    Stop(PreparedRequestedStop),
    PutDown(PreparedPutDown),
    Journey(PreparedJourney),
    Continuation(PreparedContinuationResponse),
    Road(PreparedRoadMotion),
    Lift(PreparedLiftMotion),
    LiftStart(PreparedLiftStart),
    Offer(PreparedCarryOffer),
    Reconcile(PreparedReconciliation),
    Confinement(PreparedConfinementMotion),
}

#[derive(Clone, Copy)]
pub enum MotionScope {
    Ordinary,
    StationaryReview,
}

pub fn require_no_location_edits(batch: &Batch) -> Result<(), Error> {
    if batch.edited_ids::<Location>().next().is_some() {
        Err(Error::Unavailable)
    } else {
        Ok(())
    }
}

pub trait MotionInputs {
    fn bodies(&self) -> impl BodyFacts + '_;
    fn journeys(&self) -> impl JourneyFacts + '_;
    fn roads(&self) -> impl RoadFacts + '_;
    fn carrying(&self) -> impl LiftRequestFacts + '_;
    fn activities(&self) -> impl LiftActivityFacts + '_;
    fn reconciliation(&self) -> impl ReconciliationFacts + '_;
}

impl MotionPlan {
    pub fn projection(&self) -> &PreparedMotion {
        match self {
            Self::Direct(plan) => plan,
            Self::Stop(plan) => plan.projection(),
            Self::PutDown(plan) => plan.projection(),
            Self::Journey(plan) => plan.projection(),
            Self::Continuation(plan) => plan.projection(),
            Self::Road(plan) => plan.projection(),
            Self::Lift(plan) => plan.projection(),
            Self::LiftStart(plan) => plan.projection(),
            Self::Offer(plan) => plan.projection(),
            Self::Reconcile(plan) => plan.projection(),
            Self::Confinement(plan) => plan.projection(),
        }
    }
    pub fn stage_into(
        self,
        carrying: Option<&Runtime>,
        store: &Store,
        facts: &impl MotionInputs,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        let fallback = Runtime::new(Rules::default()).expect("valid default transport rules");
        let runtime = carrying.unwrap_or(&fallback);
        match self {
            Self::Journey(plan) => plan.stage_into(carrying, store, &facts.journeys(), now, batch),
            Self::Continuation(plan) => {
                plan.stage_into(carrying, store, &facts.journeys(), now, batch)
            }
            Self::Stop(plan) => plan.stage_into(runtime, store, &facts.journeys(), now, batch),
            Self::PutDown(plan) => plan.stage_into(runtime, store, &facts.journeys(), now, batch),
            Self::Direct(plan) => plan.stage_into(runtime, store, &facts.bodies(), now, batch),
            Self::Road(plan) => {
                plan.stage_into(runtime, store, &facts.bodies(), &facts.roads(), now, batch)
            }
            Self::Lift(plan) => plan.stage_into(
                runtime,
                store,
                &facts.bodies(),
                &facts.activities(),
                now,
                batch,
            ),
            Self::LiftStart(plan) => plan.stage_into(runtime, store, &facts.carrying(), now, batch),
            Self::Offer(plan) => plan.stage_into(runtime, store, &facts.carrying(), now, batch),
            Self::Reconcile(plan) => {
                plan.stage_into(runtime, store, &facts.reconciliation(), now, batch)
            }
            Self::Confinement(plan) => {
                plan.stage_into(runtime, store, &facts.reconciliation(), now, batch)
            }
        }
    }
}

impl MotionFinalization {
    pub fn validate_scope(
        &self,
        store: &Store,
        batch: &Batch,
        scope: MotionScope,
    ) -> Result<(), Error> {
        match scope {
            MotionScope::Ordinary => self.validate_footprint(store, batch),
            MotionScope::StationaryReview => self.validate_preserving_locations(store, batch),
        }
    }
    pub fn finish_optional(self, runtime: Option<&mut Runtime>) -> Vec<MotionEffect> {
        let mut fallback = Runtime::new(Rules::default()).expect("valid default transport rules");
        self.finish(runtime.unwrap_or(&mut fallback))
    }
}

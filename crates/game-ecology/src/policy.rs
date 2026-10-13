//! Physical eligibility and presentation of personally known natural resources.
use crate::*;
use game_ecs::Store;

/// Foreign owners supply canonical body, placement and restriction facts. The
/// gathering owner interprets how these facts constrain its own work.
#[derive(Clone, Debug)]
pub struct ActorCondition {
    pub location: Option<Location>,
    pub vitals: Option<Vitals>,
    pub stage: Option<LifeStage>,
    pub development_present: bool,
    pub combat_restricted: bool,
    pub receiving_care: bool,
    pub fatigue_limit: u64,
    pub carrying_capacity: Quantity,
}
impl ActorCondition {
    pub fn observation_area(&self) -> Option<AreaId> {
        let vitals = self.vitals.as_ref()?;
        if !vitals.alive || vitals.health == 0 {
            return None;
        }
        match self.location.as_ref()? {
            Location::AtArea(area) => Some(*area),
            _ => None,
        }
    }
    pub fn able(&self) -> bool {
        self.observation_area().is_some()
            && (!self.development_present
                || self.stage.is_some_and(|stage| stage != LifeStage::Infant))
            && !self.combat_restricted
            && !self.receiving_care
    }
    pub fn gather_facts(
        &self,
        now: SimTime,
        held: u128,
        tool: Option<InventoryId>,
    ) -> Result<GatherFacts, Error> {
        Ok(GatherFacts {
            now,
            area: self.observation_area().ok_or(Error::Unavailable)?,
            capable: self.able()
                && self
                    .vitals
                    .as_ref()
                    .is_some_and(|v| v.fatigue < self.fatigue_limit),
            adult: !self.development_present
                || self
                    .stage
                    .is_some_and(|stage| matches!(stage, LifeStage::Adult | LifeStage::Elder)),
            carrying_room: Quantity(
                u128::from(self.carrying_capacity.0).saturating_sub(held) as u64
            ),
            tool,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ToolCandidate {
    pub id: InventoryId,
    pub good: Good,
}
pub trait ObservationPort {
    fn actors(&self) -> Vec<PersonId>;
    fn condition(&self, actor: PersonId) -> ActorCondition;
}
/// A source-and-reservation witness for replacing the actor's ecology work.
pub struct PreparedGatheringStart {
    actor: PersonId,
    plan: GatheringPlan,
    opposition: Option<PersonId>,
    reads: game_ecs::ReadSet,
}
/// A resisted admission publishes the accepted attempt without installing
/// transient work or debiting the natural deposit.
pub struct OpposedGatheringEffect {
    pub actor: PersonId,
    pub resource: ResourceId,
    pub unauthorized: bool,
}
/// The land-use owner releases undeveloped metadata while retaining Soil for
/// the receiving agricultural owner on the same stable entity.
pub struct PreparedPlotDevelopment {
    plot: Plot,
    soil: Soil,
}
impl PreparedPlotDevelopment {
    pub fn plot(&self) -> &Plot {
        &self.plot
    }
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut game_ecs::Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        let id = game_ecs::StableId(self.plot.id.0);
        let mut reads = game_ecs::ReadSet::new();
        reads.expect_row(id, Some(self.plot));
        reads.expect_row(id, Some(self.soil));
        reads.validate(store)?;
        if batch.touches::<Plot>(id) {
            return Err(Error::Unavailable);
        }
        batch.remove::<Plot>(id);
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<Plot>(id);
        reads.bind(store, batch, &admitted)
    }
}
impl PreparedGatheringStart {
    pub fn plan(&self) -> &GatheringPlan {
        &self.plan
    }
    pub fn opposition(&self) -> Option<PersonId> {
        self.opposition
    }
    /// Conflict has accepted immediate resistance from the resource custodian.
    /// Ecology ends both participants' jobs in the enclosing encounter batch,
    /// admitting those cancellations into the source reservation projection.
    pub fn stage_opposed(
        mut self,
        store: &Store,
        custodian: PersonId,
        batch: &mut game_ecs::Batch,
    ) -> Result<(OpposedGatheringEffect, game_ecs::ReadGuard), Error> {
        if self.opposition != Some(custodian) {
            return Err(Error::Unavailable);
        }
        self.reads
            .read::<Gathering>(store, game_ecs::StableId(custodian.0));
        self.reads.validate(store)?;
        let mut admitted = game_ecs::ReadFootprint::new();
        for actor in [self.actor, custodian] {
            let id = game_ecs::StableId(actor.0);
            if batch.touches::<Gathering>(id) {
                return Err(Error::Unavailable);
            }
            batch.remove::<Gathering>(id);
            admitted.protect::<Gathering>(id);
        }
        let effect = OpposedGatheringEffect {
            actor: self.actor,
            resource: self.plan.resource,
            unauthorized: self.plan.unauthorized,
        };
        Ok((effect, self.reads.bind(store, batch, &admitted)?))
    }
    pub fn stage_into(
        self,
        store: &Store,
        destination: InventoryId,
        batch: &mut game_ecs::Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        let actor = game_ecs::StableId(self.actor.0);
        let job = self.plan.job(destination);
        job.validate()?;
        self.reads.validate(store)?;
        if batch.touches::<Gathering>(actor) {
            return Err(Error::Unavailable);
        }
        batch.insert(actor, job);
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<Gathering>(actor);
        self.reads.bind(store, batch, &admitted)
    }
}
impl Runtime {
    pub fn prepare_plot_development(
        &self,
        store: &Store,
        site: SiteId,
        area: AreaId,
    ) -> Result<PreparedPlotDevelopment, Error> {
        let plot = self.plot(store, site).ok_or(Error::Unavailable)?;
        let soil = self.soil(store, site).ok_or(Error::Unavailable)?;
        plot.validate()?;
        soil.validate()?;
        if plot.id != site || plot.area != area {
            return Err(Error::Unavailable);
        }
        Ok(PreparedPlotDevelopment {
            plot: plot.clone(),
            soil: soil.clone(),
        })
    }
    /// Authoring a replacement simulation epoch can end this owner's work
    /// without draining or fabricating any natural stock.
    pub fn reset_gatherings(&self, store: &mut Store) -> Result<(), Error> {
        let mut batch = game_ecs::Batch::new();
        for (actor, _) in self.gatherings(store) {
            batch.remove::<Gathering>(game_ecs::StableId(actor.0));
        }
        store.apply_batch(batch)
    }
    pub fn prepare_gathering_start(
        &self,
        store: &Store,
        actor: PersonId,
        resource: ResourceId,
        quantity: Quantity,
        unauthorized: bool,
        facts: GatherFacts,
    ) -> Result<PreparedGatheringStart, Error> {
        let plan = self.prepare_gather(store, actor, resource, quantity, unauthorized, facts)?;
        let mut reads = game_ecs::ReadSet::new();
        let actor_id = game_ecs::StableId(actor.0);
        if !reads.entity(store, actor_id) {
            return Err(Error::Unavailable);
        }
        reads.read::<Gathering>(store, actor_id);
        let source = reads
            .read::<Resource>(store, game_ecs::StableId(resource.0))
            .ok_or(Error::Unavailable)?;
        let opposition = source
            .custodian
            .filter(|custodian| unauthorized && *custodian != actor);
        for id in reads.membership::<Gathering>(store, move |id, job| {
            id != actor_id && job.resource == resource
        }) {
            reads.read::<Gathering>(store, id);
        }
        Ok(PreparedGatheringStart {
            actor,
            plan,
            opposition,
            reads,
        })
    }
    pub fn select_tool(
        candidates: impl IntoIterator<Item = ToolCandidate>,
        mut available: impl FnMut(InventoryId) -> bool,
    ) -> Option<InventoryId> {
        candidates
            .into_iter()
            .find(|row| row.good == Good::Tool && available(row.id))
            .map(|row| row.id)
    }
    /// Entity enumeration and ecological observer admission are feature policy;
    /// the port only reads another owner's actual canonical facts.
    pub fn prepare_observations(
        &self,
        store: &Store,
        now: SimTime,
        port: &impl ObservationPort,
    ) -> Result<PreparedObservations, Error> {
        let observers = port
            .actors()
            .into_iter()
            .filter_map(|actor| {
                port.condition(actor)
                    .observation_area()
                    .map(|area| ObserverArea { actor, area })
            })
            .collect::<Vec<_>>();
        self.prepare_local_observations(store, now, &observers)
    }
    pub fn view(&self, store: &Store, actor: PersonId) -> EcologyView {
        let Some(known) = self.knowledge(store, actor) else {
            return EcologyView::default();
        };
        EcologyView {
            resources: known
                .known_resources
                .values()
                .rev()
                .take(32)
                .cloned()
                .collect(),
            plots: known.known_plots.values().rev().take(16).cloned().collect(),
        }
    }
    pub fn interrupt_gathering(&self, store: &mut Store, actor: PersonId) -> Result<(), Error> {
        self.remove_gathering(store, actor)?;
        Ok(())
    }
}

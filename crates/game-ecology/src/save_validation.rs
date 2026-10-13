//! Saved ecology relationships are checked by ecology against factual ports.
use crate::*;
use game_ecs::Store;

#[derive(Clone, Copy, Debug)]
pub struct SaveContext {
    pub now: SimTime,
    pub next_id: u64,
    pub physical_present: bool,
    pub decisions_present: bool,
    pub environment_day: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct InventoryFacts {
    pub good: Good,
    pub location: StockPosition,
    pub quantity: Quantity,
    pub capacity: Quantity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatheringActivity {
    pub resource: ResourceId,
    pub destination: InventoryId,
    pub quantity: Quantity,
    pub completes: SimTime,
}
pub trait SavePort {
    fn context(&self) -> SaveContext;
    fn actor_exists(&self, actor: PersonId) -> bool;
    fn area_exists(&self, area: AreaId) -> bool;
    fn occupied_plot(&self, site: SiteId) -> bool;
    fn cultivated_plot(&self, site: SiteId) -> bool;
    fn inventory(&self, id: InventoryId) -> Option<InventoryFacts>;
    fn gathering_activity(&self, actor: PersonId) -> Option<GatheringActivity>;
    fn physical_deadline(&self, actor: PersonId, at: SimTime) -> bool;
}
pub fn validate_save(
    runtime: Option<&Runtime>,
    store: &Store,
    port: &impl SavePort,
) -> Result<(), Error> {
    let bad = || Error::InvalidSave("invalid ecological deposits, plots or collection".into());
    let Some(runtime) = runtime else {
        if store.iter::<Resource>().next().is_some()
            || store.iter::<Plot>().next().is_some()
            || gatherings(store).next().is_some()
            || all_knowledge(store).next().is_some()
            || all_requests(store).next().is_some()
        {
            return Err(Error::InvalidSave(
                "ecological components have no runtime".into(),
            ));
        }
        return Ok(());
    };
    let context = port.context();
    if !context.physical_present
        || !context.decisions_present
        || runtime.validate(store).is_err()
        || context
            .environment_day
            .is_some_and(|day| runtime.day != day)
    {
        return Err(bad());
    }
    for (id, source) in runtime.resources(store) {
        if id.0 >= context.next_id
            || !port.area_exists(source.area)
            || source.access.iter().any(|actor| !port.actor_exists(*actor))
            || source
                .custodian
                .is_some_and(|actor| !port.actor_exists(actor))
        {
            return Err(bad());
        }
    }
    for (id, plot) in runtime.plots(store) {
        if id.0 >= context.next_id
            || !port.area_exists(plot.area)
            || plot.access.iter().any(|actor| !port.actor_exists(*actor))
            || plot
                .custodian
                .is_some_and(|actor| !port.actor_exists(actor))
            || port.occupied_plot(id)
        {
            return Err(bad());
        }
    }
    for (actor, job) in gatherings(store) {
        let source = runtime.resource(store, job.resource).ok_or_else(bad)?;
        let destination = port.inventory(job.destination).ok_or_else(bad)?;
        if !port.actor_exists(actor)
            || job.completes <= context.now
            || destination.good != source.kind.good()
            || destination.location != StockPosition::Carried(actor)
            || destination
                .capacity
                .0
                .checked_sub(destination.quantity.0)
                .is_none_or(|room| room < job.quantity.0)
            || job.tool.is_some_and(|id| {
                port.inventory(id).is_none_or(|tool| {
                    tool.good != Good::Tool || tool.location != StockPosition::Carried(actor)
                })
            })
            || (source.kind == ResourceKind::Coppice) != job.tool.is_some()
            || port.gathering_activity(actor)
                != Some(GatheringActivity {
                    resource: job.resource,
                    destination: job.destination,
                    quantity: job.quantity,
                    completes: job.completes,
                })
            || !port.physical_deadline(actor, job.completes)
        {
            return Err(bad());
        }
    }
    for (actor, row) in all_requests(store) {
        if !port.actor_exists(actor)
            || row
                .next_request
                .keys()
                .any(|id| runtime.resource(store, *id).is_none())
        {
            return Err(bad());
        }
    }
    for (actor, known) in all_knowledge(store) {
        if !port.actor_exists(actor)
            || known
                .observed_areas
                .iter()
                .any(|(area, at)| !port.area_exists(*area) || *at > context.now)
            || known.known_resources.iter().any(|(id, view)| {
                runtime
                    .resource(store, *id)
                    .is_none_or(|source| view.available > source.capacity)
                    || view.observed_at > context.now
                    || !port.area_exists(view.area)
                    || view
                        .custodian
                        .is_some_and(|actor| !port.actor_exists(actor))
            })
            || known.known_plots.iter().any(|(id, view)| {
                (runtime.plot(store, *id).is_none() && !port.cultivated_plot(*id))
                    || view.observed_at > context.now
                    || !port.area_exists(view.area)
                    || view
                        .custodian
                        .is_some_and(|actor| !port.actor_exists(actor))
            })
        {
            return Err(bad());
        }
    }
    Ok(())
}

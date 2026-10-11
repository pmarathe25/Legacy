//! Local perception reads actual land; dated memories remain personal.
use crate::*;
use game_ecs::{Batch, StableId, Store};

/// Actual observation location supplied by the spatial and bodily owners.
/// Roads, carried bodies, and actors unable to observe are omitted by the adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObserverArea {
    pub actor: PersonId,
    pub area: AreaId,
}

pub(super) fn remember_area(knowledge: &mut Knowledge, area: AreaId, now: SimTime) -> bool {
    let observed = &mut knowledge.observed_areas;
    if observed.get(&area).is_some_and(|at| *at > now) {
        return false;
    }
    if observed.len() == 16
        && !observed.contains_key(&area)
        && let Some(oldest) = observed
            .iter()
            .min_by_key(|(id, at)| (**at, **id))
            .map(|(id, _)| *id)
    {
        observed.remove(&oldest);
    }
    observed.insert(area, now);
    true
}

struct KnowledgeChange {
    actor: PersonId,
    before: Option<Knowledge>,
    after: Knowledge,
}

/// Own-row and relevant land guards precede one atomic knowledge batch.
/// Foreign observer facts are validated by their owners' enclosing transaction.
pub struct PreparedObservations {
    areas: BTreeSet<AreaId>,
    resources: Vec<(ResourceId, Resource)>,
    plots: Vec<(SiteId, Plot, Soil)>,
    changes: Vec<KnowledgeChange>,
}
impl PreparedObservations {
    pub fn observers(&self) -> usize {
        self.changes.len()
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<(), Error> {
        let resources = runtime
            .resources(store)
            .filter(|(_, row)| self.areas.contains(&row.area));
        if resources.ne(self.resources.iter().map(|(id, row)| (*id, row))) {
            return Err(Error::InvalidSave(
                "stale local resource observation".into(),
            ));
        }
        let plots = runtime
            .plots(store)
            .filter(|(_, row)| self.areas.contains(&row.area));
        if plots.ne(self.plots.iter().map(|(id, row, _)| (*id, row)))
            || self
                .plots
                .iter()
                .any(|(id, _, before)| runtime.soil(store, *id) != Some(before))
            || self.changes.iter().any(|change| {
                !store.contains(StableId(change.actor.0))
                    || runtime.knowledge(store, change.actor) != change.before.as_ref()
            })
        {
            return Err(Error::InvalidSave(
                "stale local ecological observation".into(),
            ));
        }
        let mut batch = Batch::new();
        for change in self.changes {
            batch.insert(StableId(change.actor.0), change.after);
        }
        store.apply_batch(batch)
    }
}
impl Runtime {
    pub fn prepare_local_observations(
        &self,
        store: &Store,
        now: SimTime,
        observers: &[ObserverArea],
    ) -> Result<PreparedObservations, Error> {
        let mut actors = BTreeMap::new();
        for facts in observers {
            if facts.actor.0 == 0
                || facts.area.0 == 0
                || !store.contains(StableId(facts.actor.0))
                || actors.insert(facts.actor, facts.area).is_some()
            {
                return Err(Error::InvalidSave(
                    "invalid or repeated ecological observer".into(),
                ));
            }
        }
        let areas: BTreeSet<_> = actors.values().copied().collect();
        let resources: Vec<_> = self
            .resources(store)
            .filter(|(_, row)| areas.contains(&row.area))
            .map(|(id, row)| (id, row.clone()))
            .collect();
        let mut plots = Vec::new();
        for (id, plot) in self
            .plots(store)
            .filter(|(_, row)| areas.contains(&row.area))
        {
            let soil = self
                .soil(store, id)
                .ok_or_else(|| Error::InvalidSave("missing observed plot soil".into()))?;
            plot.validate()?;
            soil.validate()?;
            if id != plot.id {
                return Err(Error::InvalidSave(
                    "inconsistent observed plot entity".into(),
                ));
            }
            plots.push((id, plot.clone(), soil.clone()));
        }
        let mut local_resources = BTreeMap::<AreaId, Vec<&Resource>>::new();
        for (id, row) in &resources {
            row.validate()?;
            if *id != row.id {
                return Err(Error::InvalidSave(
                    "inconsistent observed resource entity".into(),
                ));
            }
            local_resources.entry(row.area).or_default().push(row);
        }
        let mut local_plots = BTreeMap::<AreaId, Vec<(&Plot, &Soil)>>::new();
        for (_, row, soil) in &plots {
            local_plots.entry(row.area).or_default().push((row, soil));
        }
        let mut changes = Vec::new();
        for (actor, area) in actors {
            let before = self.knowledge(store, actor).cloned();
            let mut after = before.clone().unwrap_or_default();
            remember_area(&mut after, area, now);
            for source in local_resources.get(&area).into_iter().flatten() {
                after.known_resources.insert(
                    source.id,
                    ResourceView {
                        id: source.id,
                        area,
                        kind: source.kind,
                        available: source.remaining,
                        accessible: source.access.contains(&actor),
                        custodian: source.custodian,
                        contamination: source.contamination,
                        observed_at: now,
                    },
                );
            }
            for (plot, soil) in local_plots.get(&area).into_iter().flatten() {
                after.known_plots.insert(
                    plot.id,
                    PlotView {
                        id: plot.id,
                        area,
                        soil_fertility: soil.fertility,
                        soil_water: soil.water,
                        accessible: plot.access.contains(&actor),
                        custodian: plot.custodian,
                        observed_at: now,
                    },
                );
            }
            after.validate()?;
            changes.push(KnowledgeChange {
                actor,
                before,
                after,
            });
        }
        Ok(PreparedObservations {
            areas,
            resources,
            plots,
            changes,
        })
    }
}
#[cfg(test)]
#[path = "observation_tests.rs"]
mod tests;

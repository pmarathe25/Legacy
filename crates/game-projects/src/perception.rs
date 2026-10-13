//! Local project perception and account access are prepared before publication.
use crate::*;
use game_agreements::AgreementTerm;
use game_ecs::{Component, ReadFootprint, ReadGuard, ReadSet};

#[derive(Clone, Copy)]
pub struct Observer {
    pub actor: PersonId,
    pub area: Option<AreaId>,
}
pub struct ObservationFacts<'a> {
    pub now: SimTime,
    pub observers: &'a [Observer],
    pub accounts_available: bool,
}
pub fn observed_area(alive: bool, health: u64, location: Option<&Location>) -> Option<AreaId> {
    if !alive || health == 0 {
        return None;
    }
    match location {
        Some(Location::AtArea(area)) => Some(*area),
        _ => None,
    }
}
pub struct PreparedObservation {
    runtime: Runtime,
    source: ReadSet,
    projected: Option<ReadGuard>,
    origin: Option<ReadGuard>,
    memory: ReadSet,
    rows: Vec<(PersonId, Knowledge)>,
}
pub struct ObservationPublication {
    runtime: Runtime,
    guards: Vec<ReadGuard>,
}
struct ObservationRows<'a> {
    store: &'a Store,
    batch: Option<&'a Batch>,
    reads: ReadSet,
    admitted: ReadFootprint,
}
impl<'a> ObservationRows<'a> {
    fn read<C: Component>(&mut self, id: StableId) -> Result<Option<&'a C>, Error> {
        let opening = self.reads.read::<C>(self.store, id);
        if let Some(batch) = self.batch {
            if batch.touches::<C>(id) {
                self.admitted.protect::<C>(id);
            }
            batch.projected(self.store, id)
        } else {
            Ok(opening)
        }
    }
    fn accounts(&mut self, asset: &Enterprise) -> Result<EnterpriseAccountView, Error> {
        let mut obligations = Vec::new();
        for id in &asset.operating_agreements {
            let agreement = self
                .read::<game_agreements::Agreement>(StableId(id.0))?
                .ok_or(Error::Unavailable)?
                .clone();
            for id in &agreement.obligations {
                let row = self
                    .read::<game_agreements::Obligation>(StableId(id.0))?
                    .ok_or(Error::Unavailable)?
                    .clone();
                let delivery_good = if let AgreementTerm::Delivery { source, .. } = row.term {
                    Some(
                        self.read::<game_cargo::Inventory>(StableId(source.0))?
                            .ok_or(Error::Unavailable)?
                            .good,
                    )
                } else {
                    None
                };
                obligations.push(AccountObligation {
                    term: row.term,
                    performed: row.performed,
                    accrued: row.accrued,
                    defaulted: row.defaulted,
                    delivery_good,
                });
            }
        }
        account_view(asset, &obligations)
    }
}
impl Runtime {
    pub fn prepare_observation(
        &self,
        store: &Store,
        facts: ObservationFacts<'_>,
    ) -> Result<PreparedObservation, Error> {
        self.observation(store, None, None, facts)
    }
    /// Read the owner's already-staged source rows. The origin's exact guard
    /// remains part of publication, and observations guard their closing inputs.
    pub fn prepare_projected_observation(
        &self,
        store: &Store,
        batch: &Batch,
        origin: &ProjectFinalization,
        facts: ObservationFacts<'_>,
    ) -> Result<PreparedObservation, Error> {
        let (guard, writes) = origin.observation_origin();
        guard.validate(store, batch)?;
        self.observation(store, Some(batch), Some((guard, writes)), facts)
    }
    fn observation(
        &self,
        store: &Store,
        batch: Option<&Batch>,
        origin: Option<(ReadGuard, ReadFootprint)>,
        facts: ObservationFacts<'_>,
    ) -> Result<PreparedObservation, Error> {
        let mut people = BTreeSet::new();
        for observer in facts.observers {
            if !people.insert(observer.actor) {
                return Err(Error::Unavailable);
            }
        }
        let areas = facts
            .observers
            .iter()
            .filter_map(|observer| observer.area)
            .collect::<BTreeSet<_>>();
        let mut rows = ObservationRows {
            store,
            batch,
            reads: ReadSet::new(),
            admitted: origin
                .as_ref()
                .map(|(_, writes)| writes.clone())
                .unwrap_or_default(),
        };
        let scope = areas.clone();
        rows.reads
            .membership::<Project>(store, move |_, plan| scope.contains(&plan.area));
        let scope = areas.clone();
        rows.reads
            .membership::<Enterprise>(store, move |_, asset| scope.contains(&asset.area));
        let project_ids = projects(store)
            .map(|(id, _)| StableId(id.0))
            .chain(
                batch
                    .into_iter()
                    .flat_map(|batch| batch.edited_ids::<Project>()),
            )
            .collect::<BTreeSet<_>>();
        let mut local_projects = BTreeMap::<AreaId, Vec<(ProjectId, ProjectView)>>::new();
        for id in project_ids {
            let plan = if let Some(batch) = batch {
                batch.projected::<Project>(store, id)?
            } else {
                store.get::<Project>(id)
            };
            if let Some(plan) = plan.filter(|plan| areas.contains(&plan.area)) {
                let view = plan.observed_view(facts.now);
                let area = plan.area;
                rows.read::<Project>(id)?;
                local_projects
                    .entry(area)
                    .or_default()
                    .push((ProjectId(id.0), view));
            }
        }
        let enterprise_ids = enterprises(store)
            .map(|(id, _)| StableId(id.0))
            .chain(
                batch
                    .into_iter()
                    .flat_map(|batch| batch.edited_ids::<Enterprise>()),
            )
            .collect::<BTreeSet<_>>();
        let mut local_enterprises = BTreeMap::<
            AreaId,
            Vec<(
                Enterprise,
                BTreeSet<PersonId>,
                Option<EnterpriseAccountView>,
            )>,
        >::new();
        for id in enterprise_ids {
            let asset = if let Some(batch) = batch {
                batch.projected::<Enterprise>(store, id)?
            } else {
                store.get::<Enterprise>(id)
            };
            if let Some(asset) = asset.filter(|asset| areas.contains(&asset.area)) {
                let asset = asset.clone();
                rows.read::<Enterprise>(id)?;
                let mut audience = asset.beneficiaries.clone();
                for agreement in &asset.operating_agreements {
                    if let Some(agreement) =
                        rows.read::<game_agreements::Agreement>(StableId(agreement.0))?
                    {
                        audience.extend(&agreement.parties);
                    }
                }
                let accounts = if facts.accounts_available
                    && facts.observers.iter().any(|observer| {
                        observer.area == Some(asset.area) && audience.contains(&observer.actor)
                    }) {
                    rows.accounts(&asset).ok()
                } else {
                    None
                };
                local_enterprises
                    .entry(asset.area)
                    .or_default()
                    .push((asset, audience, accounts));
            }
        }
        let mut memory = ReadSet::new();
        let mut updates = Vec::new();
        for observer in facts.observers {
            let Some(area) = observer.area else {
                continue;
            };
            let mut known = memory
                .read::<Knowledge>(store, StableId(observer.actor.0))
                .cloned()
                .unwrap_or_default();
            for (id, view) in local_projects.get(&area).into_iter().flatten() {
                known.projects.insert(*id, view.clone());
            }
            for (asset, audience, accounts) in local_enterprises.get(&area).into_iter().flatten() {
                let accounts = audience
                    .contains(&observer.actor)
                    .then(|| accounts.clone())
                    .flatten();
                known
                    .enterprises
                    .insert(asset.id, asset.observed_view(facts.now, accounts));
            }
            known.validate()?;
            updates.push((observer.actor, known));
        }
        let (source, projected) = if let Some(batch) = batch {
            (
                ReadSet::new(),
                Some(rows.reads.bind(store, batch, &rows.admitted)?),
            )
        } else {
            (rows.reads, None)
        };
        Ok(PreparedObservation {
            runtime: self.clone(),
            source,
            projected,
            origin: origin.map(|(guard, _)| guard),
            memory,
            rows: updates,
        })
    }
    pub fn remembered_view(&self, store: &Store, actor: PersonId) -> ProjectsView {
        let Some(known) = knowledge(store, actor) else {
            return ProjectsView::default();
        };
        ProjectsView {
            projects: known.projects.values().rev().take(32).cloned().collect(),
            enterprises: known.enterprises.values().rev().take(32).cloned().collect(),
        }
    }
}
impl PreparedObservation {
    pub fn stage_into(
        mut self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<ObservationPublication, Error> {
        if runtime != &self.runtime {
            return Err(Error::Unavailable);
        }
        self.source.validate(store)?;
        self.memory.validate(store)?;
        if let Some(guard) = &self.projected {
            guard.validate(store, batch)?;
        }
        if let Some(guard) = &self.origin {
            guard.validate(store, batch)?;
        }
        let mut admitted = ReadFootprint::new();
        for (actor, _) in &self.rows {
            admitted.protect::<Knowledge>(StableId(actor.0));
        }
        if !admitted.is_untouched(batch) {
            return Err(Error::Unavailable);
        }
        for (actor, known) in self.rows {
            batch.insert(StableId(actor.0), known);
        }
        self.source.extend(self.memory);
        let mut guards = vec![self.source.bind(store, batch, &admitted)?];
        guards.extend(self.projected);
        guards.extend(self.origin);
        Ok(ObservationPublication {
            runtime: self.runtime,
            guards,
        })
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage_into(runtime, store, &mut batch)?
            .validate(runtime, store, &batch)?;
        store.apply_batch(batch)
    }
}
impl ObservationPublication {
    pub fn row_guards(&self) -> Vec<ReadGuard> {
        self.guards.clone()
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store, batch: &Batch) -> Result<(), Error> {
        if runtime != &self.runtime {
            return Err(Error::Unavailable);
        }
        for guard in &self.guards {
            guard.validate(store, batch)?;
        }
        Ok(())
    }
}

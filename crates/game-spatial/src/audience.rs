//! A local audience keeps its spatial phase borrow until batch admission.
use crate::*;
use game_ecs::{Batch, BatchKey, Component, ReadFootprint};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Audience {
    /// Exact raw equality, including road fractions and carrier identity.
    Raw(Location),
    /// The same coarse area and adjacent road candidates as LocalRoster.
    Nearby(Location),
}
pub struct PreparedLocalAudience<'a, 's, C: Component> {
    roster: &'a LocalRoster<'s>,
    marker: std::marker::PhantomData<C>,
    audience: Audience,
    reads: ReadFootprint,
    members: BTreeSet<StableId>,
}
pub struct LocalAudienceGuard<'a, 's, C: Component> {
    source: PreparedLocalAudience<'a, 's, C>,
    key: BatchKey,
}
impl<'s> LocalRoster<'s> {
    pub fn prepare_audience<C: Component>(
        &self,
        audience: Audience,
    ) -> Result<PreparedLocalAudience<'_, 's, C>, Error> {
        let mut reads = ReadFootprint::new();
        let mut members = BTreeSet::new();
        let candidates: Vec<_> = match &audience {
            Audience::Raw(raw) => {
                let resolved = match raw {
                    Location::Carried { carrier } => resolve(self.store, StableId(carrier.0))?,
                    raw => raw.clone(),
                };
                self.at_location(&resolved).collect()
            }
            Audience::Nearby(location) => self.at_location(location).collect(),
        };
        for id in candidates {
            let related = match &audience {
                Audience::Raw(raw) => location(self.store, id) == Some(raw),
                Audience::Nearby(_) => true,
            };
            if related && self.store.get::<C>(id).is_some() {
                reads.extend(location_reads(self.store, id)?);
                members.insert(id);
            }
        }
        if let Audience::Raw(Location::Carried { carrier }) = &audience {
            reads.extend(location_reads(self.store, StableId(carrier.0))?);
        }
        Ok(PreparedLocalAudience {
            roster: self,
            marker: std::marker::PhantomData,
            audience,
            reads,
            members,
        })
    }
}
impl<'a, 's, C: Component> PreparedLocalAudience<'a, 's, C> {
    pub fn stage_into(self, batch: &Batch) -> Result<LocalAudienceGuard<'a, 's, C>, Error> {
        let guard = LocalAudienceGuard {
            source: self,
            key: batch.key(),
        };
        guard.validate_batch(batch)?;
        Ok(guard)
    }
}
impl<C: Component> LocalAudienceGuard<'_, '_, C> {
    pub fn validate_batch(&self, batch: &Batch) -> Result<(), Error> {
        if self.key != batch.key() || !self.source.reads.is_untouched(batch) {
            return Err(Error::Unavailable);
        }
        // Marker values belong to eligibility owners. Spatial guards presence.
        for id in &self.source.members {
            let edits = batch
                .edited_ids::<C>()
                .filter(|edited| edited == id)
                .count();
            if edits > 0
                && (edits != 1 || !batch.insertions::<C>().any(|(edited, _)| edited == *id))
            {
                return Err(Error::Unavailable);
            }
        }
        for (id, raw) in batch.insertions::<Location>() {
            let actor = self.source.roster.store.get::<C>(id).is_some()
                || batch.insertions::<C>().any(|(actor, _)| actor == id);
            let carrier = self
                .source
                .roster
                .carried_by
                .get(&id)
                .into_iter()
                .flatten()
                .any(|actor| self.source.roster.store.get::<C>(*actor).is_some());
            if !actor && !carrier {
                continue;
            }
            let related = match &self.source.audience {
                Audience::Raw(location) => raw == location,
                Audience::Nearby(query) => nearby(
                    query,
                    &projected_anchor(self.source.roster.store, batch, id)?,
                ),
            };
            if related {
                return Err(Error::Unavailable);
            }
        }
        for (id, _) in batch.insertions::<C>() {
            if self.source.roster.store.get::<C>(id).is_some() {
                continue;
            }
            let related = match &self.source.audience {
                Audience::Raw(location) => {
                    projected_location(self.source.roster.store, batch, id)? == location
                }
                Audience::Nearby(query) => nearby(
                    query,
                    &projected_anchor(self.source.roster.store, batch, id)?,
                ),
            };
            if related {
                return Err(Error::Unavailable);
            }
        }
        Ok(())
    }
}
fn projected_location<'a>(
    store: &'a Store,
    batch: &'a Batch,
    id: StableId,
) -> Result<&'a Location, Error> {
    let edits = batch
        .edited_ids::<Location>()
        .filter(|edited| *edited == id)
        .count();
    if edits > 0 {
        if edits != 1 {
            return Err(Error::Unavailable);
        }
        batch
            .insertions::<Location>()
            .find(|(edited, _)| *edited == id)
            .map(|(_, row)| row)
    } else {
        location(store, id)
    }
    .ok_or(Error::Unavailable)
}
fn projected_anchor(store: &Store, batch: &Batch, mut id: StableId) -> Result<Location, Error> {
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(id) {
            return Err(Error::Unavailable);
        }
        let edits = batch
            .edited_ids::<Location>()
            .filter(|edited| *edited == id)
            .count();
        let raw = if edits > 0 {
            if edits != 1 {
                return Err(Error::Unavailable);
            }
            batch
                .insertions::<Location>()
                .find(|(edited, _)| *edited == id)
                .map(|(_, row)| row)
        } else {
            location(store, id)
        }
        .ok_or(Error::Unavailable)?;
        match raw {
            Location::Carried { carrier } => id = StableId(carrier.0),
            raw => return Ok(raw.clone()),
        }
    }
}
fn nearby(query: &Location, candidate: &Location) -> bool {
    match (query, candidate) {
        (Location::AtArea(area), Location::AtArea(other)) => area == other,
        (
            Location::AtArea(area),
            Location::InTransit { from, to, .. } | Location::StoppedOnRoad { from, to, .. },
        ) => area == from || area == to,
        (
            Location::InTransit { from, to, .. } | Location::StoppedOnRoad { from, to, .. },
            Location::AtArea(area),
        ) => area == from || area == to,
        (
            Location::InTransit { from, to, .. } | Location::StoppedOnRoad { from, to, .. },
            Location::InTransit {
                from: other_from,
                to: other_to,
                ..
            }
            | Location::StoppedOnRoad {
                from: other_from,
                to: other_to,
                ..
            },
        ) => (from.min(to), from.max(to)) == (other_from.min(other_to), other_from.max(other_to)),
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Store {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        schema.register::<bool>("test/actor", |_| Ok(())).unwrap();
        let mut store = Store::with_schema(schema);
        for (id, area) in [(1, 10), (2, 11), (3, 30)] {
            set_location(&mut store, StableId(id), Location::AtArea(AreaId(area))).unwrap();
            store.insert(StableId(id), true).unwrap();
        }
        store
    }
    #[test]
    fn local_guard_rejects_new_moves_removals_and_carrying_but_allows_remote_edits() {
        let store = fixture();
        let roster = LocalRoster::build(&store).unwrap();
        for edit in 0..6 {
            let mut batch = Batch::new();
            let guard = roster
                .prepare_audience::<bool>(Audience::Nearby(Location::AtArea(AreaId(10))))
                .unwrap()
                .stage_into(&batch)
                .unwrap();
            match edit {
                0 => {
                    batch.remove::<Location>(StableId(1));
                }
                1 => {
                    batch.insert(StableId(4), true);
                    batch.insert(StableId(4), Location::AtArea(AreaId(10)));
                }
                2 => {
                    batch.insert(StableId(3), Location::AtArea(AreaId(10)));
                }
                3 => {
                    batch.insert(StableId(4), true);
                    batch.insert(
                        StableId(4),
                        Location::Carried {
                            carrier: game_core::PersonId(1),
                        },
                    );
                }
                4 => {
                    batch.insert(StableId(3), Location::AtArea(AreaId(40)));
                }
                _ => {
                    batch.insert(StableId(9), Location::AtArea(AreaId(10)));
                }
            }
            assert_eq!(guard.validate_batch(&batch).is_ok(), edit >= 4);
        }
    }
    #[test]
    fn projected_carrier_chains_and_actor_kind_changes_cannot_add_an_audience() {
        let mut store = fixture();
        set_location(&mut store, StableId(8), Location::AtArea(AreaId(30))).unwrap();
        set_location(
            &mut store,
            StableId(3),
            Location::Carried {
                carrier: game_core::PersonId(8),
            },
        )
        .unwrap();
        set_location(&mut store, StableId(9), Location::AtArea(AreaId(10))).unwrap();
        let roster = LocalRoster::build(&store).unwrap();
        for changes in 0..3 {
            let mut batch = Batch::new();
            let guard = roster
                .prepare_audience::<bool>(Audience::Nearby(Location::AtArea(AreaId(10))))
                .unwrap()
                .stage_into(&batch)
                .unwrap();
            match changes {
                0 => {
                    batch.insert(StableId(8), Location::AtArea(AreaId(10)));
                }
                1 => {
                    batch.insert(StableId(9), true);
                }
                _ => {
                    batch.insert(StableId(7), true);
                    batch.insert(
                        StableId(7),
                        Location::Carried {
                            carrier: game_core::PersonId(12),
                        },
                    );
                    batch.insert(
                        StableId(12),
                        Location::Carried {
                            carrier: game_core::PersonId(1),
                        },
                    );
                }
            }
            assert!(guard.validate_batch(&batch).is_err());
        }
        let mut batch = Batch::new();
        let guard = roster
            .prepare_audience::<bool>(Audience::Nearby(Location::AtArea(AreaId(10))))
            .unwrap()
            .stage_into(&batch)
            .unwrap();
        batch.insert(StableId(1), false);
        guard.validate_batch(&batch).unwrap();
        batch.remove::<bool>(StableId(1));
        assert!(guard.validate_batch(&batch).is_err());
        let mut batch = Batch::new();
        let guard = roster
            .prepare_audience::<bool>(Audience::Nearby(Location::AtArea(AreaId(10))))
            .unwrap()
            .stage_into(&batch)
            .unwrap();
        batch.remove::<bool>(StableId(1));
        assert!(guard.validate_batch(&batch).is_err());
        let mut batch = Batch::new();
        batch.insert(StableId(9), Location::AtArea(AreaId(10)));
        let guard = roster
            .prepare_audience::<bool>(Audience::Nearby(Location::AtArea(AreaId(10))))
            .unwrap()
            .stage_into(&batch)
            .unwrap();
        guard.validate_batch(&batch).unwrap();
    }
    #[test]
    fn raw_guard_uses_exact_carrier_and_road_state_and_is_batch_affine() {
        let mut store = fixture();
        let road = Location::StoppedOnRoad {
            from: AreaId(10),
            to: AreaId(11),
            progress: game_core::SimDuration(2),
            duration: game_core::SimDuration(10),
        };
        set_location(&mut store, StableId(1), road.clone()).unwrap();
        let roster = LocalRoster::build(&store).unwrap();
        let mut batch = Batch::new();
        let guard = roster
            .prepare_audience::<bool>(Audience::Raw(road.clone()))
            .unwrap()
            .stage_into(&batch)
            .unwrap();
        batch.insert(
            StableId(4),
            Location::StoppedOnRoad {
                from: AreaId(10),
                to: AreaId(11),
                progress: game_core::SimDuration(3),
                duration: game_core::SimDuration(10),
            },
        );
        assert!(guard.validate_batch(&batch).is_ok());
        batch.insert(StableId(5), true);
        batch.insert(StableId(5), road);
        assert!(guard.validate_batch(&batch).is_err());
        assert!(guard.validate_batch(&Batch::new()).is_err());
        let mut batch = Batch::new();
        let guard = roster
            .prepare_audience::<bool>(Audience::Raw(Location::Carried {
                carrier: game_core::PersonId(1),
            }))
            .unwrap()
            .stage_into(&batch)
            .unwrap();
        batch.insert(
            StableId(4),
            Location::Carried {
                carrier: game_core::PersonId(2),
            },
        );
        assert!(guard.validate_batch(&batch).is_ok());
        batch.insert(StableId(5), true);
        batch.insert(
            StableId(5),
            Location::Carried {
                carrier: game_core::PersonId(1),
            },
        );
        assert!(guard.validate_batch(&batch).is_err());
    }
}

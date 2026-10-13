//! Deterministic scene construction and intrinsic map admission.
use crate::*;
use game_ecs::Store;
#[derive(Clone, Debug)]
pub enum PreparedMap {
    Existing(Map),
    New(Map),
}
impl PreparedMap {
    pub fn map(&self) -> &Map {
        match self {
            Self::Existing(row) | Self::New(row) => row,
        }
    }
    pub fn is_new(&self) -> bool {
        matches!(self, Self::New(_))
    }
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut game_ecs::Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        use game_ecs::{ReadFootprint, ReadSet, StableId};
        let row = self.map().clone();
        let is_new = self.is_new();
        let mut reads = ReadSet::new();
        let mut admitted = ReadFootprint::new();
        let id = StableId(row.id.0);
        if is_new {
            reads.exclusive_entity(store, id)?;
            reads.expect_row::<Map>(id, None);
            admitted.protect::<Map>(id);
        } else {
            reads.expect_row(id, Some(row.clone()));
        }
        match game_transport::position_from_location(&row.location, SimTime(0))?.key() {
            game_transport::PositionKey::Area(area) => {
                let id = StableId(area.0);
                reads.read::<MapRef>(store, id);
                if is_new {
                    if batch.touches::<MapRef>(id) {
                        return Err(Error::Unavailable);
                    }
                    admitted.protect::<MapRef>(id);
                }
            }
            game_transport::PositionKey::Road { from, .. } => {
                let id = StableId(from.0);
                reads.read::<RoadMapRefs>(store, id);
                if is_new {
                    if batch.touches::<RoadMapRefs>(id) {
                        return Err(Error::Unavailable);
                    }
                    admitted.protect::<RoadMapRefs>(id);
                }
            }
        }
        if is_new {
            crate::storage::append_map(store, row, batch)?;
        }
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<CombatMapId, Error> {
        let id = self.map().id;
        let mut batch = game_ecs::Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)?;
        Ok(id)
    }
}
impl Map {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.version != 1
            || !(1..=1_000_000).contains(&self.width)
            || !(1..=1_000_000).contains(&self.height)
            || self.obstacles.iter().enumerate().any(|(i, o)| {
                o.index != i as u32
                    || !self.contains(o.minimum)
                    || !self.contains(o.maximum)
                    || o.minimum.x >= o.maximum.x
                    || o.minimum.y >= o.maximum.y
            })
            || self.exits.iter().any(|(_, point)| !self.contains(*point))
        {
            return Err(Error::InvalidSave("invalid combat map".into()));
        }
        Ok(())
    }
}
impl World {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_map(
        &self,
        store: &Store,
        location: &Location,
        now: SimTime,
        seed: u64,
        id: CombatMapId,
        links: &[AreaId],
        areas_exist: bool,
    ) -> Result<PreparedMap, Error> {
        if let Some(row) = map_for_location(store, location, now)? {
            return Ok(PreparedMap::Existing(row.clone()));
        }
        if !areas_exist {
            return Err(Error::UnknownArea);
        }
        let key = game_transport::position_from_location(location, now)?.key();
        let (area, location, offset, exits) = match key {
            game_transport::PositionKey::Area(area) => (
                area,
                Location::AtArea(area),
                ((seed ^ area.0).wrapping_mul(0x9e3779b97f4a7c15) % 2_000) as i64,
                links
                    .iter()
                    .enumerate()
                    .map(|(i, area)| {
                        (
                            *area,
                            Point {
                                x: if i.is_multiple_of(2) { 0 } else { 24_000 },
                                y: 8_000,
                            },
                        )
                    })
                    .collect(),
            ),
            game_transport::PositionKey::Road { from, to, .. } => (
                from,
                location.clone(),
                0,
                vec![
                    (from, Point { x: 0, y: 8_000 }),
                    (
                        to,
                        Point {
                            x: 24_000,
                            y: 8_000,
                        },
                    ),
                ],
            ),
        };
        let row = Map {
            id,
            version: 1,
            area,
            location,
            width: 24_000,
            height: 16_000,
            obstacles: vec![ObstacleView {
                index: 0,
                minimum: Point {
                    x: 12_000 + offset,
                    y: 4_000,
                },
                maximum: Point {
                    x: 14_000 + offset,
                    y: 10_000,
                },
                integrity: 180_000,
            }],
            exits,
        };
        row.validate()?;
        Ok(PreparedMap::New(row))
    }
}

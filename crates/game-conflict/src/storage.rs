//! Conflict components live in the shared store; scene lookups are keyed by place.
use crate::*;
use game_ecs::{Batch, Schema, StableId, Store};

#[derive(Clone, Debug)]
pub struct PreparedNewbornProfile {
    child: PersonId,
    reads: game_ecs::ReadSet,
}
pub fn prepare_newborn_profile(
    store: &Store,
    child: PersonId,
) -> Result<PreparedNewbornProfile, Error> {
    if child.0 == 0 {
        return Err(Error::Unavailable);
    }
    let mut reads = game_ecs::ReadSet::new();
    if reads.entity(store, StableId(child.0))
        || reads.read::<Profile>(store, StableId(child.0)).is_some()
    {
        return Err(Error::Unavailable);
    }
    Ok(PreparedNewbornProfile { child, reads })
}
impl PreparedNewbornProfile {
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.reads.validate(store)?;
        let id = StableId(self.child.0);
        if batch.touches::<Profile>(id) {
            return Err(Error::Unavailable);
        }
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<Profile>(id);
        batch.insert(id, Profile { initiative: 100 });
        self.reads.bind(store, batch, &admitted)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapRef {
    pub map: CombatMapId,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoadMapRefs {
    #[serde(with = "road_map_rows")]
    pub maps: BTreeMap<RoadSceneKey, CombatMapId>,
}
mod road_map_rows {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        rows: &BTreeMap<RoadSceneKey, CombatMapId>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        rows.iter().collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<RoadSceneKey, CombatMapId>, D::Error> {
        let rows = Vec::<(RoadSceneKey, CombatMapId)>::deserialize(deserializer)?;
        let mut result = BTreeMap::new();
        for (key, value) in rows {
            if result.insert(key, value).is_some() {
                return Err(serde::de::Error::custom("duplicate road scene reference"));
            }
        }
        Ok(result)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RoadSceneKey {
    pub to: AreaId,
    pub numerator: u64,
    pub denominator: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participation {
    pub encounter: EncounterId,
}

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Profile>("conflict/profile", |row| {
        if row.initiative <= 1_000_000 {
            Ok(())
        } else {
            Err(Error::InvalidSave("invalid combat profile".into()))
        }
    })?;
    schema.register::<Map>("conflict/map", Map::validate)?;
    schema.register::<MapRef>("conflict/map_ref", |_| Ok(()))?;
    schema.register::<RoadMapRefs>("conflict/road_maps", |_| Ok(()))?;
    schema.register::<Encounter>("conflict/encounter", |_| Ok(()))?;
    schema.register::<Participation>("conflict/participation", |_| Ok(()))?;
    schema.register::<Captivity>("conflict/captivity", |_| Ok(()))
}
macro_rules! row {
    ($get:ident,$get_mut:ident,$insert:ident,$all:ident,$ty:ty,$id:ty,$wrap:ident) => {
        pub fn $get(store: &Store, id: $id) -> Option<&$ty> {
            store.get(StableId(id.0))
        }
        pub fn $get_mut(store: &mut Store, id: $id) -> Option<&mut $ty> {
            store.get_mut(StableId(id.0))
        }
        pub fn $insert(store: &mut Store, id: $id, row: $ty) -> Result<(), Error> {
            store.insert_or_spawn(StableId(id.0), row)
        }
        pub fn $all(store: &Store) -> impl Iterator<Item = ($id, &$ty)> {
            store.iter::<$ty>().map(|(id, row)| ($wrap(id.0), row))
        }
    };
}
row!(
    profile,
    profile_mut,
    insert_profile,
    profiles,
    Profile,
    PersonId,
    PersonId
);
row!(
    map,
    map_mut,
    insert_map_row,
    maps,
    Map,
    CombatMapId,
    CombatMapId
);
row!(
    encounter,
    encounter_mut,
    insert_encounter_row,
    encounters,
    Encounter,
    EncounterId,
    EncounterId
);
row!(
    captivity,
    captivity_mut,
    insert_captivity,
    captivities,
    Captivity,
    PersonId,
    PersonId
);

pub fn remove_captivity(store: &mut Store, actor: PersonId) -> Result<Option<Captivity>, Error> {
    store.remove(StableId(actor.0))
}
pub fn area_map(store: &Store, area: AreaId) -> Option<&Map> {
    map(store, store.get::<MapRef>(StableId(area.0))?.map)
}
pub fn map_for_location<'a>(
    store: &'a Store,
    location: &Location,
    now: SimTime,
) -> Result<Option<&'a Map>, Error> {
    match game_transport::position_from_location(location, now)?.key() {
        game_transport::PositionKey::Area(area) => Ok(area_map(store, area)),
        game_transport::PositionKey::Road {
            from,
            to,
            numerator,
            denominator,
        } => Ok(store
            .get::<RoadMapRefs>(StableId(from.0))
            .and_then(|refs| {
                refs.maps.get(&RoadSceneKey {
                    to,
                    numerator,
                    denominator,
                })
            })
            .and_then(|id| map(store, *id))),
    }
}
pub(crate) fn append_map(store: &Store, row: Map, batch: &mut Batch) -> Result<(), Error> {
    row.validate()?;
    if map_for_location(store, &row.location, SimTime(0))?
        .is_some_and(|existing| existing.id != row.id)
        || map(store, row.id).is_some_and(|existing| {
            game_transport::position_from_location(&existing.location, SimTime(0))
                .map(|p| p.key())
                .ok()
                != game_transport::position_from_location(&row.location, SimTime(0))
                    .map(|p| p.key())
                    .ok()
        })
    {
        return Err(Error::Unavailable);
    }
    match game_transport::position_from_location(&row.location, SimTime(0))?.key() {
        game_transport::PositionKey::Area(area) => {
            batch.insert(StableId(area.0), MapRef { map: row.id });
        }
        game_transport::PositionKey::Road {
            from,
            to,
            numerator,
            denominator,
        } => {
            let mut refs = store
                .get::<RoadMapRefs>(StableId(from.0))
                .cloned()
                .unwrap_or_default();
            refs.maps.insert(
                RoadSceneKey {
                    to,
                    numerator,
                    denominator,
                },
                row.id,
            );
            batch.insert(StableId(from.0), refs);
        }
    }
    batch.insert(StableId(row.id.0), row);
    Ok(())
}
pub fn insert_map(store: &mut Store, row: Map) -> Result<(), Error> {
    let mut batch = Batch::new();
    append_map(store, row, &mut batch)?;
    store.apply_batch(batch)
}
pub fn insert_encounter(store: &mut Store, row: Encounter) -> Result<(), Error> {
    let mut batch = Batch::new();
    for actor in row.fighters.keys() {
        batch.insert(StableId(actor.0), Participation { encounter: row.id });
    }
    batch.insert(StableId(row.id.0), row);
    store.apply_batch(batch)
}
pub fn actor_encounter(store: &Store, actor: PersonId) -> Option<&Encounter> {
    encounter(
        store,
        store.get::<Participation>(StableId(actor.0))?.encounter,
    )
}

pub fn area_map_mut(store: &mut Store, area: AreaId) -> Option<&mut Map> {
    let id = store.get::<MapRef>(StableId(area.0))?.map;
    map_mut(store, id)
}

//! Shared soil data; growth and recovery policies belong to its consumers.
use game_core::{Error, SiteId};
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Soil {
    pub fertility: u16,
    pub water: u64,
    pub capacity: u64,
}
impl Soil {
    pub fn validate(&self) -> Result<(), Error> {
        if self.fertility > 1_000 || self.capacity == 0 || self.water > self.capacity {
            return Err(Error::InvalidConfiguration("invalid soil component".into()));
        }
        Ok(())
    }
}

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Soil>("land/soil", Soil::validate)
}
pub fn soil(store: &Store, id: SiteId) -> Option<&Soil> {
    store.get(StableId(id.0))
}
pub fn soil_mut(store: &mut Store, id: SiteId) -> Option<&mut Soil> {
    store.get_mut(StableId(id.0))
}
pub fn soils(store: &Store) -> impl Iterator<Item = (SiteId, &Soil)> {
    store.iter::<Soil>().map(|(id, row)| (SiteId(id.0), row))
}
pub fn insert_soil(store: &mut Store, id: SiteId, value: Soil) -> Result<(), Error> {
    value.validate()?;
    store.insert_or_spawn(StableId(id.0), value)
}
pub fn remove_soil(store: &mut Store, id: SiteId) -> Result<Option<Soil>, Error> {
    if !store.contains(StableId(id.0)) {
        return Ok(None);
    }
    store.remove(StableId(id.0))
}

/// Each soil parcel has exactly one land use. Consumers supply parcel IDs;
/// this owner need not know whether a use is a field, plot, or another type.
pub fn validate_uses(store: &Store, uses: impl IntoIterator<Item = SiteId>) -> Result<(), Error> {
    let mut expected = std::collections::BTreeSet::new();
    for id in uses {
        if id.0 == 0 || !expected.insert(id) {
            return Err(Error::InvalidSave(
                "inconsistent shared soil ownership".into(),
            ));
        }
    }
    if soils(store).map(|(id, _)| id).ne(expected) {
        return Err(Error::InvalidSave(
            "inconsistent shared soil ownership".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        schema
            .register::<String>("test/land-use", |_| Ok(()))
            .unwrap();
        Store::with_schema(schema)
    }
    fn sample() -> Soil {
        Soil {
            fertility: 900,
            water: 75,
            capacity: 120,
        }
    }

    #[test]
    fn land_use_validation_rejects_missing_orphan_and_duplicate_ownership() {
        let mut store = store();
        insert_soil(&mut store, SiteId(7), sample()).unwrap();
        insert_soil(&mut store, SiteId(8), sample()).unwrap();
        assert!(validate_uses(&store, [SiteId(8), SiteId(7)]).is_ok());
        for uses in [
            vec![SiteId(7)],
            vec![SiteId(7), SiteId(8), SiteId(9)],
            vec![SiteId(7), SiteId(8), SiteId(7)],
            vec![SiteId(0), SiteId(7), SiteId(8)],
        ] {
            assert!(validate_uses(&store, uses).is_err());
        }
        // Changing a consumer's land-use type leaves this invariant intact.
        store.insert(StableId(7), "field".to_string()).unwrap();
        assert!(validate_uses(&store, [SiteId(7), SiteId(8)]).is_ok());
    }

    #[test]
    fn land_use_changes_preserve_one_soil_component() {
        let mut store = store();
        let id = SiteId(7);
        insert_soil(&mut store, id, sample()).unwrap();
        store.insert(StableId(id.0), "plot".to_string()).unwrap();
        store.remove::<String>(StableId(id.0)).unwrap();
        store.insert(StableId(id.0), "field".to_string()).unwrap();
        assert_eq!(soil(&store, id), Some(&sample()));
        assert_eq!(soils(&store).count(), 1);
        let snapshot = store.snapshot().unwrap();
        let encoded = serde_json::to_vec(&snapshot).unwrap();
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        schema
            .register::<String>("test/land-use", |_| Ok(()))
            .unwrap();
        let restored = Store::restore(schema, serde_json::from_slice(&encoded).unwrap()).unwrap();
        assert_eq!(store, restored);
    }

    #[test]
    fn invalid_soil_never_creates_or_replaces_an_entity() {
        let mut store = store();
        let before = store.clone();
        for value in [
            Soil {
                fertility: 1_001,
                ..sample()
            },
            Soil {
                capacity: 0,
                ..sample()
            },
            Soil {
                water: 121,
                ..sample()
            },
        ] {
            assert!(insert_soil(&mut store, SiteId(9), value).is_err());
            assert_eq!(store, before);
            assert!(!store.contains(StableId(9)));
        }
        assert!(insert_soil(&mut store, SiteId(0), sample()).is_err());
        assert_eq!(store, before);
        insert_soil(&mut store, SiteId(9), sample()).unwrap();
        let before = store.clone();
        assert!(
            insert_soil(
                &mut store,
                SiteId(9),
                Soil {
                    water: 121,
                    ..sample()
                }
            )
            .is_err()
        );
        assert_eq!(store, before);
    }

    #[test]
    fn removal_preserves_other_components_and_snapshot_validation_rejects_bad_water() {
        let mut store = store();
        insert_soil(&mut store, SiteId(3), sample()).unwrap();
        store.insert(StableId(3), "plot".to_string()).unwrap();
        let mut encoded = serde_json::to_value(store.snapshot().unwrap()).unwrap();
        encoded["columns"]["land/soil"][0]["value"]["water"] = serde_json::json!(121);
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        schema
            .register::<String>("test/land-use", |_| Ok(()))
            .unwrap();
        assert!(Store::restore(schema, serde_json::from_value(encoded).unwrap()).is_err());
        assert_eq!(remove_soil(&mut store, SiteId(3)).unwrap(), Some(sample()));
        assert_eq!(
            store.get::<String>(StableId(3)).map(String::as_str),
            Some("plot")
        );
        assert_eq!(remove_soil(&mut store, SiteId(3)).unwrap(), None);
        assert_eq!(remove_soil(&mut store, SiteId(4)).unwrap(), None);
    }
}

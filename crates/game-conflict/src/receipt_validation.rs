//! Saved receipt references checked against Conflict's canonical rows.
use game_core::{CombatMapId, EncounterId, Error, PersonId};
use game_ecs::Store;

fn require(valid: bool) -> Result<(), Error> {
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    }
}

pub fn validate_reinforcement(
    store: &Store,
    encounter: EncounterId,
    actor: PersonId,
) -> Result<(), Error> {
    require(crate::encounter(store, encounter).is_some_and(|row| row.fighters.contains_key(&actor)))
}

pub fn validate_obstacle_damage(
    store: &Store,
    map: CombatMapId,
    obstacle: u32,
) -> Result<(), Error> {
    require(
        crate::map(store, map)
            .is_some_and(|row| row.obstacles.iter().any(|entry| entry.index == obstacle)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Map, ObstacleView};
    use game_core::{AreaId, Location, Point};

    #[test]
    fn obstacle_receipts_use_native_indices_and_require_the_map() {
        let mut store = crate::test_store();
        crate::insert_map(
            &mut store,
            Map {
                id: CombatMapId(1),
                version: 1,
                area: AreaId(1),
                location: Location::AtArea(AreaId(1)),
                width: 100,
                height: 100,
                obstacles: vec![ObstacleView {
                    index: 0,
                    minimum: Point { x: 10, y: 10 },
                    maximum: Point { x: 20, y: 20 },
                    integrity: 0,
                }],
                exits: vec![],
            },
        )
        .unwrap();
        assert_eq!(validate_obstacle_damage(&store, CombatMapId(1), 0), Ok(()));
        assert!(validate_obstacle_damage(&store, CombatMapId(1), 1).is_err());
        assert!(validate_obstacle_damage(&store, CombatMapId(2), 0).is_err());
        assert!(validate_reinforcement(&store, EncounterId(1), PersonId(1)).is_err());
    }

    #[test]
    fn reinforcement_receipts_require_membership_in_the_named_encounter() {
        let mut store = crate::test_store();
        let actor = PersonId(1);
        crate::insert_encounter(
            &mut store,
            crate::Encounter {
                id: EncounterId(2),
                area: AreaId(1),
                location: Location::AtArea(AreaId(1)),
                map: CombatMapId(1),
                round: 0,
                started: game_core::SimTime(0),
                ends: game_core::SimTime(10),
                fighters: std::collections::BTreeMap::from([(
                    actor,
                    crate::Fighter {
                        actor,
                        joins_round: 0,
                        team: 0,
                        position: Point { x: 0, y: 0 },
                        surrendered: false,
                        withdrawn: true,
                        defending: false,
                        retreat: None,
                        known: std::collections::BTreeMap::new(),
                        search: crate::SearchState::default(),
                    },
                )]),
                order: vec![],
                cursor: 0,
                active: false,
            },
        )
        .unwrap();
        assert_eq!(
            validate_reinforcement(&store, EncounterId(2), actor),
            Ok(())
        );
        assert!(validate_reinforcement(&store, EncounterId(2), PersonId(3)).is_err());
        assert!(validate_reinforcement(&store, EncounterId(3), actor).is_err());
    }
}

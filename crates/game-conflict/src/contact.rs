//! Tactical contact uses canonical per-person participation, without a world scan.
use crate::*;
use game_ecs::Store;
pub struct TacticalPlacement {
    pub encounter: EncounterId,
    pub map: CombatMapId,
    pub position: Point,
}
impl World {
    pub fn physical_contact_projected(
        &self,
        store: &Store,
        projected: &Encounter,
        first: PersonId,
        second: PersonId,
    ) -> bool {
        let placement = |actor| {
            if let Some(fighter) = projected.fighters.get(&actor) {
                return (projected.active && !fighter.withdrawn).then_some(TacticalPlacement {
                    encounter: projected.id,
                    map: projected.map,
                    position: fighter.position,
                });
            }
            self.tactical_placement(store, actor)
        };
        match (placement(first), placement(second)) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                a.encounter == b.encounter
                    && distance(a.position, b.position) <= u128::from(self.rules.reach).pow(2)
                    && map(store, a.map).is_some_and(|map| map.clear(a.position, b.position))
            }
            _ => false,
        }
    }
    pub fn tactical_clear(
        &self,
        store: &Store,
        first: Option<(EncounterId, PersonId)>,
        second: Option<(EncounterId, PersonId)>,
        contact: bool,
    ) -> bool {
        match (first, second) {
            (None, None) => true,
            (Some((scene, a)), Some((other, b))) if scene == other => {
                if a == b {
                    return true;
                }
                encounter(store, scene).is_some_and(|encounter| {
                    let positions = encounter.fighters.get(&a).zip(encounter.fighters.get(&b));
                    positions.is_some_and(|(a, b)| {
                        map(store, encounter.map).is_some_and(|map| {
                            map.clear(a.position, b.position)
                                && (!contact
                                    || distance(a.position, b.position)
                                        <= u128::from(self.rules.reach).pow(2))
                        })
                    })
                })
            }
            _ => false,
        }
    }
    pub fn tactical_placement(&self, store: &Store, actor: PersonId) -> Option<TacticalPlacement> {
        let encounter = actor_encounter(store, actor)?;
        if !encounter.active {
            return None;
        }
        let fighter = encounter.fighters.get(&actor).filter(|f| !f.withdrawn)?;
        Some(TacticalPlacement {
            encounter: encounter.id,
            map: encounter.map,
            position: fighter.position,
        })
    }
    pub fn physical_contact(
        &self,
        store: &Store,
        first_anchor: PersonId,
        second_anchor: PersonId,
    ) -> bool {
        match (
            self.tactical_placement(store, first_anchor),
            self.tactical_placement(store, second_anchor),
        ) {
            (None, None) => true,
            (Some(first), Some(second)) => {
                first.encounter == second.encounter
                    && distance(first.position, second.position)
                        <= u128::from(self.rules.reach).pow(2)
                    && map(store, first.map)
                        .is_some_and(|map| map.clear(first.position, second.position))
            }
            _ => false,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fighter(actor: u64, x: i64) -> Fighter {
        Fighter {
            actor: PersonId(actor),
            joins_round: 0,
            team: 0,
            position: Point { x, y: 5_000 },
            surrendered: false,
            withdrawn: false,
            defending: false,
            retreat: None,
            known: BTreeMap::new(),
            search: SearchState::default(),
        }
    }
    fn fixture() -> (World, game_ecs::Store) {
        {
            let world = World {
                rules: Rules {
                    encounter_limit: 128,
                    round: SimDuration(6_000),
                    movement: 5_000,
                    reach: 2_000,
                    damage: 1,
                    defence: 1,
                    incapacitation: 1,
                },
            };
            let mut store = crate::test_store();
            let rows: BTreeMap<AreaId, Map> = BTreeMap::from([(
                AreaId(10),
                Map {
                    id: CombatMapId(1),
                    version: 1,
                    area: AreaId(10),
                    location: Location::AtArea(AreaId(10)),
                    width: 10_000,
                    height: 10_000,
                    exits: Vec::new(),
                    obstacles: Vec::new(),
                },
            )]);
            for (_, row) in rows {
                crate::insert_map(&mut store, row).unwrap();
            }
            let rows: BTreeMap<EncounterId, Encounter> = BTreeMap::from([(
                EncounterId(1),
                Encounter {
                    id: EncounterId(1),
                    area: AreaId(10),
                    location: Location::AtArea(AreaId(10)),
                    map: CombatMapId(1),
                    round: 0,
                    started: SimTime(0),
                    ends: SimTime(6_000),
                    fighters: BTreeMap::from([
                        (PersonId(1), fighter(1, 3_000)),
                        (PersonId(2), fighter(2, 5_000)),
                    ]),
                    order: Vec::new(),
                    cursor: 0,
                    active: true,
                },
            )]);
            for (_, row) in rows {
                crate::insert_encounter(&mut store, row).unwrap();
            }
            (world, store)
        }
    }
    #[test]
    fn contact_uses_actual_same_encounter_reach_withdrawal_and_clear_geometry() {
        let (world, mut store) = fixture();
        assert!(world.physical_contact(&store, PersonId(8), PersonId(9)));
        assert!(!world.physical_contact(&store, PersonId(1), PersonId(8)));
        assert!(world.physical_contact(&store, PersonId(1), PersonId(2)));
        crate::encounter_mut(&mut store, EncounterId(1))
            .unwrap()
            .fighters
            .get_mut(&PersonId(2))
            .unwrap()
            .position
            .x = 5_001;
        assert!(!world.physical_contact(&store, PersonId(1), PersonId(2)));
        crate::encounter_mut(&mut store, EncounterId(1))
            .unwrap()
            .fighters
            .get_mut(&PersonId(2))
            .unwrap()
            .position
            .x = 5_000;
        crate::area_map_mut(&mut store, AreaId(10))
            .unwrap()
            .obstacles
            .push(ObstacleView {
                index: 0,
                minimum: Point { x: 4_000, y: 4_000 },
                maximum: Point { x: 4_100, y: 6_000 },
                integrity: 1,
            });
        assert!(!world.physical_contact(&store, PersonId(1), PersonId(2)));
        crate::area_map_mut(&mut store, AreaId(10))
            .unwrap()
            .obstacles[0]
            .integrity = 0;
        assert!(world.physical_contact(&store, PersonId(1), PersonId(2)));
        crate::encounter_mut(&mut store, EncounterId(1))
            .unwrap()
            .fighters
            .get_mut(&PersonId(2))
            .unwrap()
            .withdrawn = true;
        assert!(!world.physical_contact(&store, PersonId(1), PersonId(2)));
        crate::encounter_mut(&mut store, EncounterId(1))
            .unwrap()
            .active = false;
        assert!(world.physical_contact(&store, PersonId(1), PersonId(2)));
    }
}

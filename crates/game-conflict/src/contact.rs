//! Tactical contact between actual anchor identities.
use crate::*;
pub struct TacticalPlacement {
    pub encounter: EncounterId,
    pub map: CombatMapId,
    pub position: Point,
}
impl World {
    pub fn tactical_placement(&self, actor: PersonId) -> Option<TacticalPlacement> {
        self.encounters
            .values()
            .filter(|encounter| encounter.active)
            .find_map(|encounter| {
                encounter
                    .fighters
                    .get(&actor)
                    .filter(|fighter| !fighter.withdrawn)
                    .map(|fighter| TacticalPlacement {
                        encounter: encounter.id,
                        map: encounter.map,
                        position: fighter.position,
                    })
            })
    }

    /// Callers first establish ordinary physical colocation and resolve carrier
    /// anchors through Spatial. Conflict owns encounter membership, withdrawal,
    /// reach and obstacle policy; absent tactical placement permits contact.
    pub fn physical_contact(&self, first_anchor: PersonId, second_anchor: PersonId) -> bool {
        let tactical = |actor: PersonId| {
            self.encounters
                .values()
                .filter(|encounter| encounter.active)
                .find_map(|encounter| {
                    encounter
                        .fighters
                        .get(&actor)
                        .filter(|fighter| !fighter.withdrawn)
                        .map(|fighter| (encounter.id, encounter.map, fighter.position))
                })
        };
        match (tactical(first_anchor), tactical(second_anchor)) {
            (None, None) => true,
            (Some((first, map, from)), Some((second, _, to))) => {
                first == second
                    && distance(from, to) <= u128::from(self.rules.reach).pow(2)
                    && self.map(map).is_some_and(|map| map.clear(from, to))
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
    fn fixture() -> World {
        World {
            rules: Rules {
                encounter_limit: 128,
                round: SimDuration(6_000),
                movement: 5_000,
                reach: 2_000,
                damage: 1,
                defence: 1,
                incapacitation: 1,
            },
            profiles: BTreeMap::new(),
            captivity: BTreeMap::new(),
            road_maps: BTreeMap::new(),
            maps: BTreeMap::from([(
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
            )]),
            encounters: BTreeMap::from([(
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
            )]),
        }
    }
    #[test]
    fn contact_uses_actual_same_encounter_reach_withdrawal_and_clear_geometry() {
        let mut world = fixture();
        assert!(world.physical_contact(PersonId(8), PersonId(9)));
        assert!(!world.physical_contact(PersonId(1), PersonId(8)));
        assert!(world.physical_contact(PersonId(1), PersonId(2)));
        world
            .encounters
            .get_mut(&EncounterId(1))
            .unwrap()
            .fighters
            .get_mut(&PersonId(2))
            .unwrap()
            .position
            .x = 5_001;
        assert!(!world.physical_contact(PersonId(1), PersonId(2)));
        world
            .encounters
            .get_mut(&EncounterId(1))
            .unwrap()
            .fighters
            .get_mut(&PersonId(2))
            .unwrap()
            .position
            .x = 5_000;
        world
            .maps
            .get_mut(&AreaId(10))
            .unwrap()
            .obstacles
            .push(ObstacleView {
                index: 0,
                minimum: Point { x: 4_000, y: 4_000 },
                maximum: Point { x: 4_100, y: 6_000 },
                integrity: 1,
            });
        assert!(!world.physical_contact(PersonId(1), PersonId(2)));
        world.maps.get_mut(&AreaId(10)).unwrap().obstacles[0].integrity = 0;
        assert!(world.physical_contact(PersonId(1), PersonId(2)));
        world
            .encounters
            .get_mut(&EncounterId(1))
            .unwrap()
            .fighters
            .get_mut(&PersonId(2))
            .unwrap()
            .withdrawn = true;
        assert!(!world.physical_contact(PersonId(1), PersonId(2)));
        world.encounters.get_mut(&EncounterId(1)).unwrap().active = false;
        assert!(world.physical_contact(PersonId(1), PersonId(2)));
    }
}

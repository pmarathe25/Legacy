//! A service decision changes an actual fighter's allegiance through its owner.
use crate::*;
#[derive(Clone, Debug)]
pub struct PreparedAllegiance {
    encounter: EncounterId,
    actor: PersonId,
    opponent: PersonId,
    before: u32,
    team: u32,
}
impl World {
    pub fn prepare_allegiance(
        &self,
        encounter: EncounterId,
        actor: PersonId,
        opponent: PersonId,
        before: u32,
        team: u32,
    ) -> Result<PreparedAllegiance, Error> {
        let prepared = PreparedAllegiance {
            encounter,
            actor,
            opponent,
            before,
            team,
        };
        prepared.check(self)?;
        Ok(prepared)
    }
}
impl PreparedAllegiance {
    pub fn check(&self, world: &World) -> Result<(), Error> {
        let encounter = world
            .encounters
            .get(&self.encounter)
            .ok_or(Error::Unavailable)?;
        if encounter
            .fighters
            .get(&self.actor)
            .is_none_or(|f| f.team != self.before)
            || encounter
                .fighters
                .get(&self.opponent)
                .is_none_or(|f| f.team != self.team)
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn commit(self, world: &mut World) -> Result<(), Error> {
        self.check(world)?;
        world
            .encounters
            .get_mut(&self.encounter)
            .unwrap()
            .fighters
            .get_mut(&self.actor)
            .unwrap()
            .team = self.team;
        Ok(())
    }
}
impl World {
    /// Actual living opposing fighters visible in this actor's current scene.
    pub fn visible_opponents(
        &self,
        encounter: EncounterId,
        actor: PersonId,
    ) -> impl Iterator<Item = &Fighter> {
        self.encounters
            .get(&encounter)
            .into_iter()
            .flat_map(move |encounter| {
                let fighter = encounter.fighters.get(&actor);
                let map = self.map(encounter.map);
                fighter.into_iter().flat_map(move |fighter| {
                    fighter.known.keys().filter_map(move |person| {
                        encounter.fighters.get(person).filter(|other| {
                            other.team != fighter.team
                                && !other.withdrawn
                                && !other.surrendered
                                && map
                                    .is_some_and(|map| map.clear(fighter.position, other.position))
                        })
                    })
                })
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    fn world() -> World {
        let actor = PersonId(1);
        let opponent = PersonId(2);
        let fighter = Fighter {
            actor,
            joins_round: 0,
            team: 0,
            position: Point { x: 1000, y: 1000 },
            surrendered: false,
            withdrawn: false,
            defending: false,
            retreat: None,
            known: BTreeMap::new(),
            search: SearchState::default(),
        };
        let other = Fighter {
            actor: opponent,
            team: 1,
            ..fighter.clone()
        };
        let encounter = Encounter {
            id: EncounterId(1),
            area: AreaId(1),
            location: Location::AtArea(AreaId(1)),
            map: CombatMapId(1),
            round: 0,
            started: SimTime(0),
            ends: SimTime(6),
            fighters: BTreeMap::from([(actor, fighter), (opponent, other)]),
            order: vec![actor, opponent],
            cursor: 0,
            active: true,
        };
        World {
            rules: Rules {
                encounter_limit: 128,
                round: SimDuration(6),
                movement: 1000,
                reach: 1000,
                damage: 10,
                defence: 2,
                incapacitation: 10,
            },
            profiles: BTreeMap::new(),
            maps: BTreeMap::new(),
            road_maps: BTreeMap::new(),
            encounters: BTreeMap::from([(encounter.id, encounter)]),
            captivity: BTreeMap::new(),
        }
    }
    #[test]
    fn allegiance_receipt_checks_actual_teams_and_preserves_other_fighter_fields() {
        let mut world = world();
        let plan = world
            .prepare_allegiance(EncounterId(1), PersonId(1), PersonId(2), 0, 1)
            .unwrap();
        world
            .encounters
            .get_mut(&EncounterId(1))
            .unwrap()
            .fighters
            .get_mut(&PersonId(1))
            .unwrap()
            .defending = true;
        plan.commit(&mut world).unwrap();
        let actor = &world.encounters[&EncounterId(1)].fighters[&PersonId(1)];
        assert_eq!(actor.team, 1);
        assert!(actor.defending);
        let plan = world
            .prepare_allegiance(EncounterId(1), PersonId(1), PersonId(2), 1, 1)
            .unwrap();
        world
            .encounters
            .get_mut(&EncounterId(1))
            .unwrap()
            .fighters
            .get_mut(&PersonId(2))
            .unwrap()
            .team = 2;
        let before = world.clone();
        assert!(plan.commit(&mut world).is_err());
        assert_eq!(world, before);
    }
}

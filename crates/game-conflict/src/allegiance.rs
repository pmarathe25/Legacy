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
        store: &game_ecs::Store,
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
        prepared.check(store)?;
        Ok(prepared)
    }
}
impl PreparedAllegiance {
    pub fn check(&self, store: &game_ecs::Store) -> Result<(), Error> {
        let encounter = crate::encounter(store, self.encounter).ok_or(Error::Unavailable)?;
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
    pub fn stage_into(
        self,
        store: &game_ecs::Store,
        batch: &mut game_ecs::Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.check(store)?;
        let id = game_ecs::StableId(self.encounter.0);
        if batch.touches::<Encounter>(id) {
            return Err(Error::Unavailable);
        }
        let mut reads = game_ecs::ReadSet::new();
        let before = reads
            .read::<Encounter>(store, id)
            .ok_or(Error::Unavailable)?;
        let mut after = before.clone();
        after
            .fighters
            .get_mut(&self.actor)
            .ok_or(Error::Unavailable)?
            .team = self.team;
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<Encounter>(id);
        batch.insert(id, after);
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut game_ecs::Store) -> Result<(), Error> {
        let mut batch = game_ecs::Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}
impl World {
    /// Actual living opposing fighters visible in this actor's current scene.
    pub fn visible_opponents<'a>(
        &self,
        store: &'a game_ecs::Store,
        encounter: EncounterId,
        actor: PersonId,
    ) -> impl Iterator<Item = &'a Fighter> + 'a {
        crate::encounter(store, encounter)
            .into_iter()
            .flat_map(move |encounter| {
                let fighter = encounter.fighters.get(&actor);
                let map = map(store, encounter.map);
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
    fn world() -> (World, game_ecs::Store) {
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
        {
            let world = World {
                rules: Rules {
                    encounter_limit: 128,
                    round: SimDuration(6),
                    movement: 1000,
                    reach: 1000,
                    damage: 10,
                    defence: 2,
                    incapacitation: 10,
                },
            };
            let mut store = crate::test_store();
            let rows: BTreeMap<EncounterId, Encounter> =
                BTreeMap::from([(encounter.id, encounter)]);
            for (_, row) in rows {
                crate::insert_encounter(&mut store, row).unwrap();
            }
            (world, store)
        }
    }
    #[test]
    fn allegiance_receipt_checks_actual_teams_and_preserves_other_fighter_fields() {
        let (world, mut store) = world();
        let plan = world
            .prepare_allegiance(&store, EncounterId(1), PersonId(1), PersonId(2), 0, 1)
            .unwrap();
        crate::encounter_mut(&mut store, EncounterId(1))
            .unwrap()
            .fighters
            .get_mut(&PersonId(1))
            .unwrap()
            .defending = true;
        plan.commit(&mut store).unwrap();
        let actor = &crate::encounter(&store, EncounterId(1)).unwrap().fighters[&PersonId(1)];
        assert_eq!(actor.team, 1);
        assert!(actor.defending);
        let plan = world
            .prepare_allegiance(&store, EncounterId(1), PersonId(1), PersonId(2), 1, 1)
            .unwrap();
        crate::encounter_mut(&mut store, EncounterId(1))
            .unwrap()
            .fighters
            .get_mut(&PersonId(2))
            .unwrap()
            .team = 2;
        let before = store.clone();
        assert!(plan.commit(&mut store).is_err());
        assert_eq!(store, before);
    }
}

use super::*;
impl Tactics {
    pub fn index(&self) -> Result<Index, &'static str> {
        let regions: std::collections::HashMap<_, _> = self
            .maps
            .iter()
            .enumerate()
            .map(|(i, map)| (map.region, i))
            .collect();
        let mut battles: Vec<_> = self
            .battles
            .par_iter()
            .enumerate()
            .map(|(slot, battle)| {
                let map = *regions.get(&battle.region).ok_or("missing tactical map")?;
                Ok((
                    battle.encounter,
                    IndexedBattle {
                        map,
                        slot,
                        // Movement commands build these sparse indexes lazily. Combat
                        // rounds need only the compact, sorted actor geometry below.
                        slots: None,
                        occupied: std::collections::HashMap::new(),
                    },
                ))
            })
            .collect::<Result<_, &'static str>>()?;
        if battles.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            battles.par_sort_unstable_by_key(|row| row.0);
        }
        if battles.par_windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("duplicate tactical battle");
        }
        let mut actors: Vec<_> = if self
            .battles
            .par_iter()
            .any(|battle| battle.people.len() >= 4096)
        {
            self.battles
                .par_iter()
                .flat_map(|battle| {
                    let map = regions[&battle.region];
                    battle.people.par_iter().map(move |person| Geometry {
                        id: person.person,
                        encounter: battle.encounter,
                        map,
                        point: person.position,
                        active: true,
                    })
                })
                .collect()
        } else {
            self.battles
                .par_iter()
                .flat_map_iter(|battle| {
                    let map = regions[&battle.region];
                    battle.people.iter().map(move |person| Geometry {
                        id: person.person,
                        encounter: battle.encounter,
                        map,
                        point: person.position,
                        active: true,
                    })
                })
                .collect()
        };
        if actors.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            actors.par_sort_unstable_by_key(|actor| actor.id);
        }
        if actors.par_windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err("duplicate tactical participant");
        }
        Ok(Index {
            battles,
            actors,
            maps: self.maps.clone(),
        })
    }
}
impl Index {
    pub(super) fn battle_slot(&self, id: u64) -> Option<usize> {
        let first = self.battles.first()?.0;
        if self.battles.last()?.0.checked_sub(first) == Some((self.battles.len() - 1) as u64) {
            usize::try_from(id.checked_sub(first)?)
                .ok()
                .filter(|&i| i < self.battles.len())
        } else {
            self.battles.binary_search_by_key(&id, |row| row.0).ok()
        }
    }
    pub(super) fn locate(&self, id: u64) -> Option<usize> {
        locate(&self.actors, id)
    }
    pub fn can_attack_between(&self, attacker: u64, target: u64) -> bool {
        let (Some(from), Some(to)) = (self.locate(attacker), self.locate(target)) else {
            return false;
        };
        let (from, to) = (&self.actors[from], &self.actors[to]);
        from.active
            && to.active
            && from.encounter == to.encounter
            && self.maps[from.map].can_attack(from.point, to.point)
    }
    pub fn can_attack(&self, encounter: u64, attacker: u64, target: u64) -> bool {
        self.locate(attacker)
            .is_some_and(|i| self.actors[i].encounter == encounter)
            && self.can_attack_between(attacker, target)
    }
    /// Test a fighter's reach and line of sight to a fixed local point.
    pub fn can_attack_point(&self, encounter: u64, attacker: u64, target: Point) -> bool {
        self.locate(attacker).is_some_and(|i| {
            let actor = &self.actors[i];
            actor.active
                && actor.encounter == encounter
                && self.maps[actor.map].can_attack(actor.point, target)
        })
    }
}
pub(super) fn locate(actors: &[Geometry], id: u64) -> Option<usize> {
    let first = actors.first()?.id;
    if actors.last()?.id.checked_sub(first) == Some((actors.len() - 1) as u64) {
        usize::try_from(id.checked_sub(first)?)
            .ok()
            .filter(|&i| i < actors.len())
    } else {
        actors.binary_search_by_key(&id, |actor| actor.id).ok()
    }
}

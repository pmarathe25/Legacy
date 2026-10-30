use super::*;
impl Tactics {
    pub fn index(&self) -> Result<Index, &'static str> {
        let regions: std::collections::HashMap<_, _> = self
            .maps
            .iter()
            .enumerate()
            .map(|(i, map)| (map.region, i))
            .collect();
        let entries: Vec<_> = self
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
        let count = entries.len();
        let battles: std::collections::HashMap<_, _> = entries.into_par_iter().collect();
        if battles.len() != count {
            return Err("duplicate tactical battle");
        }
        let mut actors: Vec<_> = self
            .battles
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
            .collect();
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

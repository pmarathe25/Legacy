use super::*;
use game_parallel_conflict::Encounter;
use std::collections::HashMap;
impl Tactics {
    /// Apply canonical sorted survivor rosters; keep map sites after resolution.
    pub fn sync_changed(
        &mut self,
        changes: &HashMap<u64, Option<&Encounter>>,
        index: Option<&mut Index>,
    ) {
        let removed: Vec<_> = self
            .battles
            .par_iter_mut()
            .flat_map(|battle| match changes.get(&battle.encounter) {
                Some(Some(fight)) => retain(battle, fight),
                Some(None) => battle
                    .people
                    .par_iter()
                    .map(|person| person.person)
                    .collect(),
                None => Vec::new(),
            })
            .collect();
        self.battles
            .retain(|battle| !matches!(changes.get(&battle.encounter), Some(None)));
        if let Some(index) = index {
            index.membership_changed(self, &changes.keys().copied().collect::<Vec<_>>(), &removed);
        }
    }
}
fn retain(battle: &mut Battle, fight: &Encounter) -> Vec<u64> {
    let sorted = battle
        .people
        .par_windows(2)
        .all(|pair| pair[0].person < pair[1].person);
    let (kept, removed): (Vec<_>, Vec<_>) = battle
        .people
        .par_chunks(2048)
        .map(|chunk| {
            let mut cursor = fight
                .fighters
                .partition_point(|fighter| fighter.person < chunk[0].person);
            let mut kept = Vec::new();
            let mut removed = Vec::new();
            for person in chunk {
                let survives = if sorted {
                    while cursor < fight.fighters.len()
                        && fight.fighters[cursor].person < person.person
                    {
                        cursor += 1;
                    }
                    fight
                        .fighters
                        .get(cursor)
                        .is_some_and(|fighter| fighter.person == person.person)
                } else {
                    fight
                        .fighters
                        .binary_search_by_key(&person.person, |fighter| fighter.person)
                        .is_ok()
                };
                if survives {
                    kept.push(person.clone());
                } else {
                    removed.push(person.person);
                }
            }
            (kept, removed)
        })
        .unzip();
    battle.people = kept.into_par_iter().flatten().collect();
    removed.into_par_iter().flatten().collect()
}

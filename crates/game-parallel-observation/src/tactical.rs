use game_parallel::{TacticalPoint, World};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TacticalView {
    pub encounter: u64,
    pub position: TacticalPoint,
    /// Only placements in attack range with unobstructed line of sight.
    pub nearby: Vec<(u64, TacticalPoint)>,
}
#[cfg(test)]
#[path = "../tests/support/tactical.rs"]
mod tests;
pub(super) fn view(world: &World, ruler: u64) -> Option<TacticalView> {
    let tactics = world.tactics.as_ref()?;
    let (battle, own) = tactics.battles.iter().find_map(|battle| {
        battle
            .people
            .iter()
            .find(|person| person.person == ruler)
            .map(|person| (battle, person))
    })?;
    let map = tactics
        .maps
        .iter()
        .find(|map| map.region == battle.region)?;
    Some(TacticalView {
        encounter: battle.encounter,
        position: own.position,
        nearby: battle
            .people
            .iter()
            .filter(|person| {
                person.person != ruler && map.can_attack(own.position, person.position)
            })
            .map(|person| (person.person, person.position))
            .collect(),
    })
}

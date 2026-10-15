use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleView {
    pub id: u64,
    pub area: AreaId,
    pub map: Map,
    pub fighters: BTreeMap<PersonId, Fighter>,
    pub round: u64,
    pub awaiting_input: bool,
    pub finished: bool,
}
/// The tactical fixture has complete map visibility; pending orders stay private.
pub fn view(host: &Host, person: PersonId) -> Result<Vec<BattleView>> {
    Ok(host
        .get::<Battles>()?
        .0
        .iter()
        .filter(|(_, battle)| battle.fighters.contains_key(&person))
        .map(|(id, b)| BattleView {
            id: *id,
            area: b.area,
            map: b.map.clone(),
            fighters: b.fighters.clone(),
            round: b.round,
            awaiting_input: !b.finished && b.scheduled.is_none() && !b.orders.contains_key(&person),
            finished: b.finished,
        })
        .collect())
}

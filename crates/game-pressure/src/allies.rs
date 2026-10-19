use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Aid {
    pub person: PersonId,
    pub subject: PersonId,
    pub aggressor: PersonId,
}
impl Request for Aid {
    const KEY: &'static str = "pressure.aid";
    type Response = bool;
}
fn tie(tx: &Transaction<'_>, p: PersonId, q: PersonId) -> i64 {
    tx.get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&p)?.get(&q))
        .map_or(0, |r| {
            i64::from(r.affection + r.trust + r.respect - r.fear - r.grievance)
        })
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Aid>(|tx, r| {
        if [r.subject, r.aggressor].contains(&r.person)
            || tx.get::<game_lineage::Lineage>()?.current == Some(r.person)
            || tx.get::<Actions>()?.0.contains_key(&r.person)
            || game_physiology::capable(tx, r.person).is_err()
            || !game_vitality::can_manipulate(tx, r.person)
        {
            return Ok(false);
        }
        let area = game_transport::area(tx.get()?, r.person)?;
        require(
            game_transport::area(tx.get()?, r.subject)? == area
                && game_transport::area(tx.get()?, r.aggressor)? == area,
            "aid needs contact",
        )?;
        let concern = tie(tx, r.person, r.subject)
            + disposition(tx, r.person, "altruism") * disposition(tx, r.person, "empathy") / 5000;
        let body = &tx.get::<game_physiology::Bodies>()?.people[&r.person];
        let danger = judgment::fear(tx, r.person, r.aggressor)?;
        let cost = 50
            + tie(tx, r.person, r.aggressor).max(0)
            + disposition(tx, r.person, "patience") / 20
            + body.hunger.min(1000) as i64 / 2
            + body.fatigue.min(1000) as i64 / 2
            + danger * (1000 - disposition(tx, r.person, "risk-tolerance")) / 500;
        Ok(concern > cost)
    })
}
pub(super) fn expected(
    tx: &Transaction<'_>,
    p: PersonId,
    other: PersonId,
) -> Result<(i64, Vec<PersonId>)> {
    if game_vitality::ability(tx, p, "vocalization") == 0 {
        return Ok((0, vec![]));
    }
    let area = game_transport::area(tx.get()?, p)?;
    let friends: Vec<_> = tx
        .get::<game_relationships::Social>()?
        .relationships
        .get(&p)
        .into_iter()
        .flat_map(|r| r.keys())
        .copied()
        .filter(|q| {
            *q != other
                && *q != p
                && tie(tx, p, *q) > 50
                && game_transport::area(tx.get().unwrap(), *q).ok() == Some(area)
        })
        .take(3)
        .collect();
    // Expected help uses the caller's relationships, never another person's private willingness.
    let support = friends.iter().map(|q| tie(tx, p, *q) / 4).sum();
    Ok((support, friends))
}
pub(super) fn recruit(
    tx: &mut Transaction<'_>,
    subject: PersonId,
    aggressor: PersonId,
    friends: Vec<PersonId>,
) -> Result<Vec<PersonId>> {
    let mut helpers = vec![];
    for person in friends {
        if game_vitality::ability(tx, person, "hearing") > 0
            && tx.call(Aid {
                person,
                subject,
                aggressor,
            })?
        {
            helpers.push(person);
        }
    }
    Ok(helpers)
}

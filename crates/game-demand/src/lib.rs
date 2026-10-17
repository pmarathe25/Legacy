//! Personal evidence of acceptance, with no designated currency or global prices.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acceptance {
    pub count: u32,
    pub at: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub seen: Table<AgreementId, ()>,
    pub goods: Table<GoodId, Table<PersonId, Acceptance>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Demand {
    pub people: Table<PersonId, Memory>,
}
impl Resource for Demand {
    const KEY: &'static str = "demand.personal-acceptance";
    fn validate(&self) -> Result<()> {
        require(
            self.people.values().all(|m| {
                m.goods.values().all(|peers| {
                    peers.len() <= 64 && peers.values().all(|e| (1..=8).contains(&e.count))
                })
            }),
            "invalid acceptance memory",
        )
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        require(
            self.people.changed(&old.people).all(|(_, m)| {
                m.goods.values().all(|peers| {
                    peers.len() <= 64 && peers.values().all(|e| (1..=8).contains(&e.count))
                })
            }),
            "invalid acceptance memory",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ObserveSettlement {
    pub observer: PersonId,
    pub agreement: AgreementId,
}
impl Request for ObserveSettlement {
    const KEY: &'static str = "demand.observe-settlement";
    type Response = ();
}
/// Evidence loses force after absence. No one knows another person's private valuation.
pub fn confidence(memory: &Memory, good: GoodId, at: SimTime) -> u64 {
    memory
        .goods
        .get(&good)
        .into_iter()
        .flat_map(|r| r.values())
        .map(|e| {
            let days = at.0.saturating_sub(e.at.0) / DAY.0;
            u64::from(e.count.min(8)) * 1000 / (1 + days / 30)
        })
        .sum::<u64>()
        .min(16000)
}
pub fn resale(tx: &Transaction<'_>, person: PersonId, good: GoodId) -> i64 {
    let Some(memory) = tx.get::<Demand>().ok().and_then(|d| d.people.get(&person)) else {
        return 0;
    };
    let Ok(object) = tx
        .get::<game_objects::Objects>()
        .and_then(|o| o.by_id(good))
    else {
        return 0;
    };
    let confidence = confidence(memory, good, tx.now());
    let foresight = tx
        .get::<game_traits::Traits>()
        .map_or(1000, |_| game_vitality::ability(tx, person, "foresight"))
        .min(16000);
    let portability = 1000 / object.mass.min(1000);
    let durability = object
        .shelf_days()
        .map_or(1000, |d| (d * 1000 / 90).min(1000));
    (u128::from(confidence)
        * u128::from(foresight)
        * u128::from(portability)
        * u128::from(durability)
        / 10_000_000_000)
        .min(500) as i64
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("demand", 1)?;
    b.resource::<Demand>()?;
    b.request::<ObserveSettlement>(|tx, r| {
        let a = tx
            .get::<game_agreements::Agreements>()?
            .0
            .get(&r.agreement)
            .ok_or_else(|| Error::Missing("agreement".into()))?;
        require(
            a.status == game_agreements::Status::Complete && a.parties.contains(&r.observer),
            "settlement is not personally known",
        )?;
        let outgoing: Vec<_> = a
            .terms
            .iter()
            .filter(|t| t.debtor == r.observer && t.creditor != r.observer && t.quantity > 0)
            .map(|t| (t.good, t.creditor))
            .collect();
        let at = tx.now();
        let m = tx.edit::<Demand>()?.people.entry(r.observer).or_default();
        if m.seen.insert(r.agreement, ()).is_some() {
            return Ok(());
        }
        for (good, peer) in outgoing {
            let peers = m.goods.entry(good).or_default();
            let e = peers
                .entry(peer)
                .or_insert_with(|| Acceptance { count: 0, at });
            let decay = at.0.saturating_sub(e.at.0) / (30 * DAY.0);
            e.count = e
                .count
                .saturating_sub(decay.min(u64::from(u32::MAX)) as u32)
                .saturating_add(1)
                .min(8);
            e.at = at;
            if peers.len() > 64 {
                let oldest = *peers.iter().min_by_key(|(id, e)| (e.at, **id)).unwrap().0;
                peers.remove(&oldest);
            }
        }
        Ok(())
    })?;
    Ok(())
}

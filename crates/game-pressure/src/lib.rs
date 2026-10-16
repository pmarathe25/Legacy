//! A refused demand can lead to real conflict; fear does not imply loyalty.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Demand {
    pub leader: PersonId,
    pub subject: PersonId,
    pub battle: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pressure {
    pub active: Table<PersonId, Demand>,
}
impl Resource for Pressure {
    const KEY: &'static str = "pressure.demands";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct DemandSubmission {
    pub leader: PersonId,
    pub subject: PersonId,
}
impl Request for DemandSubmission {
    const KEY: &'static str = "pressure.demand-submission";
    type Response = ();
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Submitted {
    pub leader: PersonId,
    pub subject: PersonId,
}
impl Event for Submitted {
    const KEY: &'static str = "pressure.submitted";
}
pub fn disposition(tx: &Transaction<'_>, person: PersonId, key: &str) -> i64 {
    tx.get::<game_traits::Traits>()
        .map_or(500, |t| t.disposition(person, key))
}
pub fn afraid(tx: &Transaction<'_>, subject: PersonId, leader: PersonId) -> Result<bool> {
    let social = tx.get::<game_relationships::Social>()?;
    let Some(rel) = social
        .relationships
        .get(&subject)
        .and_then(|r| r.get(&leader))
    else {
        return Ok(false);
    };
    let authority = social
        .beliefs
        .get(&subject)
        .map_or(0, |b| b.authority.clamp(-10, 10));
    let resistance =
        20 + disposition(tx, subject, "risk-tolerance") / 20 + i64::from(rel.grievance) / 5
            - i64::from(authority);
    Ok(i64::from(rel.fear) >= resistance.clamp(20, 95))
}
fn clear(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if let Some(d) = tx.get::<Pressure>()?.active.get(&person).cloned() {
        for p in [d.leader, d.subject] {
            tx.edit::<Pressure>()?.active.remove(&p);
        }
        for p in [d.leader, d.subject] {
            if tx
                .get::<Actions>()?
                .0
                .get(&p)
                .is_some_and(|a| a.kind == "combat")
            {
                tx.cancel_action(p)?;
            }
        }
    }
    Ok(())
}
fn yield_to(tx: &mut Transaction<'_>, leader: PersonId, subject: PersonId) -> Result<()> {
    clear(tx, subject)?;
    tx.emit(Submitted { leader, subject }, vec![leader, subject])
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("pressure", 1)?;
    b.resource::<Pressure>()?;
    b.validate_save(|h| {
        for (p, d) in &h.get::<Pressure>()?.active {
            require(
                [d.leader, d.subject].contains(p)
                    && d.leader != d.subject
                    && [d.leader, d.subject].iter().all(|p| {
                        h.get::<Pressure>().unwrap().active.get(p) == Some(d)
                            && h.get::<Actions>()
                                .unwrap()
                                .0
                                .get(p)
                                .is_some_and(|a| a.kind == "combat")
                    })
                    && h.get::<game_conflict::Battles>()?
                        .0
                        .get(&d.battle)
                        .is_some_and(|battle| {
                            !battle.finished
                                && [d.leader, d.subject]
                                    .iter()
                                    .all(|p| battle.fighters.get(p).is_some_and(|f| f.health > 0))
                        }),
                "invalid pressure engagement",
            )?;
        }
        Ok(())
    });
    b.request::<DemandSubmission>(|tx, r| {
        require(r.leader != r.subject, "self submission")?;
        let player = tx.get::<game_lineage::Lineage>()?.current;
        require(
            ![Some(r.leader), Some(r.subject)].contains(&player),
            "player response requires explicit control",
        )?;
        for p in [r.leader, r.subject] {
            game_physiology::capable(tx, p)?;
            require(!tx.get::<Actions>()?.0.contains_key(&p), "person is busy")?;
        }
        let area = game_transport::area(tx.get()?, r.leader)?;
        require(
            area == game_transport::area(tx.get()?, r.subject)?,
            "demand is not local",
        )?;
        if afraid(tx, r.subject, r.leader)? {
            return yield_to(tx, r.leader, r.subject);
        }
        let fighters = [(r.leader, 1, 10), (r.subject, 2, 20)]
            .into_iter()
            .map(|(p, team, x)| {
                (
                    p,
                    game_conflict::Fighter {
                        team,
                        point: game_conflict::Point { x, y: 10 },
                        health: tx.get::<game_physiology::Bodies>().unwrap().people[&p].health,
                    },
                )
            })
            .collect();
        let battle = tx.call(game_conflict::Start(game_conflict::Battle {
            area,
            map: game_conflict::Map {
                width: 1000,
                obstacles: vec![],
            },
            fighters,
            controlled: BTreeSet::new(),
            orders: BTreeMap::new(),
            round: 0,
            interval: MINUTE,
            movement: 100,
            reach: 50,
            damage: 10,
            scheduled: None,
            finished: false,
        }))?;
        let demand = Demand {
            leader: r.leader,
            subject: r.subject,
            battle,
        };
        for p in [r.leader, r.subject] {
            tx.edit::<Pressure>()?.active.insert(p, demand.clone());
        }
        Ok(())
    })?;
    b.subscribe::<ActionFinished>("pressure.release", |tx, e| clear(tx, e.person))?;
    b.subscribe::<game_conflict::Struck>("pressure.experience", |tx, e| {
        tx.call(game_relationships::SufferForce {
            observer: e.target,
            actor: e.actor,
            harm: e.amount,
        })?;
        let Some(d) = tx.get::<Pressure>()?.active.get(&e.target).cloned() else {
            return Ok(());
        };
        if e.battle != d.battle {
            return Ok(());
        }
        if [d.leader, d.subject]
            .iter()
            .any(|p| game_physiology::capable(tx, *p).is_err())
        {
            return clear(tx, e.target);
        }
        if e.target == d.subject && afraid(tx, d.subject, d.leader)? {
            yield_to(tx, d.leader, d.subject)
        } else if e.target == d.leader && afraid(tx, d.leader, d.subject)? {
            clear(tx, d.leader)
        } else {
            Ok(())
        }
    })
}

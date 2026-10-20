//! Known precedents influence judgments. Enforcement requires a physical actor.
use game_core::*;
use game_sim::*;
use game_storage::{Records, Table};
mod incidents;
pub use incidents::Prosecute;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub taking: bool,
    pub consent: bool,
    pub necessity: bool,
    pub credible: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Remedy {
    None,
    Return,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Precedent {
    pub id: CaseId,
    pub evidence: Evidence,
    pub remedy: Remedy,
}
pub fn judge(evidence: &Evidence, known: &[Precedent], limit: usize) -> Remedy {
    if !evidence.credible {
        return Remedy::None;
    }
    let closest = known.iter().take(limit).min_by_key(|p| {
        (
            (p.evidence.taking != evidence.taking) as u8
                + (p.evidence.consent != evidence.consent) as u8
                + (p.evidence.necessity != evidence.necessity) as u8,
            p.id,
        )
    });
    closest.map(|p| p.remedy.clone()).unwrap_or(Remedy::None)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    pub claimant: PersonId,
    pub defendant: PersonId,
    pub evidence: Evidence,
    pub transfer: game_cargo::Transfer,
    pub remedy: Option<Remedy>,
    pub enforced: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Justice {
    #[serde(default)]
    pub incidents: Table<PersonId, Table<PersonId, Case>>,
    pub cases: Records<CaseId, Case>,
    pub known: Records<PersonId, Vec<Precedent>>,
    #[serde(default)]
    pub heard: Records<PersonId, std::collections::BTreeSet<CaseId>>,
}
impl Resource for Justice {
    fn allocated_ids(&self) -> Vec<u64> {
        self.cases.keys().map(|id| id.0).collect()
    }
    const KEY: &'static str = "justice.cases";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct File(pub Case);
impl Request for File {
    const KEY: &'static str = "justice.file";
    type Response = CaseId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Decide {
    pub judge: PersonId,
    pub case: CaseId,
}
impl Request for Decide {
    const KEY: &'static str = "justice.decide";
    type Response = Remedy;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enforce {
    pub person: PersonId,
    pub case: CaseId,
    pub willing: bool,
}
impl Request for Enforce {
    const KEY: &'static str = "justice.enforce";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct KnowPrecedent {
    pub person: PersonId,
    pub precedent: Precedent,
}
impl Request for KnowPrecedent {
    const KEY: &'static str = "justice.know_precedent";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct TellCase {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub case: CaseId,
}
impl Request for TellCase {
    const KEY: &'static str = "justice.tell-case";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ExecuteOrder {
    pub person: PersonId,
    pub case: CaseId,
}
impl Request for ExecuteOrder {
    const KEY: &'static str = "justice.execute-order";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("justice", 1)?;
    b.resource::<Justice>()?;
    incidents::register(b)?;
    b.request::<TellCase>(|tx, r| {
        require(
            tx.get::<Justice>()?
                .heard
                .get(&r.speaker)
                .is_some_and(|heard| heard.contains(&r.case)),
            "case is unfamiliar",
        )?;
        require(
            game_transport::area(tx.get()?, r.speaker)?
                == game_transport::area(tx.get()?, r.listener)?,
            "testimony needs contact",
        )?;
        tx.edit::<Justice>()?
            .heard
            .entry(r.listener)
            .or_default()
            .insert(r.case);
        Ok(())
    })?;
    b.request::<ExecuteOrder>(|tx, r| {
        let case = &tx.get::<Justice>()?.cases[&r.case];
        let own = r.person == case.claimant;
        let social = tx.get::<game_relationships::Social>().ok();
        let value = social
            .and_then(|s| s.values.get(&r.person)?.get("justice"))
            .copied()
            .unwrap_or(0);
        let willing = if own {
            true
        } else {
            i64::from(value) * 4 + game_vitality::disposition(tx, r.person, "altruism") / 10 > 60
        };
        tx.call(Enforce {
            person: r.person,
            case: r.case,
            willing,
        })
    })?;
    game_choice::provider(b, "justice.choices", |tx, e| {
        if !game_choice::collecting(tx, e.person) {
            return Ok(());
        }
        let cases: Vec<_> = tx
            .get::<Justice>()?
            .heard
            .get(&e.person)
            .into_iter()
            .flatten()
            .copied()
            .take(32)
            .collect();
        for case in cases {
            let c = &tx.get::<Justice>()?.cases[&case];
            let own = c.claimant == e.person;
            let interest = if own {
                80
            } else {
                game_vitality::disposition(tx, e.person, "altruism") / 20
            };
            if c.remedy.is_none() {
                game_choice::offer(
                    tx,
                    e.person,
                    format!("justice:judge:{}", case.0),
                    game_choice::motive(0, interest, 0, 10, 0),
                    game_choice::Undertake {
                        person: e.person,
                        kind: "adjudicate".into(),
                        duration: HOUR,
                        request: Pending::immediate(Decide {
                            judge: e.person,
                            case,
                        }),
                    },
                )?;
            } else if c.remedy == Some(Remedy::Return) && !c.enforced {
                game_choice::offer(
                    tx,
                    e.person,
                    format!("justice:enforce:{}", case.0),
                    game_choice::motive(0, interest, 0, 10, 20),
                    ExecuteOrder {
                        person: e.person,
                        case,
                    },
                )?;
            }
        }
        Ok(())
    })?;
    b.request::<KnowPrecedent>(|tx, r| {
        tx.edit::<Justice>()?
            .known
            .entry(r.person)
            .or_default()
            .push(r.precedent);
        Ok(())
    })?;
    b.request::<File>(|tx, r| {
        require(
            r.0.remedy.is_none() && !r.0.enforced,
            "new case already resolved",
        )?;
        let id = CaseId(tx.allocate()?);
        tx.edit::<Justice>()?
            .heard
            .entry(r.0.claimant)
            .or_default()
            .insert(id);
        tx.edit::<Justice>()?.cases.insert(id, r.0);
        Ok(id)
    })?;
    b.request::<Decide>(|tx, r| {
        let state = tx.get::<Justice>()?;
        let case = state
            .cases
            .get(&r.case)
            .ok_or_else(|| Error::Missing("case".into()))?;
        require(case.remedy.is_none(), "case already decided")?;
        let mut remedy = judge(
            &case.evidence,
            state.known.get(&r.judge).map_or(&[], Vec::as_slice),
            (game_vitality::ability(tx, r.judge, "intelligence") / 100).clamp(1, 128) as usize,
        );
        if let Ok(social) = tx.get::<game_relationships::Social>() {
            let property = social
                .values
                .get(&r.judge)
                .and_then(|v| v.get("property"))
                .copied()
                .unwrap_or(0);
            let compassion = if case.evidence.necessity {
                game_vitality::disposition(tx, r.judge, "empathy") / 10
            } else {
                0
            };
            if case.evidence.credible && case.evidence.taking && !case.evidence.consent {
                let beliefs = social.beliefs.get(&r.judge).cloned().unwrap_or_default();
                let standards = game_vitality::disposition(tx, r.judge, "honesty") / 10
                    - game_vitality::disposition(tx, r.judge, "altruism") / 10
                    + (i64::from(beliefs.honesty) - i64::from(beliefs.sharing)) * 3
                    + if case.claimant == r.judge { 10 } else { 0 };
                let tradition = state
                    .known
                    .get(&r.judge)
                    .filter(|v| !v.is_empty())
                    .map_or(0, |_| if remedy == Remedy::Return { 20 } else { -20 });
                remedy = if standards + tradition + i64::from(property) > compassion {
                    Remedy::Return
                } else {
                    Remedy::None
                };
            }
        }
        let precedent = Precedent {
            id: r.case,
            evidence: case.evidence.clone(),
            remedy: remedy.clone(),
        };
        tx.edit::<Justice>()?
            .known
            .entry(r.judge)
            .or_default()
            .push(precedent);
        tx.edit::<Justice>()?.cases.get_mut(&r.case).unwrap().remedy = Some(remedy.clone());
        Ok(remedy)
    })?;
    b.request::<Enforce>(|tx, r| {
        require(r.willing, "enforcer refuses")?;
        let c = tx
            .get::<Justice>()?
            .cases
            .get(&r.case)
            .ok_or_else(|| Error::Missing("case".into()))?
            .clone();
        require(
            c.remedy == Some(Remedy::Return) && !c.enforced,
            "no enforcement due",
        )?;
        tx.call(game_property::Take {
            actor: r.person,
            transfer: c.transfer,
            unauthorized: true,
        })?;
        tx.edit::<Justice>()?
            .cases
            .get_mut(&r.case)
            .unwrap()
            .enforced = true;
        Ok(())
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_cases_cannot_change_a_judgment() {
        let e = Evidence {
            taking: true,
            consent: false,
            necessity: true,
            credible: true,
        };
        assert_eq!(judge(&e, &[], 32), Remedy::None);
        assert_eq!(
            judge(
                &e,
                &[Precedent {
                    id: CaseId(1),
                    evidence: e.clone(),
                    remedy: Remedy::Return
                }],
                32
            ),
            Remedy::Return
        );
    }
}

//! Known precedents influence judgments. Enforcement requires a physical actor.
use game_core::*;
use game_sim::*;
use game_storage::Records;
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
    pub cases: Records<CaseId, Case>,
    pub known: Records<PersonId, Vec<Precedent>>,
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
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("justice", 1)?;
    b.resource::<Justice>()?;
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
        let remedy = judge(
            &case.evidence,
            state.known.get(&r.judge).map_or(&[], Vec::as_slice),
            32,
        );
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

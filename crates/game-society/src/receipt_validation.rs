//! Persisted social receipt relationships in canonical owner rows.
use game_core::*;
use game_ecs::Store;

pub fn validate_proposal_receipt(
    store: &Store,
    institution: InstitutionId,
    proposal: RuleProposalId,
) -> Result<(), Error> {
    if crate::proposal(store, proposal).is_some_and(|row| row.institution == institution) {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    }
}

pub fn validate_judgment_receipt(
    store: &Store,
    case: EnforcementCaseId,
    judgment: JudgmentId,
) -> Result<(), Error> {
    if crate::enforcement::case(store, case)
        .is_some_and(|case| case.judgment.as_ref().is_some_and(|row| row.id == judgment))
    {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{enforcement::*, institutions::*};
    use std::collections::{BTreeMap, BTreeSet};
    #[test]
    fn saved_receipt_relations_follow_canonical_proposal_and_case_rows() {
        let mut schema = game_ecs::Schema::new();
        crate::register_components(&mut schema).unwrap();
        crate::enforcement::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        crate::insert_proposal(
            &mut store,
            RuleProposalId(30),
            RuleProposal {
                id: RuleProposalId(30),
                institution: InstitutionId(10),
                proposer: PersonId(1),
                change: InstitutionChange::RemoveRule { index: 0 },
                votes: BTreeMap::new(),
                base_revision: 0,
                status: RuleProposalStatus::Proposed,
            },
        )
        .unwrap();
        assert!(validate_proposal_receipt(&store, InstitutionId(10), RuleProposalId(30)).is_ok());
        assert!(validate_proposal_receipt(&store, InstitutionId(11), RuleProposalId(30)).is_err());
        let case = EnforcementCaseId(20);
        crate::enforcement::insert_case(
            &mut store,
            Case {
                id: case,
                institution: InstitutionId(10),
                office: 0,
                filer: PersonId(1),
                target: PersonId(2),
                topic: EnforcementTopic::Assault,
                evidence: EnforcementEvidence::Assault(WitnessedAssault {
                    event: EventId(1),
                    attacker: PersonId(2),
                    target: PersonId(1),
                    area: AreaId(3),
                    damage: 1,
                    observed_at: SimTime(0),
                }),
                contact: None,
                policy: EnforcementPolicy {
                    topics: BTreeSet::from([EnforcementTopic::Assault]),
                    areas: BTreeSet::from([AreaId(3)]),
                    subjects: Subjects::members(),
                    enforcer_office: 0,
                    remedies: BTreeSet::from([RemedyKind::Dismiss]),
                    spending: BTreeMap::new(),
                    supplies: BTreeMap::new(),
                    maximum_confinement: SimDuration(100),
                },
                judgment: Some(EnforcementJudgmentView {
                    id: JudgmentId(40),
                    proposer: PersonId(1),
                    proposal: RuleProposalId(30),
                    remedy: EnforcementRemedy::Dismiss,
                    adopted_at: SimTime(0),
                }),
                mission: None,
                settlement: None,
                status: EnforcementStatus::Dismissed,
                explanation: "dismissed".into(),
                changed_at: SimTime(0),
                notice_area: Some(AreaId(3)),
                witnesses: BTreeSet::from([PersonId(1)]),
            },
        )
        .unwrap();
        assert!(validate_judgment_receipt(&store, case, JudgmentId(40)).is_ok());
        assert!(validate_judgment_receipt(&store, case, JudgmentId(41)).is_err());
        crate::enforcement::case_mut(&mut store, case)
            .unwrap()
            .judgment = None;
        assert!(validate_judgment_receipt(&store, case, JudgmentId(40)).is_err());
    }
}

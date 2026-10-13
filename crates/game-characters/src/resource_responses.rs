//! Personal responses to permission requests and unasked resource use.
use crate::decisions::{Personality, Relationship};

/// Physical, spatial and control owners supply readiness; characters supply judgment.
#[derive(Clone, Copy, Debug)]
pub struct ResponseFacts {
    pub autonomous: bool,
    pub capable: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionResponse {
    Grant,
    Refuse,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnauthorizedUseResponse {
    Resist,
    Ignore,
}
pub fn permission_response(
    personality: &Personality,
    relationship: Option<&Relationship>,
    facts: ResponseFacts,
) -> PermissionResponse {
    if facts.autonomous
        && facts.capable
        && personality.empathy >= 700
        && relationship.is_none_or(|relation| relation.trust >= 0)
    {
        PermissionResponse::Grant
    } else {
        PermissionResponse::Refuse
    }
}
pub fn unauthorized_use_response(
    personality: &Personality,
    facts: ResponseFacts,
) -> UnauthorizedUseResponse {
    if facts.autonomous
        && facts.capable
        && personality.aggression >= 500
        && personality.risk_tolerance >= 500
    {
        UnauthorizedUseResponse::Resist
    } else {
        UnauthorizedUseResponse::Ignore
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_judgment_preserves_threshold_trust_control_and_readiness() {
        for empathy in [0, 699, 700, 1_000] {
            for trust in [None, Some(-1), Some(0), Some(1)] {
                for autonomous in [false, true] {
                    for capable in [false, true] {
                        let mut personality = Personality::restful();
                        personality.empathy = empathy;
                        let relationship = trust.map(|trust| Relationship {
                            trust,
                            affection: 0,
                        });
                        let expected = if empathy >= 700
                            && trust.is_none_or(|trust| trust >= 0)
                            && autonomous
                            && capable
                        {
                            PermissionResponse::Grant
                        } else {
                            PermissionResponse::Refuse
                        };
                        assert_eq!(
                            permission_response(
                                &personality,
                                relationship.as_ref(),
                                ResponseFacts {
                                    autonomous,
                                    capable
                                }
                            ),
                            expected
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn unauthorized_use_response_requires_both_risk_and_aggression_without_player_override() {
        for aggression in [0, 499, 500, 1_000] {
            for risk_tolerance in [0, 499, 500, 1_000] {
                for autonomous in [false, true] {
                    for capable in [false, true] {
                        let mut personality = Personality::restful();
                        personality.aggression = aggression;
                        personality.risk_tolerance = risk_tolerance;
                        let expected = if aggression >= 500
                            && risk_tolerance >= 500
                            && autonomous
                            && capable
                        {
                            UnauthorizedUseResponse::Resist
                        } else {
                            UnauthorizedUseResponse::Ignore
                        };
                        assert_eq!(
                            unauthorized_use_response(
                                &personality,
                                ResponseFacts {
                                    autonomous,
                                    capable
                                }
                            ),
                            expected
                        );
                    }
                }
            }
        }
    }
}

/// Foreign owners supply separate physical readiness facts. Receiving Characters
/// determines whether the custodian has agency to oppose unauthorized use.
#[derive(Clone, Copy, Debug)]
pub struct OppositionFacts {
    pub manual: bool,
    pub co_present: bool,
    pub restricted: bool,
    pub combat_available: bool,
    pub user_can_fight: bool,
    pub custodian_can_fight: bool,
}
pub fn unauthorized_use_reaction(
    personality: &Personality,
    facts: OppositionFacts,
) -> UnauthorizedUseResponse {
    unauthorized_use_response(
        personality,
        ResponseFacts {
            autonomous: !facts.manual,
            capable: facts.co_present
                && !facts.restricted
                && facts.combat_available
                && facts.user_can_fight
                && facts.custodian_can_fight,
        },
    )
}

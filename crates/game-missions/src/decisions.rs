use game_agreements::AgreementTerm;
use game_core::*;

#[derive(Clone, Copy, Debug)]
pub struct RecruitmentInput {
    pub actor: PersonId,
    pub guardian_duty: bool,
    pub violent: bool,
    pub risk_tolerance: u16,
    pub ambition: u16,
    pub sociability: u16,
    pub empathy: u16,
    pub sponsor_trust: i32,
    pub social_disruption: i64,
    pub urgent_care: bool,
    pub hungry: bool,
}
/// An offer remains a proposal; a mandate never replaces individual judgment.
pub fn recruitment_score(input: RecruitmentInput, compensation: &[AgreementTerm]) -> i64 {
    if input.guardian_duty {
        return -10_000;
    }
    let risk = if input.violent {
        1_500 - i64::from(input.risk_tolerance)
    } else {
        300
    };
    let payment: i64 = compensation
        .iter()
        .map(|term| {
            if term.parties()[1] == input.actor {
                term.amount().min(10_000) as i64
            } else if term.parties()[0] == input.actor {
                -(term.amount().min(10_000) as i64)
            } else {
                0
            }
        })
        .sum();
    i64::from(input.ambition) / 2
        + i64::from(input.sociability) / 2
        + i64::from(input.sponsor_trust)
        + payment
        - input.social_disruption
        - risk
        - if input.urgent_care { 3_000 } else { 0 }
        - if input.hungry { 2_000 } else { 0 }
        - if input.violent {
            i64::from(input.empathy) / 2
        } else {
            0
        }
}

impl crate::Runtime {
    pub fn leader_judgment(
        &self,
        store: &game_ecs::Store,
        leader: PersonId,
        id: MissionId,
        objective: &crate::MissionObjective,
        mut input: RecruitmentInput,
    ) -> Result<i64, Error> {
        let m = crate::mission(store, id).ok_or(Error::Unavailable)?;
        if m.proposal.leader != leader || input.actor != leader {
            return Err(Error::Unavailable);
        }
        let offer = crate::recruitment(
            store,
            m.members
                .get(&leader)
                .ok_or(Error::Unavailable)?
                .recruitment,
        )
        .ok_or(Error::Unavailable)?;
        input.violent = objective.requires_force();
        Ok(recruitment_score(input, &offer.compensation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> RecruitmentInput {
        RecruitmentInput {
            actor: PersonId(1),
            guardian_duty: false,
            violent: false,
            risk_tolerance: 500,
            ambition: 900,
            sociability: 900,
            empathy: 500,
            sponsor_trust: 0,
            social_disruption: 0,
            urgent_care: false,
            hungry: false,
        }
    }
    #[test]
    fn the_same_offer_can_be_accepted_or_refused_according_to_personal_needs_and_experience() {
        assert_eq!(recruitment_score(input(), &[]), 600);
        assert!(
            recruitment_score(
                RecruitmentInput {
                    urgent_care: true,
                    ..input()
                },
                &[]
            ) < 0
        );
        assert!(
            recruitment_score(
                RecruitmentInput {
                    sponsor_trust: -900,
                    ..input()
                },
                &[]
            ) < 0
        );
        assert!(
            recruitment_score(
                RecruitmentInput {
                    violent: true,
                    ..input()
                },
                &[]
            ) < 0
        );
        assert_eq!(
            recruitment_score(
                RecruitmentInput {
                    guardian_duty: true,
                    ..input()
                },
                &[]
            ),
            -10_000
        );
    }
    #[test]
    fn confinement_needs_an_actual_capture_before_the_end_and_a_later_release() {
        let person = PersonId(2);
        let mut record = crate::ConfinementObservation {
            person,
            captured_at: SimTime(10),
            released_at: None,
        };
        assert!(!record.finished(person, SimTime(20), SimTime(30)));
        record.released_at = Some(SimTime(19));
        assert!(!record.finished(person, SimTime(20), SimTime(30)));
        record.released_at = Some(SimTime(20));
        assert!(record.finished(person, SimTime(20), SimTime(30)));
        assert!(!record.finished(PersonId(3), SimTime(20), SimTime(30)));
        record.captured_at = SimTime(20);
        assert!(!record.finished(person, SimTime(20), SimTime(30)));
    }
}

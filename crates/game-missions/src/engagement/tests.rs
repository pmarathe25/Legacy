use super::*;
use crate::fixture::Fixture;
use crate::transition_tests::*;
use std::collections::BTreeMap;

#[test]
fn engagement_attempts_consume_the_decision_including_rejected_receivers() {
    #[derive(Default)]
    struct Execution {
        approaches: Vec<(PersonId, MissionId, EncounterId)>,
        starts: Vec<Vec<(PersonId, u32)>>,
        reject: bool,
    }
    impl EngagementExecution for Execution {
        fn approach(
            &mut self,
            actor: PersonId,
            mission: MissionId,
            encounter: EncounterId,
        ) -> Result<(), Error> {
            self.approaches.push((actor, mission, encounter));
            if self.reject {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        }
        fn start_encounter(
            &mut self,
            participants: Vec<(PersonId, u32)>,
        ) -> Result<EncounterId, Error> {
            self.starts.push(participants);
            if self.reject {
                Err(Error::Overflow)
            } else {
                Ok(EncounterId(91))
            }
        }
    }
    let mut execution = Execution::default();
    let actor = PersonId(7);
    let mission = MissionId(8);
    let encounter = EncounterId(9);
    let participants = vec![(actor, 0), (PersonId(10), 1)];
    let kept = execute_engagement_attempt(&mut execution, EngagementPlan::Keep, actor, mission);
    assert_eq!(kept, EngagementAttempt::Kept);
    assert!(kept.consumes_member_action());
    assert!(execution.approaches.is_empty() && execution.starts.is_empty());
    let approached = execute_engagement_attempt(
        &mut execution,
        EngagementPlan::Approach(encounter),
        actor,
        mission,
    );
    assert_eq!(approached, EngagementAttempt::Approached);
    assert!(approached.consumes_member_action());
    let started = execute_engagement_attempt(
        &mut execution,
        EngagementPlan::Start(participants.clone()),
        actor,
        mission,
    );
    assert_eq!(started, EngagementAttempt::Started(EncounterId(91)));
    assert!(started.consumes_member_action());
    execution.reject = true;
    let rejected_approach = execute_engagement_attempt(
        &mut execution,
        EngagementPlan::Approach(encounter),
        actor,
        mission,
    );
    assert_eq!(
        rejected_approach,
        EngagementAttempt::Failed(Error::Unavailable)
    );
    assert!(rejected_approach.consumes_member_action());
    let rejected_start = execute_engagement_attempt(
        &mut execution,
        EngagementPlan::Start(participants.clone()),
        actor,
        mission,
    );
    assert_eq!(rejected_start, EngagementAttempt::Failed(Error::Overflow));
    assert!(rejected_start.consumes_member_action());
    assert_eq!(execution.approaches, vec![(actor, mission, encounter); 2]);
    assert_eq!(execution.starts, vec![participants; 2]);
}

struct Facts {
    encounters: Vec<EncounterFacts>,
    bodies: BTreeMap<PersonId, FighterFacts>,
}
impl EngagementFacts for Facts {
    fn encounters(&self, _: PersonId) -> impl Iterator<Item = EncounterFacts> {
        self.encounters.iter().copied()
    }
    fn fighter(&self, _: MissionId, person: PersonId) -> FighterFacts {
        self.bodies[&person]
    }
}
fn body() -> FighterFacts {
    FighterFacts {
        player: false,
        at_area: Some(HOME),
        can_fight: true,
        in_combat: false,
        recruitment: RecruitmentInput {
            actor: A,
            guardian_duty: false,
            violent: false,
            risk_tolerance: 900,
            ambition: 900,
            sociability: 900,
            empathy: 0,
            sponsor_trust: 0,
            social_disruption: 0,
            urgent_care: false,
            hungry: false,
        },
    }
}
fn world() -> Fixture {
    let mut w = state();
    let f = facts();
    let mut p = proposal();
    p.objective = MissionObjective::Defeat { person: ENEMY };
    start(&mut w, p, &f);
    accept(&mut w, A, OFFER, &f);
    w
}
#[test]
fn target_encounter_priority_is_stable_and_precedes_roster_reads() {
    let w = world();
    let f = Facts {
        encounters: vec![
            EncounterFacts {
                id: EncounterId(8),
                active: true,
                target_present: true,
            },
            EncounterFacts {
                id: EncounterId(3),
                active: false,
                target_present: true,
            },
            EncounterFacts {
                id: EncounterId(5),
                active: true,
                target_present: true,
            },
        ],
        bodies: BTreeMap::new(),
    };
    assert_eq!(
        w.engagement_plan(&w.store, ID, ENEMY, Some(HOME), &BTreeSet::from([ID]), &f),
        EngagementPlan::Approach(EncounterId(5))
    );
    assert_eq!(
        w.engagement_plan(&w.store, ID, ENEMY, None, &BTreeSet::from([ID]), &f),
        EngagementPlan::Keep
    );
}
#[test]
fn local_accepted_qualified_nonplayer_allies_join_in_stable_id_order() {
    let mut w = world();
    mission_mut(&mut w.store, ID)
        .unwrap()
        .members
        .get_mut(&B)
        .unwrap()
        .status = CompanionStatus::Accepted;
    let mut f = Facts {
        encounters: Vec::new(),
        bodies: BTreeMap::from([
            (A, body()),
            (
                B,
                FighterFacts {
                    recruitment: RecruitmentInput {
                        actor: B,
                        ..body().recruitment
                    },
                    ..body()
                },
            ),
        ]),
    };
    let expected = EngagementPlan::Start(vec![(A, 0), (B, 0), (ENEMY, 1)]);
    assert_eq!(
        w.engagement_plan(&w.store, ID, ENEMY, Some(HOME), &BTreeSet::from([ID]), &f),
        expected
    );
    f.bodies.get_mut(&B).unwrap().in_combat = true;
    assert_eq!(
        w.engagement_plan(&w.store, ID, ENEMY, Some(HOME), &BTreeSet::from([ID]), &f),
        EngagementPlan::Start(vec![(A, 0), (ENEMY, 1)])
    );
    f.bodies.get_mut(&A).unwrap().player = true;
    assert_eq!(
        w.engagement_plan(&w.store, ID, ENEMY, Some(HOME), &BTreeSet::from([ID]), &f),
        EngagementPlan::Start(vec![(ENEMY, 1)])
    );
}
#[test]
fn allied_service_keeps_individual_refusal_without_cloning_a_proposed_mission() {
    let mut w = world();
    let other = MissionId(30);
    let mut proposal = proposal();
    proposal.leader = B;
    proposal.recruits.clear();
    proposal.supplies.clear();
    proposal.objective = MissionObjective::Defeat { person: ENEMY };
    command(
        &mut w,
        SP,
        MissionAction::Propose { proposal },
        &facts(),
        MissionIds {
            mission: Some(other),
            recruitments: BTreeMap::from([(B, RecruitmentId(31))]),
            backing: None,
        },
    );
    accept(&mut w, B, RecruitmentId(31), &facts());
    let mut refusal = body();
    refusal.recruitment.actor = B;
    refusal.recruitment.sponsor_trust = -1000;
    let f = Facts {
        encounters: Vec::new(),
        bodies: BTreeMap::from([(A, body()), (B, refusal)]),
    };
    assert!(!w.force_willing(&w.store, B, other, ENEMY, refusal.recruitment, false));
    assert_eq!(
        w.engagement_plan(
            &w.store,
            ID,
            ENEMY,
            Some(HOME),
            &BTreeSet::from([ID, other]),
            &f
        ),
        EngagementPlan::Start(vec![(A, 0), (ENEMY, 1)])
    );
    let mut guardian = body();
    guardian.recruitment.guardian_duty = true;
    assert!(!w.force_willing(&w.store, A, ID, ENEMY, guardian.recruitment, true));
}

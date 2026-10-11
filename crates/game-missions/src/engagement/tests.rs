use super::*;
use crate::transition_tests::*;
use std::collections::BTreeMap;
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
fn world() -> World {
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
        w.engagement_plan(ID, ENEMY, Some(HOME), &BTreeSet::from([ID]), &f),
        EngagementPlan::Approach(EncounterId(5))
    );
    assert_eq!(
        w.engagement_plan(ID, ENEMY, None, &BTreeSet::from([ID]), &f),
        EngagementPlan::Keep
    );
}
#[test]
fn local_accepted_qualified_nonplayer_allies_join_in_stable_id_order() {
    let mut w = world();
    w.missions
        .get_mut(&ID)
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
        w.engagement_plan(ID, ENEMY, Some(HOME), &BTreeSet::from([ID]), &f),
        expected
    );
    f.bodies.get_mut(&B).unwrap().in_combat = true;
    assert_eq!(
        w.engagement_plan(ID, ENEMY, Some(HOME), &BTreeSet::from([ID]), &f),
        EngagementPlan::Start(vec![(A, 0), (ENEMY, 1)])
    );
    f.bodies.get_mut(&A).unwrap().player = true;
    assert_eq!(
        w.engagement_plan(ID, ENEMY, Some(HOME), &BTreeSet::from([ID]), &f),
        EngagementPlan::Start(vec![(ENEMY, 1)])
    );
}
#[test]
fn allied_service_keeps_individual_refusal_without_cloning_a_proposed_mission() {
    let mut w = world();
    let other = MissionId(30);
    let mut m = w.missions[&ID].clone();
    m.id = other;
    m.members.retain(|p, _| *p == B);
    m.members.get_mut(&B).unwrap().status = CompanionStatus::Accepted;
    w.missions.insert(other, m);
    let mut refusal = body();
    refusal.recruitment.actor = B;
    refusal.recruitment.sponsor_trust = -1000;
    let f = Facts {
        encounters: Vec::new(),
        bodies: BTreeMap::from([(A, body()), (B, refusal)]),
    };
    assert!(!w.force_willing(B, other, ENEMY, refusal.recruitment, false));
    assert_eq!(
        w.engagement_plan(ID, ENEMY, Some(HOME), &BTreeSet::from([ID, other]), &f),
        EngagementPlan::Start(vec![(A, 0), (ENEMY, 1)])
    );
    let mut guardian = body();
    guardian.recruitment.guardian_duty = true;
    assert!(!w.force_willing(A, ID, ENEMY, guardian.recruitment, true));
}

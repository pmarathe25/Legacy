use super::*;
use crate::transition_tests::*;
use game_conflict::{Fighter, SearchState};
use std::collections::BTreeMap;
struct Facts {
    arrivals: Vec<ArrivalFacts>,
    encounters: Vec<Encounter>,
    body: ReinforcementBody,
    linked: BTreeSet<MissionId>,
}
impl ReinforcementFacts for Facts {
    fn arrivals(&self) -> impl Iterator<Item = ArrivalFacts> {
        self.arrivals.iter().copied()
    }
    fn body(&self, _: MissionId, _: PersonId) -> ReinforcementBody {
        self.body
    }
    fn encounters(&self) -> impl Iterator<Item = &Encounter> {
        self.encounters.iter()
    }
    fn linked_missions(&self, _: MissionId) -> BTreeSet<MissionId> {
        self.linked.clone()
    }
}
fn fighter(actor: PersonId, team: u32) -> Fighter {
    Fighter {
        actor,
        joins_round: 0,
        team,
        position: Point { x: 1000, y: 1000 },
        surrendered: false,
        withdrawn: false,
        defending: false,
        retreat: None,
        known: BTreeMap::new(),
        search: SearchState::default(),
    }
}
fn encounter(id: u64) -> Encounter {
    Encounter {
        id: EncounterId(id),
        area: HOME,
        location: Location::AtArea(HOME),
        map: CombatMapId(1),
        round: 0,
        started: SimTime(0),
        ends: SimTime(10),
        fighters: BTreeMap::from([(B, fighter(B, 7)), (ENEMY, fighter(ENEMY, 9))]),
        order: vec![B, ENEMY],
        cursor: 0,
        active: true,
    }
}
fn world() -> World {
    let mut w = state();
    let f = facts();
    let mut p = proposal();
    p.objective = MissionObjective::Defeat { person: ENEMY };
    start(&mut w, p, &f);
    accept(&mut w, A, OFFER, &f);
    let m = w.missions.get_mut(&ID).unwrap();
    m.members.get_mut(&A).unwrap().last_leg_from = Some(AWAY);
    m.members.get_mut(&B).unwrap().status = CompanionStatus::Accepted;
    w
}
fn fixture() -> Facts {
    Facts {
        arrivals: vec![ArrivalFacts {
            actor: A,
            area: HOME,
            at: SimTime(5),
        }],
        encounters: vec![encounter(8), encounter(3)],
        body: ReinforcementBody {
            player: false,
            can_fight: true,
            in_combat: false,
            area: Some(HOME),
            recruitment: RecruitmentInput {
                actor: A,
                guardian_duty: false,
                violent: true,
                risk_tolerance: 900,
                ambition: 900,
                sociability: 900,
                empathy: 0,
                sponsor_trust: 0,
                social_disruption: 0,
                urgent_care: false,
                hungry: false,
            },
        },
        linked: BTreeSet::from([ID]),
    }
}
#[test]
fn only_current_arrivals_choose_first_eligible_encounter_and_ally_team() {
    let w = world();
    let mut facts = fixture();
    assert_eq!(
        w.reinforcements(SimTime(5), &facts),
        vec![Reinforcement {
            encounter: EncounterId(3),
            actor: A,
            team: 7,
            from: AWAY
        }]
    );
    assert!(w.reinforcements(SimTime(6), &facts).is_empty());
    facts.encounters[1]
        .fighters
        .get_mut(&B)
        .unwrap()
        .surrendered = true;
    assert_eq!(
        w.reinforcements(SimTime(5), &facts)[0].encounter,
        EncounterId(8)
    );
    facts.body.player = true;
    assert!(w.reinforcements(SimTime(5), &facts).is_empty());
}
#[test]
fn busy_members_and_arrivals_elsewhere_do_not_join() {
    let w = world();
    let mut facts = fixture();
    facts.body.in_combat = true;
    assert!(w.reinforcements(SimTime(5), &facts).is_empty());
    facts.body.in_combat = false;
    facts.arrivals[0].area = AWAY;
    assert!(w.reinforcements(SimTime(5), &facts).is_empty());
}

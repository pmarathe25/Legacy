use crate::*;
use game_core::*;
use game_missions::{CompanionStatus, CompanionView, MissionObjective, MissionStatus, MissionView};
use std::collections::{BTreeMap, BTreeSet};

const COMMANDER: PersonId = PersonId(100);
const ARMY: ArmyId = ArmyId(1);
fn objective() -> MissionObjective {
    MissionObjective::Defeat {
        person: PersonId(200),
    }
}
fn facts() -> BTreeMap<MissionId, DetachmentFacts> {
    [(1, 1, 22), (2, 23, 22), (3, 45, 21)]
        .into_iter()
        .map(|(mission, first, count)| {
            let id = MissionId(mission);
            (
                id,
                DetachmentFacts {
                    mission: id,
                    leader: PersonId(first),
                    members: (first..first + count).map(PersonId).collect(),
                    status: MissionStatus::Preparing,
                    leader_capable: true,
                },
            )
        })
        .collect()
}
fn communicate(speaker: PersonId, listener: PersonId, at: u64) -> Communication {
    Communication {
        speaker,
        listener,
        started_at: SimTime(at - 1),
        completed_at: SimTime(at),
    }
}
fn form(
    world: &mut World,
    id: ArmyId,
    missions: BTreeSet<MissionId>,
    facts: &BTreeMap<MissionId, DetachmentFacts>,
    at: u64,
) {
    world
        .prepare_form(id, COMMANDER, &missions, facts, SimTime(at))
        .unwrap()
        .commit(world)
        .unwrap();
}
fn join(
    world: &mut World,
    id: ArmyId,
    mission: MissionId,
    facts: &BTreeMap<MissionId, DetachmentFacts>,
    at: u64,
) {
    let leader = facts[&mission].leader;
    world
        .prepare_hear_invitation(id, mission, communicate(COMMANDER, leader, at), SimTime(at))
        .unwrap()
        .commit(world)
        .unwrap();
    world
        .prepare_respond(
            ParticipationDecision {
                army: id,
                mission,
                actor: leader,
                accept: true,
                explanation: "Agreed to cooperate under this commander".into(),
            },
            &facts[&mission],
            SimTime(at),
        )
        .unwrap()
        .commit(world)
        .unwrap();
    world
        .prepare_hear_participation(
            id,
            mission,
            communicate(leader, COMMANDER, at + 1),
            SimTime(at + 1),
        )
        .unwrap()
        .commit(world)
        .unwrap();
}
fn army() -> (World, BTreeMap<MissionId, DetachmentFacts>) {
    let f = facts();
    let mut world = World::default();
    form(&mut world, ARMY, f.keys().copied().collect(), &f, 0);
    for m in f.keys() {
        join(&mut world, ARMY, *m, &f, 1);
    }
    (world, f)
}
fn issue(world: &mut World, id: u64, at: u64) {
    world
        .prepare_order(
            OrderProposal {
                army: ARMY,
                id: ArmyOrderId(id),
                commander: COMMANDER,
                missions: BTreeSet::from([MissionId(1), MissionId(2)]),
                objective: objective(),
                expires: SimTime(100),
            },
            SimTime(at),
        )
        .unwrap()
        .commit(world)
        .unwrap();
}
fn hear(
    world: &mut World,
    f: &BTreeMap<MissionId, DetachmentFacts>,
    id: u64,
    mission: u64,
    at: u64,
) {
    let m = MissionId(mission);
    world
        .prepare_hear_order(
            ARMY,
            ArmyOrderId(id),
            m,
            communicate(COMMANDER, f[&m].leader, at),
            &f[&m],
            SimTime(at),
        )
        .unwrap()
        .commit(world)
        .unwrap();
}
fn respond(
    world: &mut World,
    f: &BTreeMap<MissionId, DetachmentFacts>,
    id: u64,
    mission: u64,
    response: OrderResponse,
    at: u64,
) -> Vec<Effect> {
    let m = MissionId(mission);
    world
        .prepare_respond_order(
            OrderDecision {
                army: ARMY,
                order: ArmyOrderId(id),
                mission: m,
                actor: f[&m].leader,
                response,
                explanation: "A personal choice after considering the received objective".into(),
            },
            &f[&m],
            SimTime(at),
        )
        .unwrap()
        .commit(world)
        .unwrap()
}

#[test]
fn sixty_five_people_remain_owned_by_three_real_detachments() {
    let (world, f) = army();
    let actual_people: BTreeSet<_> = f.values().flat_map(|d| d.members.iter().copied()).collect();
    assert_eq!(actual_people.len(), 65);
    assert_eq!(world.armies[&ARMY].detachments.len(), 3);
    assert!(
        world.armies[&ARMY]
            .detachments
            .values()
            .all(|d| d.participation.active())
    );
    // The army stores three command links, not 65 replicated bodies or packs.
    let serialized = serde_json::to_value(&world.armies[&ARMY]).unwrap();
    assert!(serialized.get("members").is_none());
    assert!(serialized.get("inventories").is_none());
    assert!(serialized.get("location").is_none());
    assert_eq!(
        actual_people,
        f.values().flat_map(|d| d.members.iter().copied()).collect()
    );
}
#[test]
fn invitations_need_hearing_and_each_leader_can_refuse() {
    let f = facts();
    let m = MissionId(2);
    let leader = f[&m].leader;
    let mut world = World::default();
    form(&mut world, ARMY, BTreeSet::from([m]), &f, 0);
    let before = world.clone();
    assert!(
        world
            .prepare_respond(
                ParticipationDecision {
                    army: ARMY,
                    mission: m,
                    actor: leader,
                    accept: true,
                    explanation: "Yes".into()
                },
                &f[&m],
                SimTime(1)
            )
            .is_err()
    );
    assert_eq!(world, before);
    assert!(world.view(leader, ARMY).is_none());
    world
        .prepare_hear_invitation(ARMY, m, communicate(COMMANDER, leader, 1), SimTime(1))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert!(
        world
            .prepare_respond(
                ParticipationDecision {
                    army: ARMY,
                    mission: m,
                    actor: COMMANDER,
                    accept: true,
                    explanation: "I command it".into()
                },
                &f[&m],
                SimTime(1)
            )
            .is_err()
    );
    world
        .prepare_respond(
            ParticipationDecision {
                army: ARMY,
                mission: m,
                actor: leader,
                accept: false,
                explanation: "I must care for people at home".into(),
            },
            &f[&m],
            SimTime(1),
        )
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert_eq!(
        world.armies[&ARMY].detachments[&m].participation,
        Participation::Refused
    );
    assert_eq!(
        world.view(COMMANDER, ARMY).unwrap().detachments[&m]
            .value
            .participation,
        Participation::Offered
    );
    world
        .prepare_hear_participation(ARMY, m, communicate(leader, COMMANDER, 2), SimTime(2))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert_eq!(
        world.view(COMMANDER, ARMY).unwrap().detachments[&m]
            .value
            .participation,
        Participation::Refused
    );
}
#[test]
fn orders_and_individual_responses_arrive_only_through_communication() {
    let (mut world, f) = army();
    issue(&mut world, 1, 3);
    assert!(world.view(PersonId(1), ARMY).unwrap().orders.is_empty());
    assert!(
        world
            .prepare_respond_order(
                OrderDecision {
                    army: ARMY,
                    order: ArmyOrderId(1),
                    mission: MissionId(1),
                    actor: PersonId(1),
                    response: OrderResponse::Accept,
                    explanation: "Accepted".into()
                },
                &f[&MissionId(1)],
                SimTime(4)
            )
            .is_err()
    );
    hear(&mut world, &f, 1, 1, 4);
    assert_eq!(
        world.view(PersonId(1), ARMY).unwrap().orders[&ArmyOrderId(1)]
            .value
            .recipients
            .len(),
        1
    );
    assert!(world.view(PersonId(23), ARMY).unwrap().orders.is_empty());
    assert_eq!(
        respond(&mut world, &f, 1, 1, OrderResponse::Accept, 4),
        vec![Effect::OfferObjective {
            mission: MissionId(1),
            leader: PersonId(1),
            objective: objective()
        }]
    );
    assert!(world.view(COMMANDER, ARMY).unwrap().responses.is_empty());
    world
        .prepare_hear_order_response(
            ARMY,
            ArmyOrderId(1),
            MissionId(1),
            communicate(PersonId(1), COMMANDER, 5),
            SimTime(5),
        )
        .unwrap()
        .commit(&mut world)
        .unwrap();
    let known = world.view(COMMANDER, ARMY).unwrap();
    assert_eq!(
        known.responses[&ArmyOrderId(1)][&MissionId(1)].value.status,
        OrderStatus::Accepted
    );
    assert_eq!(
        known.responses[&ArmyOrderId(1)][&MissionId(1)].source,
        KnowledgeSource::Reported(PersonId(1))
    );
    assert!(!known.responses[&ArmyOrderId(1)].contains_key(&MissionId(2)));
    hear(&mut world, &f, 1, 2, 6);
    assert!(respond(&mut world, &f, 1, 2, OrderResponse::Refuse, 6).is_empty());
    assert_eq!(
        world.armies[&ARMY].detachments[&MissionId(2)].participation,
        Participation::Accepted
    );
}
#[test]
fn a_delayed_older_order_cannot_replace_a_newer_received_order() {
    let (mut world, f) = army();
    issue(&mut world, 1, 3);
    issue(&mut world, 2, 4);
    hear(&mut world, &f, 2, 1, 5);
    hear(&mut world, &f, 1, 1, 6);
    assert_eq!(
        world.armies[&ARMY].orders[&ArmyOrderId(1)].recipients[&MissionId(1)].status,
        OrderStatus::Superseded
    );
    assert!(
        world
            .prepare_respond_order(
                OrderDecision {
                    army: ARMY,
                    order: ArmyOrderId(1),
                    mission: MissionId(1),
                    actor: PersonId(1),
                    response: OrderResponse::Accept,
                    explanation: "Yes".into()
                },
                &f[&MissionId(1)],
                SimTime(6)
            )
            .is_err()
    );
    assert_eq!(
        respond(&mut world, &f, 2, 1, OrderResponse::Accept, 6).len(),
        1
    );
    // A newly received command does not undo already performed owner work.
    issue(&mut world, 3, 7);
    hear(&mut world, &f, 3, 1, 8);
    assert_eq!(
        world.armies[&ARMY].orders[&ArmyOrderId(2)].recipients[&MissionId(1)].status,
        OrderStatus::Accepted
    );
}
#[test]
fn retreat_and_defection_release_only_the_chosen_command_relationship() {
    let (mut world, f) = army();
    issue(&mut world, 1, 3);
    hear(&mut world, &f, 1, 1, 4);
    hear(&mut world, &f, 1, 2, 4);
    let effects = respond(&mut world, &f, 1, 1, OrderResponse::Retreat, 4);
    assert_eq!(
        effects,
        vec![
            Effect::ReleaseCommand {
                mission: MissionId(1),
                leader: PersonId(1),
                reason: Departure::Retreat
            },
            Effect::OfferRetreat {
                mission: MissionId(1),
                leader: PersonId(1)
            }
        ]
    );
    assert!(world.commanding_army(MissionId(1)).is_none());
    assert_eq!(world.commanding_army(MissionId(2)), Some(ARMY));
    assert_eq!(
        respond(&mut world, &f, 1, 2, OrderResponse::Defect, 4),
        vec![Effect::ReleaseCommand {
            mission: MissionId(2),
            leader: PersonId(23),
            reason: Departure::Defect
        }]
    );
    assert_eq!(
        world.armies[&ARMY].detachments[&MissionId(2)].participation,
        Participation::Defected
    );
    assert_eq!(f.values().map(|f| f.members.len()).sum::<usize>(), 65);
}
#[test]
fn cross_army_acceptance_races_and_stale_changes_are_atomic() {
    let f = facts();
    let m = MissionId(1);
    let mut world = World::default();
    for id in [ARMY, ArmyId(2)] {
        form(&mut world, id, BTreeSet::from([m]), &f, 0);
        world
            .prepare_hear_invitation(id, m, communicate(COMMANDER, PersonId(1), 1), SimTime(1))
            .unwrap()
            .commit(&mut world)
            .unwrap();
    }
    let first = world
        .prepare_respond(
            ParticipationDecision {
                army: ARMY,
                mission: m,
                actor: PersonId(1),
                accept: true,
                explanation: "Accepted first".into(),
            },
            &f[&m],
            SimTime(1),
        )
        .unwrap();
    let second = world
        .prepare_respond(
            ParticipationDecision {
                army: ArmyId(2),
                mission: m,
                actor: PersonId(1),
                accept: true,
                explanation: "Accepted second".into(),
            },
            &f[&m],
            SimTime(1),
        )
        .unwrap();
    first.commit(&mut world).unwrap();
    let before = world.clone();
    assert!(second.commit(&mut world).is_err());
    assert_eq!(world, before);
    let stale = world
        .prepare_leave(
            ARMY,
            m,
            PersonId(1),
            Departure::Withdraw,
            "Withdraw".into(),
            SimTime(2),
        )
        .unwrap();
    world
        .prepare_disband(ARMY, COMMANDER, SimTime(2))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    let before = world.clone();
    assert!(stale.commit(&mut world).is_err());
    assert_eq!(world, before);
}
#[test]
fn expiration_neither_executes_an_order_nor_updates_distant_knowledge() {
    let (mut world, f) = army();
    issue(&mut world, 1, 3);
    hear(&mut world, &f, 1, 1, 4);
    assert_eq!(world.next_boundary(SimTime(4)), Some(SimTime(100)));
    let before = world.known.clone();
    assert!(
        world
            .prepare_expire(ARMY, SimTime(100))
            .unwrap()
            .commit(&mut world)
            .unwrap()
            .is_empty()
    );
    assert_eq!(world.known, before);
    assert_eq!(world.next_boundary(SimTime(100)), None);
    assert!(
        world
            .prepare_respond_order(
                OrderDecision {
                    army: ARMY,
                    order: ArmyOrderId(1),
                    mission: MissionId(1),
                    actor: PersonId(1),
                    response: OrderResponse::Accept,
                    explanation: "Accepted".into()
                },
                &f[&MissionId(1)],
                SimTime(100)
            )
            .is_err()
    );
}
fn mission_report() -> MissionView {
    MissionView {
        id: MissionId(1),
        sponsor: COMMANDER,
        leader: PersonId(1),
        objective: objective(),
        status: MissionStatus::Operating,
        home: AreaId(1),
        deadline: SimTime(200),
        companions: vec![CompanionView {
            person: PersonId(2),
            status: CompanionStatus::Captured,
            location: Some(Location::AtArea(AreaId(2))),
            explanation: "Saw this person's capture".into(),
            observed_at: SimTime(5),
        }],
        recruitments: vec![],
        supplies: vec![],
        explanation: "Withdrew after a capture".into(),
        observed_at: SimTime(5),
        reported_by: None,
    }
}
#[test]
fn frozen_reports_preserve_attributable_losses_and_survive_save_continuation() {
    let (mut world, _) = army();
    world
        .prepare_report(
            ARMY,
            ArmyReport {
                id: ArmyReportId(1),
                army: ARMY,
                mission: MissionId(1),
                speaker: PersonId(1),
                sent_at: SimTime(6),
                snapshot: mission_report(),
            },
            SimTime(6),
        )
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert!(world.view(COMMANDER, ARMY).unwrap().reports.is_empty());
    let bytes = serde_json::to_vec(&world).unwrap();
    let mut saved: World = serde_json::from_slice(&bytes).unwrap();
    for w in [&mut world, &mut saved] {
        w.prepare_hear_report(
            ARMY,
            ArmyReportId(1),
            communicate(PersonId(1), COMMANDER, 20),
            SimTime(20),
        )
        .unwrap()
        .commit(w)
        .unwrap();
    }
    assert_eq!(world, saved);
    let report = &world.view(COMMANDER, ARMY).unwrap().reports[&ArmyReportId(1)];
    assert_eq!(report.observed_at, SimTime(5));
    assert_eq!(report.learned_at, SimTime(20));
    assert_eq!(report.value.snapshot.companions[0].person, PersonId(2));
    assert_eq!(
        report.value.snapshot.companions[0].status,
        CompanionStatus::Captured
    );
}
#[test]
fn a_self_led_detachment_requires_the_commanders_own_choices() {
    let mut f = facts();
    f.get_mut(&MissionId(1)).unwrap().leader = COMMANDER;
    f.get_mut(&MissionId(1)).unwrap().members.insert(COMMANDER);
    let mut world = World::default();
    form(&mut world, ARMY, BTreeSet::from([MissionId(1)]), &f, 0);
    world
        .prepare_consider_own_invitation(ARMY, MissionId(1), COMMANDER, SimTime(0))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    world
        .prepare_respond(
            ParticipationDecision {
                army: ARMY,
                mission: MissionId(1),
                actor: COMMANDER,
                accept: true,
                explanation: "I will lead personally".into(),
            },
            &f[&MissionId(1)],
            SimTime(0),
        )
        .unwrap()
        .commit(&mut world)
        .unwrap();
    world
        .prepare_order(
            OrderProposal {
                army: ARMY,
                id: ArmyOrderId(1),
                commander: COMMANDER,
                missions: BTreeSet::from([MissionId(1)]),
                objective: objective(),
                expires: SimTime(100),
            },
            SimTime(0),
        )
        .unwrap()
        .commit(&mut world)
        .unwrap();
    world
        .prepare_consider_own_order(
            ARMY,
            ArmyOrderId(1),
            MissionId(1),
            COMMANDER,
            &f[&MissionId(1)],
            SimTime(0),
        )
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert_eq!(
        world
            .prepare_respond_order(
                OrderDecision {
                    army: ARMY,
                    order: ArmyOrderId(1),
                    mission: MissionId(1),
                    actor: COMMANDER,
                    response: OrderResponse::Accept,
                    explanation: "My chosen objective".into()
                },
                &f[&MissionId(1)],
                SimTime(0)
            )
            .unwrap()
            .commit(&mut world)
            .unwrap()
            .len(),
        1
    );
}

fn validate(world: &World, now: SimTime) -> Result<(), Error> {
    let f = facts();
    let people: BTreeSet<_> = f
        .values()
        .flat_map(|d| d.members.iter().copied())
        .chain([COMMANDER, PersonId(200)])
        .collect();
    let missions = f.keys().copied().collect();
    world.validate(
        &ValidationCatalog {
            people: &people,
            missions: &missions,
            now,
        },
        |objective| {
            if objective
                .target_person()
                .is_none_or(|p| people.contains(&p))
            {
                Ok(())
            } else {
                Err(Error::InvalidSave("Unknown objective target".into()))
            }
        },
        |view| {
            if view.companions.iter().all(|c| people.contains(&c.person)) {
                Ok(())
            } else {
                Err(Error::InvalidSave(
                    "Unknown person in native mission report".into(),
                ))
            }
        },
    )
}
#[test]
fn save_validation_accepts_dated_beliefs_but_rejects_unknown_and_conflicting_authority() {
    let (mut world, f) = army();
    issue(&mut world, 1, 3);
    hear(&mut world, &f, 1, 1, 4);
    respond(&mut world, &f, 1, 1, OrderResponse::Accept, 4);
    world
        .prepare_hear_order_response(
            ARMY,
            ArmyOrderId(1),
            MissionId(1),
            communicate(PersonId(1), COMMANDER, 5),
            SimTime(5),
        )
        .unwrap()
        .commit(&mut world)
        .unwrap();
    world
        .prepare_expire(ARMY, SimTime(100))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    validate(&world, SimTime(100)).unwrap();
    let json = serde_json::to_vec(&world).unwrap();
    let restored: World = serde_json::from_slice(&json).unwrap();
    assert_eq!(world, restored);
    validate(&restored, SimTime(100)).unwrap();
    let mut unknown = world.clone();
    unknown
        .armies
        .get_mut(&ARMY)
        .unwrap()
        .detachments
        .get_mut(&MissionId(1))
        .unwrap()
        .leader = PersonId(999);
    assert!(validate(&unknown, SimTime(100)).is_err());
    let mut conflicting = world.clone();
    let mut other = conflicting.armies[&ARMY].clone();
    other.id = ArmyId(2);
    other.orders.clear();
    conflicting.armies.insert(other.id, other);
    assert!(validate(&conflicting, SimTime(100)).is_err());
    let mut forged = world;
    forged
        .known
        .get_mut(&COMMANDER)
        .unwrap()
        .get_mut(&ARMY)
        .unwrap()
        .responses
        .get_mut(&ArmyOrderId(1))
        .unwrap()
        .get_mut(&MissionId(1))
        .unwrap()
        .value
        .status = OrderStatus::Refused;
    assert!(validate(&forged, SimTime(100)).is_err());
}
#[test]
fn a_pending_expiration_has_the_same_result_under_irregular_clock_slices() {
    let (mut whole, f) = army();
    issue(&mut whole, 1, 3);
    hear(&mut whole, &f, 1, 1, 4);
    let mut sliced = whole.clone();
    whole
        .prepare_expire(ARMY, SimTime(100))
        .unwrap()
        .commit(&mut whole)
        .unwrap();
    for at in [5, 6, 17, 91, 99, 100] {
        sliced
            .prepare_expire(ARMY, SimTime(at))
            .unwrap()
            .commit(&mut sliced)
            .unwrap();
    }
    assert_eq!(whole, sliced);
    validate(&whole, SimTime(100)).unwrap();
}

#[test]
fn native_mission_completion_releases_a_link_without_sending_a_remote_report() {
    let (mut world, mut f) = army();
    let before = world.known.clone();
    let ended = f.get_mut(&MissionId(1)).unwrap();
    ended.status = MissionStatus::Completed;
    let effects = world
        .prepare_native_termination(ARMY, ended, SimTime(10))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert!(effects.is_empty());
    assert!(world.commanding_army(MissionId(1)).is_none());
    assert_eq!(world.known, before);
    assert_eq!(world.commanding_army(MissionId(2)), Some(ARMY));
    validate(&world, SimTime(10)).unwrap();
}

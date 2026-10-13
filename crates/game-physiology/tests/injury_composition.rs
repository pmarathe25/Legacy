use game_core::{
    AreaId, CombatMapId, EncounterId, Error, Location, PersonId, Point, SimDuration, SimTime,
    Skill, Vitals,
};
use game_ecs::{Batch, StableId, Store};
use game_physiology::{Body, prepare_injury};
use std::collections::BTreeMap;
fn fighter(actor: PersonId, x: i64) -> game_conflict::Fighter {
    game_conflict::Fighter {
        actor,
        joins_round: 0,
        team: actor.0 as u32,
        position: Point { x, y: 1_000 },
        surrendered: false,
        withdrawn: false,
        defending: false,
        retreat: None,
        known: BTreeMap::new(),
        search: game_conflict::SearchState::default(),
    }
}
fn fixture(reject: u8) -> (Store, game_conflict::World, game_skills::Runtime) {
    let mut schema = game_ecs::Schema::new();
    if reject == 1 {
        schema
            .register::<Body>("physiology/body", |body| {
                body.validate()?;
                if body.vitals.health < 100 {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
    } else {
        game_physiology::register_components(&mut schema).unwrap();
    }
    if reject == 2 {
        schema
            .register::<game_skills::Skills>("skills/personal", |skills| {
                skills.validate()?;
                if skills.records.is_empty() {
                    Ok(())
                } else {
                    Err(Error::Unavailable)
                }
            })
            .unwrap();
    } else {
        game_skills::register_components(&mut schema).unwrap();
    }
    game_conflict::register_components(&mut schema).unwrap();
    game_spatial::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let area = AreaId(20);
    for actor in [PersonId(1), PersonId(2)] {
        game_physiology::insert_body(
            &mut store,
            actor,
            Body {
                vitals: Vitals {
                    hunger: 0,
                    fatigue: 0,
                    health: 100,
                    alive: true,
                },
                fatigue_remainder: 0,
                health_remainder: 0,
            },
        )
        .unwrap();
        game_spatial::set_location(&mut store, StableId(actor.0), Location::AtArea(area)).unwrap();
    }
    game_conflict::insert_map(
        &mut store,
        game_conflict::Map {
            id: CombatMapId(10),
            version: 1,
            area,
            location: Location::AtArea(area),
            width: 10_000,
            height: 10_000,
            obstacles: vec![],
            exits: vec![],
        },
    )
    .unwrap();
    game_conflict::insert_encounter(
        &mut store,
        game_conflict::Encounter {
            id: EncounterId(11),
            area,
            location: Location::AtArea(area),
            map: CombatMapId(10),
            round: 0,
            started: SimTime(0),
            ends: SimTime(1_000),
            fighters: BTreeMap::from([
                (PersonId(1), fighter(PersonId(1), 1_000)),
                (PersonId(2), fighter(PersonId(2), 2_000)),
            ]),
            order: vec![PersonId(1), PersonId(2)],
            cursor: 0,
            active: true,
        },
    )
    .unwrap();
    (
        store,
        game_conflict::World {
            rules: game_conflict::Rules {
                encounter_limit: 128,
                round: SimDuration(1_000),
                movement: 5_000,
                reach: 2_000,
                damage: 20,
                defence: 1,
                incapacitation: 1,
            },
        },
        game_skills::Runtime::default(),
    )
}
#[test]
fn attack_body_encounter_and_practice_publish_together_and_schema_failure_preserves_all_three() {
    for rejected_owner in 0..=2 {
        let (mut store, conflict, skills) = fixture(rejected_owner);
        let before = store.snapshot().unwrap();
        let turn = conflict
            .prepare_turn(
                &store,
                EncounterId(11),
                None,
                game_conflict::Action::Attack(PersonId(2)),
                game_conflict::TurnFacts {
                    actor: PersonId(1),
                    independent: true,
                    body: game_conflict::TargetBody {
                        alive: true,
                        health: 100,
                    },
                    force_can_act: false,
                    force_allowed: true,
                    target_body: Some(game_conflict::TargetBody {
                        alive: true,
                        health: 100,
                    }),
                    binding_available: false,
                    release_available: false,
                    mission_order_valid: false,
                    retreat_route_valid: false,
                    damage: 20,
                    practice_ms: 6_000,
                },
            )
            .unwrap();
        let [
            game_conflict::Effect::Struck {
                target,
                damage,
                practice,
                ..
            },
        ] = turn.effects()
        else {
            panic!("one resolved strike");
        };
        let injury = prepare_injury(&store, *target, *damage, false).unwrap();
        let practice = skills.prepare_practice(&store, [practice.clone()]).unwrap();
        let mut batch = Batch::new();
        let injury_receipt = injury.stage_into(&store, &mut batch).unwrap();
        assert!(injury_receipt.matches(PersonId(2), 20));
        assert!(!injury_receipt.matches(PersonId(1), 20));
        assert!(!injury_receipt.matches(PersonId(2), 21));
        let practice_receipt = practice.stage_into(&skills, &store, &mut batch).unwrap();
        let combat_receipt = turn
            .stage_with_practice(
                &conflict,
                &store,
                &mut batch,
                Some(&injury_receipt),
                None,
                Some(&practice_receipt),
            )
            .unwrap();
        combat_receipt.validate(&conflict, &store, &batch).unwrap();
        injury_receipt.validate(&store, &batch).unwrap();
        practice_receipt.validate(&skills, &store, &batch).unwrap();
        assert!(!injury_receipt.lethal());
        assert_eq!(store.snapshot().unwrap(), before);
        let result = store.apply_batch(batch);
        if rejected_owner == 0 {
            result.unwrap();
            assert_eq!(
                game_physiology::body(&store, PersonId(2))
                    .unwrap()
                    .vitals
                    .health,
                80
            );
            assert_eq!(
                game_conflict::encounter(&store, EncounterId(11))
                    .unwrap()
                    .cursor,
                1
            );
            assert_eq!(skills.practice(&store, PersonId(1), Skill::Combat), 6_000);
        } else {
            assert_eq!(result, Err(Error::Unavailable));
            assert_eq!(store.snapshot().unwrap(), before);
            assert_eq!(
                game_physiology::body(&store, PersonId(2))
                    .unwrap()
                    .vitals
                    .health,
                100
            );
            assert_eq!(
                game_conflict::encounter(&store, EncounterId(11))
                    .unwrap()
                    .cursor,
                0
            );
            assert!(game_skills::skills(&store, PersonId(1)).is_none());
        }
    }
}

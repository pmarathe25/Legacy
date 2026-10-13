use super::*;
use std::collections::{BTreeMap, BTreeSet};
fn rules() -> Rules {
    Rules {
        encounter_limit: 128,
        round: SimDuration(6),
        movement: 1000,
        reach: 1000,
        damage: 10,
        defence: 2,
        incapacitation: 10,
    }
}
fn facts() -> ReleaseFacts {
    ReleaseFacts {
        actor: PersonId(1),
        target: PersonId(2),
        body: TargetBody {
            alive: true,
            health: 100,
        },
        independent: true,
        available_area: true,
        actor_captive: false,
        contact: true,
        moving: false,
    }
}
fn captive() -> Captivity {
    Captivity {
        actor: PersonId(2),
        captor: PersonId(3),
        area: AreaId(1),
        position: Point { x: 0, y: 0 },
        map: CombatMapId(1),
        binding: InventoryId(4),
    }
}
#[test]
fn release_requires_real_contact_body_and_one_carried_tool_or_captor_authority() {
    let rules = rules();
    let mut captive = captive();
    let tool = Inventory {
        id: InventoryId(5),
        location: InventoryLocation::Carried(PersonId(1)),
        quantity: Quantity(1000),
        capacity: Quantity(1000),
        access: BTreeSet::new(),
        last_granted: None,
        good: Good::Tool,
        spoils_at: None,
    };
    assert!(rules.release_available(facts(), Some(&captive), [&tool]));
    assert!(!rules.release_available(
        ReleaseFacts {
            contact: false,
            ..facts()
        },
        Some(&captive),
        [&tool]
    ));
    assert!(!rules.release_available(
        ReleaseFacts {
            moving: true,
            ..facts()
        },
        Some(&captive),
        [&tool]
    ));
    let split = Inventory {
        quantity: Quantity(500),
        ..tool.clone()
    };
    assert!(!rules.release_available(facts(), Some(&captive), [&split, &split]));
    captive.captor = PersonId(1);
    assert!(rules.release_available(facts(), Some(&captive), []));
    assert!(!rules.release_available(
        ReleaseFacts {
            body: TargetBody {
                alive: true,
                health: 10
            },
            ..facts()
        },
        Some(&captive),
        []
    ));
}
#[test]
fn search_patch_rejects_moved_or_changed_fighter_without_replacing_other_state() {
    let fighter = Fighter {
        actor: PersonId(1),
        joins_round: 0,
        team: 0,
        position: Point { x: 1000, y: 1000 },
        surrendered: false,
        withdrawn: false,
        defending: false,
        retreat: None,
        known: BTreeMap::new(),
        search: SearchState::default(),
    };
    let encounter = Encounter {
        id: EncounterId(1),
        area: AreaId(1),
        location: Location::AtArea(AreaId(1)),
        map: CombatMapId(1),
        round: 0,
        started: SimTime(0),
        ends: SimTime(6),
        fighters: BTreeMap::from([(fighter.actor, fighter.clone())]),
        order: vec![fighter.actor],
        cursor: 0,
        active: true,
    };
    let _world = World { rules: rules() };
    let mut store = crate::test_store();
    let rows: BTreeMap<AreaId, Map> = BTreeMap::from([(
        AreaId(1),
        Map {
            id: CombatMapId(1),
            version: 1,
            area: AreaId(1),
            location: Location::AtArea(AreaId(1)),
            width: 20_000,
            height: 20_000,
            obstacles: Vec::new(),
            exits: Vec::new(),
        },
    )]);
    for (_, row) in rows {
        crate::insert_map(&mut store, row).unwrap();
    }
    let rows: BTreeMap<EncounterId, Encounter> = BTreeMap::from([(encounter.id, encounter)]);
    for (_, row) in rows {
        crate::insert_encounter(&mut store, row).unwrap();
    }

    let patch = PreparedSearch::new(
        EncounterId(1),
        &fighter,
        SearchState {
            next_waypoint: 1,
            goal: None,
        },
    );
    crate::encounter_mut(&mut store, EncounterId(1))
        .unwrap()
        .fighters
        .get_mut(&fighter.actor)
        .unwrap()
        .position
        .x += 1;
    let before = store.clone();
    assert!(patch.commit(&mut store).is_err());
    assert_eq!(store, before);
}

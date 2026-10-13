use super::*;
use game_ecs::{Batch, Schema, StableId, Store};
use game_world::{GenericHost, Transaction};
use std::collections::BTreeSet;

fn world() -> World {
    World {
        rules: Rules {
            encounter_limit: 128,
            round: SimDuration(6_000),
            movement: 5_000,
            reach: 2_000,
            damage: 45,
            defence: 30,
            incapacitation: 20,
        },
    }
}
fn participants() -> [ParticipantFacts; 2] {
    [
        ParticipantFacts {
            actor: PersonId(1),
            team: 0,
            body: TargetBody {
                alive: true,
                health: 100,
            },
            independent: true,
            force_can_act: false,
            force_passive: false,
            in_combat: false,
        },
        ParticipantFacts {
            actor: PersonId(2),
            team: 1,
            body: TargetBody {
                alive: true,
                health: 100,
            },
            independent: true,
            force_can_act: false,
            force_passive: false,
            in_combat: false,
        },
    ]
}
fn seed(store: &mut Store) {
    store.spawn(StableId(3)).unwrap();
    for actor in [PersonId(1), PersonId(2)] {
        insert_profile(store, actor, Profile { initiative: 100 }).unwrap();
        game_spatial::set_location(store, StableId(actor.0), Location::AtArea(AreaId(3))).unwrap();
    }
}
#[test]
fn newborn_profile_and_skills_share_reserved_identity_and_rejected_birth_is_atomic() {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_skills::register_components(&mut schema).unwrap();
    schema
        .register::<Activity>("fixture/activity", |row| {
            if row.0 == 0 {
                Ok(())
            } else {
                Err(Error::Unavailable)
            }
        })
        .unwrap();
    let mut host: GenericHost<(), u8> =
        GenericHost::with_storage(Store::with_schema(schema), 7, ());
    let runtime = game_skills::Runtime::default();
    let child = PersonId(host.next_id());
    let profile_plan = prepare_newborn_profile(&host.storage, child).unwrap();
    let skills = runtime.prepare_newborn(&host.storage, child).unwrap();
    let before = host.clone();
    let mut batch = Batch::new();
    batch.insert(StableId(child.0), Activity(1));
    let profile_guard = profile_plan
        .clone()
        .stage_into(&host.storage, &mut batch)
        .unwrap();
    let skills_guard = skills
        .clone()
        .stage_into(&runtime, &host.storage, &mut batch)
        .unwrap()
        .into_guard(&runtime, &host.storage, &batch)
        .unwrap();
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.ids = 1;
    transaction.events = vec![1];
    transaction.revisions = 1;
    transaction.guards = vec![profile_guard, skills_guard];
    assert!(host.commit_transaction(transaction).is_err());
    assert_eq!(host, before);
    let mut batch = Batch::new();
    batch.insert(StableId(child.0), Activity(0));
    let profile_guard = profile_plan.stage_into(&host.storage, &mut batch).unwrap();
    let skills_guard = skills
        .stage_into(&runtime, &host.storage, &mut batch)
        .unwrap()
        .into_guard(&runtime, &host.storage, &batch)
        .unwrap();
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.ids = 1;
    transaction.events = vec![1];
    transaction.revisions = 1;
    transaction.guards = vec![profile_guard, skills_guard];
    host.commit_transaction(transaction).unwrap();
    assert_eq!(profile(&host.storage, child).unwrap().initiative, 100);
    assert_eq!(
        game_skills::skills(&host.storage, child),
        Some(&game_skills::Skills::default())
    );
}
#[test]
fn projected_transport_stop_and_encounter_start_share_exact_owner_guards() {
    let world = world();
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_spatial::register_components(&mut schema).unwrap();
    game_transport::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    seed(&mut store);
    store.spawn(StableId(4)).unwrap();
    let travelling = Location::InTransit {
        from: AreaId(3),
        to: AreaId(4),
        departed: SimTime(0),
        road_start: None,
        arrives: SimTime(100),
    };
    game_spatial::set_location(&mut store, StableId(1), travelling.clone()).unwrap();
    game_spatial::set_location(
        &mut store,
        StableId(2),
        Location::StoppedOnRoad {
            from: AreaId(3),
            to: AreaId(4),
            progress: SimDuration(30),
            duration: SimDuration(100),
        },
    )
    .unwrap();
    assert!(
        world
            .prepare_opening(&store, &participants(), SimTime(30))
            .is_err()
    );
    let transport = game_transport::Runtime::new(game_transport::Rules::default()).unwrap();
    let stop = transport
        .prepare_stop(&store, PersonId(1), SimTime(30))
        .unwrap();
    let opening = world
        .prepare_opening_after_motion(&store, &participants(), SimTime(30), &stop)
        .unwrap();
    let scene = world
        .prepare_map(
            &store,
            opening.location(),
            SimTime(30),
            7,
            CombatMapId(10),
            &[],
            true,
        )
        .unwrap();
    let start = world
        .prepare_start(
            &store,
            EncounterId(11),
            opening,
            scene.map(),
            &[(PersonId(1), 0), (PersonId(2), 1)],
            &[(PersonId(1), 0), (PersonId(2), 0)],
            SimTime(30),
        )
        .unwrap();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let bodies = BTreeMap::<PersonId, game_transport::Body>::new();
    let motion = stop
        .stage_into(&transport, &store, &bodies, SimTime(30), &mut batch)
        .unwrap();
    let scene = scene.stage_into(&store, &mut batch).unwrap();
    let start = start
        .stage_after_motion(&world, &store, &mut batch, &motion)
        .unwrap();
    motion.validate_footprint(&store, &batch).unwrap();
    scene.validate(&store, &batch).unwrap();
    start.validate(&store, &batch).unwrap();
    assert_eq!(before, store.snapshot().unwrap());
    batch.insert(StableId(1), Location::AtArea(AreaId(3)));
    assert!(motion.validate_footprint(&store, &batch).is_err());
    assert!(start.validate(&store, &batch).is_err());
    assert_eq!(
        game_spatial::location(&store, StableId(1)),
        Some(&travelling)
    );
}
fn arena() -> (World, Store, EncounterId) {
    arena_with_transport(false)
}
fn arena_with_transport(with_transport: bool) -> (World, Store, EncounterId) {
    let world = world();
    let mut schema = crate::test_store().schema().clone();
    if with_transport {
        game_transport::register_components(&mut schema).unwrap();
    }
    let mut store = Store::with_schema(schema);
    seed(&mut store);
    let facts = participants();
    let opening = world.prepare_opening(&store, &facts, SimTime(0)).unwrap();
    let scene = world
        .prepare_map(
            &store,
            opening.location(),
            SimTime(0),
            7,
            CombatMapId(10),
            &[],
            true,
        )
        .unwrap();
    let start = world
        .prepare_start(
            &store,
            EncounterId(11),
            opening,
            scene.map(),
            &[(PersonId(1), 0), (PersonId(2), 1)],
            &[(PersonId(1), 0), (PersonId(2), 0)],
            SimTime(0),
        )
        .unwrap();
    let mut batch = Batch::new();
    let map = scene.stage_into(&store, &mut batch).unwrap();
    let start = start.stage_into(&world, &store, &mut batch).unwrap();
    map.validate(&store, &batch).unwrap();
    start.validate(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    (world, store, EncounterId(11))
}
#[test]
fn setup_admission_perception_and_round_progression_are_owner_entrypoints() {
    let (world, mut store, id) = arena();
    assert_eq!(actor_encounter(&store, PersonId(1)).unwrap().id, id);
    world
        .prepare_observation(
            &store,
            id,
            PersonId(1),
            &[PersonId(1), PersonId(2)].into_iter().collect(),
        )
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    assert_eq!(
        encounter(&store, id).unwrap().fighters[&PersonId(1)].known[&PersonId(2)],
        Point { x: 6_000, y: 8_000 }
    );
    insert_profile(&mut store, PersonId(4), Profile { initiative: 500 }).unwrap();
    game_spatial::set_location(&mut store, StableId(4), Location::AtArea(AreaId(3))).unwrap();
    let facts = AdmissionFacts {
        actor: PersonId(4),
        team: 0,
        body: TargetBody {
            alive: true,
            health: 100,
        },
        in_combat: false,
        same_place: true,
        arrived: true,
        departed: true,
        from: AreaId(8),
    };
    let (position, joins) = world.mission_admission(&store, id, facts).unwrap();
    world
        .prepare_admission(&store, id, PersonId(4), 0, position, joins)
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    assert!(!encounter(&store, id).unwrap().order.contains(&PersonId(4)));
    let facts: Vec<_> = encounter(&store, id)
        .unwrap()
        .fighters
        .keys()
        .map(|actor| RoundFighterFacts {
            actor: *actor,
            alive: true,
            force_can_act: false,
            carried: false,
        })
        .collect();
    let outcome = world
        .prepare_finish_round(&store, id, SimTime(6_000), &facts)
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    assert!(!outcome.ended);
    world
        .prepare_begin_round(
            &store,
            id,
            &[(PersonId(1), 0), (PersonId(2), 0), (PersonId(4), 0)],
        )
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    assert_eq!(encounter(&store, id).unwrap().order[0], PersonId(4));
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Activity(u8);
fn schema_rejecting_encounter() -> Schema {
    let mut schema = Schema::new();
    schema
        .register::<Profile>("conflict/profile", |_| Ok(()))
        .unwrap();
    schema
        .register::<Map>("conflict/map", Map::validate)
        .unwrap();
    schema
        .register::<MapRef>("conflict/map_ref", |_| Ok(()))
        .unwrap();
    schema
        .register::<RoadMapRefs>("conflict/road_maps", |_| Ok(()))
        .unwrap();
    schema
        .register::<Participation>("conflict/participation", |_| Ok(()))
        .unwrap();
    schema
        .register::<Captivity>("conflict/captivity", |_| Ok(()))
        .unwrap();
    schema
        .register::<Encounter>("conflict/encounter", |_| Err(Error::Unavailable))
        .unwrap();
    schema
        .register::<Activity>("fixture/activity", |_| Ok(()))
        .unwrap();
    game_spatial::register_components(&mut schema).unwrap();
    schema
}
#[test]
fn rejected_composed_admission_preserves_ids_events_activities_and_locations() {
    let world = world();
    let mut store = Store::with_schema(schema_rejecting_encounter());
    seed(&mut store);
    store.insert(StableId(1), Activity(0)).unwrap();
    store.insert(StableId(2), Activity(0)).unwrap();
    let mut host: GenericHost<(), u8> = GenericHost::with_storage(store, 7, ());
    host.allocate_ids(4).unwrap();
    let before = host.clone();
    let opening_token = host.transaction_opening();
    let opening = world
        .prepare_opening(&host.storage, &participants(), SimTime(0))
        .unwrap();
    let scene = world
        .prepare_map(
            &host.storage,
            opening.location(),
            SimTime(0),
            7,
            CombatMapId(host.next_id()),
            &[],
            true,
        )
        .unwrap();
    let id = EncounterId(host.next_id() + 1);
    let start = world
        .prepare_start(
            &host.storage,
            id,
            opening,
            scene.map(),
            &[(PersonId(1), 0), (PersonId(2), 1)],
            &[(PersonId(1), 0), (PersonId(2), 0)],
            SimTime(0),
        )
        .unwrap();
    let mut batch = Batch::new();
    let scene_guard = scene.stage_into(&host.storage, &mut batch).unwrap();
    let start_guard = start.stage_into(&world, &host.storage, &mut batch).unwrap();
    batch.insert(StableId(1), Activity(1));
    batch.insert(StableId(2), Activity(1));
    let mut transaction = Transaction::new(opening_token, batch);
    transaction.ids = 2;
    transaction.events = vec![1];
    transaction.revisions = 1;
    transaction.guards = vec![scene_guard, start_guard];
    assert_eq!(
        host.commit_transaction(transaction),
        Err(Error::Unavailable)
    );
    assert_eq!(host, before);
}
#[test]
fn scene_keys_roundtrip_reversed_road_geometry_and_reject_stale_map_sources() {
    let world = world();
    let mut store = crate::test_store();
    let road = Location::StoppedOnRoad {
        from: AreaId(3),
        to: AreaId(4),
        progress: SimDuration(30),
        duration: SimDuration(100),
    };
    let reverse = Location::StoppedOnRoad {
        from: AreaId(4),
        to: AreaId(3),
        progress: SimDuration(70),
        duration: SimDuration(100),
    };
    let scene = world
        .prepare_map(&store, &road, SimTime(0), 7, CombatMapId(10), &[], true)
        .unwrap();
    scene.commit(&mut store).unwrap();
    assert_eq!(
        map_for_location(&store, &reverse, SimTime(0))
            .unwrap()
            .unwrap()
            .id,
        CombatMapId(10)
    );
    let mut changed = map(&store, CombatMapId(10)).unwrap().clone();
    changed.obstacles[0].integrity = 1;
    let plan = world
        .prepare_map(&store, &road, SimTime(0), 7, CombatMapId(11), &[], true)
        .unwrap();
    insert_map(&mut store, changed).unwrap();
    let before = store.clone();
    assert!(plan.commit(&mut store).is_err());
    assert_eq!(store, before);
    let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    assert_eq!(
        map_for_location(&restored, &reverse, SimTime(0))
            .unwrap()
            .unwrap()
            .id,
        CombatMapId(10)
    );
}
#[test]
fn malformed_old_runtime_maps_are_rejected() {
    let mut value = serde_json::to_value(world()).unwrap();
    value["encounters"] = serde_json::json!({});
    assert!(serde_json::from_value::<World>(value).is_err());
}

fn turn(actor: PersonId) -> TurnFacts {
    TurnFacts {
        actor,
        independent: true,
        body: TargetBody {
            alive: true,
            health: 100,
        },
        force_can_act: false,
        force_allowed: true,
        target_body: Some(TargetBody {
            alive: true,
            health: 100,
        }),
        binding_available: false,
        release_available: false,
        mission_order_valid: false,
        retreat_route_valid: false,
        damage: 45,
        practice_ms: 6_000,
    }
}
#[test]
fn lethal_strike_accepts_exact_native_death_location_cleanup_and_rejects_ordinary_stop() {
    let (world, mut store, id) = arena_with_transport(true);
    for actor in [PersonId(1), PersonId(2)] {
        game_physiology::insert_body(
            &mut store,
            actor,
            game_physiology::Body {
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
    }
    let transport = game_transport::Runtime::new(game_transport::Rules::default()).unwrap();
    let bodies = BTreeMap::<PersonId, game_transport::Body>::new();
    for tagged in [false, true] {
        let mut facts = turn(PersonId(1));
        facts.damage = 100;
        let strike = world
            .prepare_turn(&store, id, None, Action::Attack(PersonId(2)), facts)
            .unwrap();
        let injury = game_physiology::prepare_injury(&store, PersonId(2), 100, false).unwrap();
        assert!(injury.lethal());
        let motion = if tagged {
            transport.prepare_death_motion(&store, PersonId(2), SimTime(0))
        } else {
            transport.prepare_stop(&store, PersonId(2), SimTime(0))
        }
        .unwrap();
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let injury = injury.stage_into(&store, &mut batch).unwrap();
        let motion = motion
            .stage_into(&transport, &store, &bodies, SimTime(0), &mut batch)
            .unwrap();
        let staged = strike.stage_with_death(
            &world,
            &store,
            &mut batch,
            Some(&injury),
            None,
            None,
            None,
            None,
            Some(&motion),
        );
        if tagged {
            let staged = staged.unwrap();
            staged.validate(&world, &store, &batch).unwrap();
            injury.validate(&store, &batch).unwrap();
            motion.validate_footprint(&store, &batch).unwrap();
            store.apply_batch(batch).unwrap();
            assert!(
                !game_physiology::body(&store, PersonId(2))
                    .unwrap()
                    .vitals
                    .alive
            );
            assert_eq!(encounter(&store, id).unwrap().cursor, 1);
        } else {
            assert!(staged.is_err());
            assert_eq!(before, store.snapshot().unwrap());
        }
    }
}
#[test]
fn capture_receiver_rejects_a_valid_binding_for_a_different_tactical_position() {
    let (world, mut store, id) = arena();
    encounter_mut(&mut store, id)
        .unwrap()
        .fighters
        .get_mut(&PersonId(2))
        .unwrap()
        .surrendered = true;
    let source = game_cargo::Inventory {
        id: InventoryId(50),
        location: game_cargo::InventoryLocation::Carried(PersonId(1)),
        quantity: Quantity(2000),
        capacity: Quantity(2000),
        access: BTreeSet::new(),
        last_granted: None,
        good: Good::Binding,
        spoils_at: None,
    };
    game_cargo::insert_inventory(&mut store, source.clone()).unwrap();
    let mut facts = turn(PersonId(1));
    facts.binding_available = true;
    let capture = world
        .prepare_turn(&store, id, None, Action::Capture(PersonId(2)), facts)
        .unwrap();
    let mut context = capture
        .effects()
        .iter()
        .find_map(|effect| match effect {
            Effect::Capture { context, .. } => Some(context.clone()),
            _ => None,
        })
        .unwrap();
    context.target_position.x -= 100;
    let admission = ConfinementFacts {
        captor: PersonId(1),
        target: PersonId(2),
        captor_alive: true,
        captor_health: 100,
        independent: true,
        captor_confined: false,
        target_alive: true,
        target_health: 100,
        target_confined: false,
        surrendered_to_captor: true,
        contact: true,
        admitted_site_contact: true,
        captor_location: Location::AtArea(AreaId(3)),
        target_location: Location::AtArea(AreaId(3)),
    };
    let confinement = prepare_confinement(
        &world.rules,
        admission.clone(),
        &source,
        InventoryId(60),
        context,
        ConfinementMode::Tactical { encounter: id },
        SimTime(0),
    )
    .unwrap()
    .begin(&store, &admission)
    .unwrap();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let receipt = confinement.stage_into(&store, &mut batch).unwrap();
    receipt.validate(&store, &batch).unwrap();
    assert!(
        capture
            .stage_with_receivers(&world, &store, &mut batch, None, Some(&receipt))
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(captivity(&store, PersonId(2)).is_none());
}
#[test]
fn source_skill_changes_reject_while_exact_receiving_strike_credit_composes() {
    let (world, mut store, id) = arena();
    let runtime = game_skills::Runtime::default();
    let prepared = world
        .prepare_turn(
            &store,
            id,
            None,
            Action::Attack(PersonId(2)),
            turn(PersonId(1)),
        )
        .unwrap();
    runtime
        .credit(&mut store, PersonId(1), Skill::Combat, 60_000, None)
        .unwrap();
    let before = store.clone();
    assert!(prepared.commit(&world, &mut store).is_err());
    assert_eq!(before, store);
    let prepared = world
        .prepare_turn(
            &store,
            id,
            None,
            Action::Attack(PersonId(2)),
            turn(PersonId(1)),
        )
        .unwrap();
    let delta = prepared
        .effects()
        .iter()
        .find_map(|effect| {
            if let Effect::Struck { practice, .. } = effect {
                Some(practice.clone())
            } else {
                None
            }
        })
        .unwrap();
    let mut batch = Batch::new();
    let practice = runtime
        .prepare_practice(&store, [delta])
        .unwrap()
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    let combat = prepared
        .stage_with_practice(&world, &store, &mut batch, None, None, Some(&practice))
        .unwrap();
    practice.validate(&runtime, &store, &batch).unwrap();
    combat.validate(&world, &store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(runtime.practice(&store, PersonId(1), Skill::Combat), 66_000);
    assert_eq!(encounter(&store, id).unwrap().cursor, 1);
}
#[test]
fn rejected_turn_and_external_factual_changes_preserve_search_and_encounter() {
    let (world, mut store, id) = arena();
    insert_profile(&mut store, PersonId(4), Profile { initiative: 100 }).unwrap();
    let mut facts = game_ecs::ReadSet::new();
    facts.read::<Profile>(&store, StableId(4));
    let prepared = world
        .prepare_turn(&store, id, None, Action::Defend, turn(PersonId(1)))
        .unwrap()
        .with_factual_reads(facts)
        .with_search(
            &store,
            PersonId(1),
            SearchState {
                next_waypoint: 1,
                goal: None,
            },
        )
        .unwrap();
    profile_mut(&mut store, PersonId(4)).unwrap().initiative = 101;
    let before = store.clone();
    assert!(prepared.commit(&world, &mut store).is_err());
    assert_eq!(before, store);
    assert_eq!(
        encounter(&store, id).unwrap().fighters[&PersonId(1)].search,
        SearchState::default()
    );
}

struct Driver {
    world: World,
    store: Store,
    reject_default: bool,
    scheduled: Option<SimTime>,
    proposed_search: bool,
}
impl CombatDrivePort for Driver {
    type Action = Action;
    fn store(&self) -> &Store {
        &self.store
    }
    fn ruler(&self) -> PersonId {
        PersonId(2)
    }
    fn controlled(&self, _: PersonId) -> bool {
        false
    }
    fn can_act(&self, actor: PersonId) -> bool {
        actor == PersonId(1)
    }
    fn observe(&mut self, id: EncounterId, actor: PersonId) -> Result<(), Error> {
        self.world
            .prepare_observation(&self.store, id, actor, &[PersonId(1)].into_iter().collect())?
            .commit(&self.world, &mut self.store)?;
        Ok(())
    }
    fn review_mission(&mut self, _: EncounterId, _: PersonId) {}
    fn mission_action(&mut self, _: EncounterId, _: PersonId) -> Option<(Option<Point>, Action)> {
        None
    }
    fn force_action(&self, _: EncounterId, _: PersonId) -> Option<Action> {
        None
    }
    fn autonomy_facts(
        &self,
        id: EncounterId,
        actor: PersonId,
    ) -> Result<CombatAutonomyFacts, Error> {
        let fighter = &encounter(&self.store, id).unwrap().fighters[&actor];
        Ok(CombatAutonomyFacts {
            actor,
            team: fighter.team,
            position: fighter.position,
            health: 100,
            max_health: 100,
            risk_tolerance: 500,
            guardian: false,
            ruler: PersonId(2),
            rescue_target: None,
            can_release: false,
            binding_available: false,
            targets: Vec::new(),
            search: fighter.search.clone(),
        })
    }
    fn bind_action(&self, action: CombatAutonomyAction) -> Action {
        match action {
            CombatAutonomyAction::Defend => Action::Defend,
            CombatAutonomyAction::Surrender => Action::Surrender,
            CombatAutonomyAction::Capture(target) => Action::Capture(target),
            CombatAutonomyAction::Attack(target) => Action::Attack(target),
            CombatAutonomyAction::Release(target) => Action::Release(target),
        }
    }
    fn wait_action(&self) -> Action {
        Action::Wait
    }
    fn apply_turn(
        &mut self,
        id: EncounterId,
        actor: PersonId,
        movement: Option<Point>,
        action: Action,
        search: Option<SearchState>,
    ) -> Result<(), Error> {
        if self.reject_default {
            self.reject_default = false;
            self.proposed_search = search.is_some_and(|state| state != SearchState::default());
            return Err(Error::Unavailable);
        }
        let mut prepared =
            self.world
                .prepare_turn(&self.store, id, movement, action, turn(actor))?;
        if let Some(search) = search {
            prepared = prepared.with_search(&self.store, actor, search)?;
        }
        prepared.commit(&self.world, &mut self.store)?;
        Ok(())
    }
    fn skip(&mut self, id: EncounterId) -> Result<(), Error> {
        self.world.skip_turn(&mut self.store, id)
    }
    fn schedule(&mut self, _: EncounterId, ends: SimTime) {
        self.scheduled = Some(ends);
    }
}
#[test]
fn production_npc_drive_fallback_drops_rejected_search_and_skips_incapable_fighters() {
    let (world, store, id) = arena();
    let mut port = Driver {
        world: world.clone(),
        store,
        reject_default: true,
        scheduled: None,
        proposed_search: false,
    };
    world.drive_npcs(id, &mut port).unwrap();
    assert!(port.proposed_search);
    let e = encounter(&port.store, id).unwrap();
    assert_eq!(e.cursor, e.order.len());
    assert_eq!(e.fighters[&PersonId(1)].search, SearchState::default());
    assert_eq!(port.scheduled, Some(SimTime(6_000)));
}

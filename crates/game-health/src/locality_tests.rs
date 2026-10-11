use super::*;

fn runtime() -> Runtime {
    Runtime {
        rules: Rules::fixture(),
        last_review: SimTime(0),
    }
}
fn store() -> Store {
    crate::component_store()
}
fn actor(id: u64, place: &str, area: Option<u64>) -> LocalHealthActor<String> {
    LocalHealthActor {
        actor: PersonId(id),
        place: Some(place.into()),
        physical_area: area.map(AreaId),
        carrier: None,
        tactical: None,
        alive: true,
        moving: false,
        activity: HealthActivityFacts::default(),
        health: 1_000,
        max_health: 1_000,
        temperature_c: Some(20),
    }
}
fn seed_actors(store: &mut Store, actors: &[LocalHealthActor<String>]) {
    for actor in actors {
        store.ensure(StableId(actor.actor.0)).unwrap();
    }
}
fn seed_shelter(store: &mut Store, id: u64, area: u64, access: BTreeSet<PersonId>) {
    insert_shelter(
        store,
        Shelter {
            id: ShelterId(id),
            area: AreaId(area),
            custodian: PersonId(1),
            access,
            capacity: 64,
            insulation: 800,
            built_at: SimTime(0),
            timber_used: Quantity(1_000),
            labor_ms: 1_000,
        },
    )
    .unwrap();
}
fn infect(store: &mut Store, id: PersonId, kind: IllnessKind) {
    let mut state = clinical(store, id).cloned().unwrap_or_default();
    state.illnesses.insert(
        kind,
        Infection {
            kind,
            acquired: SimTime(0),
            onset: SimTime(10),
            recovers: SimTime(1_000),
            symptomatic: true,
        },
    );
    insert_clinical(store, id, state).unwrap();
}
// Independent scalar oracles intentionally use repeated pair/chain inspection,
// rather than the production cohort aggregation and memoized carrier paths.
fn scalar_shelter(
    store: &Store,
    actors: &[LocalHealthActor<String>],
    id: PersonId,
) -> Option<ShelterId> {
    let actor = actors.iter().find(|actor| actor.actor == id).unwrap();
    if !actor.alive {
        return None;
    }
    if let Some(carrier) = actor.carrier {
        return scalar_shelter(store, actors, carrier);
    }
    let row = shelter(store, occupancy(store, id)?.shelter)?;
    let active = actor.activity.combat
        || actor.activity.cultivation
        || actor.activity.gathering
        || actor.activity.construction
        || actor.activity.play;
    (!active && actor.physical_area == Some(row.area)).then_some(row.id)
}
fn scalar_tactical(actors: &[LocalHealthActor<String>], id: PersonId) -> Option<TacticalIdentity> {
    let actor = actors.iter().find(|actor| actor.actor == id).unwrap();
    actor
        .carrier
        .map_or(actor.tactical, |parent| scalar_tactical(actors, parent))
}
fn scalar_clear(
    a: Option<TacticalIdentity>,
    b: Option<TacticalIdentity>,
    geometry: &[HealthGeometry],
    contact: bool,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) if a == b => true,
        (Some(a), Some(b)) if a.encounter == b.encounter => geometry
            .iter()
            .find(|hint| {
                (hint.first == a && hint.second == b) || (hint.first == b && hint.second == a)
            })
            .is_some_and(|hint| {
                if contact {
                    hint.contact_clear
                } else {
                    hint.sight_clear
                }
            }),
        _ => false,
    }
}
fn scalar_exposures(
    store: &Store,
    actors: &[LocalHealthActor<String>],
    interactions: &[HealthInteraction],
    geometry: &[HealthGeometry],
    at: SimTime,
) -> BTreeMap<PersonId, ExposureInput> {
    actors.iter().map(|receiver| {
        let inside=scalar_shelter(store,actors,receiver.actor);
        let mut counts=[0;2];
        for source in actors {
            if source.actor==receiver.actor || receiver.place.is_none() || receiver.place!=source.place {continue;}
            let other_inside=scalar_shelter(store,actors,source.actor);
            let same_shelter=inside.is_some()&&inside==other_inside;
            let pair=ordered(receiver.actor,source.actor);
            let carrying=receiver.carrier==Some(source.actor)||source.carrier==Some(receiver.actor);
            let treatment=jobs(store).any(|(_,job)|matches!(job.work,Work::Treat {patient,..} if ordered(job.actor,patient)==pair));
            let interaction=interactions.iter().any(|hint|ordered(hint.first,hint.second)==pair);
            let contact=scalar_clear(scalar_tactical(actors,receiver.actor),scalar_tactical(actors,source.actor),geometry,true)
                && (carrying||treatment||(inside==other_inside&&interaction));
            if same_shelter||contact {
                for (count,kind) in counts.iter_mut().zip([IllnessKind::Enteric,IllnessKind::Respiratory]) {
                    *count+=u64::from(clinical(store,source.actor).and_then(|state|state.illnesses.get(&kind))
                        .is_some_and(|state|state.symptomatic&&state.recovers>at));
                }
            }
        }
        (receiver.actor,ExposureInput {temperature_c:receiver.temperature_c,
            insulation:inside.and_then(|id|shelter(store,id)).map_or(0,|row|row.insulation),infectious_contacts:counts})
    }).collect()
}
fn scalar_observations(
    runtime: &Runtime,
    store: &Store,
    actors: &[LocalHealthActor<String>],
    geometry: &[HealthGeometry],
    at: SimTime,
) -> BTreeMap<PersonId, Knowledge> {
    let mut output: BTreeMap<_, _> = knowledges(store)
        .map(|(id, row)| (id, row.clone()))
        .collect();
    for observer in actors
        .iter()
        .filter(|actor| actor.alive && !actor.moving && actor.place.is_some())
    {
        let effective = scalar_shelter(store, actors, observer.actor);
        let tactical = scalar_tactical(actors, observer.actor);
        let visible = |subject: &LocalHealthActor<String>| {
            observer.place == subject.place
                && effective == scalar_shelter(store, actors, subject.actor)
                && scalar_clear(
                    tactical,
                    scalar_tactical(actors, subject.actor),
                    geometry,
                    false,
                )
        };
        let peers: BTreeMap<_, _> = actors
            .iter()
            .filter(|subject| visible(subject))
            .map(|subject| (subject.actor, subject))
            .collect();
        let record = output.entry(observer.actor).or_default();
        for (id, subject) in peers.into_iter().take(64) {
            record
                .people
                .insert(id, symptoms(store, &runtime.rules, subject, at));
        }
        if let Some(area) = observer.physical_area.filter(|_| tactical.is_none()) {
            for (id, row) in shelters(store).filter(|(_, row)| row.area == area) {
                let known = actors
                    .iter()
                    .filter(|subject| visible(subject))
                    .filter(|subject| {
                        if subject.carrier.is_some() {
                            scalar_shelter(store, actors, subject.actor) == Some(id)
                        } else {
                            occupancy(store, subject.actor).is_some_and(|place| place.shelter == id)
                        }
                    })
                    .map(|actor| actor.actor)
                    .collect();
                record.shelters.insert(
                    id,
                    ShelterView {
                        id,
                        area: row.area,
                        capacity: row.capacity,
                        insulation: row.insulation,
                        custodian: row.custodian,
                        accessible: row.access.contains(&observer.actor),
                        known_occupants: known,
                        observed_at: at,
                    },
                );
            }
        }
    }
    output
}
#[test]
fn indexed_exposures_match_exhaustive_actual_contacts_with_duplicates_and_local_weather() {
    let runtime = runtime();
    for phase in 0..12 {
        let mut store = store();
        let mut actors: Vec<_> = (1..=18)
            .map(|id| {
                actor(
                    id,
                    if id % 3 == 0 { "wood" } else { "village" },
                    Some(if id % 3 == 0 { 2 } else { 1 }),
                )
            })
            .collect();
        seed_actors(&mut store, &actors);
        seed_shelter(&mut store, 50, 1, BTreeSet::from([PersonId(1)]));
        seed_shelter(&mut store, 51, 2, BTreeSet::from([PersonId(3)]));
        for (i, facts) in actors.iter_mut().enumerate() {
            if (i + phase) % 4 == 0 {
                insert_occupancy(
                    &mut store,
                    facts.actor,
                    Occupancy {
                        shelter: if facts.physical_area == Some(AreaId(1)) {
                            ShelterId(50)
                        } else {
                            ShelterId(51)
                        },
                    },
                )
                .unwrap();
            }
            facts.activity.gathering = (i + phase) % 7 == 0;
            facts.temperature_c = if (i + phase) % 5 == 0 {
                None
            } else {
                Some((i as i32) % 30 - 10)
            };
            if (i + phase) % 3 == 0 {
                infect(&mut store, facts.actor, IllnessKind::Enteric);
            }
            if (i + phase) % 4 == 0 {
                infect(&mut store, facts.actor, IllnessKind::Respiratory);
            }
        }
        actors[1].carrier = Some(PersonId(1));
        actors[1].place = actors[0].place.clone();
        actors[1].physical_area = actors[0].physical_area;
        insert_job(
            &mut store,
            Job {
                actor: PersonId(1),
                area: AreaId(1),
                completes: SimTime(500),
                work: Work::Treat {
                    patient: PersonId(2),
                    skill_ms: 0,
                    elapsed: 0,
                },
            },
        )
        .unwrap();
        let hints: Vec<_> = (1..18)
            .flat_map(|id| {
                [
                    HealthInteraction {
                        first: PersonId(id),
                        second: PersonId(id + 1),
                    },
                    HealthInteraction {
                        first: PersonId(id + 1),
                        second: PersonId(id),
                    },
                ]
            })
            .collect();
        let before = store.snapshot().unwrap();
        assert_eq!(
            runtime
                .local_exposures(&store, SimTime(100), &actors, &hints, &[])
                .unwrap(),
            scalar_exposures(&store, &actors, &hints, &[], SimTime(100))
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
}
#[test]
fn indexed_observations_preserve_the_first64_and_existing_knowledge_under_reordering() {
    let runtime = runtime();
    let mut store = store();
    let actors: Vec<_> = (1..=90).map(|id| actor(id, "village", Some(1))).collect();
    seed_actors(&mut store, &actors);
    seed_shelter(&mut store, 200, 1, BTreeSet::from([PersonId(1)]));
    for id in 1..=12 {
        insert_occupancy(
            &mut store,
            PersonId(id),
            Occupancy {
                shelter: ShelterId(200),
            },
        )
        .unwrap();
    }
    infect(&mut store, PersonId(30), IllnessKind::Enteric);
    let expected = scalar_observations(&runtime, &store, &actors, &[], SimTime(100));
    let before = store.snapshot().unwrap();
    let first = runtime
        .prepare_local_observations(&store, SimTime(100), &actors, &[])
        .unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    let mut reversed = actors.clone();
    reversed.reverse();
    let second = runtime
        .prepare_local_observations(&store, SimTime(100), &reversed, &[])
        .unwrap();
    let mut other = store.clone();
    first.commit(&runtime, &mut store).unwrap();
    second.commit(&runtime, &mut other).unwrap();
    assert_eq!(store.snapshot().unwrap(), other.snapshot().unwrap());
    assert_eq!(
        knowledges(&store)
            .map(|(id, row)| (id, row.clone()))
            .collect::<BTreeMap<_, _>>(),
        expected
    );
    assert_eq!(
        knowledge(&store, PersonId(13))
            .unwrap()
            .people
            .keys()
            .next_back(),
        Some(&PersonId(76))
    );
}
#[test]
fn carrying_inherits_shelter_and_tactical_identity_without_permissions_and_road_is_not_area() {
    let runtime = runtime();
    let mut store = store();
    let mut actors = vec![
        actor(1, "road", None),
        actor(2, "road", None),
        actor(3, "road", None),
    ];
    seed_actors(&mut store, &actors);
    seed_shelter(&mut store, 50, 1, BTreeSet::from([PersonId(1)]));
    actors[1].carrier = Some(PersonId(1));
    actors[2].carrier = Some(PersonId(2));
    infect(&mut store, PersonId(3), IllnessKind::Enteric);
    for facts in &mut actors {
        facts.physical_area = Some(AreaId(1));
        facts.place = Some("village".into());
    }
    insert_occupancy(
        &mut store,
        PersonId(1),
        Occupancy {
            shelter: ShelterId(50),
        },
    )
    .unwrap();
    let exposure = runtime
        .local_exposures(&store, SimTime(100), &actors, &[], &[])
        .unwrap();
    assert_eq!(exposure[&PersonId(2)].insulation, 800);
    runtime
        .prepare_local_observations(&store, SimTime(100), &actors, &[])
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert!(!knowledge(&store, PersonId(3)).unwrap().shelters[&ShelterId(50)].accessible);
    assert!(occupancy(&store, PersonId(3)).is_none());
    for facts in &mut actors {
        facts.physical_area = None;
        facts.place = Some("road".into());
    }
    actors[0].tactical = Some(TacticalIdentity {
        encounter: EncounterId(10),
        anchor: PersonId(1),
    });
    actors[0].activity.combat = true;
    let expected = scalar_observations(&runtime, &store, &actors, &[], SimTime(100));
    runtime
        .prepare_local_observations(&store, SimTime(100), &actors, &[])
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(
        knowledges(&store)
            .map(|(id, row)| (id, row.clone()))
            .collect::<BTreeMap<_, _>>(),
        expected
    );
    let exposure = runtime
        .local_exposures(&store, SimTime(100), &actors, &[], &[])
        .unwrap();
    assert_eq!(exposure[&PersonId(1)].insulation, 0);
    assert_eq!(exposure[&PersonId(2)].infectious_contacts[0], 1);
}
#[test]
fn tactical_sight_and_reach_are_distinct_and_private_shelters_separate_perception() {
    let runtime = runtime();
    let mut store = store();
    let mut actors = vec![
        actor(1, "village", Some(1)),
        actor(2, "village", Some(1)),
        actor(3, "village", Some(1)),
    ];
    seed_actors(&mut store, &actors);
    for facts in &mut actors[..2] {
        facts.tactical = Some(TacticalIdentity {
            encounter: EncounterId(10),
            anchor: facts.actor,
        });
        facts.activity.combat = true;
    }
    let geometry = vec![HealthGeometry {
        first: actors[0].tactical.unwrap(),
        second: actors[1].tactical.unwrap(),
        contact_clear: false,
        sight_clear: true,
    }];
    infect(&mut store, PersonId(2), IllnessKind::Respiratory);
    let interactions = [HealthInteraction {
        first: PersonId(1),
        second: PersonId(2),
    }];
    let exposure = runtime
        .local_exposures(&store, SimTime(100), &actors, &interactions, &geometry)
        .unwrap();
    assert_eq!(exposure[&PersonId(1)].infectious_contacts, [0, 0]);
    runtime
        .prepare_local_observations(&store, SimTime(100), &actors, &geometry)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert!(knowledge(&store, PersonId(1)).unwrap().people[&PersonId(2)].cough);
    assert!(
        !knowledge(&store, PersonId(1))
            .unwrap()
            .people
            .contains_key(&PersonId(3))
    );
    seed_shelter(&mut store, 50, 1, BTreeSet::from([PersonId(3)]));
    insert_occupancy(
        &mut store,
        PersonId(3),
        Occupancy {
            shelter: ShelterId(50),
        },
    )
    .unwrap();
    let expected = scalar_observations(&runtime, &store, &actors, &geometry, SimTime(101));
    runtime
        .prepare_local_observations(&store, SimTime(101), &actors, &geometry)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(
        knowledges(&store)
            .map(|(id, row)| (id, row.clone()))
            .collect::<BTreeMap<_, _>>(),
        expected
    );
}
#[test]
fn observation_is_guarded_atomic_and_preserves_concurrent_water_knowledge() {
    let runtime = runtime();
    let mut store = store();
    let actors = vec![actor(1, "village", Some(1)), actor(2, "village", Some(1))];
    seed_actors(&mut store, &actors);
    let prepared = runtime
        .prepare_local_observations(&store, SimTime(100), &actors, &[])
        .unwrap();
    insert_knowledge(
        &mut store,
        PersonId(1),
        Knowledge {
            water: BTreeMap::from([(
                InventoryId(20),
                WaterSafetyView {
                    inventory: InventoryId(20),
                    purified: true,
                    suspected_unsafe: false,
                    observed_at: SimTime(99),
                },
            )]),
            ..Knowledge::default()
        },
    )
    .unwrap();
    prepared.commit(&runtime, &mut store).unwrap();
    assert!(knowledge(&store, PersonId(1)).unwrap().water[&InventoryId(20)].purified);
    let prepared = runtime
        .prepare_local_observations(&store, SimTime(101), &actors, &[])
        .unwrap();
    infect(&mut store, PersonId(2), IllnessKind::Enteric);
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let mut schema = game_ecs::Schema::new();
    schema
        .register::<Knowledge>("health/knowledge", |row| {
            if row.shelters.values().any(|shelter| !shelter.accessible) {
                Err(Error::Unavailable)
            } else {
                row.validate()
            }
        })
        .unwrap();
    schema
        .register::<Shelter>("health/shelter", Shelter::validate)
        .unwrap();
    let mut strict = Store::with_schema(schema);
    seed_actors(&mut strict, &actors);
    seed_shelter(&mut strict, 50, 1, BTreeSet::from([PersonId(1)]));
    let prepared = runtime
        .prepare_local_observations(&strict, SimTime(100), &actors, &[])
        .unwrap();
    let before = strict.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut strict).is_err());
    assert_eq!(strict.snapshot().unwrap(), before);
}
#[test]
fn invalid_carrier_geometry_and_duplicate_actor_inputs_never_publish_knowledge() {
    let runtime = runtime();
    let mut store = store();
    let mut actors = vec![actor(1, "village", Some(1)), actor(2, "village", Some(1))];
    seed_actors(&mut store, &actors);
    let before = store.snapshot().unwrap();
    actors[0].carrier = Some(PersonId(2));
    actors[1].carrier = Some(PersonId(1));
    assert!(
        runtime
            .prepare_local_observations(&store, SimTime(100), &actors, &[])
            .is_err()
    );
    actors[1].carrier = Some(PersonId(99));
    assert!(
        runtime
            .local_exposures(&store, SimTime(100), &actors, &[], &[])
            .is_err()
    );
    actors[0].carrier = None;
    actors[1] = actors[0].clone();
    assert!(
        runtime
            .prepare_local_observations(&store, SimTime(100), &actors, &[])
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

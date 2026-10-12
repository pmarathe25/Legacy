use game_core::{AreaId, Error, PersonId, Quantity, ShelterId, SimTime};
use game_ecs::{Batch, Schema, StableId};
use game_health::{HealthActivityFacts, LocalHealthActor, Occupancy, Shelter, ShelterPlacement};
use game_spatial::Location;
use game_transport::{Authority, CarryGroup, PutDownFacts};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn real_putdown_and_owner_health_projection_admit_once_with_all_counters_preserved_on_failure() {
    for failure in 0..4 {
        let mut schema = Schema::new();
        game_transport::register_components(&mut schema).unwrap();
        game_health::register_components(&mut schema).unwrap();
        game_spatial::register_components(&mut schema).unwrap();
        schema
            .register::<String>("test/foreign", |row| {
                if row == "reject" {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let mut host = game_world::GenericHost::<(), String>::new(schema, 11, ());
        host.allocate_ids(30).unwrap();
        let mut transport = game_transport::Runtime::new(game_transport::Rules::default()).unwrap();
        let health = game_health::Runtime::default();
        for actor in [1, 2] {
            game_spatial::set_location(
                &mut host.storage,
                StableId(actor),
                Location::AtArea(AreaId(10)),
            )
            .unwrap();
            game_health::insert_clinical(
                &mut host.storage,
                PersonId(actor),
                game_health::Clinical::default(),
            )
            .unwrap();
        }
        game_spatial::set_location(
            &mut host.storage,
            StableId(2),
            Location::Carried {
                carrier: PersonId(1),
            },
        )
        .unwrap();
        game_transport::insert_group(
            &mut host.storage,
            PersonId(2),
            CarryGroup {
                passenger: PersonId(2),
                anchor: PersonId(1),
                bearers: BTreeSet::from([PersonId(1)]),
                authority: Authority::Newborn,
                accepted_at: SimTime(0),
            },
        )
        .unwrap();
        game_health::insert_shelter(
            &mut host.storage,
            Shelter {
                id: ShelterId(20),
                area: AreaId(10),
                custodian: PersonId(1),
                access: BTreeSet::from([PersonId(1)]),
                capacity: 1,
                insulation: 100,
                built_at: SimTime(0),
                timber_used: Quantity(1),
                labor_ms: 1,
            },
        )
        .unwrap();
        game_health::insert_occupancy(
            &mut host.storage,
            PersonId(1),
            Occupancy {
                shelter: ShelterId(20),
            },
        )
        .unwrap();
        let motion = transport
            .prepare_put_down(
                &host.storage,
                PersonId(1),
                PersonId(2),
                SimTime(0),
                PutDownFacts {
                    passenger_independent: false,
                    passenger_combat_restricted: false,
                },
            )
            .unwrap();
        let shelter_facts = [1, 2]
            .into_iter()
            .map(|actor| {
                (
                    PersonId(actor),
                    game_health::ShelterActorFacts {
                        actor: PersonId(actor),
                        carrier: (actor == 2).then_some(PersonId(1)),
                        alive: true,
                        physical_area: Some(AreaId(10)),
                        activity: HealthActivityFacts::default(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let placement = health
            .prepare_placements(
                &host.storage,
                &[],
                &[ShelterPlacement {
                    actor: PersonId(2),
                    final_area: match motion
                        .projected_position(&host.storage, PersonId(2), SimTime(0))
                        .unwrap()
                    {
                        game_transport::Position::Area(area) => Some(area),
                        _ => None,
                    },
                }],
                &shelter_facts,
            )
            .unwrap();
        let actors = [1, 2].map(|actor| LocalHealthActor {
            actor: PersonId(actor),
            place: Some(AreaId(10)),
            physical_area: Some(AreaId(10)),
            carrier: match motion
                .projected_location(&host.storage, PersonId(actor))
                .unwrap()
            {
                Location::Carried { carrier } => Some(*carrier),
                _ => None,
            },
            tactical: None,
            alive: true,
            moving: false,
            activity: HealthActivityFacts::default(),
            health: 100,
            max_health: 100,
            temperature_c: Some(20),
        });
        let observations = placement
            .prepare_projected_observations(
                &health,
                &host.storage,
                &shelter_facts,
                SimTime(0),
                &actors,
                &[],
            )
            .unwrap();
        if failure == 2 {
            game_health::shelter_mut(&mut host.storage, ShelterId(20))
                .unwrap()
                .area = AreaId(11);
        }
        if failure == 3 {
            *game_spatial::location_mut(&mut host.storage, StableId(1)).unwrap() =
                Location::AtArea(AreaId(11));
        }
        let before = host.snapshot().unwrap();
        let header = transport.clone();
        let operation = (|| -> Result<_, Error> {
            host.preflight_records(1, motion.effects().len() as u64, 1)?;
            let mut batch = Batch::new();
            let movement = motion.stage_into(
                &transport,
                &host.storage,
                &BTreeMap::<PersonId, game_transport::Body>::new(),
                SimTime(0),
                &mut batch,
            )?;
            let placement =
                placement.stage_into(&health, &host.storage, &shelter_facts, &mut batch)?;
            observations.stage_into(&health, &host.storage, &mut batch)?;
            batch.insert(
                StableId(host.next_id()),
                if failure == 1 {
                    "reject".into()
                } else {
                    "accepted".to_owned()
                },
            );
            movement.validate_footprint(&batch)?;
            placement.validate_footprint(&batch)?;
            host.storage.apply_batch(batch)?;
            host.allocate_id()?;
            Ok((movement, placement))
        })();
        if failure != 0 {
            assert!(operation.is_err());
            let after = host.snapshot().unwrap();
            assert_eq!(after.components, before.components);
            assert_eq!(
                (after.next_id, after.next_event, after.time, after.events),
                (
                    before.next_id,
                    before.next_event,
                    before.time,
                    before.events
                )
            );
            assert_eq!(transport, header);
        } else {
            let (movement, placement) = operation.unwrap();
            let effects = movement.finish(&mut transport);
            assert!(!effects.is_empty());
            assert!(placement.finish().is_empty());
            host.append_many(vec!["placed".to_owned()]).unwrap();
            assert_eq!(
                game_spatial::location(&host.storage, StableId(2)),
                Some(&Location::AtArea(AreaId(10)))
            );
            assert!(game_transport::group(&host.storage, PersonId(2)).is_none());
            assert_eq!(
                game_health::occupancy(&host.storage, PersonId(2)),
                Some(&Occupancy {
                    shelter: ShelterId(20)
                })
            );
            let known = game_health::knowledge(&host.storage, PersonId(1)).unwrap();
            assert!(known.people.contains_key(&PersonId(2)));
            assert!(
                known.shelters[&ShelterId(20)]
                    .known_occupants
                    .contains(&PersonId(2))
            );
            assert_eq!(host.next_id(), before.next_id + 1);
            assert_eq!(host.next_event(), before.next_event + 1);
        }
    }
}

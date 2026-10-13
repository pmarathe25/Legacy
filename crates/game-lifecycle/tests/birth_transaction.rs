use game_characters::decisions::{self, Aspiration, Goal, Mind, Personality, Stocks};
use game_core::{
    AreaId, Error, InventoryId, LifeStage, Location, PersonId, PregnancyId, SimDuration, SimTime,
    Vitals,
};
use game_ecs::{Batch, Schema, StableId};
use game_lifecycle::{
    BodyFacts, Effect, Life, Pregnancy, PregnancyState, Reproduction, Rules, Runtime,
};
use game_physiology::Body;
use game_sim::{Clock, ClockWorld};
use game_world::{GenericHost, Transaction};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Born(PersonId),
}
type Host = GenericHost<(), Event>;
fn fixture(reject: u8) -> (Host, Runtime, game_physiology::Rules) {
    let mut schema = Schema::new();
    game_lifecycle::register_components(&mut schema).unwrap();
    game_characters::register_people(&mut schema).unwrap();
    if reject == 1 {
        schema
            .register::<Body>("physiology/body", |body| {
                body.validate()?;
                if body.vitals.health > 10 {
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
            .register::<Mind<u8>>("characters/mind", |mind| {
                mind.validate()?;
                if mind.goal == Goal::Survival {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        schema
            .register::<Stocks>("characters/stocks", Stocks::validate)
            .unwrap();
    } else {
        decisions::register_minds::<u8>(&mut schema).unwrap();
    }
    game_cargo::register_components(&mut schema).unwrap();
    game_spatial::register_components(&mut schema).unwrap();
    game_skills::register_components(&mut schema).unwrap();
    game_health::register_components(&mut schema).unwrap();
    game_realm::register_components(&mut schema).unwrap();
    game_characters::social::register_components(&mut schema).unwrap();
    game_property::register_components(&mut schema).unwrap();
    let mut host = Host::new(schema, 73, ());
    host.allocate_ids(3).unwrap();
    host.publish_clock(Clock {
        time: SimTime(10),
        ..host.clock()
    });
    let lifecycle = Runtime {
        rules: Rules::human(),
    };
    for (actor, role) in [
        (PersonId(1), Reproduction::Gestates),
        (PersonId(2), Reproduction::SuppliesSeed),
    ] {
        game_lifecycle::insert_life(
            &mut host.storage,
            Life {
                id: actor,
                born: SimTime(0),
                initial_age: lifecycle.rules.adulthood,
                stage: LifeStage::Adult,
                reproduction: role,
                parents: vec![],
                caregivers: BTreeSet::new(),
                preference: None,
            },
        )
        .unwrap();
        game_physiology::insert_body(
            &mut host.storage,
            actor,
            Body {
                vitals: Vitals {
                    hunger: 1,
                    fatigue: 2,
                    health: 10,
                    alive: true,
                },
                fatigue_remainder: 0,
                health_remainder: 0,
            },
        )
        .unwrap();
        decisions::insert_mind::<u8>(
            &mut host.storage,
            actor,
            Mind::new(Personality::industrious(), Aspiration::Parenthood).unwrap(),
        )
        .unwrap();
        game_spatial::set_location(
            &mut host.storage,
            StableId(actor.0),
            Location::AtArea(AreaId(8)),
        )
        .unwrap();
    }
    game_lifecycle::insert_pregnancy(
        &mut host.storage,
        Pregnancy {
            id: PregnancyId(3),
            gestator: PersonId(1),
            partner: PersonId(2),
            state: PregnancyState::Gestating { due: SimTime(10) },
        },
    )
    .unwrap();
    let physical = game_physiology::Rules {
        hunger_harm: 100,
        fatigue_limit: 100,
        max_health: 100,
        meal_relief: 50,
        meal_size: game_core::Quantity(1),
        eating_time: SimDuration(1),
        loading_time: SimDuration(1),
        carrying_capacity: game_core::Quantity(10_000),
    };
    (host, lifecycle, physical)
}
fn actual_birth(
    host: &mut Host,
    lifecycle: &Runtime,
    physical: &game_physiology::Rules,
) -> Result<(), Error> {
    let opening = host.transaction_opening();
    let child = PersonId(opening.next_id);
    let inventory = InventoryId(child.0 + 1);
    let birth = lifecycle.prepare_birth(
        &host.storage,
        PregnancyId(3),
        host.clock().time,
        child,
        &BodyFacts {
            actor: PersonId(1),
            alive: true,
            health: 10,
            position: Location::AtArea(AreaId(8)),
            action_restricted: false,
            attempt: None,
        },
        host.seed(),
    )?;
    assert!(matches!(birth.effects(), [Effect::Born { .. }]));
    let person = game_characters::people::prepare_newborn_person(&host.storage, child, inventory)?;
    let body = game_physiology::prepare_newborn_body(&host.storage, child, physical)?;
    let mind = game_characters::orchestration::prepare_newborn_mind::<u8>(
        &host.storage,
        child,
        PersonId(1),
        [PersonId(1), PersonId(2)],
        host.clock().time,
        &[PersonId(1), PersonId(2)],
    )?;
    let temperament = game_characters::social::Temperament::from_personality(mind.personality());
    let social_owner = game_characters::social::World::default();
    let social = game_society::personal::prepare_receipt(
        &social_owner,
        &host.storage,
        game_society::personal::PersonalReceipt::Born {
            child,
            temperament,
            witnesses: vec![],
        },
        host.clock().time,
    )?;
    let health_owner = game_health::Runtime::default();
    let health = health_owner.prepare_reaction(
        &host.storage,
        host.clock().time,
        game_health::HealthReaction::Born {
            child,
            inherited_shelter: None,
        },
    )?;
    let skills_owner = game_skills::Runtime::default();
    let skills = skills_owner.prepare_newborn(&host.storage, child)?;
    let realm_owner = game_realm::Runtime::default();
    let realm = realm_owner.prepare_newborn(&host.storage, child)?;
    let cargo = game_cargo::prepare_newborn_inventory(&host.storage, inventory, child)?;
    let property = game_property::prepare_newborn(&host.storage, child, inventory)?;
    let mut batch = Batch::new();
    let life = birth.stage_into(lifecycle, &host.storage, &mut batch)?;
    let mut guards = vec![
        person.stage_into(&host.storage, &mut batch)?,
        body.stage_into(physical, None, &host.storage, &mut batch)?,
        mind.stage_into(&host.storage, &mut batch)?,
        cargo.stage_into(&host.storage, &mut batch)?,
        property.stage_into(&host.storage, &mut batch)?,
        realm.stage_into(&host.storage, &mut batch)?,
        game_spatial::prepare_location(
            &host.storage,
            StableId(child.0),
            Location::AtArea(AreaId(8)),
        )?
        .stage_into(&host.storage, &mut batch)?,
    ];
    let social = social.stage_into(&social_owner, &host.storage, host.clock().time, &mut batch)?;
    let health = health.stage_into(&health_owner, &host.storage, &mut batch)?;
    let skills = skills.stage_into(&skills_owner, &host.storage, &mut batch)?;
    life.validate(&host.storage, &batch)?;
    social.validate(&social_owner, &host.storage, host.clock().time, &batch)?;
    health.validate_batch(&health_owner, &host.storage, &batch)?;
    skills.validate(&skills_owner, &host.storage, &batch)?;
    guards.push(social.row_guard());
    let mut transaction = Transaction::new(opening, batch);
    transaction.ids = 2;
    transaction.events.push(Event::Born(child));
    transaction.guards = guards;
    host.commit_transaction(transaction)
}
#[test]
fn actual_prepared_birth_and_receiving_schemas_publish_pregnancy_parent_minds_counters_and_event_once()
 {
    for reject in 0..=2 {
        let (mut host, lifecycle, physical) = fixture(reject);
        let before = host.snapshot().unwrap();
        let result = actual_birth(&mut host, &lifecycle, &physical);
        if reject == 0 {
            result.unwrap();
            assert_eq!(host.next_id(), 6);
            assert_eq!(host.next_event(), 2);
            assert_eq!(
                game_lifecycle::pregnancy(&host.storage, PregnancyId(3))
                    .unwrap()
                    .state,
                PregnancyState::Born(PersonId(4))
            );
            assert_eq!(
                game_lifecycle::life(&host.storage, PersonId(4))
                    .unwrap()
                    .parents,
                vec![PersonId(1), PersonId(2)]
            );
            assert_eq!(
                game_physiology::body(&host.storage, PersonId(4))
                    .unwrap()
                    .vitals
                    .health,
                100
            );
            assert!(
                decisions::mind::<u8>(&host.storage, PersonId(1))
                    .unwrap()
                    .relationships
                    .contains_key(&PersonId(4))
            );
            assert_eq!(host.events().len(), 1);
        } else {
            assert_eq!(result, Err(Error::Unavailable));
            assert_eq!(host.snapshot().unwrap(), before);
            assert!(!host.storage.contains(StableId(4)));
            assert!(!host.storage.contains(StableId(5)));
            assert_eq!(
                game_lifecycle::pregnancy(&host.storage, PregnancyId(3))
                    .unwrap()
                    .state,
                PregnancyState::Gestating { due: SimTime(10) }
            );
        }
    }
}

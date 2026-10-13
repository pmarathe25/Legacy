#![cfg(feature = "genetics")]
use game_core::{
    AreaId, Error, LifeStage, Location, PersonId, PregnancyId, SimDuration, SimTime, Vitals,
};
use game_ecs::{Batch, ReadFootprint, ReadSet, Schema, StableId};
use game_lifecycle::{
    Attempt, BodyFacts, Consent, Effect, Life, Pregnancy, PregnancyState, Reproduction, Rules,
    Runtime,
};
use game_sim::{Clock, ClockWorld};
use game_world::{GenericHost, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Motor {
    Rest,
    Conceive(PregnancyId, SimTime),
    Idle,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Conceived(PregnancyId),
    Refused,
}
type Host = GenericHost<(), Event>;

fn fixture(reject: u8) -> (Host, Runtime, game_genetics::Runtime) {
    let mut schema = Schema::new();
    schema
        .register::<Life>("lifecycle/life", Life::validate)
        .unwrap();
    let pregnancy_validator: fn(&Pregnancy) -> Result<(), Error> = if reject == 1 {
        |row| {
            row.validate()?;
            if matches!(row.state, PregnancyState::Conceiving { .. }) {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        }
    } else if reject == 2 {
        |row| {
            row.validate()?;
            if matches!(row.state, PregnancyState::Gestating { .. }) {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        }
    } else {
        Pregnancy::validate
    };
    schema
        .register::<Pregnancy>("lifecycle/pregnancy", pregnancy_validator)
        .unwrap();
    let motor_validator: fn(&Motor) -> Result<(), Error> = if reject == 4 {
        |row| {
            if *row == Motor::Idle {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        }
    } else {
        |_| Ok(())
    };
    schema
        .register::<Motor>("test/motor", motor_validator)
        .unwrap();
    game_physiology::register_components(&mut schema).unwrap();
    schema
        .register::<game_genetics::PersonGenome>(
            "genetics/genome",
            game_genetics::PersonGenome::validate,
        )
        .unwrap();
    let inheritance_validator: fn(&game_genetics::Inheritance) -> Result<(), Error> = if reject == 3
    {
        |row| {
            row.validate()?;
            Err(Error::Unavailable)
        }
    } else {
        game_genetics::Inheritance::validate
    };
    schema
        .register::<game_genetics::Inheritance>("genetics/inheritance", inheritance_validator)
        .unwrap();
    let mut host = Host::new(schema, 73, ());
    host.allocate_ids(2).unwrap();
    let life = Runtime {
        rules: Rules {
            conception_time: SimDuration(1),
            gestation: SimDuration(2),
            infancy: SimDuration(3),
            adulthood: SimDuration(4),
            elderhood: SimDuration(5),
            lifespan: SimDuration(6),
        },
    };
    for (actor, reproduction) in [
        (PersonId(1), Reproduction::Gestates),
        (PersonId(2), Reproduction::SuppliesSeed),
    ] {
        game_lifecycle::insert_life(
            &mut host.storage,
            Life {
                id: actor,
                born: SimTime(0),
                initial_age: life.rules.adulthood,
                stage: LifeStage::Adult,
                reproduction,
                parents: vec![],
                caregivers: BTreeSet::new(),
                preference: None,
            },
        )
        .unwrap();
        game_physiology::insert_body(
            &mut host.storage,
            actor,
            game_physiology::Body {
                vitals: Vitals {
                    hunger: 7,
                    fatigue: 11,
                    health: 100,
                    alive: true,
                },
                fatigue_remainder: 13,
                health_remainder: 17,
            },
        )
        .unwrap();
        host.storage.insert(StableId(actor.0), Motor::Rest).unwrap();
    }
    let genes = game_genetics::Runtime::new(
        game_genetics::Rules {
            mutation_chance: 10_000,
            crossover_chance: 1_000,
        },
        vec![game_genetics::LocusDefinition {
            id: game_genetics::LocusId("test.opaque".into()),
            chromosome: "test".into(),
            position: 0,
            min: 0,
            max: 1_000,
            initial_alleles: vec![0, 500, 1_000],
            mutation_step: 1,
            mutable: true,
        }],
    )
    .unwrap();
    for actor in [PersonId(1), PersonId(2)] {
        genes.seed_person(&mut host.storage, 73, actor).unwrap();
    }
    (host, life, genes)
}
fn bodies(host: &Host, attempt: bool) -> [BodyFacts; 2] {
    [PersonId(1), PersonId(2)].map(|actor| {
        let body = game_physiology::body(&host.storage, actor).unwrap();
        BodyFacts {
            actor,
            alive: body.vitals.alive,
            health: body.vitals.health,
            position: Location::AtArea(AreaId(8)),
            action_restricted: false,
            attempt: attempt.then_some(Attempt {
                pregnancy: PregnancyId(3),
                completes: SimTime(1),
            }),
        }
    })
}
fn start(host: &mut Host, life: &Runtime, accepts: bool) -> Result<(), Error> {
    let facts = bodies(host, false);
    let plan = life.prepare_start_conception(
        &host.storage,
        host.clock().time,
        PregnancyId(host.next_id()),
        [&facts[0], &facts[1]],
        Consent {
            initiator: true,
            partner: accepts,
        },
    )?;
    assert!(!plan.is_completed_pregnancy(PregnancyId(3)));
    let refused = matches!(plan.effects(), [Effect::Refused { .. }]);
    let mut batch = Batch::new();
    let finish = plan.stage_into(life, &host.storage, &mut batch)?;
    let mut guards = vec![];
    if !refused {
        let mut reads = ReadSet::new();
        let mut writes = ReadFootprint::new();
        for actor in [PersonId(1), PersonId(2)] {
            let id = StableId(actor.0);
            reads.read::<Motor>(&host.storage, id);
            batch.insert(id, Motor::Conceive(PregnancyId(3), SimTime(1)));
            writes.protect::<Motor>(id);
        }
        guards.push(reads.bind(&host.storage, &batch, &writes)?);
    }
    finish.validate(&host.storage, &batch)?;
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.ids = u64::from(!refused);
    transaction.revisions = 1;
    transaction.guards = guards;
    if refused {
        transaction.events.push(Event::Refused);
    }
    host.commit_transaction(transaction)
}
fn complete(
    host: &mut Host,
    life: &Runtime,
    genes: &mut game_genetics::Runtime,
) -> Result<(), Error> {
    let facts = bodies(host, true);
    let mut reads = ReadSet::new();
    for actor in [PersonId(1), PersonId(2)] {
        reads.read::<Motor>(&host.storage, StableId(actor.0));
        reads.read::<game_physiology::Body>(&host.storage, StableId(actor.0));
    }
    let plan = life
        .prepare_complete_conception(
            &host.storage,
            PregnancyId(3),
            host.clock().time,
            [&facts[0], &facts[1]],
        )?
        .with_factual_reads(reads);
    assert!(plan.is_completed_pregnancy(PregnancyId(3)));
    assert!(!plan.is_completed_pregnancy(PregnancyId(4)));
    let freeze = genes.prepare_conception(
        &host.storage,
        host.seed(),
        PregnancyId(3),
        [PersonId(1), PersonId(2)],
        host.clock().time,
    )?;
    let mut batch = Batch::new();
    let mut motor_reads = ReadSet::new();
    let mut writes = ReadFootprint::new();
    for actor in [PersonId(1), PersonId(2)] {
        let id = StableId(actor.0);
        motor_reads.read::<Motor>(&host.storage, id);
        batch.insert(id, Motor::Idle);
        writes.protect::<Motor>(id);
    }
    let motor = motor_reads.bind(&host.storage, &batch, &writes)?;
    let life = plan.stage_into_with_factual_writes(life, &host.storage, &mut batch, writes)?;
    let freeze = freeze.stage_into(genes, &host.storage, &mut batch)?;
    life.validate(&host.storage, &batch)?;
    freeze.validate(genes, &host.storage, &batch)?;
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.guards.push(motor);
    transaction.events.push(Event::Conceived(PregnancyId(3)));
    host.commit_transaction(transaction)?;
    freeze.finish(genes);
    Ok(())
}
#[test]
fn selected_conception_start_and_frozen_genome_admit_with_activities_ids_and_records_once() {
    for reject in 0..=4 {
        let (mut host, life, mut genes) = fixture(reject);
        let before = (host.snapshot().unwrap(), genes.clone());
        let started = start(&mut host, &life, true);
        if reject == 1 {
            assert_eq!(started, Err(Error::Unavailable));
            assert_eq!((host.snapshot().unwrap(), genes), before);
            continue;
        }
        started.unwrap();
        assert_eq!(host.next_id(), 4);
        assert_eq!(host.next_event(), 1);
        host.publish_clock(Clock {
            time: SimTime(1),
            ..host.clock()
        });
        let before = (host.snapshot().unwrap(), genes.clone());
        let completed = complete(&mut host, &life, &mut genes);
        if reject == 0 {
            completed.unwrap();
            assert_eq!(
                game_lifecycle::pregnancy(&host.storage, PregnancyId(3))
                    .unwrap()
                    .state,
                PregnancyState::Gestating { due: SimTime(3) }
            );
            assert_eq!(
                game_genetics::inheritance(&host.storage, PregnancyId(3))
                    .unwrap()
                    .parents,
                [PersonId(1), PersonId(2)]
            );
            assert_eq!(host.storage.get::<Motor>(StableId(1)), Some(&Motor::Idle));
            assert_eq!(host.next_id(), 4);
            assert_eq!(host.next_event(), 2);
            assert!(complete(&mut host, &life, &mut genes).is_err());
            assert_eq!(host.next_event(), 2);
        } else {
            assert_eq!(completed, Err(Error::Unavailable));
            assert_eq!((host.snapshot().unwrap(), genes), before);
        }
    }
}
#[test]
fn refused_conception_keeps_id_and_motor_rows_without_an_inheritance_record() {
    let (mut host, life, genes) = fixture(0);
    start(&mut host, &life, false).unwrap();
    assert_eq!(host.next_id(), 3);
    assert_eq!(host.next_event(), 2);
    assert_eq!(host.storage.get::<Motor>(StableId(1)), Some(&Motor::Rest));
    assert!(game_lifecycle::pregnancy(&host.storage, PregnancyId(3)).is_none());
    assert!(game_genetics::inheritance(&host.storage, PregnancyId(3)).is_none());
    assert_eq!(genes, fixture(0).2);
}

#[test]
fn completed_conception_keeps_exact_intentional_motor_projection_guarded() {
    let (mut host, life, _) = fixture(0);
    start(&mut host, &life, true).unwrap();
    host.publish_clock(Clock {
        time: SimTime(1),
        ..host.clock()
    });
    let facts = bodies(&host, true);
    let mut reads = ReadSet::new();
    for actor in [PersonId(1), PersonId(2)] {
        reads.read::<Motor>(&host.storage, StableId(actor.0));
    }
    let plan = life
        .prepare_complete_conception(
            &host.storage,
            PregnancyId(3),
            host.clock().time,
            [&facts[0], &facts[1]],
        )
        .unwrap()
        .with_factual_reads(reads);
    let mut batch = Batch::new();
    let mut writes = ReadFootprint::new();
    for actor in [PersonId(1), PersonId(2)] {
        batch.insert(StableId(actor.0), Motor::Idle);
        writes.protect::<Motor>(StableId(actor.0));
    }
    let receipt = plan
        .stage_into_with_factual_writes(&life, &host.storage, &mut batch, writes)
        .unwrap();
    let before = host.snapshot().unwrap();
    batch.insert(StableId(1), Motor::Conceive(PregnancyId(99), SimTime(1)));
    assert_eq!(
        receipt.validate(&host.storage, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);
}

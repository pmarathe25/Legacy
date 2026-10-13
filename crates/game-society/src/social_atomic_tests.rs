//! Real owner preparation joins Cargo, activity and personal receivers before origin publication.
use crate::personal::*;
use game_characters::social::{self, Interaction, Personal, World};
use game_core::*;
use game_ecs::{Batch, Schema, StableId};
use game_sim::ClockWorld;
use game_world::{GenericHost, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Activity {
    Idle,
    Social,
    Other,
}
type Host = GenericHost<World, &'static str>;
fn fixture(reject_personal: bool, reject_activity: bool) -> Host {
    let mut schema = Schema::new();
    let personal: fn(&Personal) -> Result<(), Error> = if reject_personal {
        |row| {
            if row.development.next_choice.0 > 0 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        }
    } else {
        |_| Ok(())
    };
    schema
        .register::<Personal>("characters/social-personal", personal)
        .unwrap();
    schema
        .register::<social::InteractionRow>("characters/social-interaction", |_| Ok(()))
        .unwrap();
    schema
        .register::<social::Participant>("characters/social-participant", |_| Ok(()))
        .unwrap();
    let activity: fn(&Activity) -> Result<(), Error> = if reject_activity {
        |row| {
            if matches!(row, Activity::Social) {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        }
    } else {
        |_| Ok(())
    };
    schema
        .register::<Activity>("test/activity", activity)
        .unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    let mut host = Host::new(
        schema,
        1,
        World {
            rules: social::Rules::fixture(),
            last_review: SimTime(0),
        },
    );
    host.allocate_ids(20).unwrap();
    let temperament = social::Temperament {
        discipline: 500,
        ambition: 500,
        sociability: 800,
        hedonism: 300,
        empathy: 800,
    };
    for actor in [PersonId(1), PersonId(2)] {
        social::insert_personal(
            &mut host.storage,
            actor,
            Personal {
                development: social::Development::baseline(
                    temperament,
                    social::Aspiration::Comfort,
                )
                .unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
        host.storage
            .insert(StableId(actor.0), Activity::Idle)
            .unwrap();
    }
    for (id, good, quantity) in [(10, Good::Tool, 1_000), (11, Good::Timber, 2_000)] {
        game_cargo::insert_inventory(
            &mut host.storage,
            game_cargo::Inventory {
                id: InventoryId(id),
                location: game_cargo::InventoryLocation::Area(AreaId(3)),
                good,
                quantity: Quantity(quantity),
                capacity: Quantity(3_000),
                access: BTreeSet::from([PersonId(1)]),
                last_granted: None,
                spoils_at: None,
            },
        )
        .unwrap();
    }
    host
}
fn practice(host: &Host) -> (Batch, social::SocialStartGuard, game_ecs::ReadGuard) {
    let actor = PersonFacts {
        actor: PersonId(1),
        area: Some(AreaId(3)),
        speech: true,
        adult: true,
        practical: true,
        tool: Some(InventoryId(10)),
        materials: BTreeMap::from([(Good::Timber, InventoryId(11))]),
        farming_access: false,
        routes: BTreeMap::new(),
        practice: BTreeMap::new(),
        empathy: 800,
        patience: 800,
        affection: 0,
        trust: 0,
        conversation: None,
    };
    let decision = prepare_start(
        &host.state,
        &host.storage,
        &actor,
        None,
        social::SocialAction::Practice {
            skill: Skill::Craft,
        },
        host.clock().time,
    )
    .unwrap();
    let StartDecision::Started {
        interaction,
        materials: Some((_, source, quantity)),
        ..
    } = decision
    else {
        panic!("practice starts")
    };
    let start = host
        .state
        .prepare_start(&host.storage, actor.actor, interaction, host.clock().time)
        .unwrap();
    let cargo = game_cargo::prepare_consumptions(&host.storage, &[(source, quantity)]).unwrap();
    let mut batch = Batch::new();
    let start = start
        .stage_into(&host.state, &host.storage, host.clock().time, &mut batch)
        .unwrap();
    let cargo = cargo.stage_guarded_into(&host.storage, &mut batch).unwrap();
    batch.insert(StableId(1), Activity::Social);
    (batch, start, cargo)
}
#[test]
fn start_receiver_schema_rejection_preserves_material_reservations_activities_events_and_clock() {
    for (reject_personal, reject_activity) in [(true, false), (false, true)] {
        let mut host = fixture(reject_personal, reject_activity);
        let before = host.snapshot().unwrap();
        let clock = host.clock();
        let (batch, start, cargo) = practice(&host);
        start
            .validate(&host.state, &host.storage, host.clock().time, &batch)
            .unwrap();
        cargo.validate(&host.storage, &batch).unwrap();
        let mut transaction = Transaction::new(host.transaction_opening(), batch);
        transaction.guards = vec![start.row_guard(), cargo];
        transaction.events = vec!["consumed"];
        transaction.revisions = 1;
        assert_eq!(
            host.commit_transaction(transaction),
            Err(Error::Unavailable)
        );
        assert_eq!(host.snapshot().unwrap(), before);
        assert_eq!(host.clock(), clock);
        assert!(social::participant(&host.storage, PersonId(1)).is_none());
        assert_eq!(
            game_cargo::inventory(&host.storage, InventoryId(11))
                .unwrap()
                .quantity,
            Quantity(2_000)
        );
    }
}
#[test]
fn interruption_owner_cleanup_preserves_other_activity_and_rejects_late_schema_failure_atomically()
{
    let mut host = fixture(false, false);
    let (batch, start, cargo) = practice(&host);
    start
        .validate(&host.state, &host.storage, host.clock().time, &batch)
        .unwrap();
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.guards = vec![start.row_guard(), cargo];
    host.commit_transaction(transaction).unwrap();
    let kinds = BTreeMap::from([(PersonId(1), social::SocialActivityKind::Practice)]);
    let prepared = host
        .state
        .prepare_interruption::<Activity>(&host.storage, PersonId(1), &kinds)
        .unwrap()
        .unwrap();
    assert_eq!(
        prepared.cleanup(),
        &[social::ActivityCleanup::Idle { actor: PersonId(1) }]
    );
    // The existing activity binding can admit starting Social while independently rejecting Idle cleanup.
    let mut replacement = Schema::new();
    social::register_components(&mut replacement).unwrap();
    game_cargo::register_components(&mut replacement).unwrap();
    replacement
        .register::<Activity>("test/activity", |row| {
            if matches!(row, Activity::Idle) {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    // Actor two is outside this interrupted interaction and carries an unrelated activity.
    host.storage.insert(StableId(2), Activity::Other).unwrap();
    host.storage = game_ecs::Store::restore(replacement, host.storage.snapshot().unwrap()).unwrap();
    let before = host.snapshot().unwrap();
    let mut batch = Batch::new();
    let cleanup = prepared
        .stage_into(&host.storage, &mut batch, Activity::Idle)
        .unwrap();
    cleanup.validate(&host.storage, &batch).unwrap();
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.guards = cleanup.row_guards();
    assert_eq!(
        host.commit_transaction(transaction),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);
    assert!(social::participant(&host.storage, PersonId(1)).is_some());
    assert_eq!(
        host.storage.get::<Activity>(StableId(2)),
        Some(&Activity::Other)
    );
}

#[test]
fn completion_personal_or_activity_receiver_rejection_preserves_the_origin_and_reservations() {
    for reject_personal in [true, false] {
        let mut host = fixture(false, false);
        let at = SimTime(60_000);
        let interaction = Interaction::Talk {
            first: PersonId(1),
            second: PersonId(2),
            topic: social::SocialTopic::Companionship,
            completes: at,
        };
        host.state
            .start(&mut host.storage, PersonId(1), interaction, SimTime(0))
            .unwrap();
        for actor in [1, 2] {
            host.storage
                .insert(StableId(actor), Activity::Social)
                .unwrap();
        }
        let mut schema = Schema::new();
        let personal_validator: fn(&Personal) -> Result<(), Error> = if reject_personal {
            |row| {
                if row.development.experiences > 0 {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            }
        } else {
            |_| Ok(())
        };
        let activity_validator: fn(&Activity) -> Result<(), Error> = if !reject_personal {
            |row| {
                if matches!(row, Activity::Idle) {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            }
        } else {
            |_| Ok(())
        };
        schema
            .register::<Personal>("characters/social-personal", personal_validator)
            .unwrap();
        schema
            .register::<social::InteractionRow>("characters/social-interaction", |_| Ok(()))
            .unwrap();
        schema
            .register::<social::Participant>("characters/social-participant", |_| Ok(()))
            .unwrap();
        schema
            .register::<Activity>("test/activity", activity_validator)
            .unwrap();
        game_cargo::register_components(&mut schema).unwrap();
        host.storage = game_ecs::Store::restore(schema, host.storage.snapshot().unwrap()).unwrap();
        let mut clock = host.clock();
        clock.time = at;
        host.publish_clock(clock);
        let before = host.snapshot().unwrap();
        let (effect, prepared, minds) =
            prepare_completion::<u8>(&host.state, &host.storage, PersonId(1), at, true).unwrap();
        assert!(matches!(effect, Completion::Socialized { .. }));
        let prepared = prepared.unwrap();
        let kinds = BTreeMap::from([
            (PersonId(1), social::SocialActivityKind::Conversation),
            (PersonId(2), social::SocialActivityKind::Conversation),
        ]);
        let cleanup = host
            .state
            .prepare_interruption::<Activity>(&host.storage, PersonId(1), &kinds)
            .unwrap()
            .unwrap();
        let mut batch = Batch::new();
        let cleanup = cleanup
            .stage_into(&host.storage, &mut batch, Activity::Idle)
            .unwrap();
        let personal = prepared
            .stage_with_cleanup(&host.state, &host.storage, at, &mut batch, &cleanup)
            .unwrap();
        let mind = minds.stage_into(&host.storage, &mut batch).unwrap();
        cleanup.validate(&host.storage, &batch).unwrap();
        personal
            .validate(&host.state, &host.storage, at, &batch)
            .unwrap();
        let mut transaction = Transaction::new(host.transaction_opening(), batch);
        transaction.guards = cleanup.row_guards();
        transaction.guards.extend([personal.row_guard(), mind]);
        transaction.events = vec!["socialized"];
        assert_eq!(
            host.commit_transaction(transaction),
            Err(Error::Unavailable)
        );
        assert_eq!(host.snapshot().unwrap(), before);
        assert!(social::participant(&host.storage, PersonId(1)).is_some());
        assert!(
            social::personal(&host.storage, PersonId(1))
                .unwrap()
                .companionship
                .is_empty()
        );
        assert!(host.events().is_empty());
    }
}

#[test]
fn reserved_birth_receiving_shares_the_producer_entity_and_rejects_before_any_allocation_or_origin()
{
    for reject in [false, true] {
        let mut host = fixture(reject, false);
        let at = SimTime(100);
        let mut clock = host.clock();
        clock.time = at;
        host.publish_clock(clock);
        let child = PersonId(host.next_id());
        let before = host.snapshot().unwrap();
        let receipt = PersonalReceipt::Born {
            child,
            temperament: social::Temperament {
                discipline: 500,
                ambition: 500,
                sociability: 800,
                hedonism: 300,
                empathy: 800,
            },
            witnesses: vec![
                BirthWitness {
                    actor: PersonId(1),
                    same_location: true,
                    alive: true,
                },
                BirthWitness {
                    actor: PersonId(2),
                    same_location: false,
                    alive: true,
                },
            ],
        };
        let prepared = prepare_receipt(&host.state, &host.storage, receipt, at).unwrap();
        let mut batch = Batch::new();
        batch.insert(StableId(child.0), Activity::Other);
        let personal = prepared
            .stage_into(&host.state, &host.storage, at, &mut batch)
            .unwrap();
        personal
            .validate(&host.state, &host.storage, at, &batch)
            .unwrap();
        let mut transaction = Transaction::new(host.transaction_opening(), batch);
        transaction.ids = 1;
        transaction.events = vec!["born"];
        transaction.guards.push(personal.row_guard());
        if reject {
            assert_eq!(
                host.commit_transaction(transaction),
                Err(Error::Unavailable)
            );
            assert_eq!(host.snapshot().unwrap(), before);
        } else {
            host.commit_transaction(transaction).unwrap();
            assert_eq!(
                social::personal(&host.storage, child)
                    .unwrap()
                    .development
                    .next_choice,
                at
            );
            assert_eq!(
                social::personal(&host.storage, PersonId(1))
                    .unwrap()
                    .development
                    .experiences,
                1
            );
            assert_eq!(
                social::personal(&host.storage, PersonId(2))
                    .unwrap()
                    .development
                    .experiences,
                0
            );
            assert_eq!(
                host.storage.get::<Activity>(StableId(child.0)),
                Some(&Activity::Other)
            );
        }
    }
}

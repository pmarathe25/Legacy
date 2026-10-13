use super::*;
use crate::planning::tests::{facts, mind};
use game_ecs::{Schema, Store};
use game_world::GenericHost;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    decisions: Option<Runtime>,
    temperament: Option<crate::temperament::Runtime>,
}
struct Peers;
impl OwnerSystem<Event> for Peers {
    type Runtime = ();
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, Event>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
macro_rules! read {
    ($name:ident, $request:ty, $response:ty, $value:expr) => {
        fn $name(
            _: &Peers,
            _: &mut OwnerReadTxn<'_, '_, Peers, Event>,
            _: $request,
        ) -> Result<$response, Error> {
            Ok($value)
        }
    };
}
read!(
    physical,
    ReadPhysical,
    Option<PhysicalFacts>,
    Some(PhysicalFacts {
        vitals: facts().vitals,
        limits: facts().limits
    })
);
read!(
    placement,
    ReadPlacement,
    Option<PlacementFacts>,
    Some(PlacementFacts {
        location: Location::AtArea(AreaId(1)),
        point: Some(ObservationPoint::Area(AreaId(1))),
        can_resume: false
    })
);
read!(
    stage,
    ReadStage,
    StageFacts,
    StageFacts {
        stage: Some(LifeStage::Adult),
        independent: true
    }
);
read!(
    pressure,
    ReadPressure,
    PressureFacts,
    PressureFacts {
        hydration: 0,
        exposure: 0,
        drink_quantity: Quantity(100)
    }
);
read!(freedom, ReadFreedom, bool, false);
read!(transport_busy, ReadTransportBusy, bool, false);
read!(resting, ReadResting, bool, false);
read!(equipment, ReadWorkEquipment, bool, true);
read!(stock, ReadLocalStock, LocalStock, LocalStock::default());
read!(sites, ReadVisibleSites, Vec<ObservedSite>, vec![]);
read!(routes, ReadLocalRoutes, BTreeMap<AreaId, SimDuration>, BTreeMap::new());
read!(
    force,
    ReadForceCandidate<u8>,
    Option<ActionEvidence<u8>>,
    None
);
read!(
    health,
    ReadHealthCandidate<u8>,
    Option<ActionEvidence<u8>>,
    None
);
read!(
    provision,
    ReadProvisionCandidates<u8>,
    Vec<(ActionEvidence<u8>, game_logistics::Requirement)>,
    vec![]
);
read!(
    projects,
    ReadProjectCandidates<u8>,
    Vec<ProjectOpportunity<u8>>,
    vec![]
);
read!(
    property,
    ReadPropertyCandidates<u8>,
    Vec<(ActionEvidence<u8>, InventoryId)>,
    vec![]
);
read!(
    cultivation,
    ReadCultivationCandidate<u8>,
    Option<ActionEvidence<u8>>,
    None
);
fn execute(
    _: &Peers,
    tx: &mut OwnerTxn<'_, '_, Peers, Event>,
    request: ExecuteIntention<u8>,
) -> Result<bool, Error> {
    // Admission can query Characters after the review owner has returned.
    assert!(tx.request(ReadPerson(request.actor))?.is_some());
    Ok(false)
}
struct Birth(ReceiveNewborn);
impl Request for Birth {
    type Response = ();
}
fn birth(_: &Peers, tx: &mut OwnerTxn<'_, '_, Peers, Event>, request: Birth) -> Result<(), Error> {
    // Lifecycle reserves newborn identities before notifying receivers.
    tx.reserve_ids(3)?;
    tx.request(request.0)
}
fn fixture() -> (GenericHost<State, Event>, Registry<State, Event>) {
    let mut schema = Schema::new();
    crate::register_people(&mut schema).unwrap();
    register_minds::<u8>(&mut schema).unwrap();
    crate::temperament::register_components(&mut schema).unwrap();
    game_world::actions::register_components(&mut schema).unwrap();
    let runtime = Runtime {
        concepts: BTreeMap::from([(ConceptId(20), Good::Food)]),
        work_concept: ConceptId(20),
        last_learning_event: 0,
        switching_margin: 50,
    };
    let mut host = GenericHost::new(
        schema,
        7,
        State {
            decisions: Some(runtime),
            temperament: None,
        },
    );
    host.allocate_id().unwrap();
    let person = Person {
        id: PersonId(1),
        role: Role::Citizen,
        inventory: InventoryId(2),
        work_units: 0,
        work_remainder: 0,
        work_rate: 1_000,
        hunger_exempt: false,
    };
    crate::insert_person(&mut host.storage, person).unwrap();
    let mut own = mind();
    own.personality = Personality::restful();
    own.dream = Aspiration::Comfort;
    insert_mind(&mut host.storage, PersonId(1), own).unwrap();
    let mut registry = Registry::new();
    register_systems(
        &mut registry,
        RuntimeSlot::new(
            |state: &State| &state.decisions,
            |state| &mut state.decisions,
        ),
        |request| {
            if matches!(request, ActionRequest::Rest) {
                3_u8
            } else {
                4
            }
        },
    )
    .unwrap();
    crate::temperament::services::register_systems(
        &mut registry,
        RuntimeSlot::new(
            |state: &State| &state.temperament,
            |state| &mut state.temperament,
        ),
    )
    .unwrap();
    registry.register(Peers, RuntimeSlot::stateless()).unwrap();
    registry.register_request::<Peers, Birth>(birth).unwrap();
    macro_rules! register { ($($request:ty => $handler:ident),* $(,)?) => { $(registry.register_read_request::<Peers, $request>($handler).unwrap();)* } }
    register!(ReadPhysical => physical, ReadPlacement => placement, ReadStage => stage, ReadPressure => pressure, ReadFreedom => freedom, ReadResting => resting, ReadWorkEquipment => equipment, ReadLocalStock => stock, ReadVisibleSites => sites, ReadLocalRoutes => routes, ReadForceCandidate<u8> => force, ReadHealthCandidate<u8> => health, ReadProvisionCandidates<u8> => provision, ReadProjectCandidates<u8> => projects, ReadPropertyCandidates<u8> => property, ReadCultivationCandidate<u8> => cultivation);
    registry
        .register_request::<Peers, ExecuteIntention<u8>>(execute)
        .unwrap();
    registry
        .register_read_request::<Peers, ReadTransportBusy>(transport_busy)
        .unwrap();
    game_world::actions::Actions::new()
        .register(&mut registry, RuntimeSlot::stateless())
        .unwrap();
    (host, registry)
}
fn apply<Q: Request>(
    host: &mut GenericHost<State, Event>,
    registry: &Registry<State, Event>,
    request: Q,
) -> Q::Response {
    let prepared = registry
        .prepare(host, SimTime(0), None, |tx| tx.request(request))
        .unwrap();
    registry.commit(host, prepared).unwrap()
}
#[test]
fn review_owns_observation_scoring_deferred_execution_and_failed_retry() {
    let (mut host, registry) = fixture();
    apply(&mut host, &registry, Review);
    let mind = crate::decisions::mind::<u8>(&host.storage, PersonId(1)).unwrap();
    assert_eq!(mind.plan.as_ref().unwrap().action, 3);
    assert_eq!(mind.retry_after, SimTime(120_000));
    assert_eq!(
        mind.explanation.blocker.as_deref(),
        Some("physical attempt failed")
    );
    assert!(mind.routes.contains_key(&AreaId(1)));
}
#[test]
fn newborn_clears_learned_mind_and_changes_only_colocated_parent_affection() {
    let (mut host, registry) = fixture();
    crate::decisions::mind_mut::<u8>(&mut host.storage, PersonId(1))
        .unwrap()
        .interests
        .insert(ChildhoodInterest::Food, 800);
    apply(
        &mut host,
        &registry,
        Birth(ReceiveNewborn {
            child: PersonId(3),
            inventory: InventoryId(4),
            parents: [PersonId(1), PersonId(1)],
            gestator: PersonId(1),
            location: Location::AtArea(AreaId(1)),
        }),
    );
    let child = crate::decisions::mind::<u8>(&host.storage, PersonId(3)).unwrap();
    assert!(
        child.interests.is_empty() && child.routes.is_empty() && child.relationships.is_empty()
    );
    assert_eq!(child.dream, Aspiration::Comfort);
    assert_eq!(
        crate::decisions::mind::<u8>(&host.storage, PersonId(1))
            .unwrap()
            .relationships[&PersonId(3)]
            .affection,
        500
    );
    assert_eq!(
        crate::person(&host.storage, PersonId(3)).unwrap().inventory,
        InventoryId(4)
    );
}
#[test]
fn production_learning_is_sequence_bounded_and_play_preserves_bounded_interests() {
    let (mut host, registry) = fixture();
    apply(
        &mut host,
        &registry,
        LearnProduction {
            sequence: 8,
            actor: PersonId(1),
        },
    );
    let before = host.storage.clone();
    apply(
        &mut host,
        &registry,
        LearnProduction {
            sequence: 8,
            actor: PersonId(1),
        },
    );
    assert_eq!(host.storage, before);
    for _ in 0..12 {
        apply(
            &mut host,
            &registry,
            ChildPlayed {
                actor: PersonId(1),
                interest: ChildhoodInterest::Food,
            },
        );
    }
    assert_eq!(
        crate::decisions::mind::<u8>(&host.storage, PersonId(1))
            .unwrap()
            .interests[&ChildhoodInterest::Food],
        1_000
    );
}
#[test]
fn prepared_disposition_rejects_changed_canonical_mind_atomically() {
    let (mut host, registry) = fixture();
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(SetDisposition {
                actor: PersonId(1),
                personality: Personality::industrious(),
                dream: Aspiration::Parenthood,
            })
        })
        .unwrap();
    crate::decisions::mind_mut::<u8>(&mut host.storage, PersonId(1))
        .unwrap()
        .dream = Aspiration::SupportOthers;
    let before: Store = host.storage.clone();
    assert!(registry.commit(&mut host, prepared).is_err());
    assert_eq!(host.storage, before);
}

struct Tick(game_world::scheduling::ClockSpan);
impl Request for Tick {
    type Response = ();
}
fn tick(
    owner: &Systems<u8>,
    tx: &mut OwnerTxn<'_, '_, Systems<u8>, Event>,
    request: Tick,
) -> Result<(), Error> {
    prepare(owner, tx, request.0)?;
    advance(owner, tx, request.0)
}
#[test]
fn native_cumulative_work_uses_current_lease_and_preserves_fractional_progress() {
    let (mut host, mut registry) = fixture();
    registry
        .register_request::<Systems<u8>, Tick>(tick)
        .unwrap();
    host.storage
        .insert_or_spawn(
            StableId(1),
            game_world::actions::ActionSlot {
                actor: StableId(1),
                owner: "workshop".into(),
                started: SimTime(0),
                completes: None,
            },
        )
        .unwrap();
    apply(
        &mut host,
        &registry,
        Tick(game_world::scheduling::ClockSpan {
            from: SimTime(0),
            to: SimTime(1_000),
        }),
    );
    let person = crate::person(&host.storage, PersonId(1)).unwrap();
    assert_eq!((person.work_units, person.work_remainder), (16, 40_000));
    host.storage
        .remove::<game_world::actions::ActionSlot>(StableId(1))
        .unwrap();
    apply(
        &mut host,
        &registry,
        Tick(game_world::scheduling::ClockSpan {
            from: SimTime(1_000),
            to: SimTime(2_000),
        }),
    );
    let person = crate::person(&host.storage, PersonId(1)).unwrap();
    assert_eq!((person.work_units, person.work_remainder), (16, 40_000));
}

use game_core::{Error, SimTime};
use game_ecs::{Schema, StableId};
use game_world::{
    DueQueue, GenericHost, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot,
    actions::{AcquireAction, ActionSlot, Actions, InterruptActor, ReadAction, ReleaseAction},
};
use serde::{Deserialize, Serialize};

const ACTOR: StableId = StableId(1);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FirstJob;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SecondJob {
    valid: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FirstDue(StableId);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SecondDue(StableId);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
struct State {
    actions: (),
    first: u64,
    second: u64,
    first_due: DueQueue<FirstDue>,
    second_due: DueQueue<SecondDue>,
}
struct First;
struct Second;
struct CancelFirst(StableId);
impl Request for CancelFirst {
    type Response = ();
}
struct CancelSecond(StableId);
impl Request for CancelSecond {
    type Response = ();
}

impl OwnerSystem<u8> for First {
    type Runtime = u64;
    type Input = ();
    type Output = ();
    fn execute(&self, tx: &mut OwnerTxn<'_, '_, Self, u8>, _: ()) -> Result<(), Error> {
        tx.request(AcquireAction {
            actor: ACTOR,
            owner: "first".into(),
            completes: Some(SimTime(20)),
        })?;
        tx.write(ACTOR, FirstJob)?;
        tx.schedule(SimTime(20), FirstDue(ACTOR))?;
        tx.emit(1)?;
        Ok(())
    }
}
impl OwnerSystem<u8> for Second {
    type Runtime = u64;
    type Input = bool;
    type Output = ActionSlot;
    fn execute(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, u8>,
        valid: bool,
    ) -> Result<ActionSlot, Error> {
        tx.request(AcquireAction {
            actor: ACTOR,
            owner: "second".into(),
            completes: Some(SimTime(30)),
        })?;
        // The initiating owner is already active; preserving it must not try
        // its cancellation route or clear the provisional lease.
        assert!(!tx.request(InterruptActor {
            actor: ACTOR,
            preserve: "second".into(),
        })?);
        tx.write(ACTOR, SecondJob { valid })?;
        *tx.runtime_mut() += 1;
        tx.cancel_due::<SecondDue>(|_, due| due.0 == ACTOR)?;
        tx.schedule(SimTime(30), SecondDue(ACTOR))?;
        tx.emit(3)?;
        Ok(tx.request(ReadAction { actor: ACTOR })?.unwrap())
    }
}

fn cancel_first(
    _: &First,
    tx: &mut OwnerTxn<'_, '_, First, u8>,
    request: CancelFirst,
) -> Result<(), Error> {
    // Readonly inspection may reenter the active coordinator. The outgoing
    // lease remains visible until its receiving owner has cancelled its job.
    let lease = tx.request(ReadAction { actor: request.0 })?.unwrap();
    assert_eq!(lease.owner, "first");
    tx.remove::<FirstJob>(request.0)?;
    tx.cancel_due::<FirstDue>(|_, due| due.0 == request.0)?;
    *tx.runtime_mut() += 1;
    tx.emit(2)?;
    Ok(())
}
fn cancel_second(
    _: &Second,
    tx: &mut OwnerTxn<'_, '_, Second, u8>,
    request: CancelSecond,
) -> Result<(), Error> {
    tx.remove::<SecondJob>(request.0)?;
    tx.cancel_due::<SecondDue>(|_, due| due.0 == request.0)?;
    *tx.runtime_mut() += 1;
    tx.emit(4)?;
    Ok(())
}

fn fixture() -> (GenericHost<State, u8>, Registry<State, u8>) {
    let mut schema = Schema::new();
    game_world::actions::register_components(&mut schema).unwrap();
    schema
        .register::<FirstJob>("test/first", |_| Ok(()))
        .unwrap();
    schema
        .register::<SecondJob>("test/second", |job| {
            if job.valid {
                Ok(())
            } else {
                Err(Error::Unavailable)
            }
        })
        .unwrap();
    let mut host = GenericHost::new(schema, 7, State::default());
    assert_eq!(host.allocate_id().unwrap(), ACTOR);
    // Existing actor identity is established before acquiring a lease.
    host.storage.insert_or_spawn(ACTOR, FirstJob).unwrap();
    let mut registry = Registry::new();
    Actions::new()
        .with_route("first", |tx, actor| tx.request(CancelFirst(actor)))
        .unwrap()
        .with_route("second", |tx, actor| tx.request(CancelSecond(actor)))
        .unwrap()
        .register(
            &mut registry,
            RuntimeSlot::new(|state: &State| &state.actions, |state| &mut state.actions),
        )
        .unwrap();
    registry
        .register(
            First,
            RuntimeSlot::new(|state: &State| &state.first, |state| &mut state.first),
        )
        .unwrap();
    registry
        .register(
            Second,
            RuntimeSlot::new(|state: &State| &state.second, |state| &mut state.second),
        )
        .unwrap();
    registry.owns::<First, FirstJob>().unwrap();
    registry.owns::<Second, SecondJob>().unwrap();
    registry
        .register_request::<First, CancelFirst>(cancel_first)
        .unwrap();
    registry
        .register_request::<Second, CancelSecond>(cancel_second)
        .unwrap();
    registry
        .register_due::<First, FirstDue>(RuntimeSlot::new(
            |state: &State| &state.first_due,
            |state| &mut state.first_due,
        ))
        .unwrap();
    registry
        .register_due::<Second, SecondDue>(RuntimeSlot::new(
            |state: &State| &state.second_due,
            |state| &mut state.second_due,
        ))
        .unwrap();
    let start = registry
        .prepare(&host, SimTime(0), None, |tx| tx.call::<First>(()))
        .unwrap();
    registry.commit(&mut host, start).unwrap();
    (host, registry)
}

#[test]
fn switching_leases_cancels_the_owner_atomically_and_release_is_owner_scoped() {
    let (mut host, registry) = fixture();
    let before = host.snapshot().unwrap();
    let rejected = registry
        .prepare(&host, SimTime(5), None, |tx| tx.call::<Second>(false))
        .unwrap();
    // Canonical schema admission fails after the outgoing job, runtime, due
    // queue and events were staged. None of those effects may escape.
    assert_eq!(
        registry.commit(&mut host, rejected),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);

    let switch = registry
        .prepare(&host, SimTime(5), None, |tx| tx.call::<Second>(true))
        .unwrap();
    assert_eq!(host.snapshot().unwrap(), before);
    let lease = registry.commit(&mut host, switch).unwrap();
    assert_eq!(lease.owner, "second");
    assert_eq!(lease.started, SimTime(5));
    assert_eq!(lease.completes, Some(SimTime(30)));
    assert_eq!(host.storage.get::<ActionSlot>(ACTOR), Some(&lease));
    assert!(host.storage.get::<FirstJob>(ACTOR).is_none());
    assert_eq!(
        host.storage.get::<SecondJob>(ACTOR),
        Some(&SecondJob { valid: true })
    );
    assert_eq!(host.state.first, 1);
    assert_eq!(host.state.second, 1);
    assert!(host.state.first_due.0.is_empty());
    assert_eq!(
        host.state.second_due.0.get(&SimTime(30)),
        Some(&vec![SecondDue(ACTOR)])
    );
    assert_eq!(
        host.events()
            .iter()
            .map(|row| row.event)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );

    let foreign_release = registry
        .prepare(&host, SimTime(5), None, |tx| {
            tx.request(ReleaseAction {
                actor: ACTOR,
                owner: "first".into(),
            })
        })
        .unwrap();
    assert!(!registry.commit(&mut host, foreign_release).unwrap());
    assert_eq!(host.storage.get::<ActionSlot>(ACTOR), Some(&lease));

    // Renewing a lease from its active owner must not invoke that same owner's
    // receiving cancellation request. The owner replaces its own due work.
    let renew = registry
        .prepare(&host, SimTime(6), None, |tx| tx.call::<Second>(true))
        .unwrap();
    let renewed = registry.commit(&mut host, renew).unwrap();
    assert_eq!(renewed.started, SimTime(6));
    assert_eq!(host.state.second, 2);
    assert_eq!(
        host.events()
            .iter()
            .map(|row| row.event)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 3]
    );
    assert_eq!(
        host.state.second_due.0.get(&SimTime(30)),
        Some(&vec![SecondDue(ACTOR)])
    );

    let release = registry
        .prepare(&host, SimTime(6), None, |tx| {
            tx.request(ReleaseAction {
                actor: ACTOR,
                owner: "second".into(),
            })
        })
        .unwrap();
    assert!(registry.commit(&mut host, release).unwrap());
    assert!(host.storage.get::<ActionSlot>(ACTOR).is_none());
}

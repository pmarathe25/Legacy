use game_core::{Error, SimDuration, SimTime};
use game_ecs::{Schema, StableId};
use game_sim::{
    Clock, ClockWorld, Engine, Execution, Executor, Interval, Module, ModuleId, PhaseId, PhaseKey,
    Transaction,
};
use game_world::{GenericHost, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Tally(u64);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Reserve(u64);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Metadata {
    tally_rate: u64,
    reserve_rate: u64,
    intervals: u64,
    stop_at: Option<SimTime>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Signal {
    Sample { lane: u8, id: StableId, value: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Suspend {
    at: SimTime,
}
type Host = GenericHost<Metadata, Signal>;

fn schema() -> Schema {
    let mut schema = Schema::new();
    schema.register::<Tally>("test/tally", |_| Ok(())).unwrap();
    schema
        .register::<Reserve>("test/reserve", |_| Ok(()))
        .unwrap();
    schema
}
fn fixture() -> Host {
    let mut host = Host::new(
        schema(),
        17,
        Metadata {
            tally_rate: 3,
            reserve_rate: 7,
            intervals: 0,
            stop_at: None,
        },
    );
    for index in 0..512_u64 {
        let id = host.allocate_id().unwrap();
        host.parts_mut()
            .store
            .insert_pair_or_spawn(id, Tally(index), Reserve(index * 2 + 1))
            .unwrap();
    }
    host
}
fn boundary(host: &Host, interval: Interval) -> Result<SimTime, Error> {
    let mut at = SimTime(interval.from.0.checked_add(10).ok_or(Error::Overflow)?).min(interval.to);
    if let Some(stop) = host.parts().state.stop_at.filter(|at| *at > interval.from) {
        at = at.min(stop);
    }
    Ok(at)
}
fn input_stop(host: &Host, clock: &Clock) -> Result<Option<Suspend>, Error> {
    Ok(host
        .parts()
        .state
        .stop_at
        .filter(|at| *at <= clock.time)
        .map(|_| Suspend { at: clock.time }))
}
fn engine(workers: usize, reverse_jobs: bool) -> Engine<Host, Suspend> {
    let key = PhaseKey {
        module: ModuleId(10),
        phase: PhaseId(1),
    };
    let mut first = Module::new(ModuleId(10)).phase(PhaseId(1), []);
    first.boundary = Some(boundary);
    let mut second = Module::new(ModuleId(20)).phase(PhaseId(2), [key]);
    second.input_stop = Some(input_stop);
    Engine::new(
        [second, first],
        Execution {
            workers,
            reverse_jobs,
        },
    )
    .unwrap()
}
struct Prepared {
    tally: Vec<(StableId, u64)>,
    reserve: Vec<(StableId, u64)>,
    intervals: u64,
    signals: Vec<Signal>,
}
impl Transaction<Host, Suspend> for Prepared {
    fn revision_budget(&self) -> u64 {
        self.signals.len() as u64
    }
    fn commit(self, host: &mut Host) -> Option<Suspend> {
        let parts = host.parts_mut();
        for (id, value) in self.tally {
            parts.store.get_mut::<Tally>(id).unwrap().0 = value;
        }
        for (id, value) in self.reserve {
            parts.store.get_mut::<Reserve>(id).unwrap().0 = value;
        }
        parts.state.intervals = self.intervals;
        host.publish_many(self.signals)
            .expect("the whole publication budget was preflighted");
        None
    }
}
fn prepare(
    host: &Host,
    interval: Interval,
    phases: &[PhaseKey],
    executor: &Executor,
) -> Result<Prepared, Error> {
    assert_eq!(
        phases,
        &[
            PhaseKey {
                module: ModuleId(10),
                phase: PhaseId(1)
            },
            PhaseKey {
                module: ModuleId(20),
                phase: PhaseId(2)
            },
        ]
    );
    let parts = host.parts();
    let a = interval
        .elapsed()
        .0
        .checked_mul(parts.state.tally_rate)
        .ok_or(Error::Overflow)?;
    let b = interval
        .elapsed()
        .0
        .checked_mul(parts.state.reserve_rate)
        .ok_or(Error::Overflow)?;
    let tally: Vec<_> = parts
        .store
        .iter::<Tally>()
        .map(|(id, row)| (id, row.0))
        .collect();
    let reserve: Vec<_> = parts
        .store
        .iter::<Reserve>()
        .map(|(id, row)| (id, row.0))
        .collect();
    let tally = executor.map_keyed(&tally, |value| value.checked_add(a).ok_or(Error::Overflow))?;
    let reserve = executor.map_keyed(&reserve, |value| {
        value.checked_add(b).ok_or(Error::Overflow)
    })?;
    let intervals = parts
        .state
        .intervals
        .checked_add(1)
        .ok_or(Error::Overflow)?;
    let signals: Vec<_> = tally
        .iter()
        .map(|(id, value)| Signal::Sample {
            lane: 0,
            id: *id,
            value: *value,
        })
        .chain(reserve.iter().map(|(id, value)| Signal::Sample {
            lane: 1,
            id: *id,
            value: *value,
        }))
        .collect();
    host.preflight(
        0,
        u64::try_from(signals.len()).map_err(|_| Error::Overflow)?,
    )?;
    Ok(Prepared {
        tally,
        reserve,
        intervals,
        signals,
    })
}
fn validate_context(host: &Host) -> Result<(), Error> {
    let parts = host.parts();
    let tally: BTreeSet<_> = parts.store.iter::<Tally>().map(|(id, _)| id).collect();
    let reserve: BTreeSet<_> = parts.store.iter::<Reserve>().map(|(id, _)| id).collect();
    if tally != reserve || parts.state.tally_rate == 0 || parts.state.reserve_rate == 0 {
        return Err(Error::InvalidSave(
            "test composition is inconsistent".into(),
        ));
    }
    Ok(())
}

#[test]
fn externally_supplied_components_modules_and_typed_events_are_worker_deterministic() {
    let mut baseline = fixture();
    engine(1, false)
        .advance(&mut baseline, SimDuration(30), prepare)
        .unwrap();
    assert_eq!(baseline.parts().state.intervals, 3);
    assert_eq!(baseline.events().len(), 3_072);
    assert_eq!(baseline.clock().revision, 3_075);
    assert_eq!(
        baseline.parts().store.get::<Tally>(StableId(1)),
        Some(&Tally(90))
    );
    assert_eq!(
        baseline.parts().store.get::<Reserve>(StableId(1)),
        Some(&Reserve(211))
    );
    for workers in [1, 2, 4] {
        for reverse in [false, true] {
            let mut host = fixture();
            engine(workers, reverse)
                .advance(&mut host, SimDuration(30), prepare)
                .unwrap();
            assert_eq!(host.snapshot().unwrap(), baseline.snapshot().unwrap());
            assert_eq!(host.clock(), baseline.clock());
        }
    }
}

#[test]
fn canonical_snapshot_restore_continues_through_supplied_schema_and_context() {
    let mut first = fixture();
    engine(2, true)
        .advance(&mut first, SimDuration(10), prepare)
        .unwrap();
    let bytes = serde_json::to_vec(&first.snapshot().unwrap()).unwrap();
    let snapshot: Snapshot<Metadata, Signal> = serde_json::from_slice(&bytes).unwrap();
    let mut loaded = Host::restore(schema(), snapshot, validate_context).unwrap();
    assert_eq!(loaded.clock().revision, 0);
    assert_eq!(loaded.snapshot().unwrap(), first.snapshot().unwrap());
    engine(4, true)
        .advance(&mut loaded, SimDuration(20), prepare)
        .unwrap();
    engine(1, false)
        .advance(&mut first, SimDuration(20), prepare)
        .unwrap();
    assert_eq!(loaded.snapshot().unwrap(), first.snapshot().unwrap());
}

#[test]
fn typed_input_stop_uses_the_existing_engine_and_leaves_host_data_unchanged_at_the_stop() {
    let mut host = fixture();
    host.parts_mut().state.stop_at = Some(SimTime(15));
    let result = engine(2, true)
        .advance(&mut host, SimDuration(100), prepare)
        .unwrap();
    assert_eq!(result.reached, SimTime(15));
    assert_eq!(result.input, Some(Suspend { at: SimTime(15) }));
    let before = host.snapshot().unwrap();
    let clock = host.clock();
    let result = engine(1, false)
        .advance(&mut host, SimDuration(1), prepare)
        .unwrap();
    assert!(result.input.is_some());
    assert_eq!(host.snapshot().unwrap(), before);
    assert_eq!(host.clock(), clock);
}

#[test]
fn counter_or_arithmetic_failure_precedes_every_mutation_and_publication() {
    let mut host = fixture();
    let before = host.snapshot().unwrap();
    assert_eq!(host.preflight(u64::MAX, 0), Err(Error::Overflow));
    assert_eq!(host.preflight(0, u64::MAX), Err(Error::Overflow));
    assert_eq!(host.snapshot().unwrap(), before);
    let mut full_ids = before.clone();
    full_ids.next_id = u64::MAX;
    let mut full_ids = Host::restore(schema(), full_ids, validate_context).unwrap();
    let before = full_ids.snapshot().unwrap();
    assert_eq!(full_ids.allocate_id(), Err(Error::Overflow));
    assert_eq!(full_ids.snapshot().unwrap(), before);

    host.parts_mut()
        .store
        .get_mut::<Tally>(StableId(1))
        .unwrap()
        .0 = u64::MAX;
    let before = host.snapshot().unwrap();
    let clock = host.clock();
    assert_eq!(
        engine(4, true).advance(&mut host, SimDuration(10), prepare),
        Err(Error::Overflow)
    );
    assert_eq!(host.snapshot().unwrap(), before);
    assert_eq!(host.clock(), clock);

    host.publish_clock(Clock {
        revision: u64::MAX,
        ..clock
    });
    let before = host.snapshot().unwrap();
    assert_eq!(
        host.publish_many(vec![Signal::Sample {
            lane: 0,
            id: StableId(1),
            value: 0
        }]),
        Err(Error::Overflow)
    );
    assert_eq!(host.snapshot().unwrap(), before);
    assert_eq!(host.clock().revision, u64::MAX);
}

#[test]
fn malformed_host_or_contextual_snapshots_never_change_a_live_host() {
    let mut host = fixture();
    engine(1, false)
        .advance(&mut host, SimDuration(10), prepare)
        .unwrap();
    let before = host.snapshot().unwrap();
    let mutations: [fn(&mut Snapshot<Metadata, Signal>); 7] = [
        |snapshot| snapshot.version = 2,
        |snapshot| snapshot.next_id = 0,
        |snapshot| snapshot.next_id = 512,
        |snapshot| snapshot.next_event += 1,
        |snapshot| snapshot.events[0].sequence = 0,
        |snapshot| snapshot.events[0].at = SimTime(11),
        |snapshot| snapshot.state.tally_rate = 0,
    ];
    for mutate in mutations {
        let mut snapshot = before.clone();
        mutate(&mut snapshot);
        assert!(matches!(
            Host::restore(schema(), snapshot, validate_context),
            Err(Error::InvalidSave(_))
        ));
        assert_eq!(host.snapshot().unwrap(), before);
    }
}

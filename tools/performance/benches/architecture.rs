//! Architectural kernels, not a replacement simulation or predicted whole-game gain.
#[path = "architecture/messages.rs"]
mod messages;
#[path = "architecture/stores.rs"]
mod stores;
#[path = "architecture/temporal.rs"]
mod temporal;
use game_characters::{Candidate, choose};
use game_core::*;
use game_knowledge::{Fact, FactValue, Stock, StoreLocation};
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Instant};
use stores::*;
#[derive(Serialize, Deserialize)]
struct Snapshot {
    actors: Vec<Actor>,
    stock: Vec<u64>,
    events: Vec<(u64, PersonId)>,
    cold: Table<Arc<str>, Fact>,
    next_id: u64,
}
fn stocks<S: Store>(s: &S) -> Vec<u64> {
    (0..s.world().sources.len()).map(|i| s.stock(i)).collect()
}
fn refresh<S: Store>(
    s: &mut S,
    id: PersonId,
    key: usize,
    value: Arc<FactValue>,
    at: SimTime,
    revision: u64,
) {
    s.set_fact(
        id,
        key,
        Fact {
            value,
            source: id,
            observed: at,
            revision,
            confidence: 100,
        },
    );
}
fn inspect<S: Store>(s: &mut S, id: PersonId, at: SimTime, revision: u64) {
    let bin = s.world().bin(id);
    let stock = s.stock(bin);
    refresh(
        s,
        id,
        2,
        Arc::new(FactValue::Stock(Table::from([(game_cargo::FOOD, stock)]))),
        at,
        revision,
    );
    refresh(
        s,
        id,
        3,
        Arc::new(FactValue::Location(StoreLocation {
            area: AreaId(bin as u64 + 1),
            carrier: None,
            capacity: 1_000_000_000,
            mass: stock,
        })),
        at,
        revision,
    );
}
fn review<S: Store>(
    s: &mut S,
    id: PersonId,
    at: SimTime,
    revision: u64,
    events: &mut Vec<(u64, PersonId)>,
) {
    let values = [
        s.world().source(id).values[0].clone(),
        s.world().source(id).values[1].clone(),
    ];
    for (i, v) in values.into_iter().enumerate() {
        refresh(s, id, i, v, at, revision);
    }
    let body = s.body(id);
    let mut mind = s.mind(id);
    let remembered = s.fact(id, 2).value.decode::<Stock>().unwrap()[&game_cargo::FOOD];
    let options = [
        Candidate {
            id: 1,
            need: body.fatigue.min(i64::MAX as u64) as i64,
            goal: 0,
            value: 0,
            effort: 1,
            risk: 0,
            survival: body.hunger < 100,
        },
        Candidate {
            id: 2,
            need: body.hunger.min((i64::MAX / 2) as u64) as i64 * 2,
            goal: 0,
            value: 0,
            effort: 0,
            risk: 0,
            survival: true,
        },
        Candidate {
            id: 3,
            need: body.hunger.min(i64::MAX as u64) as i64,
            goal: 5,
            value: (id.0 % 7) as i64,
            effort: 10,
            risk: 1,
            survival: true,
        },
        Candidate {
            id: 4,
            need: 0,
            goal: 2,
            value: 3,
            effort: 2,
            risk: 0,
            survival: false,
        },
    ];
    let decision = choose(
        &options,
        (mind.last != 0).then_some(mind.last),
        5,
        body.hunger >= 100,
    )
    .unwrap();
    mind.last = decision.id;
    mind.score = decision.score;
    s.set_mind(id, mind);
    if decision.id == 1 || body.fatigue == 0 {
        let mut b = body;
        b.resting = decision.id == 1;
        s.set_body(id, b);
    }
    if decision.id != 2 || body.hunger < 30 || remembered < 10 {
        return;
    }
    let bin = s.world().bin(id);
    if s.stock(bin) < 10 {
        return;
    }
    let attempt = s.mark();
    let mut next = s.body(id);
    next.hunger = next.hunger.saturating_sub(60);
    s.set_body(id, next);
    s.set_stock(bin, s.stock(bin) - 10);
    inspect(s, id, at, revision);
    // Exercise failure after multiple owner-like writes, including a nested savepoint.
    let nested = s.mark();
    let mut b = s.body(id);
    b.health = b.health.saturating_sub(1);
    s.set_body(id, b);
    s.rollback(nested);
    if (id.0 + at.0 / MINUTE.0).is_multiple_of(37) {
        s.rollback(attempt);
        let mut m = s.mind(id);
        m.failures += 1;
        s.set_mind(id, m);
    } else {
        s.commit(attempt);
        events.push((at.0, id));
    }
}
fn phase<S: Store>(
    s: &mut S,
    ids: &[PersonId],
    at: SimTime,
    revision: u64,
    events: &mut Vec<(u64, PersonId)>,
    fault: bool,
) {
    let before = fault.then(|| ids.iter().map(|id| s.actor(*id)).collect::<Vec<_>>());
    let stock_before = fault.then(|| stocks(s));
    let event_mark = events.len();
    let mark = s.mark();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for (i, id) in ids.iter().enumerate() {
            review(s, *id, at, revision, events);
            if fault && i == ids.len() / 2 {
                panic!("injected phase failure");
            }
        }
        s.validate(&mark);
    }));
    if let Err(error) = result {
        s.rollback(mark);
        events.truncate(event_mark);
        if !fault {
            std::panic::resume_unwind(error);
        }
        assert_eq!(
            ids.iter().map(|id| s.actor(*id)).collect::<Vec<_>>(),
            before.unwrap()
        );
        assert_eq!(stocks(s), stock_before.unwrap());
        phase(s, ids, at, revision, events, false);
    } else {
        s.commit(mark);
    }
}
fn structural<S: Store>(s: &mut S, ids: &mut Vec<PersonId>, at: SimTime, next_id: &mut u64) {
    let retired: Vec<_> = ids.iter().step_by(100).copied().collect();
    let mut next = *next_id;
    for chunk in retired.chunks(128) {
        let old: Vec<_> = chunk.iter().map(|id| s.actor(*id)).collect();
        let stock_before = stocks(s);
        let next_before = next;
        let test = s.mark();
        for id in chunk {
            s.remove(*id);
            let a = s.world().actor(PersonId(next), at);
            s.add(a);
            next += 7;
        }
        s.rollback(test);
        next = next_before;
        assert_eq!(old, chunk.iter().map(|id| s.actor(*id)).collect::<Vec<_>>());
        assert_eq!(stock_before, stocks(s));
        let mark = s.mark();
        for id in chunk {
            s.remove(*id);
            let id = PersonId(next);
            next += 7;
            let a = s.world().actor(id, at);
            s.add(a);
            inspect(s, id, at, at.0);
        }
        s.validate(&mark);
        s.commit(mark);
    }
    *ids = s.ids();
    *next_id = next;
}
fn run<S: Store>(
    name: &str,
    world: Arc<World>,
    n: usize,
    workers: usize,
    active: usize,
    steps: u64,
    repeat: usize,
) -> u64 {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    let initial: Vec<_> = (0..n)
        .map(|i| world.actor(PersonId(3 + i as u64 * 7), SimTime(0)))
        .collect();
    let init = Instant::now();
    let mut s = S::new(
        world.clone(),
        initial,
        vec![world.stock; world.sources.len()],
    );
    let init_ms = init.elapsed().as_secs_f64() * 1000.;
    let mut ids = s.ids();
    let original = ids.clone();
    let mut next_id = ids.last().unwrap().0 + 1_000_003;
    let mut events = vec![];
    let mut physiology = 0.;
    let mut reviews = 0.;
    let mut structural_ms = 0.;
    let mut checkpoint = 0.;
    let mut freeze_ms = 0.;
    let mut reader = None;
    let sample = game_performance::metrics::Sample::new();
    for tick in 1..=steps {
        let at = SimTime(tick * MINUTE.0);
        let t = Instant::now();
        s.advance(&pool, at);
        physiology += t.elapsed().as_secs_f64() * 1000.;
        if tick == 1 {
            let t = Instant::now();
            reader = Some(s.freeze());
            freeze_ms = t.elapsed().as_secs_f64() * 1000.;
        }
        let t = Instant::now();
        let due: Vec<_> = ids
            .iter()
            .copied()
            .filter(|id| id.0 % active as u64 == tick % active as u64)
            .collect();
        for (i, chunk) in due.chunks(128).enumerate() {
            phase(
                &mut s,
                chunk,
                at,
                tick * 1_000_000 + i as u64,
                &mut events,
                tick == 7 && i == 0,
            );
        }
        reviews += t.elapsed().as_secs_f64() * 1000.;
        if tick == steps / 3 || tick == 2 * steps / 3 {
            let t = Instant::now();
            structural(&mut s, &mut ids, at, &mut next_id);
            structural_ms += t.elapsed().as_secs_f64() * 1000.;
        }
        if tick == steps / 2 + 1 {
            let t = Instant::now();
            let snap = Snapshot {
                actors: s.actors(),
                stock: stocks(&s),
                events: events.clone(),
                cold: world.cold.clone(),
                next_id,
            };
            let bytes = serde_json::to_vec(&snap).unwrap();
            let mut loaded: Snapshot = serde_json::from_slice(&bytes).unwrap();
            normalize(&world, &mut loaded.actors);
            assert_eq!(loaded.actors, snap.actors);
            assert_eq!(loaded.cold, snap.cold);
            assert_eq!(loaded.stock, snap.stock);
            assert_eq!(loaded.events, snap.events);
            s = S::new(world.clone(), loaded.actors, loaded.stock);
            events = loaded.events;
            next_id = loaded.next_id;
            ids = s.ids();
            checkpoint = t.elapsed().as_secs_f64() * 1000.;
        }
    }
    sample.report("architectural_kernel");
    game_performance::metrics::rss();
    // The retained committed reader survives writes, structural compaction, and loading.
    let frozen = S::read(&reader.unwrap());
    assert_eq!(frozen.len(), n);
    for (id, a) in original.iter().zip(frozen) {
        let mut expected = world.actor(*id, SimTime(0));
        expected.body =
            game_physiology::advance_body(&expected.body, &world.rules, SimTime(MINUTE.0));
        assert_eq!(a, expected);
    }
    assert_eq!(s.ids().len(), n);
    let remaining: u64 = stocks(&s).iter().sum();
    assert_eq!(
        remaining + events.len() as u64 * 10,
        world.stock * world.sources.len() as u64
    );
    let snap = Snapshot {
        actors: s.actors(),
        stock: stocks(&s),
        events,
        cold: world.cold.clone(),
        next_id,
    };
    let bytes = serde_json::to_vec(&snap).unwrap();
    let hash = game_performance::hash(&bytes);
    let advance = physiology + reviews + structural_ms;
    println!(
        "architecture={name} population={n} workers={workers} active_divisor={active} history={} bins={} steps={steps} repeat={repeat} advance_ms={advance:.3} physiology_ms={physiology:.3} review_ms={reviews:.3} structural_ms={structural_ms:.3} freeze_ms={freeze_ms:.3} checkpoint_ms={checkpoint:.3} initialize_ms={init_ms:.3} logical_bytes={} hash={hash:016x}",
        world.cold.len(),
        world.sources.len(),
        bytes.len()
    );
    hash
}
fn main() {
    use game_performance::setting;
    game_performance::metrics::metadata();
    let n = setting("LEGACY_ARCH_POPULATION", 20_000usize);
    let history = setting("LEGACY_ARCH_HISTORY", 1000usize);
    let bins = setting("LEGACY_ARCH_BINS", 32usize);
    let active = setting("LEGACY_ARCH_ACTIVE", 4usize);
    let steps = setting("LEGACY_ARCH_STEPS", 120u64);
    let repeats = setting("LEGACY_ARCH_REPEATS", 3usize);
    assert!(n >= 15 && bins > 0 && active > 0 && steps >= 16 && repeats > 0);
    for _ in 0..repeats {
        messages::measure(n);
    }
    if setting("LEGACY_ARCH_MESSAGES_ONLY", false) {
        return;
    }
    let world = World::new(n, history, bins);
    temporal::verify();
    temporal::measure(n);
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info.payload().downcast_ref::<&str>() != Some(&"injected phase failure") {
            hook(info);
        }
    }));
    let mut reference = None;
    for workers in [1, 4] {
        for repeat in 0..repeats {
            for hash in [
                run::<Persistent>(
                    "persistent",
                    world.clone(),
                    n,
                    workers,
                    active,
                    steps,
                    repeat,
                ),
                run::<Packed>(
                    "packed_journal",
                    world.clone(),
                    n,
                    workers,
                    active,
                    steps,
                    repeat,
                ),
            ] {
                if let Some(r) = reference {
                    assert_eq!(hash, r);
                } else {
                    reference = Some(hash);
                }
            }
        }
    }
}

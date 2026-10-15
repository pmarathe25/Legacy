//! Storage protocol workload; the economy and latency benches exercise real features.
use game_core::*;
use game_physiology::{Body, Kind, Rules, advance_body};
use game_storage::{Records, Table};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Instant};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Person {
    id: u64,
    body: Body,
    area: u32,
    target: u32,
    arrives: u64,
    progress: u64,
}
struct Dense {
    rows: Table<u64, Person>,
    members: Table<u64, ()>,
}
impl Dense {
    fn from_rows(rows: Vec<Person>) -> Self {
        Self {
            rows: rows.into_iter().map(|p| (p.id, p)).collect(),
            members: Table::new(),
        }
    }
    fn rows(&mut self) -> Vec<Person> {
        self.rows.values().cloned().collect()
    }
    fn commit(&mut self, rows: Vec<Person>) {
        let mut next = self.rows.clone();
        for p in rows {
            next.insert(p.id, p);
        }
        self.rows = next;
    }
    fn replace(&mut self, id: u64, row: Option<Person>) {
        if let Some(p) = row {
            self.rows.insert(id, p);
        } else {
            self.rows.remove(&id);
            self.members.remove(&id);
        }
    }
    fn one(&self, id: u64) -> Person {
        self.rows[&id].clone()
    }
    fn member(&mut self, id: u64, yes: bool) {
        if yes {
            self.members.insert(id, ());
        } else {
            self.members.remove(&id);
        }
    }
}
fn rows(n: u64) -> Vec<Person> {
    (1..=n)
        .map(|id| Person {
            id,
            body: Body {
                kind: Kind::Mortal,
                born: SimTime(0),
                alive: true,
                hunger: id % 25,
                fatigue: id % 10,
                health: 100,
                resting: false,
                last: SimTime(0),
                life_limit: None,
            },
            area: (id % 32) as u32,
            target: ((id + 1) % 32) as u32,
            arrives: 3 + id % 7,
            progress: 0,
        })
        .collect()
}
fn run(n: u64, workers: usize, contested: bool, round: usize) -> (Vec<Person>, u64) {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    let mut s = Dense::from_rows(rows(n));
    let known: Records<u64, Vec<u64>> = (1..=n)
        .map(|id| (id, (0..4).map(|k| 1 + (id + k) % n).collect()))
        .collect();
    let relations: Records<u64, BTreeMap<u64, i64>> = (1..=n)
        .map(|id| (id, [(1 + (id + 1) % n, 3), (1 + (id + 2) % n, -2)].into()))
        .collect();
    let mut stock = vec![100u64; if contested { 1 } else { n as usize }];
    let initial: u64 = stock.iter().sum();
    let mut delivered = 0u64;
    let mut events = vec![];
    let rules = Rules::default();
    let begin = Instant::now();
    let mut pack = 0.;
    let mut eval = 0.;
    let mut commit = 0.;
    let mut structural = 0.;
    let mut latencies = vec![];
    let sample = game_performance::metrics::Sample::new();
    for tick in 1..=60 {
        let t = Instant::now();
        let input = s.rows();
        pack += t.elapsed().as_secs_f64();
        let t = Instant::now();
        let mut output: Vec<_> = pool.install(|| {
            input
                .par_iter()
                .map(|p| {
                    let mut next = p.clone();
                    next.body = advance_body(&p.body, &rules, SimTime(tick * MINUTE.0));
                    let mut best = i64::MIN;
                    for k in &known[&p.id] {
                        let affinity = relations[&p.id].get(k).copied().unwrap_or(0);
                        let score = next.body.hunger as i64 * 3 + affinity - (*k % 13) as i64;
                        if score > best {
                            best = score;
                        }
                    }
                    next.progress = next.progress.wrapping_add(best.max(0) as u64);
                    if next.arrives == tick {
                        next.area = next.target;
                        next.target = (next.target + 1) % 32;
                        next.arrives += 7;
                    }
                    next
                })
                .collect()
        });
        // Deliberately perturb completion/extraction order; arbitration never uses it.
        if workers.is_multiple_of(2) {
            output.reverse();
        }
        output.sort_unstable_by_key(|p| p.id);
        eval += t.elapsed().as_secs_f64();
        let t = Instant::now();
        for p in &mut output {
            let bin = if contested { 0 } else { p.id as usize - 1 };
            if tick % 5 == 0 && stock[bin] > 0 {
                stock[bin] -= 1;
                delivered += 1;
                p.body.hunger = p.body.hunger.saturating_sub(2);
                events.push((tick, p.id));
            }
        }
        s.commit(output);
        commit += t.elapsed().as_secs_f64();
        if tick == 30 {
            let t = Instant::now();
            for id in (1..=n).step_by(100) {
                s.member(id, true);
            }
            for id in (1..=n).step_by(100) {
                s.member(id, false);
                let p = s.one(id);
                s.replace(id, None);
                s.replace(id, Some(p));
            }
            structural += t.elapsed().as_secs_f64();
        }
        let t = Instant::now();
        let mut p = s.one(1);
        p.progress += 1;
        s.commit(vec![p]);
        latencies.push(t.elapsed().as_nanos());
        if tick == 31 {
            let mut r = s.rows();
            r.sort_by_key(|p| p.id);
            let json = serde_json::to_string(&r).unwrap();
            let loaded = serde_json::from_str(&json).unwrap();
            s = Dense::from_rows(loaded);
        }
    }
    let elapsed = begin.elapsed().as_secs_f64();
    sample.report("storage_workload");
    latencies.sort();
    let mut canonical = s.rows();
    canonical.sort_by_key(|p| p.id);
    assert_eq!(canonical.len(), n as usize);
    assert_eq!(stock.iter().sum::<u64>() + delivered, initial);
    assert!(
        canonical
            .iter()
            .all(|p| p.body.alive && p.body.health == 100 && p.body.last == SimTime(60 * MINUTE.0))
    );
    let json = serde_json::to_vec(&(&canonical, &stock, &events)).unwrap();
    let hash = json.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    });
    println!(
        "candidate=dense n={n} workers={workers} contested={contested} repeat={round} areas=32 progress_records={} shipments=0 relationships={} facts={} institutions=0 agreements=0 conflicts=0 history={} elapsed_ms={:.3} pack_ms={:.3} eval_ms={:.3} commit_ms={:.3} structural_ms={:.3} command_p95_us={:.3} command_p99_us={:.3} save_bytes={} hash={hash:016x}",
        n,
        n * 2,
        n * 4,
        events.len(),
        elapsed * 1000.,
        pack * 1000.,
        eval * 1000.,
        commit * 1000.,
        structural * 1000.,
        latencies[56] as f64 / 1000.,
        latencies[59] as f64 / 1000.,
        json.len()
    );
    (canonical, hash)
}
fn main() {
    game_performance::metrics::metadata();
    let budget = game_performance::setting(
        "LEGACY_WORKERS",
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(8),
    );
    let mut worker_counts = vec![1, 2, 4, budget];
    worker_counts.sort();
    worker_counts.dedup();
    for n in [15, 1000, 10_000, 20_000] {
        for contested in [false, true] {
            let mut reference = None;
            for workers in &worker_counts {
                for repeat in 0..3 {
                    let value = run(n, *workers, contested, repeat);
                    if let Some(r) = &reference {
                        assert_eq!(&value, r);
                    } else {
                        reference = Some(value);
                    }
                }
            }
        }
    }
    game_performance::metrics::rss();
}

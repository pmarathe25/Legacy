//! Args: workers population ticks family|unmatched|partnered. Export/equality outside timing.
use game_parallel_families::{Families, FamilyRules, FamilyTotals, Genome, Member, Sex};
use game_parallel_family_lifecycle::Runtime;
use game_parallel_family_model::{Birth, LifeInput, PhysicalView};
use std::time::Instant;
struct View {
    rows: Vec<LifeInput>,
}
impl PhysicalView for View {
    fn read(&self, id: u64) -> LifeInput {
        self.rows[id as usize - 1]
    }
}
impl View {
    fn update(&mut self, births: &[Birth], deaths: impl IntoIterator<Item = u64>) {
        for birth in births {
            self.rows.push(LifeInput {
                id: birth.id,
                region: birth.region,
                health: birth.health,
                hunger: 0,
            });
        }
        for id in deaths {
            self.rows[id as usize - 1].health = 0;
        }
    }
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let workers: usize = args.first().map_or(1, |v| v.parse().unwrap());
    let count: u64 = args.get(1).map_or(100000, |v| v.parse().unwrap());
    let ticks: u64 = args.get(2).map_or(8, |v| v.parse().unwrap());
    let mode = args.get(3).map_or("family", String::as_str);
    assert!(count.is_multiple_of(2) && count > 0);
    let members = (0..count)
        .map(|i| Member {
            id: i + 1,
            born: None,
            age: 18,
            lifespan_loss: 0,
            genome: Genome {
                sex: if mode == "unmatched" || i % 2 == 0 {
                    Sex::Female
                } else {
                    Sex::Male
                },
                alleles: [[1000, 1000]; 4],
            },
            parents: Default::default(),
            partner: (mode == "partnered").then_some((i ^ 1) + 1),
            pregnancy: None,
            next_conception: 0,
            alive: true,
            reproduction_allowed: true,
            reconsider_partnership: false,
        })
        .collect();
    let families = Families {
        rules: FamilyRules {
            adult_age: 12,
            min_work_age: 0,
            fertile_until: 40,
            gestation: 3,
            birth_spacing: 8,
            lifespan_min: 50,
            lifespan_max: 70,
            conception_one_in: if mode == "partnered" { u64::MAX } else { 1 },
            minimum_health: 50,
            maximum_hunger: 8,
            newborn_health: 100,
            mutation_per_million: 10000,
            mutation_step: 10,
            enroll_births: true,
        },
        next_id: count + 1,
        members,
        last_tick: 0,
        totals: FamilyTotals::default(),
        events: vec![],
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    pool.install(|| {
        let mut expected = families.clone();
        let mut physical = View { rows: (1..=count).map(|id| LifeInput { id, region: Some(1), health: 100, hunger: 0 }).collect() };
        let start = Instant::now();
        for tick in 1..=ticks {
            let draft = expected.advance_validated(&physical, 42, tick).unwrap();
            physical.update(&draft.births, draft.deaths);
            expected = draft.families;
        }
        let reference_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        let mut runtime = Runtime::compile_validated(&families).unwrap();
        let import_ms = start.elapsed().as_secs_f64() * 1000.;
        let mut physical = View { rows: (1..=count).map(|id| LifeInput { id, region: Some(1), health: 100, hunger: 0 }).collect() };
        let start = Instant::now();
        let mut prepare_ms = 0.;
        let mut publish_ms = 0.;
        for tick in 1..=ticks {
            let phase = Instant::now();
            let draft = runtime.prepare(&physical, 42, tick, None).unwrap();
            prepare_ms += phase.elapsed().as_secs_f64() * 1000.;
            physical.update(&draft.births, draft.deaths.iter().copied());
            let phase = Instant::now();
            runtime.commit(draft).unwrap();
            publish_ms += phase.elapsed().as_secs_f64() * 1000.;
        }
        let column_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(runtime.export(), expected);
        println!("workers={workers} people={count} ticks={ticks} mode={mode} reference_ms={reference_ms:.3} column_ms={column_ms:.3} prepare_ms={prepare_ms:.3} publish_ms={publish_ms:.3} import_ms={import_ms:.3} births={}", expected.totals.births);
    });
}

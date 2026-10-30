//! Args: workers, even population, hot|spread, stable|casualty|command, repetitions, [fatigue].
//! Reports full tick staging/commit; casualty filtering and command rebuilds are timed.
use game_parallel::{
    CombatCommand, CombatEncounter, CombatFighter, CombatOrder, Conflict, Person, World,
};
use game_parallel_fused_physical::Kernel;
use std::time::Instant;
#[path = "support/acquire_food_bench.rs"]
mod acquire_food_bench;
#[path = "support/cooperation_bench.rs"]
mod cooperation_bench;
#[path = "support/diet_bench.rs"]
mod diet_bench;
#[path = "support/food_aid_bench.rs"]
mod food_aid_bench;
#[path = "support/food_request_bench.rs"]
mod food_request_bench;
#[path = "support/local_food_bench.rs"]
mod local_food_bench;
#[path = "support/social_pressure_bench.rs"]
mod social_pressure_bench;
#[path = "support/work_request_bench.rs"]
mod work_request_bench;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let workers = args.first().map_or(Ok(1), |v| v.parse::<usize>())?;
    let count = args.get(1).map_or(Ok(1_000_000), |v| v.parse::<usize>())?;
    let hot = args.get(2).is_some_and(|s| s == "hot");
    let mode = args.get(3).map_or("stable", String::as_str);
    let repeats = args.get(4).map_or(Ok(5), |v| v.parse::<usize>())?;
    let fatigue = args.get(5).is_some_and(|value| value == "fatigue");
    if mode == "diet-food" || mode == "diet-nutrition" {
        return diet_bench::run(workers, count, repeats, mode == "diet-nutrition");
    }
    if mode == "social-pressure" {
        return social_pressure_bench::run(workers, count, repeats);
    }
    if mode == "cooperation" || mode == "requests" {
        return cooperation_bench::run(workers, count, repeats, mode == "requests");
    }
    if mode == "work-requests" || mode == "work-claims" {
        return work_request_bench::run(workers, count, repeats, mode == "work-claims");
    }
    if mode == "auto-food-requests" {
        return food_request_bench::run_auto(workers, count, repeats);
    }
    if mode == "food-requests" {
        return food_request_bench::run(workers, count, repeats);
    }
    if mode == "food-aid" || mode == "learned-food-aid" {
        return food_aid_bench::run(workers, count, repeats, hot, mode == "learned-food-aid");
    }
    if [
        "acquire-food",
        "store-food",
        "store-claims",
        "store-claims-hot",
    ]
    .contains(&mode)
    {
        return acquire_food_bench::run(
            workers,
            count,
            repeats,
            mode != "acquire-food",
            mode.starts_with("store-claims"),
            mode == "store-claims-hot",
        );
    }
    if mode == "local-food" {
        return local_food_bench::run(workers, count, repeats);
    }
    if mode == "food" {
        return food_benchmark(workers, count, hot, repeats);
    }
    if mode == "autowork" || mode == "goals" || mode == "purpose" {
        return autowork_benchmark(
            workers,
            count,
            repeats,
            mode != "autowork",
            mode == "purpose",
        );
    }
    if mode == "agreements" || mode == "trust" {
        return agreement_benchmark(workers, count, hot, repeats, mode == "trust");
    }
    if mode == "social" {
        return social_benchmark(
            workers,
            count,
            hot,
            repeats,
            args.get(5).is_some_and(|s| s == "needs" || s == "disease"),
            args.get(5).is_some_and(|s| s == "disease"),
        );
    }
    if workers == 0
        || count < 4
        || count % 2 != 0
        || repeats == 0
        || !["stable", "casualty", "command"].contains(&mode)
    {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!("../../../content/parallel/example.toml"))?;
    if fatigue {
        world.fatigue =
            World::from_toml(include_str!("../../../content/parallel/fatigue.toml"))?.fatigue;
    }
    world.rules.hunger_per_tick = 0;
    world.rules.recovery = 0;
    world.rules.max_health = u32::MAX;
    let health = if mode == "casualty" { 1 } else { u32::MAX };
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    let fighters = (0..count)
        .map(|i| {
            let side = if i < count / 2 { 1 } else { 2 };
            let target = if side == 1 {
                if hot {
                    count / 2 + 1
                } else {
                    i + count / 2 + 1
                }
            } else if hot {
                1
            } else {
                i - count / 2 + 1
            };
            CombatFighter {
                person: i as u64 + 1,
                side,
                order: CombatOrder::Attack {
                    target: target as u64,
                },
            }
        })
        .collect();
    world.conflict = Some(Conflict {
        damage: 1,
        next_id: 2,
        encounters: vec![CombatEncounter {
            id: 1,
            region: world.regions[0].id,
            fighters,
        }],
        events: vec![],
    });
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let start = Instant::now();
        let mut kernel = Kernel::compile(world.clone()).unwrap();
        let initial_compile = start.elapsed().as_secs_f64();
        let mut times = Vec::new();
        for round in 0..=repeats {
            // Casualty runs start fresh because their first round removes fighters.
            // Every measured round includes actual roster edits and plan filtering.
            if mode == "casualty" && round > 0 { kernel = Kernel::compile(world.clone()).unwrap(); }
            let start = Instant::now();
            if mode == "command" {
                kernel.combat_orders(&[CombatCommand { encounter: 1, person: 1,
                    order: CombatOrder::Attack { target: (count / 2 + 1 + round % 2) as u64 } }]).unwrap();
            }
            kernel.step().unwrap();
            let elapsed = start.elapsed().as_secs_f64();
            if round > 0 { times.push(elapsed); }
        }
        let result = kernel.export();
        assert_eq!(result.conflict.as_ref().unwrap().events.len(), count);
        if mode == "casualty" {
            let dead = result.regions[0].people.iter().filter(|person| person.health == 0).count();
            assert_eq!(dead, if hot { 2 } else { count });
        }
        times.sort_by(f64::total_cmp);
        println!("workers={workers} people={count} hot={hot} mode={mode} fatigue={fatigue} initial_compile={initial_compile:.6} tick_median={:.6}", times[times.len()/2]);
    });
    Ok(())
}

fn social_benchmark(
    workers: usize,
    count: usize,
    hot: bool,
    ticks: usize,
    needs: bool,
    disease: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0 || count == 0 || ticks == 0 {
        return Err("invalid benchmark arguments".into());
    }
    let file = if needs {
        include_str!("../../../content/parallel/social_needs.toml")
    } else {
        include_str!("../../../content/parallel/social.toml")
    };
    let mut world = World::from_toml(file)?;
    world.rules.hunger_per_tick = 0;
    world.routes.clear();
    let regions = if hot { 1 } else { 100 };
    world.regions = (0..regions)
        .map(|id| game_parallel::Region {
            id,
            food: 0,
            people: Vec::new(),
        })
        .collect();
    for id in 0..count {
        world.regions[id % regions as usize].people.push(Person {
            id: id as u64,
            health: 100,
            hunger: 0,
        });
    }
    world.social.as_mut().unwrap().participants = (0..count)
        .map(|id| game_parallel::SocialParticipant {
            person: id as u64,
            willing: id % 3 != 0,
        })
        .collect();
    if disease {
        world.disease =
            World::from_toml(include_str!("../../../content/parallel/disease.toml"))?.disease;
        let model = world.disease.as_mut().unwrap();
        model.strains[0].incubation = 0;
        model.cases = (0..count / 2)
            .map(|person| game_parallel::DiseaseCase {
                person: person as u64,
                strain: 1,
                infected_at: 0,
                source: None,
            })
            .collect();
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut kernel = Kernel::compile(world)?;
    let start = Instant::now();
    pool.install(|| -> Result<(), &'static str> {
        for _ in 0..ticks {
            kernel.step()?;
        }
        Ok(())
    })?;
    let elapsed = start.elapsed();
    let output = kernel.export();
    assert_eq!(output.tick, ticks as u64);
    println!(
        "workers={workers} people={count} hot={hot} mode=social needs={needs} disease={disease} ticks={ticks} elapsed_seconds={:.6}",
        elapsed.as_secs_f64()
    );
    Ok(())
}

fn agreement_benchmark(
    workers: usize,
    count: usize,
    hot: bool,
    repeats: usize,
    trust: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use game_parallel::{
        Agreement, AgreementPolicy, AgreementStatus, Agreements, Amount, GoodsHolding,
    };
    if workers == 0 || count < 4 || !count.is_multiple_of(2) || repeats == 0 {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!("../../../content/parallel/agreements.toml"))?;
    world.rules.hunger_per_tick = 0;
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    world.market = None;
    if trust {
        world.opinions = Some(game_parallel::Opinions {
            sensitivities: Vec::new(),
            rules: game_parallel::OpinionRules {
                minimum: -5,
                maximum: 5,
                initial: 0,
                fulfilled_gain: 1,
                default_loss: 1,
                theft_loss: 0,
                encounter_gain: 0,
                minimum_report_trust: None,
                affinity: None,
                newborn_obligation_development: None,
                fear_per_damage: 0,
                fear_decay_per_tick: 0,
                gratitude_gain: 0,
                obligation_relief: 0,
            },
            records: Vec::new(),
            events: Vec::new(),
        });
    }
    let half = count / 2;
    world.goods_cargo.as_mut().unwrap().holdings = (1..=half as u64)
        .map(|person| GoodsHolding {
            person,
            items: vec![Amount {
                good: 1,
                quantity: 1,
            }]
            .into(),
        })
        .collect();
    world.agreements = Some(Agreements {
        offering: vec![],
        bulk_debtors: Vec::new(),
        next_id: half as u64 + 1,
        policies: (1..=half as u64)
            .map(|person| AgreementPolicy {
                person,
                honor: true,
                accept: false,
                minimum_trust: None,
                minimum_honor_trust: None,
                fear_honor_threshold: None,
                obligation_honor_threshold: None,
                accept_pressure_threshold: None,
            })
            .collect(),
        contracts: (1..=half as u64)
            .map(|id| Agreement {
                id,
                debtor: id,
                creditor: if hot {
                    half as u64 + 1
                } else {
                    id + half as u64
                },
                good: 1,
                quantity: 1,
                delivered: 0,
                due: 1,
                status: AgreementStatus::Active,
            })
            .collect(),
        events: Vec::new(),
    });
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let mut samples=Vec::new();
        for round in 0..=repeats {
            let mut kernel=Kernel::compile(world.clone()).unwrap();
            let start=Instant::now(); kernel.step().unwrap(); let elapsed=start.elapsed().as_secs_f64();
            if round>0 { samples.push(elapsed); }
            let output=kernel.export();
            let fulfilled=output.agreements.unwrap().contracts.iter().filter(|contract| contract.status==AgreementStatus::Fulfilled).count();
            assert_eq!(fulfilled,if hot {1} else {half});
        }
        samples.sort_by(f64::total_cmp);
        println!("workers={workers} people={count} hot={hot} mode=agreements trust={trust} rounds={repeats} tick_median={:.6}",samples[samples.len()/2]);
    });
    Ok(())
}

fn autowork_benchmark(
    workers: usize,
    count: usize,
    repeats: usize,
    goals: bool,
    purpose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0 || count < 2 || repeats == 0 {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!("../../../content/parallel/autowork.toml"))?;
    world.rules.hunger_per_tick = 0;
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    world.autowork.as_mut().unwrap().policies = (1..=count as u64)
        .map(|person| game_parallel::WorkPolicy {
            person,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        })
        .collect();
    if goals {
        world.autowork.as_mut().unwrap().policies.clear();
        world.goals = Some(game_parallel::Goals {
            retry_interval: 3,
            plan_inputs: false,
            people: (1..=count as u64)
                .map(|person| game_parallel::Goal {
                    person,
                    recipe: 1,
                    priority: 0,
                    target: 1,
                    progress: 0,
                    next_review: 0,
                    enabled: true,
                })
                .collect(),
            events: Vec::new(),
        });
    }
    if purpose {
        world.purpose = Some(game_parallel::Purpose {
            rules: game_parallel::PurposeRules {
                max: 20,
                per_tick: 1,
                blocked_increase: 2,
                completion_relief: 6,
                damage_threshold: 8,
                damage: 2,
            },
            people: (1..=count as u64)
                .map(|person| game_parallel::PurposePerson {
                    person,
                    deprivation: 5,
                    eligible_at: 0,
                })
                .collect(),
        });
    }
    let stock = &mut world.production.as_mut().unwrap().inventories[0].stocks[0];
    stock.quantity = (count / 2) as u64 * 3;
    stock.capacity = stock.quantity.max(3);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let mut samples = Vec::new();
        for round in 0..=repeats {
            let mut kernel = Kernel::compile(world.clone()).unwrap();
            let start = Instant::now();
            kernel.step().unwrap();
            let elapsed = start.elapsed().as_secs_f64();
            if round > 0 {
                samples.push(elapsed);
            }
            let result = kernel.export();
            assert_eq!(result.production.unwrap().jobs.len(), count / 2);
            if goals {
                assert_eq!(result.goals.unwrap().events.iter().filter(|event| event.blocked).count(), count - count / 2);
            }
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "workers={workers} people={count} mode=autowork goals={goals} purpose={purpose} rounds={repeats} tick_median={:.6}",
            samples[samples.len() / 2]
        );
    });
    Ok(())
}

fn food_benchmark(
    workers: usize,
    count: usize,
    sparse: bool,
    repeats: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0
        || count < 4
        || !count.is_multiple_of(4)
        || repeats == 0
        || count > u32::MAX as usize
    {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!("../../../content/parallel/example.toml"))?;
    let identity = |id: u64| if sparse { id * 4294967297 } else { id };
    world.regions[0].people = (1..=count as u64)
        .filter(|id| id % 2 == 0)
        .map(|id| Person {
            id: identity(id),
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    world.travelers = (1..=count as u64)
        .filter(|id| id % 2 == 1)
        .map(|id| game_parallel::Traveler {
            person: Person {
                id: identity(id),
                health: 100,
                hunger: 0,
            },
            source: 2,
            destination: 1,
            ticks_remaining: 1,
        })
        .collect();
    world.rules.hunger_per_tick = 4;
    world.rules.hunger_satisfied = 4;
    world.rules.starvation_threshold = 9;
    world.rules.recovery = 0;
    world.rules.food_growth = 0;
    world.rules.food_per_meal = 2;
    world.rules.food_capacity = u32::MAX;
    world.regions[0].food = ((count / 2 + count / 4) * 2) as u32;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(||{
        let mut samples=Vec::new();
        for round in 0..=repeats {
            let mut kernel=Kernel::compile(world.clone()).unwrap();kernel.step().unwrap();
            let start=Instant::now();kernel.step().unwrap();let elapsed=start.elapsed().as_secs_f64();
            if round>0 {samples.push(elapsed);}
            let result=kernel.export();assert_eq!(result.regions[0].food,0);
            let fed=result.regions[0].people.iter().filter(|p| {let n=if sparse {p.id/4294967297} else {p.id}; if n%2==0 {p.hunger==0} else {p.hunger==4}}).count();
            assert_eq!(fed,count/4);
        }
        samples.sort_by(f64::total_cmp);println!("workers={workers} people={count} sparse={sparse} mode=food rounds={repeats} tick_median={:.6}",samples[samples.len()/2]);
    });
    Ok(())
}

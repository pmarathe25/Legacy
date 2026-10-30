//! Concentrated work requests competing for physical inputs in one region.
use super::*;
use game_parallel::work_requests::{Preference, Proposal};

pub fn run(
    workers: usize,
    count: usize,
    repeats: usize,
    claims: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if workers == 0
        || count < 4
        || !count.is_multiple_of(2)
        || count > (u32::MAX / 3) as usize
        || repeats == 0
    {
        return Err("invalid benchmark arguments".into());
    }
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/work_requests.toml"
    ))?;
    world.rules.hunger_per_tick = 0;
    world.rules.food_capacity = (count * 3) as u32;
    world.regions[0].food = (count * 2) as u32;
    world.regions[0].people = (1..=count as u64)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    world.market = None;
    if claims {
        let fixture = World::from_toml(include_str!("../../../../content/parallel/theft.toml"))?;
        world.property = fixture.property;
        world.property.as_mut().unwrap().claims[0].owner = count as u64;
        world.opinions = fixture.opinions;
    }
    let work_count = count as u64 - 1;
    let inventory = &mut world.production.as_mut().unwrap().inventories[0];
    inventory.stocks[0].quantity = (work_count / 2) * 3;
    inventory.stocks[0].capacity = work_count * 3;
    inventory.stocks[1].capacity = work_count * 2;
    let requests = world.autowork.as_mut().unwrap().requests.as_mut().unwrap();
    requests.preferences = (2..=count as u64)
        .map(|person| Preference {
            person,
            effort: 1,
            help_value: 1,
            disregard_claims: claims,
        })
        .collect();
    let proposals: Vec<_> = (2..=count as u64)
        .map(|worker| Proposal {
            requester: 1,
            worker,
            region: 1,
            recipe: 1,
            expires: 5,
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    pool.install(|| {
        let mut admission = Vec::new();
        let mut ticks = Vec::new();
        let mut prepare = Vec::new();
        let mut commit = Vec::new();
        for round in 0..=repeats {
            let mut kernel = Kernel::compile(world.clone()).unwrap();
            let start = Instant::now();
            let ids = kernel.request_work_batch(&proposals).unwrap();
            let admitted = start.elapsed().as_secs_f64();
            assert_eq!(ids.len(), proposals.len());
            let start = Instant::now();
            let draft = kernel.prepare().unwrap();
            let prepared = start.elapsed().as_secs_f64();
            let publish = Instant::now();
            kernel.commit(draft).unwrap();
            let committed = publish.elapsed().as_secs_f64();
            let elapsed = start.elapsed().as_secs_f64();
            if round > 0 { admission.push(admitted); ticks.push(elapsed); prepare.push(prepared); commit.push(committed); }
            let result = kernel.export();
            assert_eq!(result.production.as_ref().unwrap().jobs.len(), (work_count / 2) as usize);
            assert_eq!(result.production.unwrap().inventories[0].stocks[0].quantity, 0);
            assert_eq!(result.autowork.unwrap().requests.unwrap().pending.len(), (work_count - work_count / 2) as usize);
            if claims {
                assert_eq!(result.property.unwrap().thefts.len(), (work_count / 2) as usize);
                assert_eq!(result.opinions.unwrap().records.len(), (work_count / 2) as usize);
            }
        }
        admission.sort_by(f64::total_cmp);
        ticks.sort_by(f64::total_cmp);
        prepare.sort_by(f64::total_cmp);
        commit.sort_by(f64::total_cmp);
        println!("workers={workers} people={count} requests={work_count} claims={claims} rounds={repeats} admission_median={:.6} tick_median={:.6} prepare_median={:.6} commit_median={:.6}", admission[admission.len()/2], ticks[ticks.len()/2], prepare[prepare.len()/2], commit[commit.len()/2]);
    });
    Ok(())
}

//! Snapshot computation, ordered inventory effects, disjoint body effects, evidence.
use super::*;
use game_body_effort::ExertMany;
use rayon::prelude::*;
use std::collections::BTreeSet;

struct Input {
    row: Row,
    work: Work,
    device: Device,
    goods: Vec<(GoodId, u64)>,
    body: game_physiology::Body,
    effort: Effort,
    current: Effort,
    endurance: u64,
}
struct Output {
    event: Tested,
    exert: game_physiology::Exert,
}
pub(super) fn apply(tx: &mut Transaction<'_>, rows: Vec<Row>) -> Result<()> {
    require(
        !tx.has_before::<game_physiology::Exert>() && !tx.has_before::<ExertMany>(),
        "effort hooks require ordered trials",
    )?;
    // Property hooks execute before the immutable physical snapshot is captured.
    for row in &rows {
        require(live(tx, row), "session action changed")?;
        let w = &tx.get::<Machines>()?.work[&row.person];
        let inventory = tx.get::<Machines>()?.devices[&w.device.unwrap()].inventory;
        let weights = w.weights.unwrap();
        for inventory in [inventory, weights] {
            tx.call(game_property::Use {
                actor: row.person,
                inventory,
                purpose: "continuing a mechanical trial".into(),
            })?;
        }
    }
    require(
        limits(tx, rows.iter().map(|r| r.person).collect())?.is_some(),
        "session priority changed",
    )?;
    let mut devices = BTreeSet::new();
    let mut writes = BTreeSet::new();
    let mut reads = BTreeSet::new();
    let mut inputs = Vec::new();
    for row in rows {
        require(physical(tx, &row), "session physical conditions changed")?;
        let work = tx.get::<Machines>()?.work[&row.person].clone();
        let device = tx.get::<Machines>()?.devices[&work.device.unwrap()].clone();
        require(
            devices.insert(work.device.unwrap()) && writes.insert(device.inventory),
            "shared apparatus",
        )?;
        reads.insert(work.weights.unwrap());
        let now = tx.now();
        let current = source(tx, row.person);
        let effort = Effort {
            force: work.source.force.min(current.force),
            travel: (u128::from(work.source.travel.min(current.travel))
                * u128::from(now.0.saturating_sub(work.since.0))
                / u128::from(work.unit.0))
            .min(u128::from(u64::MAX)) as u64,
        };
        let goods = tx.get::<game_cargo::Cargo>()?.0[&device.inventory]
            .goods
            .iter()
            .map(|(g, n)| (*g, *n))
            .collect();
        let body = tx.get::<Bodies>()?.people[&row.person];
        let endurance = game_vitality::factor(game_vitality::ability(tx, row.person, "endurance"));
        inputs.push(Input {
            row,
            work,
            device,
            goods,
            body,
            effort,
            current,
            endurance,
        });
    }
    require(
        writes.is_disjoint(&reads),
        "trial weights share consumed components",
    )?;
    let config = tx.get::<Machines>()?.config.clone();
    let now = tx.now();
    game_diagnostics::interaction_profile!(batch "mechanical-transmission", inputs.len(), inputs.len() > 1 && tx.compute().current_num_threads() > 1);
    let results: Vec<Result<Output>> = tx.compute().install(|| {
        inputs
            .par_iter()
            .map(|input| {
                let links = game_physics::transmissions(
                    &config,
                    &input.device.parts,
                    &input.device.broken,
                    input.goods.iter().copied(),
                )?;
                let (effort, result) = resolve_loaded(
                    &links,
                    input.effort,
                    input.work.task.force,
                    input.work.outlet,
                )?;
                let output = result.outputs[input.work.outlet];
                let lifted = if output.force >= input.work.task.force && output.travel > 0 {
                    input.work.task.force
                } else {
                    0
                };
                Ok(Output {
                    event: Tested {
                        person: input.row.person,
                        device: input.work.device,
                        lifted,
                        travel: if lifted > 0 { output.travel } else { 0 },
                        broken: result.broken,
                        source: effort,
                        outlet: input.work.outlet,
                        task: None,
                        parts: Some(input.device.parts.clone()),
                    },
                    exert: game_physiology::Exert {
                        person: input.row.person,
                        energy: (effort.work() / 1000).min(u128::from(u64::MAX)) as u64,
                        fatigue: (u128::from(now.0.saturating_sub(input.work.since.0)) * 1000
                            / u128::from(HOUR.0))
                        .min(u128::from(u64::MAX)) as u64,
                    },
                })
            })
            .collect()
    });
    let outputs: Vec<_> = results.into_iter().collect::<Result<_>>()?;
    let mut expected_devices: Vec<_> = inputs.iter().map(|i| i.device.clone()).collect();
    for (index, (input, output)) in inputs.iter().zip(&outputs).enumerate() {
        for &part in &output.event.broken {
            let good = input.device.parts[part].good;
            tx.call(game_cargo::Consume {
                inventory: input.device.inventory,
                good,
                quantity: 1,
            })?;
            let state = tx.edit::<Machines>()?;
            let device = state
                .devices
                .get_mut(&input.work.device.unwrap())
                .ok_or_else(|| Error::Rejected("apparatus removed during stock phase".into()))?;
            device.broken.insert(part);
            let waste = state.waste.entry(good).or_default();
            *waste = waste.checked_add(1).ok_or(Error::Overflow)?;
            expected_devices[index].broken.insert(part);
        }
    }
    // A stock observer can alter a later actor or apparatus. Reject the entire
    // speculative wave before body publication; its savepoint undoes the hooks.
    for ((input, expected), output) in inputs.iter().zip(&expected_devices).zip(&outputs) {
        let mut goods: std::collections::BTreeMap<_, _> = input.goods.iter().copied().collect();
        for &part in &output.event.broken {
            let good = input.device.parts[part].good;
            let quantity = goods
                .get_mut(&good)
                .ok_or_else(|| Error::Rejected("missing trial component".into()))?;
            *quantity = quantity.checked_sub(1).ok_or(Error::Overflow)?;
            if *quantity == 0 {
                goods.remove(&good);
            }
        }
        require(
            live(tx, &input.row)
                && physical(tx, &input.row)
                && tx.get::<Machines>()?.work[&input.row.person] == input.work
                && tx
                    .get::<Machines>()?
                    .devices
                    .get(&input.work.device.unwrap())
                    == Some(expected)
                && tx.get::<Bodies>()?.people[&input.row.person] == input.body
                && tx
                    .get::<game_cargo::Cargo>()?
                    .0
                    .get(&input.device.inventory)
                    .is_some_and(|i| {
                        i.goods
                            .iter()
                            .map(|(g, n)| (*g, *n))
                            .eq(goods.iter().map(|(g, n)| (*g, *n)))
                    })
                && source(tx, input.row.person) == input.current
                && game_vitality::factor(game_vitality::ability(tx, input.row.person, "endurance"))
                    == input.endurance,
            "trial snapshot invalidated by stock effects",
        )?;
    }
    tx.call(ExertMany(outputs.iter().map(|o| o.exert.clone()).collect()))?;
    // Mark every physical interval settled before any evidence listener runs.
    for input in &inputs {
        let work = tx
            .edit::<Machines>()?
            .work
            .get_mut(&input.row.person)
            .unwrap();
        work.since = now;
        work.session.as_mut().unwrap().trial += 1;
    }
    for output in &outputs {
        tx.emit(output.event.clone(), vec![output.event.person])?;
    }
    for (input, output) in inputs.iter().zip(&outputs) {
        if live(tx, &input.row) {
            let mass = tx
                .get::<game_cargo::Cargo>()?
                .0
                .get(&input.work.weights.unwrap())
                .map_or(0, |i| i.mass);
            let old = input.work.task.force;
            let next = if output.event.lifted > 0 {
                old.saturating_add((old / 4).max(1)).min(mass)
            } else {
                old.saturating_sub((old / 4).max(1)).max(1)
            };
            let current = source(tx, input.row.person);
            let work = tx
                .edit::<Machines>()?
                .work
                .get_mut(&input.row.person)
                .unwrap();
            work.task.force = next.max(1);
            work.source = current;
            if next == old || !output.event.broken.is_empty() {
                work.session.as_mut().unwrap().until = now;
            }
        }
    }
    game_diagnostics::interaction_profile!(count "activity.mechanics.trials", inputs.len() as u64);
    Ok(())
}

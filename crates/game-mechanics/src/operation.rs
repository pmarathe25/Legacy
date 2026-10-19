use super::*;
#[derive(Clone, Serialize, Deserialize)]
struct Complete(PersonId);
request!(Complete, "mechanics.complete", ());
#[derive(Clone, Serialize, Deserialize)]
struct Cancel {
    person: PersonId,
    job: ScheduleId,
}
request!(Cancel, "mechanics.cancel", ());
fn source(tx: &Transaction<'_>, p: PersonId) -> Effort {
    Effort {
        force: game_vitality::factor(game_vitality::ability(tx, p, "strength")),
        travel: game_vitality::factor(game_vitality::ability(tx, p, "agility")),
    }
}
fn finish(tx: &mut Transaction<'_>, p: PersonId) -> Result<bool> {
    if !tx.get::<Machines>()?.work.contains_key(&p) {
        return Ok(true);
    }
    let work = tx.edit::<Machines>()?.work.remove(&p).unwrap();
    let elapsed = tx.now().0.saturating_sub(work.since.0);
    if elapsed == 0 {
        return Ok(false);
    }
    let current = ready(tx, p).map(|_| source(tx, p)).unwrap_or_default();
    let mut effort = Effort {
        force: work.source.force.min(current.force),
        travel: (u128::from(work.source.travel.min(current.travel)) * u128::from(elapsed)
            / u128::from(work.unit.0))
        .min(u128::from(u64::MAX)) as u64,
    };
    let load = if let Some(inventory) = work.weights {
        work.task.force.min(
            tx.get::<game_cargo::Cargo>()?
                .0
                .get(&inventory)
                .filter(|i| {
                    i.area(tx.get().unwrap()) == game_transport::area(tx.get().unwrap(), p).ok()
                })
                .map_or(0, |i| i.mass),
        )
    } else {
        work.task.force
    };
    let mut output = effort;
    let mut broken = vec![];
    if load == 0 {
        output = Effort::default();
    } else if let Some(id) = work.device {
        let device = tx.get::<Machines>()?.devices[&id].clone();
        let (applied, result) = resolve_loaded(
            &game_physics::transmissions(
                &tx.get::<Machines>()?.config,
                &device.parts,
                &device.broken,
                tx.get::<game_cargo::Cargo>()?.0[&device.inventory]
                    .goods
                    .iter()
                    .map(|(g, n)| (*g, *n)),
            )?,
            effort,
            load.max(1),
            work.outlet,
        )?;
        effort = applied;
        output = result.outputs[work.outlet];
        broken = result.broken;
        for index in &broken {
            let good = device.parts[*index].good;
            tx.call(game_cargo::Consume {
                inventory: device.inventory,
                good,
                quantity: 1,
            })?;
            let state = tx.edit::<Machines>()?;
            state.devices.get_mut(&id).unwrap().broken.insert(*index);
            let waste = state.waste.entry(good).or_default();
            *waste = waste.checked_add(1).ok_or(Error::Overflow)?;
        }
    }
    let lifted = if output.force >= load && output.travel > 0 {
        load
    } else {
        0
    };
    let success = lifted == work.task.force && output.travel >= work.task.travel;
    if game_physiology::capable(tx, p).is_ok() {
        tx.call(game_physiology::Exert {
            person: p,
            energy: (effort.work() / 1000).min(u128::from(u64::MAX)) as u64,
            fatigue: (u128::from(elapsed) * 1000 / u128::from(HOUR.0)).min(u128::from(u64::MAX))
                as u64,
        })?;
    }
    tx.emit(
        Tested {
            person: p,
            device: work.device,
            lifted,
            travel: if lifted > 0 { output.travel } else { 0 },
            broken,
            source: effort,
            outlet: work.outlet,
            task: work.recipe.map(|label| (label, work.task)),
        },
        vec![p],
    )?;
    Ok(success)
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Experiment>(|tx, r| {
        let area = ready(tx, r.person)?;
        let device = tx
            .get::<Machines>()?
            .devices
            .get(&r.device)
            .ok_or_else(|| Error::Missing("apparatus".into()))?;
        require(
            area == device.area
                && !busy(tx, r.device)
                && r.outlet < device.parts.len()
                && !device.parts.iter().any(|p| p.input == Some(r.outlet)),
            "apparatus is inaccessible or occupied",
        )?;
        require(
            r.load > 0
                && game_cargo::locality(tx, r.weights)? == area
                && tx.get::<game_cargo::Cargo>()?.0[&r.weights].mass >= r.load,
            "test needs a physical load",
        )?;
        let apparatus_inventory = device.inventory;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: apparatus_inventory,
            purpose: "using a mechanism".into(),
        })?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.weights,
            purpose: "testing a mechanism".into(),
        })?;
        let since = tx.now();
        let until = since.after(HOUR)?;
        let source = source(tx, r.person);
        let job = tx.schedule(until, 20, Complete(r.person))?;
        tx.start_action(
            r.person,
            "experiment",
            Some(until),
            Cancel {
                person: r.person,
                job,
            },
        )?;
        tx.edit::<Machines>()?.work.insert(
            r.person,
            Work {
                device: Some(r.device),
                task: Task {
                    force: r.load,
                    travel: 1,
                },
                recipe: None,
                source,
                since,
                unit: HOUR,
                weights: Some(r.weights),
                outlet: r.outlet,
            },
        );
        Ok(until)
    })?;
    b.request::<Complete>(|tx, r| {
        finish(tx, r.0)?;
        tx.finish_action(r.0, "experiment")
    })?;
    b.request::<Cancel>(|tx, r| {
        tx.unschedule(r.job);
        finish(tx, r.person)?;
        Ok(())
    })?;
    b.request::<PrepareLabor>(|tx, r| {
        let area = ready(tx, r.person)?;
        require(
            r.task.force > 0
                && r.task.travel > 0
                && r.baseline.0 > 0
                && !tx.get::<Machines>()?.work.contains_key(&r.person),
            "invalid or overlapping physical work",
        )?;
        let mut effort = source(tx, r.person);
        effort.force = effort.force.min(r.task.force);
        let chosen = if tx.registered::<SelectAid>() {
            tx.call(SelectAid(r.clone()))?
        } else {
            None
        };
        let mut travel = 1000;
        if let Some((id, outlet)) = chosen {
            let device = tx
                .get::<Machines>()?
                .devices
                .get(&id)
                .ok_or_else(|| Error::Missing("apparatus".into()))?;
            require(
                device.area == area
                    && !busy(tx, id)
                    && outlet < device.parts.len()
                    && !device.parts.iter().any(|p| p.input == Some(outlet)),
                "apparatus unavailable",
            )?;
            let m = known(tx, r.person, id, outlet)
                .ok_or_else(|| Error::Rejected("unfamiliar technique".into()))?;
            travel = m.travel;
            effort.force = (u128::from(r.task.force) * 1000)
                .div_ceil(u128::from(m.gain.max(1)))
                .min(u128::from(source(tx, r.person).force)) as u64;
        }
        let duration = SimDuration(
            (u128::from(r.baseline.0) * u128::from(r.task.travel) * 1000)
                .div_ceil(u128::from(effort.travel.max(1)) * u128::from(travel.max(1)))
                .min(u128::from(u64::MAX))
                .max(1) as u64,
        );
        if let Some((id, _)) = chosen {
            tx.call(game_property::Use {
                actor: r.person,
                inventory: tx.get::<Machines>()?.devices[&id].inventory,
                purpose: "using a mechanism".into(),
            })?;
        }
        let since = tx.now();
        tx.edit::<Machines>()?.work.insert(
            r.person,
            Work {
                device: chosen.map(|v| v.0),
                outlet: chosen.map_or(0, |v| v.1),
                task: r.task,
                recipe: Some(r.label),
                source: effort,
                since,
                unit: r.baseline,
                weights: None,
            },
        );
        Ok(duration)
    })?;
    b.request::<FinishLabor>(|tx, r| finish(tx, r.person))?;
    b.subscribe::<ActionFinished>("mechanics.interruption", |tx, e| {
        finish(tx, e.person)?;
        Ok(())
    })
}

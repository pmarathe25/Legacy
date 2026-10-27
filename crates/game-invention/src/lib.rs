mod designs;
mod devices;
mod techniques;
use game_core::*;
use game_mechanics::{Assemble, Experiment, Machines, busy, known};
use game_sim::*;
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("invention", 1)?;
    observations(b)?;
    techniques::register(b)?;
    game_choice::deliberative_provider(b, "mechanics.opportunities", |tx, e| {
        if !game_choice::collecting(tx, e.person)
            || !tx.get::<Machines>()?.enabled
            || !game_vitality::can_manipulate(tx, e.person)
        {
            return Ok(());
        }
        let p = e.person;
        let Ok(area) = game_transport::area(tx.get()?, p) else {
            return Ok(());
        };
        let curiosity = game_vitality::disposition(tx, p, "openness") / 10
            + game_vitality::disposition(tx, p, "creativity") / 10;
        let need = designs::need(tx, p);
        let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(p).collect();
        let devices = devices::local(tx.get::<Machines>()?, area);
        let hooked = tx.has_before::<game_choice::Offer>();
        let mass = |tx: &Transaction<'_>| {
            packs
                .iter()
                .copied()
                .filter(|id| tx.get::<game_cargo::Cargo>().unwrap().0[id].mass > 0)
                .max_by_key(|id| tx.get::<game_cargo::Cargo>().unwrap().0[id].mass)
        };
        let heaviest = (!hooked).then(|| mass(tx));
        for d in devices.iter() {
            let device = d.id;
            let acting = tx
                .get::<Machines>()?
                .trial(p)
                .is_some_and(|w| w.device == device);
            if busy(tx, device) && !acting {
                continue;
            }
            for (outlet, header) in d.outlets.iter().enumerate() {
                let eligible = if hooked {
                    devices::eligible(&tx.get::<Machines>()?.devices[&device], outlet)
                } else {
                    header.eligible
                };
                if !eligible {
                    continue;
                }
                let Some(weights) = heaviest.unwrap_or_else(|| mass(tx)) else {
                    continue;
                };
                let load = need
                    .as_ref()
                    .map_or_else(
                        || game_vitality::factor(game_vitality::ability(tx, p, "strength")) / 2,
                        |task| task.force,
                    )
                    .max(1)
                    .min(tx.get::<game_cargo::Cargo>()?.0[&weights].mass);
                let measured = if hooked {
                    known(tx, p, device, outlet)
                } else {
                    tx.get::<game_knowledge::Knowledge>()
                        .ok()
                        .and_then(|s| s.0.get(&p))
                        .and_then(|f| {
                            game_knowledge::known::<game_physics::Measurement>(f, &header.memory)
                        })
                };
                let uncertain =
                    measured.is_none_or(|m| need.as_ref().is_some_and(|t| t.force > m.load));
                let motive = game_choice::motive(
                    0,
                    curiosity / if uncertain { 1 } else { 8 },
                    if need.is_some() && uncertain {
                        curiosity
                    } else {
                        0
                    },
                    10,
                    0,
                );
                let request = Experiment {
                    person: p,
                    device,
                    weights,
                    load,
                    outlet,
                };
                if acting {
                    let mut offer =
                        game_choice::Proposal::new(header.offer.clone(), motive, request)?;
                    offer.continuation = tx.get::<Actions>()?.0.get(&p).map(|a| a.id);
                    game_choice::submit(tx, p, offer)?;
                } else {
                    game_choice::offer(tx, p, header.offer.clone(), motive, request)?;
                }
            }
        }
        for inventory in packs {
            let parts = designs::propose(tx, p, inventory, need.as_ref())?;
            if parts.is_empty() {
                continue;
            }
            let familiar = tx
                .get::<Machines>()?
                .designs
                .get(&p)
                .is_some_and(|seen| seen.contains(&game_physics::design_key(&parts)));
            // Familiarity reduces novelty; it does not prohibit copying useful arrangements.
            let novelty = if familiar {
                curiosity / 8
            } else {
                curiosity / 2
            };
            game_choice::offer(
                tx,
                p,
                format!("mechanics:build:{}", inventory.0),
                game_choice::motive(
                    0,
                    novelty,
                    if need.is_some() { curiosity } else { 0 },
                    30,
                    0,
                ),
                game_choice::Undertake {
                    person: p,
                    kind: "assemble".into(),
                    duration: HOUR,
                    request: Pending::immediate(Assemble {
                        person: p,
                        inventory,
                        parts,
                    }),
                },
            )?;
        }
        Ok(())
    })
}

fn observations(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_mechanics::Tested>("invention.observations", |tx, e| {
        if game_physiology::capable(tx, e.person).is_err() {
            return Ok(());
        }
        if let Some((label,task))=&e.task {
            tx.call(game_knowledge::Conclude {person:e.person,key:format!("mechanics:need:{label}"),value:serde_json::json!({"force":task.force,"travel":task.travel,"met":e.lifted>=task.force && e.travel>=task.travel}),confidence:80})?;
        }
        let Some(id) = e.device else { return Ok(()) };
        let parts = e.parts.clone().or_else(|| tx.get::<Machines>().ok()?.devices.get(&id).map(|d| d.parts.clone()));
        let gain = (u128::from(e.lifted) * 1000 / u128::from(e.source.force.max(1)))
            .min(u128::from(u64::MAX)) as u64;
        let travel = if e.lifted > 0 {
            (u128::from(e.travel) * 1000 / u128::from(e.source.travel.max(1)))
                .min(u128::from(u64::MAX)) as u64
        } else {
            0
        };
        tx.call(game_knowledge::Conclude {
            person: e.person,
            key: format!("mechanics:device:{id}:{}", e.outlet),
            value: serde_json::json!({"gain":gain,"travel":travel,"load":e.lifted,"input_force":e.source.force,"input_travel":e.source.travel}),
            confidence: 80,
        })?;
        let Some(parts) = parts else { return Ok(()) };
        tx.call(game_knowledge::Conclude {
            person: e.person, key: format!("mechanics:design:{}", game_physics::design_key(&parts)),
            value: serde_json::json!({"parts":parts,"gain":gain,"travel":travel}), confidence: 80,
        })?;
        let context = std::collections::BTreeMap::from([
            ("drive".into(), i64::from(parts[e.outlet].drive)),
            ("load-arm".into(), i64::from(parts[e.outlet].load)),
            ("weight".into(), e.lifted.min(i64::MAX as u64) as i64),
            ("stages".into(), parts.len() as i64),
        ]);
        for (outcome, value) in [("effort", e.source.force), ("travel", e.travel)] {
            tx.call(game_inquiry::Witness {
                observer: e.person,
                observation: game_reasoning::Observation {
                    subject: "mechanical-transmission".into(),
                    context: context.clone(),
                    outcome: outcome.into(),
                    trial: id,
                    value: value.min(i64::MAX as u64) as i64,
                },
            })?;
        }
        Ok(())
    })
}

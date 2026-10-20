use game_core::*;
use game_mechanics::{Machines, Part, Task};
use game_sim::*;

/// Encountered failures supply problems, never hidden physical capacities.
pub(super) fn need(tx: &Transaction<'_>, p: PersonId) -> Option<Task> {
    let facts = tx.get::<game_knowledge::Knowledge>().ok()?.0.get(&p)?;
    game_knowledge::with_prefix(facts, "mechanics:need:")
        .filter_map(|(_, f)| {
            let value = f.value.decode::<serde_json::Value>()?;
            (!value.get("met")?.as_bool()?).then(|| Task {
                force: value.get("force").and_then(|v| v.as_u64()).unwrap_or(1),
                travel: value.get("travel").and_then(|v| v.as_u64()).unwrap_or(1),
            })
        })
        .max_by_key(|task| task.force)
}

pub(super) fn propose(
    tx: &Transaction<'_>,
    p: PersonId,
    inventory: InventoryId,
    need: Option<&Task>,
) -> Result<Vec<Part>> {
    let config = &tx.get::<Machines>()?.config;
    let stock = &tx.get::<game_cargo::Cargo>()?.0[&inventory];
    let candidates: Vec<_> = config
        .materials
        .iter()
        .filter(|(g, _)| stock.amount(**g) > 0)
        .collect();
    if candidates.is_empty() {
        return Ok(vec![]);
    }
    if need.is_some()
        && let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&p)
    {
        for (_, fact) in game_knowledge::with_prefix(facts, "mechanics:design:") {
            let Some(v) = fact.value.decode::<serde_json::Value>() else {
                continue;
            };
            let gain = v.get("gain").and_then(|v| v.as_u64()).unwrap_or(0);
            let force = game_vitality::factor(game_vitality::ability(tx, p, "strength"));
            if gain <= 1000
                || need.is_some_and(|n| {
                    u128::from(gain) * u128::from(force) / 1000 < u128::from(n.force)
                })
            {
                continue;
            }
            let Some(parts) = v
                .get("parts")
                .and_then(|v| serde_json::from_value::<Vec<Part>>(v.clone()).ok())
            else {
                continue;
            };
            if game_physics::needs(config, &parts)
                .is_ok_and(|needs| needs.iter().all(|(g, n)| stock.amount(*g) >= *n))
            {
                return Ok(parts);
            }
        }
    }

    let intelligence = game_vitality::ability(tx, p, "intelligence").min(16000);
    let attention = game_vitality::ability(tx, p, "attention").min(16000);
    let budget = (1 + intelligence / 1000 + attention / 2000).min(32) as usize;
    let attempt = tx.get::<Machines>()?.attempts.get(&p).copied().unwrap_or(0);
    let nonce = attempt.wrapping_add(tx.now().0 / DAY.0);
    let seed = random(tx.seed(), p.0, nonce);
    let count = 1 + seed as usize % budget;
    let strength = game_vitality::factor(game_vitality::ability(tx, p, "strength")).max(1);
    let ratio = need.map_or(1, |n| n.force.div_ceil(strength)).clamp(1, 999) as u32;
    let mut parts: Vec<Part> = vec![];
    for index in 0..count {
        let draw = random(seed, index as u64, 19);
        let (good, material) = candidates[draw as usize % candidates.len()];
        let input = if index == 0 {
            None
        } else if draw.is_multiple_of(5) {
            Some((draw as usize / 5) % index)
        } else {
            Some(index - 1)
        };
        let motion = input.map_or(game_physics::Motion::Linear, |i| {
            parts[i].mechanism.output()
        });
        let kinds: Vec<_> = game_physics::Mechanism::ALL
            .into_iter()
            .filter(|kind| {
                kind.input() == motion
                    && (index + 1 < count || kind.output() == game_physics::Motion::Linear)
            })
            .collect();
        let mechanism = kinds[(draw >> 16) as usize % kinds.len()];
        let (mut drive, mut load) = mechanism.geometry(draw, material.span);
        if mechanism == game_physics::Mechanism::Lever {
            drive = drive.max(ratio).min(material.span - 1);
            load = load.min(material.span - drive);
        }
        if mechanism.ratio(drive, load, material.span).is_none() {
            break;
        }
        parts.push(Part {
            mechanism,
            good: *good,
            input,
            drive,
            load,
        });
        if game_physics::needs(config, &parts)?
            .iter()
            .any(|(g, n)| stock.amount(*g) < *n)
        {
            parts.pop();
            break;
        }
    }
    Ok(parts)
}

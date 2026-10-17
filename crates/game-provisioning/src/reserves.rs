use super::*;
use game_storage::Table;
/// Individual experience of physically tried sources; no global stock oracle.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reserves {
    pub enabled: bool,
    pub dependents: Table<PersonId, u32>,
    pub sources: Table<PersonId, Table<u64, Source>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub successes: u64,
    pub failures: u64,
    pub last: SimTime,
}
impl Resource for Reserves {
    const KEY: &'static str = "provisioning.experience";
}
pub(super) fn batch(tx: &Transaction<'_>, person: PersonId, meals: u64) -> u64 {
    if !tx.get::<Reserves>().is_ok_and(|r| r.enabled) {
        return meals;
    }
    let foresight = tx
        .get::<game_traits::Traits>()
        .ok()
        .filter(|t| t.definitions.contains_key("foresight"))
        .map_or(0, |_| game_vitality::ability(tx, person, "foresight"));
    let dependents = tx
        .get::<Reserves>()
        .ok()
        .and_then(|s| s.dependents.get(&person))
        .copied()
        .unwrap_or(0);
    meals
        .max(1 + foresight / 250 + u64::from(dependents) * 2)
        .min(64)
}
pub fn reserve_plan(tx: &Transaction<'_>, p: PersonId) -> Result<Option<Opportunity>> {
    let _measurement = game_diagnostics::span("reserve planning");
    let desired = batch(tx, p, 1);
    if desired <= 1 {
        return Ok(None);
    }
    let mut portions = 0;
    for id in tx.get::<game_cargo::Cargo>()?.carried(p) {
        for (g, q) in &tx.get::<game_cargo::Cargo>()?.0[&id].goods {
            if let Some(food) = &tx.get::<game_objects::Objects>()?.by_id(*g)?.food {
                portions += q / food.portion;
            }
        }
    }
    if portions >= desired {
        return Ok(None);
    }
    Ok(options(tx, p, desired - portions)?
        .into_iter()
        .filter(|o| matches!(o.plan, Plan::Forage { .. }))
        .min_by_key(|o| {
            (
                i128::from(o.travel.0 / MINUTE.0) - i128::from(o.value),
                o.id,
            )
        }))
}
pub(super) fn reliability(tx: &Transaction<'_>, p: PersonId, patch: u64) -> u64 {
    let Some(s) = tx
        .get::<Reserves>()
        .ok()
        .and_then(|r| r.sources.get(&p)?.get(&patch))
    else {
        return 0;
    };
    let intelligence = game_vitality::ability(tx, p, "intelligence");
    (s.failures.saturating_sub(s.successes) * intelligence.min(16000) / 1000).min(60) * MINUTE.0
}
#[derive(Clone, Serialize, Deserialize)]
struct Experience {
    person: PersonId,
    patch: u64,
    success: bool,
}
impl Request for Experience {
    const KEY: &'static str = "provisioning.experience";
    type Response = ();
}
pub(super) fn experience(
    tx: &mut Transaction<'_>,
    person: PersonId,
    plan: &Plan,
    success: bool,
) -> Result<()> {
    if let Plan::Forage { patch, .. } = plan
        && tx.registered::<Experience>()
    {
        tx.call(Experience {
            person,
            patch: *patch,
            success,
        })?;
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Support {
    pub person: PersonId,
    pub dependents: u32,
}
impl Request for Support {
    const KEY: &'static str = "provisioning.support";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "provisioning.enable";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("provisioning", 1)?;
    b.resource::<Reserves>()?;
    b.request::<Support>(|tx, r| {
        tx.edit::<Reserves>()?
            .dependents
            .insert(r.person, r.dependents);
        Ok(())
    })?;
    b.request::<Experience>(|tx, r| {
        let now = tx.now();
        let s = tx
            .edit::<Reserves>()?
            .sources
            .entry(r.person)
            .or_default()
            .entry(r.patch)
            .or_default();
        s.last = now;
        if r.success {
            s.successes += 1
        } else {
            s.failures += 1
        }
        Ok(())
    })?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Reserves>()?.enabled = true;
        Ok(())
    })
}

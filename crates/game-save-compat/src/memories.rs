use super::*;
use game_core::{PersonId, SimTime};
use std::{collections::BTreeMap, sync::Arc};
/// Old inquiry queues had no observation timestamps. Keep their evidence with an
/// unknown-age epoch stamp; do not manufacture a fresh witness date at migration.
pub(super) fn migrate(resources: &mut Raw<'_>) -> Result<()> {
    let mut inquiry = resource(resources, "inquiry.experience")?;
    let Some(people) = inquiry
        .get_mut("people")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return Ok(());
    };
    let mut episodes = vec![];
    for (p, row) in people {
        let Some(old) = row.as_object_mut().and_then(|r| r.remove("observations")) else {
            continue;
        };
        let entries = old
            .as_array()
            .ok_or_else(|| invalid("invalid empirical memories"))?;
        let count = row
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(entries.len() as u64);
        let person = PersonId(p.parse().map_err(invalid)?);
        for (i, value) in entries.iter().enumerate() {
            let number = count
                .saturating_sub(entries.len() as u64)
                .saturating_add(i as u64 + 1);
            episodes.push((
                person,
                format!("memory:empirical:{number:020}"),
                value.clone(),
            ));
        }
    }
    if episodes.is_empty() {
        return Ok(());
    }
    let knowledge = resource(resources, "knowledge.facts")?;
    let mut facts: BTreeMap<PersonId, game_facts::Facts<serde_json::Value>> = if let Some(archive) =
        knowledge.get("archive")
    {
        serde_json::from_value::<game_snapshot::evidence::Archive<serde_json::Value>>(
            archive.clone(),
        )
        .map_err(invalid)?
        .restore(|_, v| v)?
        .into_iter()
        .collect()
    } else {
        let mut rows = BTreeMap::new();
        if let Some(map) = knowledge.as_object() {
            for (p, row) in map {
                let mut f = game_facts::Facts::default();
                for (k, v) in row
                    .as_object()
                    .ok_or_else(|| invalid("invalid personal evidence"))?
                {
                    f.insert(
                        Arc::from(k.as_str()),
                        serde_json::from_value::<game_facts::Fact<serde_json::Value>>(v.clone())
                            .map_err(invalid)?,
                    );
                }
                rows.insert(PersonId(p.parse().map_err(invalid)?), f);
            }
        }
        rows
    };
    for (p, key, value) in episodes {
        facts.entry(p).or_default().insert(
            key.into(),
            game_facts::Fact {
                value: Arc::new(value),
                source: p,
                observed: SimTime(0),
                revision: 0,
                confidence: 80,
            },
        );
    }
    put(
        resources,
        "knowledge.facts",
        &serde_json::json!({"archive":game_snapshot::evidence::Archive::capture(facts.iter().map(|(p,f)|(*p,f)))}),
    )?;
    put(resources, "inquiry.experience", &inquiry)
}

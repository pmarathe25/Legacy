//! Normalize historical save ownership without changing live feature rules.
use game_core::Result;
mod memories;
const LOCAL_INTERACTION_PHASE: &str = "characters.local-interaction-phase";
const OPTIONAL: &[(&str, Option<&str>)] = &[
    ("memory", Some("knowledge.perception")),
    ("information", Some("information.questions")),
    ("search", Some("search.people")),
    ("mechanics", Some("mechanics.devices")),
    ("invention", None),
    ("communication", Some("communication.experience")),
    ("discourse", None),
    ("food-inquiry", Some("food-inquiry.samples")),
    ("companionship", None),
    ("attachments", None),
    ("digestion", Some("digestion.stomachs")),
    ("fauna", Some("fauna.animals")),
    ("credit", Some("credit.conventions")),
    ("testimony", None),
    ("observations", Some("observations.episodes")),
    ("choice", Some("choice.offers")),
    ("signals", Some("signals.experience")),
    ("comfort", Some("comfort.intentions")),
    ("family-life", Some("family-life.bonds")),
    ("literacy", Some("literacy.records")),
    ("inquiry", Some("inquiry.experience")),
];
type Raw<'a> =
    std::collections::BTreeMap<String, std::borrow::Cow<'a, serde_json::value::RawValue>>;
fn invalid(e: impl std::fmt::Display) -> game_core::Error {
    game_core::Error::InvalidSave(e.to_string())
}
fn put(raw: &mut Raw<'_>, key: &str, value: &impl serde::Serialize) -> Result<()> {
    raw.insert(
        key.into(),
        std::borrow::Cow::Owned(serde_json::value::to_raw_value(value).map_err(invalid)?),
    );
    Ok(())
}
fn resource(raw: &Raw<'_>, key: &str) -> Result<serde_json::Value> {
    raw.get(key)
        .map(|v| serde_json::from_str(v.get()).map_err(invalid))
        .transpose()
        .map(|v| v.unwrap_or(serde_json::Value::Null))
}
/// Introduced owners have empty, inactive state in earlier worlds. Unchanged
/// resources remain borrowed JSON, avoiding a second expanded world in memory.
pub fn normalize<'a>(
    saved: &'a str,
    defaults: impl FnOnce() -> Result<String>,
) -> Result<std::borrow::Cow<'a, str>> {
    use serde_json::{Value, value::RawValue};
    use std::{borrow::Cow, collections::BTreeMap};
    #[derive(serde::Deserialize)]
    struct Header {
        plugins: BTreeMap<String, u32>,
        resources: BTreeMap<String, serde::de::IgnoredAny>,
    }
    let header: Header = serde_json::from_str(saved).map_err(invalid)?;
    if OPTIONAL
        .iter()
        .all(|(plugin, _)| header.plugins.contains_key(*plugin))
        && (!header.plugins.contains_key("characters")
            || header.resources.contains_key(LOCAL_INTERACTION_PHASE))
    {
        return Ok(Cow::Borrowed(saved));
    }
    let mut fields: BTreeMap<String, &RawValue> = serde_json::from_str(saved).map_err(invalid)?;
    let input = fields
        .remove("resources")
        .ok_or_else(|| invalid("missing resources"))?;
    let resources: BTreeMap<String, &RawValue> =
        serde_json::from_str(input.get()).map_err(invalid)?;
    let mut resources: Raw<'_> = resources
        .into_iter()
        .map(|(k, v)| (k, Cow::Borrowed(v)))
        .collect();
    fields.remove("plugins");
    let defaults: Value = serde_json::from_str(&defaults()?).map_err(invalid)?;
    let mut plugins = header.plugins.clone();
    // A phase cannot survive a committed checkpoint. Older character saves
    // therefore have exactly the same state as an empty phase, without changing
    // any durable character state or accepting other missing resources.
    if header.plugins.contains_key("characters") && !resources.contains_key(LOCAL_INTERACTION_PHASE)
    {
        put(
            &mut resources,
            LOCAL_INTERACTION_PHASE,
            &serde_json::json!({}),
        )?;
    }
    for &(plugin, resource) in OPTIONAL {
        if !header.plugins.contains_key(plugin) {
            plugins.insert(
                plugin.into(),
                serde_json::from_value(defaults["plugins"][plugin].clone()).map_err(invalid)?,
            );
            if let Some(key) = resource {
                game_core::require(!resources.contains_key(key), "undeclared feature state")?;
                put(&mut resources, key, &defaults["resources"][key])?;
            }
        }
    }
    if !header.plugins.contains_key("memory") {
        memories::migrate(&mut resources)?;
        resources.remove("recollection.episodes"); // Content already resides in personal evidence.
    }
    if !resources.contains_key("search.goods") {
        put(
            &mut resources,
            "search.goods",
            &defaults["resources"]["search.goods"],
        )?;
    }
    if !header.plugins.contains_key("food-inquiry") {
        let mut observations = resource(&resources, "observations.episodes")?;
        let mut samples = resource(&resources, "food-inquiry.samples")?;
        for field in ["probes", "trials"] {
            if let Some(old) = observations.as_object_mut().and_then(|r| r.remove(field)) {
                samples[field] = old;
            }
        }
        put(&mut resources, "observations.episodes", &observations)?;
        put(&mut resources, "food-inquiry.samples", &samples)?;
    }
    if !header.plugins.contains_key("signals") {
        let mut communication = resource(&resources, "communication.experience")?;
        let mut signals = resource(&resources, "signals.experience")?;
        signals["enabled"] = communication["enabled"].clone();
        let state = communication
            .as_object_mut()
            .ok_or_else(|| invalid("missing communication state"))?;
        if let Some(perceptions) = state.remove("perceptions") {
            signals["perceptions"] = perceptions;
        }
        put(&mut resources, "communication.experience", &communication)?;
        put(&mut resources, "signals.experience", &signals)?;
    }
    if !header.plugins.contains_key("comfort") {
        let shelter = resource(&resources, "shelter.rooms")?;
        if let Some(fuel) = shelter.get("fuel_for") {
            let mut comfort = resource(&resources, "comfort.intentions")?;
            comfort["fuel_for"] = fuel.clone();
            put(&mut resources, "comfort.intentions", &comfort)?;
        }
    }
    if !resources.contains_key("confinement.barriers") {
        put(
            &mut resources,
            "confinement.barriers",
            &serde_json::json!({}),
        )?;
    }
    #[derive(serde::Serialize)]
    struct Normalized<'a, 'b> {
        plugins: &'a BTreeMap<String, u32>,
        resources: &'a Raw<'b>,
        #[serde(flatten)]
        fields: BTreeMap<String, &'b RawValue>,
    }
    Ok(Cow::Owned(
        serde_json::to_string(&Normalized {
            plugins: &plugins,
            resources: &resources,
            fields,
        })
        .map_err(invalid)?,
    ))
}

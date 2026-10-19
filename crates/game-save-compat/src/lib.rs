//! Normalize historical save ownership without changing live feature rules.
use game_core::Result;
const OPTIONAL: &[(&str, Option<&str>)] = &[
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
/// Introduced owners have empty, inactive state in earlier worlds.
pub fn normalize<'a>(
    saved: &'a str,
    defaults: impl FnOnce() -> Result<String>,
) -> Result<std::borrow::Cow<'a, str>> {
    #[derive(serde::Deserialize)]
    struct Header {
        plugins: std::collections::BTreeMap<String, u32>,
    }
    let header: Header =
        serde_json::from_str(saved).map_err(|e| game_core::Error::InvalidSave(e.to_string()))?;
    if OPTIONAL
        .iter()
        .all(|(plugin, _)| header.plugins.contains_key(*plugin))
    {
        return Ok(std::borrow::Cow::Borrowed(saved));
    }
    let mut value: serde_json::Value =
        serde_json::from_str(saved).map_err(|e| game_core::Error::InvalidSave(e.to_string()))?;
    let defaults: serde_json::Value = serde_json::from_str(&defaults()?).unwrap();
    for &(plugin, resource) in OPTIONAL {
        if !header.plugins.contains_key(plugin) {
            value["plugins"][plugin] = defaults["plugins"][plugin].clone();
            if let Some(resource) = resource {
                game_core::require(
                    value["resources"].get(resource).is_none(),
                    "undeclared feature state",
                )?;
                value["resources"][resource] = defaults["resources"][resource].clone();
            }
        }
    }
    if value["resources"].get("search.goods").is_none() {
        value["resources"]["search.goods"] = defaults["resources"]["search.goods"].clone();
    }
    if !header.plugins.contains_key("food-inquiry") {
        for field in ["probes", "trials"] {
            if let Some(old) = value["resources"]["observations.episodes"]
                .as_object_mut()
                .and_then(|r| r.remove(field))
            {
                value["resources"]["food-inquiry.samples"][field] = old;
            }
        }
    }
    if !header.plugins.contains_key("signals") {
        value["resources"]["signals.experience"]["enabled"] =
            value["resources"]["communication.experience"]["enabled"].clone();
        if let Some(perceptions) = value["resources"]["communication.experience"]
            .as_object_mut()
            .unwrap()
            .remove("perceptions")
        {
            value["resources"]["signals.experience"]["perceptions"] = perceptions;
        }
    }
    if !header.plugins.contains_key("comfort")
        && let Some(fuel) = value["resources"]["shelter.rooms"].get("fuel_for").cloned()
    {
        value["resources"]["comfort.intentions"]["fuel_for"] = fuel;
    }
    if value["resources"].get("confinement.barriers").is_none() {
        value["resources"]["confinement.barriers"] = serde_json::json!({});
    }
    Ok(std::borrow::Cow::Owned(
        serde_json::to_string(&value).unwrap(),
    ))
}

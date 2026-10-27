//! Strict adapter for a deliberately small physical legacy subset.
use game_history_kernel::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportIssue {
    pub path: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportReport {
    pub supported: bool,
    pub people: usize,
    pub source_resources: Vec<String>,
    pub issues: Vec<ImportIssue>,
    pub behavior_changes: Vec<String>,
}
const SUPPORTED: &[&str] = &[
    "physiology.bodies",
    "transport.positions",
    "land.areas",
    "cargo.inventories",
    "world.actions",
];
fn issue(report: &mut ImportReport, path: impl Into<String>, reason: impl Into<String>) {
    report.issues.push(ImportIssue {
        path: path.into(),
        reason: reason.into(),
    });
}
pub fn inspect_legacy(text: &str) -> Result<ImportReport, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    audit(&root)
}
fn audit(root: &Value) -> Result<ImportReport, String> {
    let resources = root
        .get("resources")
        .and_then(Value::as_object)
        .ok_or("legacy resources missing")?;
    let mut report=ImportReport{supported:false,people:0,source_resources:resources.keys().cloned().collect(),issues:vec![],behavior_changes:vec![
        "New minute-based metabolism, lifespans and persistent activities replace legacy rules; health is scaled by 60.".into(),
        "Only physical people, local positions and empty inventories are supported. Existing social, cultural, genealogical and genetic owners require migration and are rejected.".into(),
        "Synthetic display names and deterministic genomes are assigned only when their original owners are absent.".into(),
    ]};
    if root.get("version").and_then(Value::as_u64) != Some(1) {
        issue(&mut report, "version", "unsupported legacy manifest");
    }
    if root
        .get("time")
        .and_then(Value::as_u64)
        .is_none_or(|t| t % 60_000 != 0)
    {
        issue(&mut report, "time", "requires an exact minute boundary");
    }
    for key in ["jobs", "notices", "receipts"] {
        let nonempty = match root.get(key) {
            Some(Value::Array(a)) => !a.is_empty(),
            Some(Value::Object(a)) => !a.is_empty(),
            None => false,
            _ => true,
        };
        if nonempty {
            issue(
                &mut report,
                key,
                "jobs, historical notices and command receipts require explicit migration",
            );
        }
    }
    for key in resources.keys() {
        if !SUPPORTED.contains(&key.as_str()) {
            issue(
                &mut report,
                format!("resources.{key}"),
                "durable owner has no migration; original file must be retained",
            );
        }
    }
    for key in ["physiology.bodies", "transport.positions", "land.areas"] {
        if !resources.contains_key(key) {
            issue(
                &mut report,
                format!("resources.{key}"),
                "required physical owner missing",
            );
        }
    }
    for key in ["world.actions", "cargo.inventories"] {
        if let Some(value) = resources.get(key) {
            if value.as_object().is_none_or(|a| !a.is_empty()) {
                issue(
                    &mut report,
                    format!("resources.{key}"),
                    "active actions and inventory identity/goods conversion not implemented",
                );
            }
        }
    }
    if let Some(bodies) = resources
        .get("physiology.bodies")
        .and_then(|v| v.get("people"))
        .and_then(Value::as_object)
    {
        report.people = bodies.len();
        for (id, body) in bodies {
            if id.parse::<u64>().is_err()
                || ["born", "hunger", "fatigue", "health"]
                    .iter()
                    .any(|key| body.get(*key).and_then(Value::as_u64).is_none())
                || body.get("alive").and_then(Value::as_bool).is_none()
            {
                issue(&mut report, format!("people.{id}"), "malformed body");
            }
            if body.get("kind").and_then(Value::as_str) != Some("Mortal") {
                issue(
                    &mut report,
                    format!("people.{id}.kind"),
                    "guardian semantics unsupported",
                );
            }
            if resources
                .get("transport.positions")
                .and_then(|v| v.get(id))
                .and_then(|v| v.get("At"))
                .and_then(Value::as_u64)
                .is_none()
            {
                issue(
                    &mut report,
                    format!("people.{id}.position"),
                    "journeys require explicit conversion",
                );
            }
        }
    } else {
        issue(
            &mut report,
            "resources.physiology.bodies.people",
            "missing body rows",
        );
    }
    report.supported = report.issues.is_empty();
    Ok(report)
}
pub fn import_legacy(text: &str) -> Result<World, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let report = audit(&root)?;
    if !report.supported {
        return Err(serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
    }
    let r = &root["resources"];
    let now = root["time"].as_u64().unwrap() / 60_000;
    let seed = root["seed"].as_u64().ok_or("missing legacy seed")?;
    let areas = r["land.areas"]
        .as_object()
        .ok_or("invalid land areas")?
        .iter()
        .map(|(id, a)| {
            Ok(Settlement {
                id: id.parse().map_err(|_| "invalid area id")?,
                name: a["name"].as_str().ok_or("missing area name")?.into(),
                food: 0,
                capacity: 1_000_000,
                produced: 0,
                consumed: 0,
                spoiled: 0,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut people = vec![];
    for (key, b) in r["physiology.bodies"]["people"].as_object().unwrap() {
        let id = key.parse::<u64>().map_err(|_| "invalid person id")?;
        let born = b["born"].as_u64().unwrap() / 60_000;
        let alive = b["alive"].as_bool().unwrap();
        let health = b["health"]
            .as_u64()
            .unwrap()
            .checked_mul(60)
            .ok_or("health conversion overflow")?;
        let dies_at = b
            .get("life_limit")
            .and_then(Value::as_u64)
            .map(|t| t / 60_000)
            .unwrap_or(
                born.checked_add(80 * 365 * DAY)
                    .ok_or("lifespan overflow")?,
            );
        people.push(Person {
            id,
            name: format!("Imported person {id}"),
            born,
            dies_at,
            parents: vec![],
            genome: [(random(seed, id, 0) % 2) as u16, 500, 500, 500],
            area: r["transport.positions"][key]["At"].as_u64().unwrap(),
            alive,
            controlled: false,
            dependent: now.saturating_sub(born) < 12 * 365 * DAY,
            hunger: b["hunger"].as_u64().unwrap(),
            fatigue: b["fatigue"].as_u64().unwrap(),
            health,
            activity: Activity::Idle,
            work_minutes: 0,
            accounted_work: 0,
            social_minutes: 0,
        });
    }
    let society = Society::new(
        people
            .iter()
            .map(|p| SocialPerson {
                id: p.id,
                ..Default::default()
            })
            .collect(),
    )?;
    World::new(
        seed,
        now,
        Population::new(people)?,
        Economy::new(areas)?,
        society,
    )
}

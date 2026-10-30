//! Discover and choose data-authored starts for the headless client.
use game_parallel::World;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

const CATALOG: &str = "content/parallel/scenarios.toml";
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub id: String,
    pub name: String,
    pub world: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    scenarios: Vec<Scenario>,
}
pub enum Selection {
    List(Vec<Scenario>),
    World {
        world: Box<World>,
        label: Option<String>,
    },
}
fn catalog() -> Result<Vec<Scenario>, String> {
    let text = std::fs::read_to_string(CATALOG).map_err(|error| error.to_string())?;
    let mut entries: Vec<Scenario> = toml::from_str::<Catalog>(&text)
        .map_err(|error| format!("invalid scenario catalog: {error}"))?
        .scenarios;
    let mut ids = HashSet::new();
    if entries.is_empty()
        || entries.iter().any(|entry| {
            entry.id.trim().is_empty()
                || entry.name.trim().is_empty()
                || entry.world.trim().is_empty()
                || !ids.insert(entry.id.clone())
        })
    {
        return Err("invalid scenario catalog entries".into());
    }
    entries.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    Ok(entries)
}
fn seed(args: &[String]) -> Result<Option<u64>, String> {
    match args {
        [] => Ok(None),
        [flag, value] if flag == "--seed" => {
            value.parse().map(Some).map_err(|_| "invalid seed".into())
        }
        _ => Err("expected an optional --seed N".into()),
    }
}
fn read_world(path: &Path, seed: Option<u64>, label: Option<&str>) -> Result<Selection, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut world = World::from_toml(&text)?;
    if let Some(seed) = seed {
        world.seed = seed;
    }
    let label = label.map(|label| format!("scenario={label} seed={}", world.seed));
    Ok(Selection::World {
        world: Box::new(world),
        label,
    })
}
fn named(entries: &[Scenario], id: &str, seed: Option<u64>) -> Result<Selection, String> {
    let entry = entries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or("unknown scenario")?;
    let path = Path::new(CATALOG)
        .parent()
        .expect("catalog parent")
        .join(&entry.world);
    read_world(&path, seed, Some(&entry.id))
}
/// Parse a client source: an authored path, a catalog start, or a generated world.
pub fn select(args: &[String], player_mode: bool) -> Result<Selection, String> {
    match args.first().map(String::as_str) {
        Some("--list") if args.len() == 1 => Ok(Selection::List(catalog()?)),
        Some("--scenario") => {
            let id = args.get(1).ok_or("missing scenario")?;
            named(&catalog()?, id, seed(&args[2..])?)
        }
        Some("--random") => {
            let seed = seed(&args[1..])?.unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64);
            let entries = catalog()?;
            let index = (game_parallel_genetics::random(seed, 0, 0) % entries.len() as u64) as usize;
            named(&entries, &entries[index].id, Some(seed))
        }
        Some("--generate") => {
            let path = args.get(1).ok_or("missing world generation settings")?;
            let world = game_parallel_worldgen::load(Path::new(path), seed(&args[2..])?)?;
            let label = Some(format!("generated seed={}", world.seed));
            Ok(Selection::World { world: Box::new(world), label })
        }
        Some(path) if !path.starts_with("--") => read_world(Path::new(path), seed(&args[1..])?, None),
        None if player_mode => named(&catalog()?, "campaign", None),
        None => read_world(Path::new("content/parallel/example.toml"), None, None),
        _ => Err("usage: parallel-block-cli [--player] [--list | --scenario ID | --random | --generate SETTINGS | WORLD.toml] [--seed N]".into()),
    }
}

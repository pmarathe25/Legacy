//! Discover data files and validate pack dependencies before catalog compilation.
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::Path;
use toml::Value;

const FORMAT_VERSION: i64 = 1;

struct Pack {
    id: String,
    namespace: Option<String>,
    dependencies: Vec<String>,
    rows: BTreeMap<String, Value>,
}

pub(crate) struct Definition {
    pub(crate) key: String,
    pub(crate) namespace: Option<String>,
    pub(crate) runtime_id: Option<u32>,
    pub(crate) row: Value,
}

fn read(directory: &Path, kind: &str) -> Result<Vec<Pack>, String> {
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| error.to_string())
        })
        .collect::<Result<_, _>>()?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "toml")
    });
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let source = std::fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            parse(&source, kind).map_err(|error| format!("{}: {error}", path.display()))
        })
        .collect()
}

fn parse(source: &str, kind: &str) -> Result<Pack, String> {
    let document: Value = toml::from_str(source).map_err(|error| error.to_string())?;
    let id = document
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or("invalid content pack identity")?;
    let version = document
        .get("version")
        .and_then(Value::as_integer)
        .ok_or("missing content pack version")?;
    if version != FORMAT_VERSION {
        return Err(format!("unsupported content pack version {version}"));
    }
    let namespace = document
        .get("namespace")
        .map(|value| {
            value
                .as_str()
                .filter(|namespace| {
                    !namespace.is_empty()
                        && namespace.bytes().all(|byte| {
                            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                        })
                })
                .map(str::to_owned)
                .ok_or("invalid content namespace")
        })
        .transpose()?;
    let mut dependencies: Vec<_> = document
        .get("requires")
        .map(|requires| {
            requires
                .as_table()
                .ok_or("invalid content dependency table")?
                .iter()
                .map(|(name, version)| {
                    if name.is_empty() || name == id || version.as_integer() != Some(FORMAT_VERSION)
                    {
                        return Err("invalid content dependency");
                    }
                    Ok(name.clone())
                })
                .collect::<Result<_, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    dependencies.sort_unstable();
    let rows = document
        .get("data")
        .and_then(|data| data.get(kind))
        .and_then(Value::as_table)
        .ok_or_else(|| format!("missing {kind}"))?
        .iter()
        .map(|(key, row)| (key.clone(), row.clone()))
        .collect();
    Ok(Pack {
        id: id.to_owned(),
        namespace,
        dependencies,
        rows,
    })
}

pub(crate) fn load(
    objects: &Path,
    recipes: &Path,
) -> Result<(Vec<Definition>, Vec<Definition>), String> {
    let objects = read(objects, "objects")?;
    let recipes = read(recipes, "recipes")?;
    let packs: Vec<_> = objects.iter().chain(&recipes).collect();
    let index: HashMap<_, _> = packs.iter().map(|pack| (pack.id.as_str(), *pack)).collect();
    if index.len() != packs.len() {
        return Err("duplicate content pack identity".into());
    }
    let mut dependants: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut remaining: HashMap<_, _> = packs
        .iter()
        .map(|pack| (pack.id.as_str(), pack.dependencies.len()))
        .collect();
    for pack in &packs {
        for dependency in &pack.dependencies {
            if !index.contains_key(dependency.as_str()) {
                return Err(format!(
                    "content pack {} requires missing pack {dependency}",
                    pack.id
                ));
            }
            dependants
                .entry(dependency.as_str())
                .or_default()
                .push(pack.id.as_str());
        }
    }
    let mut ready: VecDeque<_> = packs
        .iter()
        .filter(|pack| pack.dependencies.is_empty())
        .map(|pack| pack.id.as_str())
        .collect();
    let mut loaded = 0;
    while let Some(id) = ready.pop_front() {
        loaded += 1;
        for &next in dependants.get(id).into_iter().flatten() {
            let count = remaining.get_mut(next).expect("known dependency consumer");
            *count -= 1;
            if *count == 0 {
                ready.push_back(next);
            }
        }
    }
    if loaded != packs.len() {
        return Err("cyclic content pack dependencies".into());
    }
    Ok((collect(objects, "objects")?, collect(recipes, "recipes")?))
}

fn collect(packs: Vec<Pack>, kind: &str) -> Result<Vec<Definition>, String> {
    let mut entries = BTreeMap::new();
    for pack in packs {
        for (name, row) in pack.rows {
            if name.is_empty() || name.contains(':') {
                return Err(format!("invalid key {name} in pack {}", pack.id));
            }
            let key = pack
                .namespace
                .as_ref()
                .map_or_else(|| name.clone(), |namespace| format!("{namespace}:{name}"));
            let runtime_id = if pack.namespace.is_some() {
                let id = row
                    .get("runtime_id")
                    .and_then(Value::as_integer)
                    .and_then(|id| u32::try_from(id).ok())
                    .filter(|&id| id != 0)
                    .ok_or_else(|| {
                        format!("{key}: namespaced definitions require a positive runtime_id")
                    })?;
                Some(id)
            } else {
                None
            };
            let entry = Definition {
                key: key.clone(),
                namespace: pack.namespace.clone(),
                runtime_id,
                row,
            };
            if entries.insert(key.clone(), entry).is_some() {
                return Err(format!("duplicate {kind} key: {key}"));
            }
        }
    }
    if entries.is_empty() {
        return Err(format!("no {kind} in the content packs"));
    }
    let mut entries: Vec<_> = entries.into_values().collect();
    entries.sort_by_key(|entry| entry.runtime_id.is_some());
    Ok(entries)
}

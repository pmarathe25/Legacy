//! Resolve chosen creative activities from the authored pastime catalogs.
use game_pastimes::{Activity, Mind, Pastimes};
use game_production::{Amount, Good};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub catalogs: Vec<PathBuf>,
    pub activities: BTreeMap<String, u32>,
    #[serde(default)]
    pub minds: Vec<Mind>,
}

#[derive(Deserialize)]
struct Pack {
    version: u64,
    data: PackData,
}

#[derive(Deserialize)]
struct PackData {
    activities: BTreeMap<String, Definition>,
}

#[derive(Deserialize)]
struct Definition {
    name: String,
    skill: String,
    duration: u64,
    satisfaction_span: u64,
    inputs: BTreeMap<String, u64>,
    requires: BTreeMap<String, u64>,
    #[serde(default)]
    marks: bool,
}

impl Config {
    pub(crate) fn compile(self, content: &Path, goods: &[Good]) -> Result<Pastimes, String> {
        let mut definitions = BTreeMap::new();
        for file in self.catalogs {
            let path = content.join(file);
            let source = std::fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let pack: Pack =
                toml::from_str(&source).map_err(|error| format!("{}: {error}", path.display()))?;
            if pack.version != 1 {
                return Err(format!(
                    "unsupported pastime catalog version {}",
                    pack.version
                ));
            }
            for (key, definition) in pack.data.activities {
                if definitions.insert(key.clone(), definition).is_some() {
                    return Err(format!("duplicate pastime {key}"));
                }
            }
        }
        let goods: HashMap<_, _> = goods
            .iter()
            .map(|good| (good.name.as_str(), good.id))
            .collect();
        let amounts = |rows: BTreeMap<String, u64>| {
            let mut amounts: Vec<_> = rows
                .into_iter()
                .map(|(good, quantity)| {
                    Ok(Amount {
                        good: *goods
                            .get(good.as_str())
                            .ok_or_else(|| format!("unknown pastime good {good}"))?,
                        quantity,
                    })
                })
                .collect::<Result<_, String>>()?;
            amounts.sort_unstable_by_key(|row| row.good);
            Ok::<_, String>(amounts)
        };
        let mut activities: Vec<_> = self
            .activities
            .into_iter()
            .map(|(key, id)| {
                let definition = definitions
                    .remove(&key)
                    .ok_or_else(|| format!("unknown pastime {key}"))?;
                if definition.marks {
                    return Err(format!("{key} requires the physical writing actions"));
                }
                Ok(Activity {
                    id,
                    name: definition.name,
                    skill: definition.skill,
                    duration_millis: definition.duration,
                    satisfaction_millis: definition.satisfaction_span,
                    materials: amounts(definition.inputs)?,
                    tools: amounts(definition.requires)?,
                })
            })
            .collect::<Result<_, String>>()?;
        activities.sort_unstable_by_key(|row| row.id);
        let mut minds = self.minds;
        minds.sort_unstable_by_key(|row| row.person);
        for mind in &mut minds {
            mind.interests.sort_unstable_by_key(|row| row.activity);
        }
        Ok(Pastimes {
            activities,
            minds,
            active: Vec::new(),
            training: Default::default(),
        })
    }
}

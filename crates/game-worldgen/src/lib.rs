//! Seed physical initial conditions, not societies or fabricated historical outcomes.
use game_core::*;
use game_scenarios::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub population: usize,
    pub areas: usize,
    pub history: SimDuration,
    pub physiology: String,
    pub characters: String,
    pub social_life: String,
    pub health: String,
    pub biomes: Vec<Biome>,
    pub name_starts: Vec<String>,
    pub name_ends: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Biome {
    pub name: String,
    pub climate: String,
    pub food: String,
    pub material: String,
    pub renewal_warmth: i32,
    pub renewal_rain: u32,
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        require(
            (4..=9_998).contains(&self.population)
                && (1..=128).contains(&self.areas)
                && self.areas <= self.population / 2
                && !self.biomes.is_empty()
                && !self.name_starts.is_empty()
                && !self.name_ends.is_empty()
                && self
                    .name_starts
                    .iter()
                    .chain(&self.name_ends)
                    .all(|s| !s.trim().is_empty())
                && self.biomes.iter().all(|b| !b.name.trim().is_empty()),
            "invalid world generation settings",
        )
    }
}
pub fn load(store: &mut game_content::Store) -> Result<Settings> {
    let doc: game_content::Document<Settings> = store.load("worldgen/default.toml")?;
    doc.data.validate()?;
    Ok(doc.data)
}
pub fn generate(
    settings: &Settings,
    seed: u64,
    catalog: &game_traits::Traits,
) -> Result<Definition> {
    settings.validate()?;
    require(
        catalog.definitions.values().all(|t| t.minimum < t.maximum),
        "invalid generated trait range",
    )?;
    let draw = |stream, index| random(seed, stream, index);
    let name = |index: u64| {
        format!(
            "{}{}",
            settings.name_starts[(draw(1, index) % settings.name_starts.len() as u64) as usize],
            settings.name_ends[(draw(2, index) % settings.name_ends.len() as u64) as usize]
        )
    };
    let mut d = Definition {
        label: "Generated world".into(),
        fixture: false,
        ruler: "person-0".into(),
        physiology: settings.physiology.clone(),
        characters: settings.characters.clone(),
        environment: settings.biomes[0].climate.clone(),
        health: settings.health.clone(),
        invocation: None,
        social_life: Some(settings.social_life.clone()),
        bonds: vec![],
        autonomy: true,
        renewal: true,
        areas: vec![],
        links: vec![],
        people: vec![],
        inventories: vec![],
        plots: vec![],
        patches: vec![],
        institutions: vec![],
        combat: None,
        reincarnation: None,
    };
    for index in 0..settings.areas {
        let biome =
            &settings.biomes[(draw(3, index as u64) % settings.biomes.len() as u64) as usize];
        let area = format!("area-{index}");
        d.areas.push(Area {
            key: area.clone(),
            name: format!("{} {}", name(index as u64), biome.name),
            climate: Some(biome.climate.clone()),
        });
        if index > 0 {
            // A seeded spanning tree guarantees physical connectivity without authored settlements.
            d.links.push(Link {
                from: format!("area-{}", draw(4, index as u64) % index as u64),
                to: area.clone(),
                duration: SimDuration((1 + draw(5, index as u64) % 5) * MINUTE.0),
            });
        }
        let locals = settings.population.div_ceil(settings.areas) as u64 + 2;
        for (kind, object, stock, renew) in [
            ("food", &biome.food, locals * 600, locals * 400),
            (
                "material",
                &biome.material,
                locals * 200,
                if ["stone", "iron-ore", "clay"].contains(&biome.material.as_str()) {
                    0
                } else {
                    locals * 20
                },
            ),
        ] {
            d.patches.push(Patch {
                key: format!("{kind}-{index}"),
                area: area.clone(),
                object: object.clone(),
                stock,
                capacity: stock,
                renew,
                renewal_warmth: (kind == "food").then_some(biome.renewal_warmth),
                renewal_rain: if kind == "food" {
                    biome.renewal_rain
                } else {
                    0
                },
            });
        }
    }
    for index in 0..settings.population + 2 {
        let guardian = index >= settings.population;
        let key = if guardian {
            format!("guardian-{}", index - settings.population + 1)
        } else {
            format!("person-{index}")
        };
        let area = format!("area-{}", if guardian { 0 } else { index % settings.areas });
        let traits = catalog
            .definitions
            .iter()
            .enumerate()
            .map(|(i, (key, t))| {
                let span = (i64::from(t.maximum) - i64::from(t.minimum) + 1) as u64;
                (
                    key.clone(),
                    (i64::from(t.minimum) + (draw(1000 + i as u64, index as u64) % span) as i64)
                        as i32,
                )
            })
            .collect();
        let belief = |stream| (draw(stream, index as u64) % 21) as i32 - 10;
        d.people.push(People {
            key: key.clone(),
            count: 1,
            kind: if guardian {
                game_physiology::Kind::Guardian
            } else {
                game_physiology::Kind::Mortal
            },
            area,
            traits,
            name: Some(format!(
                "{} {}",
                name(index as u64 + 1000),
                name(index as u64 + 2000)
            )),
            beliefs: game_relationships::Beliefs {
                sharing: belief(30),
                authority: belief(31),
                honesty: belief(32),
            },
            values: BTreeMap::from([("independence".into(), (draw(33, index as u64) % 61) as i32)]),
        });
        d.inventories.push(Inventory {
            key: format!("pack-{index}"),
            area: None,
            carried_by: Some(key.clone()),
            capacity: 2000,
            owner: key.clone(),
            access: vec![key.clone()],
            known_to: vec![key],
            goods: BTreeMap::from([("grain".into(), 1000)]),
            per_person: BTreeMap::new(),
        });
    }
    d.validate()?;
    Ok(d)
}

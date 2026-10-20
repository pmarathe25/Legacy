//! Crop catalogs resolve symbolic object references before world construction.
use game_content::{Document, Stamp, Store};
use game_core::*;
use game_objects::Objects;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CropSpec {
    pub name: String,
    pub seed_object: String,
    pub seed_input: u64,
    pub output_object: String,
    pub yield_units: u64,
    pub growth: SimDuration,
    pub sow_labor: SimDuration,
    pub harvest_labor: SimDuration,
    pub minimum_warmth: i32,
    pub minimum_rain: u32,
    #[serde(default)]
    pub maximum_warmth: Option<i32>,
    #[serde(default)]
    pub maximum_rain: Option<u32>,
    #[serde(default)]
    pub natural_growth: Option<SimDuration>,
    #[serde(default)]
    pub natural_yield: Option<u64>,
    #[serde(default)]
    pub regrowth: Option<SimDuration>,
    #[serde(default)]
    pub lifespan: Option<SimDuration>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crop {
    pub name: String,
    pub seed: GoodId,
    pub seed_input: u64,
    pub output: GoodId,
    pub yield_units: u64,
    pub growth: SimDuration,
    pub sow_labor: SimDuration,
    pub harvest_labor: SimDuration,
    pub minimum_warmth: i32,
    pub minimum_rain: u32,
    #[serde(default)]
    pub maximum_warmth: Option<i32>,
    #[serde(default)]
    pub maximum_rain: Option<u32>,
    #[serde(default)]
    pub natural_growth: Option<SimDuration>,
    #[serde(default)]
    pub natural_yield: Option<u64>,
    #[serde(default)]
    pub regrowth: Option<SimDuration>,
    #[serde(default)]
    pub lifespan: Option<SimDuration>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Growth {
    pub last: SimTime,
    pub total: u64,
    pub suited: u64,
    pub lethal: u64,
}
impl Growth {
    pub fn new(at: SimTime) -> Self {
        Self {
            last: at,
            total: 0,
            suited: 0,
            lethal: 0,
        }
    }
    pub fn advance(
        &mut self,
        climate: &game_environment::Climate,
        crop: &Crop,
        to: SimTime,
    ) -> Result<()> {
        let delta = to.0.saturating_sub(self.last.0);
        if delta == 0 {
            return Ok(());
        }
        self.suited = self.suited.saturating_add(
            climate
                .matching_duration(self.last, to, |s| {
                    s.warmth >= crop.minimum_warmth
                        && crop.maximum_warmth.is_none_or(|v| s.warmth <= v)
                        && s.rain >= crop.minimum_rain
                        && crop.maximum_rain.is_none_or(|v| s.rain <= v)
                })?
                .0,
        );
        self.lethal = self.lethal.saturating_add(
            climate
                .matching_duration(self.last, to, |s| {
                    s.warmth < crop.minimum_warmth.saturating_sub(10)
                        || crop
                            .maximum_warmth
                            .is_some_and(|v| s.warmth > v.saturating_add(10))
                })?
                .0,
        );
        self.total = self.total.saturating_add(delta);
        self.last = to;
        Ok(())
    }
    pub fn yield_units(&self, potential: u64) -> u64 {
        if self.lethal >= 2 * DAY.0 {
            0
        } else {
            (u128::from(potential) * u128::from(self.suited) / u128::from(self.total.max(1))) as u64
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Definitions {
    crops: BTreeMap<String, CropSpec>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crops {
    pub items: game_storage::Records<String, Crop>,
    pub stamps: Vec<Stamp>,
    #[serde(default)]
    pub natural: bool,
}
impl Resource for Crops {
    const KEY: &'static str = "agriculture.crops";
    fn validate(&self) -> Result<()> {
        require(
            self.items.iter().all(|(key, c)| {
                !key.is_empty()
                    && !c.name.is_empty()
                    && c.seed_input > 0
                    && c.yield_units > 0
                    && c.growth.0 > 0
                    && c.sow_labor.0 > 0
                    && c.harvest_labor.0 > 0
                    && c.maximum_warmth.is_none_or(|v| v >= c.minimum_warmth)
                    && c.maximum_rain.is_none_or(|v| v >= c.minimum_rain)
                    && c.natural_growth.is_none_or(|v| v.0 > 0)
                    && c.natural_yield.is_none_or(|v| v > 0)
                    && c.regrowth.is_none_or(|v| v.0 > 0)
                    && c.lifespan.is_none_or(|v| v.0 > 0)
            }),
            "invalid crop definition",
        )
    }
}
impl Crops {
    pub fn expressed(&self, key: &str, strain: Option<&game_cultivars::Strain>) -> Result<Crop> {
        self.items
            .get(key)
            .ok_or_else(|| Error::Missing("crop definition".into()))?
            .expressed(self.natural, strain)
    }
}
pub fn load(store: &mut Store, objects: &Objects) -> Result<Crops> {
    let mut items = BTreeMap::new();
    let mut stamps = vec![];
    for path in store.scan("agriculture")? {
        let doc: Document<Definitions> = store.load(&path)?;
        stamps.push(doc.stamp()?);
        for (key, c) in doc.data.crops {
            let crop = Crop {
                name: c.name,
                seed: objects.resolve(&c.seed_object)?,
                seed_input: c.seed_input,
                output: objects.resolve(&c.output_object)?,
                yield_units: c.yield_units,
                growth: c.growth,
                sow_labor: c.sow_labor,
                harvest_labor: c.harvest_labor,
                minimum_warmth: c.minimum_warmth,
                minimum_rain: c.minimum_rain,
                maximum_warmth: c.maximum_warmth,
                maximum_rain: c.maximum_rain,
                natural_growth: c.natural_growth,
                natural_yield: c.natural_yield,
                regrowth: c.regrowth,
                lifespan: c.lifespan,
            };
            require(items.insert(key, crop).is_none(), "duplicate crop key")?;
        }
    }
    let crops = Crops {
        items: items.into(),
        stamps,
        natural: false,
    };
    crops.validate()?;
    Ok(crops)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct InstallCrops(pub Crops);
impl Request for InstallCrops {
    const KEY: &'static str = "agriculture.install_crops";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Natural;
impl Request for Natural {
    const KEY: &'static str = "crops.natural";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("crops", 1)?;
    b.resource::<Crops>()?;
    b.request::<Natural>(|tx, _| {
        let crops: Vec<_> = tx
            .get::<Crops>()?
            .items
            .values()
            .map(|c| (c.seed, c.output))
            .collect();
        for (seed, produce) in crops {
            tx.call(game_cultivars::Define { seed, produce })?;
        }
        tx.call(game_cultivars::Enable)?;
        tx.edit::<Crops>()?.natural = true;
        Ok(())
    })?;
    b.request::<InstallCrops>(|tx, r| {
        r.0.validate()?;
        *tx.edit::<Crops>()? = r.0;
        Ok(())
    })
}

impl Crop {
    pub fn suitable(&self, s: &game_environment::Season) -> bool {
        s.warmth >= self.minimum_warmth
            && self.maximum_warmth.is_none_or(|v| s.warmth <= v)
            && s.rain >= self.minimum_rain
            && self.maximum_rain.is_none_or(|v| s.rain <= v)
    }
}
impl Crop {
    pub fn regrowth_at(
        &self,
        now: SimTime,
        established: Option<SimTime>,
        natural: bool,
    ) -> Option<SimDuration> {
        if natural
            && self
                .lifespan
                .is_none_or(|span| established.is_none_or(|at| now.0.saturating_sub(at.0) < span.0))
        {
            self.regrowth
        } else {
            None
        }
    }
    pub fn expressed(
        &self,
        natural: bool,
        strain: Option<&game_cultivars::Strain>,
    ) -> Result<Crop> {
        let mut c = self.clone();
        if natural {
            c.growth = c.natural_growth.unwrap_or(c.growth);
            c.yield_units = c.natural_yield.unwrap_or(c.yield_units);
        }
        if let Some(strain) = strain {
            let e = strain.expression();
            c.growth = e.growth(c.growth)?;
            c.regrowth = c.regrowth.map(|d| e.growth(d)).transpose()?;
            c.yield_units = (u128::from(c.yield_units) * u128::from(e.yield_factor) / 1000)
                .max(1)
                .min(u128::from(u64::MAX)) as u64;
            c.minimum_warmth = c.minimum_warmth.saturating_sub(e.cold);
            c.maximum_warmth = c
                .maximum_warmth
                .map(|v| v.saturating_add(e.heat).saturating_sub(e.cold / 4));
            c.minimum_rain = (i64::from(c.minimum_rain) - i64::from(e.drought))
                .clamp(0, i64::from(u32::MAX)) as u32;
            c.maximum_rain = c
                .maximum_rain
                .map(|v| (i64::from(v) + i64::from(e.wet)).clamp(0, i64::from(u32::MAX)) as u32);
            c.lifespan = c.lifespan.map(|d| {
                SimDuration(
                    (u128::from(d.0) * u128::from(e.lifespan_factor) / 1000)
                        .min(u128::from(u64::MAX))
                        .max(1) as u64,
                )
            });
        }
        Ok(c)
    }
}

//! Crop catalogs resolve symbolic object references before world construction.
use super::*;
use game_content::{Document, Stamp, Store};
use game_objects::Objects;
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
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Definitions {
    crops: BTreeMap<String, CropSpec>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crops {
    pub items: BTreeMap<String, Crop>,
    pub stamps: Vec<Stamp>,
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
            }),
            "invalid crop definition",
        )
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
            };
            require(items.insert(key, crop).is_none(), "duplicate crop key")?;
        }
    }
    let crops = Crops { items, stamps };
    crops.validate()?;
    Ok(crops)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct InstallCrops(pub Crops);
impl Request for InstallCrops {
    const KEY: &'static str = "agriculture.install_crops";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.resource::<Crops>()?;
    b.request::<InstallCrops>(|tx, r| {
        r.0.validate()?;
        require(
            tx.get::<Farms>()?.plots.is_empty(),
            "crop definitions are in use",
        )?;
        *tx.edit::<Crops>()? = r.0;
        Ok(())
    })
}
pub(super) fn crop(tx: &Transaction<'_>, plot: PlotId) -> Result<Crop> {
    let p = tx
        .get::<Farms>()?
        .plots
        .get(&plot)
        .ok_or_else(|| Error::Missing("plot".into()))?;
    tx.get::<Crops>()?
        .items
        .get(&p.crop)
        .cloned()
        .ok_or_else(|| Error::Missing("crop definition".into()))
}

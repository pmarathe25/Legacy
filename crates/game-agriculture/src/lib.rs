//! Crop phases, paid seed inputs, seasonal growth, and real harvest labor.
use game_storage::Table;
mod genetics;
mod harvest;
pub use game_crops::Natural as EnableNatural;
pub use game_crops::{Crop, CropSpec, Crops, InstallCrops, load};
pub use harvest::HarvestSome;
mod encroachment;
pub use encroachment::Encroached;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    #[default]
    Fallow,
    Sowing,
    Growing {
        ready: SimTime,
    },
    Ripe {
        yield_units: u64,
    },
    Harvesting {
        yield_units: u64,
    },
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plot {
    pub area: AreaId,
    pub steward: PersonId,
    pub phase: Phase,
    pub crop: String,
    #[serde(default)]
    pub strain: Option<game_cultivars::Strain>,
    #[serde(default)]
    pub established: Option<SimTime>,
    #[serde(default)]
    pub harvests: u64,
    #[serde(default)]
    pub progress: Option<game_crops::Growth>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Work {
    plot: PlotId,
    job: ScheduleId,
    destination: InventoryId,
    reservation: Option<u64>,
    #[serde(default)]
    remaining: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Farms {
    pub plots: Table<PlotId, Plot>,
    work: Table<PersonId, Work>,
    pub lost_seed: u64,
}
impl Resource for Farms {
    fn allocated_ids(&self) -> Vec<u64> {
        self.plots.keys().map(|id| id.0).collect()
    }
    const KEY: &'static str = "agriculture.plots";
    fn validate(&self) -> Result<()> {
        self.validate_rows(self.plots.values())
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        self.validate_rows(self.plots.changed(&previous.plots).map(|(_, row)| row))
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Create(pub Plot);
impl Request for Create {
    const KEY: &'static str = "agriculture.create";
    type Response = PlotId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Sow {
    pub person: PersonId,
    pub plot: PlotId,
    pub seed: InventoryId,
}
impl Request for Sow {
    const KEY: &'static str = "agriculture.sow";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Planted {
    person: PersonId,
}
impl Request for Planted {
    const KEY: &'static str = "agriculture.planted";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Ripe {
    plot: PlotId,
}
impl Request for Ripe {
    const KEY: &'static str = "agriculture.ripe";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Harvest {
    pub person: PersonId,
    pub plot: PlotId,
    pub destination: InventoryId,
}
impl Request for Harvest {
    const KEY: &'static str = "agriculture.harvest";
    type Response = SimTime;
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Harvested {
    pub person: PersonId,
    pub plot: PlotId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Event for Harvested {
    const KEY: &'static str = "agriculture.harvested";
}
#[derive(Clone, Serialize, Deserialize)]
struct Finish {
    person: PersonId,
}
impl Request for Finish {
    const KEY: &'static str = "agriculture.finish";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel {
    person: PersonId,
}
impl Request for Cancel {
    const KEY: &'static str = "agriculture.cancel";
    type Response = ();
}
fn local(
    tx: &Transaction<'_>,
    person: PersonId,
    plot: PlotId,
    inventory: InventoryId,
) -> Result<()> {
    game_physiology::capable(tx, person)?;
    let p = tx
        .get::<Farms>()?
        .plots
        .get(&plot)
        .ok_or_else(|| Error::Missing("plot".into()))?;
    require(
        game_transport::area(tx.get()?, person)? == p.area
            && game_cargo::locality(tx, inventory)? == p.area,
        "farm work is not local",
    )
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    game_crops::plugin(b)?;
    b.plugin("agriculture", 1)?;
    b.resource::<Farms>()?;
    genetics::register(b)?;
    b.validate_save(|h| {
        let s = h.get::<Farms>()?;
        for plot in s.plots.values() {
            require(
                h.get::<Crops>()?.items.contains_key(&plot.crop),
                "unknown crop in saved plot",
            )?;
        }
        for (person, w) in &s.work {
            require(
                s.plots.contains_key(&w.plot)
                    && h.get::<Actions>()?
                        .0
                        .get(person)
                        .is_some_and(|a| a.kind == "farm"),
                "farm work lacks plot or action",
            )?;
        }
        Ok(())
    });
    b.request::<Create>(|tx, r| {
        require(r.0.phase == Phase::Fallow, "new plot must be fallow")?;
        require(
            tx.get::<Crops>()?.items.contains_key(&r.0.crop),
            "unknown crop",
        )?;
        let id = PlotId(tx.allocate()?);
        tx.edit::<Farms>()?.plots.insert(id, r.0);
        Ok(id)
    })?;
    b.request::<Sow>(|tx, r| {
        local(tx, r.person, r.plot, r.seed)?;
        encroachment::note(tx, r.person, r.plot, "sowing")?;
        require(
            tx.get::<Farms>()?.plots[&r.plot].phase == Phase::Fallow,
            "plot is occupied",
        )?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.seed,
            purpose: "sowing".into(),
        })?;
        let crop = genetics::crop(tx, r.plot)?;
        genetics::select(tx, r.person, r.plot, r.seed, crop.seed)?;
        tx.call(game_cargo::Consume {
            inventory: r.seed,
            good: crop.seed,
            quantity: crop.seed_input,
        })?;
        let until = tx.now().after(crop.sow_labor)?;
        tx.start_action(r.person, "farm", Some(until), Cancel { person: r.person })?;
        let job = tx.schedule(until, 10, Planted { person: r.person })?;
        let s = tx.edit::<Farms>()?;
        s.plots.get_mut(&r.plot).unwrap().phase = Phase::Sowing;
        s.work.insert(
            r.person,
            Work {
                plot: r.plot,
                job,
                destination: r.seed,
                reservation: None,
                remaining: 0,
            },
        );
        Ok(until)
    })?;
    b.request::<Planted>(|tx, r| {
        let w = tx.edit::<Farms>()?.work.remove(&r.person).unwrap();
        let ready = tx.now().after(genetics::crop(tx, w.plot)?.growth)?;
        let progress = game_crops::Growth::new(tx.now());
        tx.edit::<Farms>()?.plots.get_mut(&w.plot).unwrap().progress = Some(progress);
        tx.edit::<Farms>()?.plots.get_mut(&w.plot).unwrap().phase = Phase::Growing { ready };
        tx.finish_action(r.person, "farm")?;
        tx.schedule(ready, 25, Ripe { plot: w.plot })?;
        tx.call(game_skills::Practice {
            person: r.person,
            skill: "farm".into(),
            effort: 60,
            attribution: w.job.0,
        })?;
        Ok(())
    })?;
    b.request::<Ripe>(|tx, r| {
        let crop = genetics::crop(tx, r.plot)?;
        let season = game_environment::at(tx, tx.get::<Farms>()?.plots[&r.plot].area)?;
        let productive = season.warmth >= crop.minimum_warmth
            && season.rain >= crop.minimum_rain
            && crop.maximum_warmth.is_none_or(|v| season.warmth <= v)
            && crop.maximum_rain.is_none_or(|v| season.rain <= v);
        genetics::update(tx, r.plot, None)?;
        let plot = tx.edit::<Farms>()?.plots.get_mut(&r.plot).unwrap();
        plot.phase = Phase::Ripe {
            yield_units: if let Some(growth) = &plot.progress {
                growth.yield_units(crop.yield_units)
            } else if productive {
                crop.yield_units
            } else {
                0
            },
        };
        Ok(())
    })?;
    harvest::register(b)?;
    b.request::<Finish>(|tx, r| {
        let w = tx.get::<Farms>()?.work[&r.person].clone();
        let Phase::Harvesting { yield_units } = tx.get::<Farms>()?.plots[&w.plot].phase else {
            return Err(Error::Rejected("harvest has changed".into()));
        };
        if yield_units > 0 {
            genetics::harvest(tx, r.person, w.plot, w.destination, yield_units)?;
            tx.call(game_cargo::DepositReserved {
                reservation: w.reservation.unwrap(),
                inventory: w.destination,
                good: genetics::crop(tx, w.plot)?.output,
                quantity: yield_units,
            })?;
        }
        genetics::after_harvest(tx, w.plot, w.remaining)?;
        tx.edit::<Farms>()?.work.remove(&r.person);
        tx.finish_action(r.person, "farm")?;
        tx.call(game_skills::Practice {
            person: r.person,
            skill: "farm".into(),
            effort: 60,
            attribution: w.job.0,
        })?;
        tx.emit(
            Harvested {
                person: r.person,
                plot: w.plot,
                good: genetics::crop(tx, w.plot)?.output,
                quantity: yield_units,
            },
            vec![r.person],
        )
    })?;
    b.request::<Cancel>(|tx, r| {
        if let Some(w) = tx.edit::<Farms>()?.work.remove(&r.person) {
            tx.unschedule(w.job);
            if let Some(reservation) = w.reservation {
                tx.call(game_cargo::ReleaseSpace {
                    inventory: w.destination,
                    reservation,
                })?;
            }
            let seed_input = genetics::crop(tx, w.plot)?.seed_input;
            let s = tx.edit::<Farms>()?;
            let p = s.plots.get_mut(&w.plot).unwrap();
            p.phase = match p.phase {
                Phase::Harvesting { yield_units } => Phase::Ripe {
                    yield_units: yield_units.saturating_add(w.remaining),
                },
                _ => {
                    s.lost_seed = s.lost_seed.checked_add(seed_input).ok_or(Error::Overflow)?;
                    Phase::Fallow
                }
            };
        }
        Ok(())
    })
}

impl Farms {
    fn validate_rows<'a>(&'a self, mut rows: impl Iterator<Item = &'a Plot>) -> Result<()> {
        require(rows.all(|p| !p.crop.is_empty()), "invalid crop reference")
    }
}

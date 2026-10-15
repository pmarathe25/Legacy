//! Crop phases, paid seed inputs, seasonal growth, and real harvest labor.
use game_storage::Table;
mod catalog;
pub use catalog::{Crop, CropSpec, Crops, InstallCrops, load};
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Fallow,
    Sowing,
    Growing { ready: SimTime },
    Ripe { yield_units: u64 },
    Harvesting { yield_units: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plot {
    pub area: AreaId,
    pub steward: PersonId,
    pub phase: Phase,
    pub crop: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Work {
    plot: PlotId,
    job: ScheduleId,
    destination: InventoryId,
    reservation: Option<u64>,
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
#[derive(Serialize, Deserialize)]
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
#[derive(Serialize, Deserialize)]
struct Planted {
    person: PersonId,
}
impl Request for Planted {
    const KEY: &'static str = "agriculture.planted";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
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
#[derive(Serialize, Deserialize)]
struct Finish {
    person: PersonId,
}
impl Request for Finish {
    const KEY: &'static str = "agriculture.finish";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
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
    require(person == p.steward, "no land-use permission")?;
    require(
        game_transport::area(tx.get()?, person)? == p.area
            && game_cargo::locality(tx, inventory)? == p.area,
        "farm work is not local",
    )
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("agriculture", 1)?;
    b.resource::<Farms>()?;
    catalog::register(b)?;
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
        require(
            tx.get::<Farms>()?.plots[&r.plot].phase == Phase::Fallow,
            "plot is occupied",
        )?;
        require(
            tx.get::<game_property::Property>()?
                .allowed(r.person, r.seed),
            "no seed access",
        )?;
        let crop = catalog::crop(tx, r.plot)?;
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
            },
        );
        Ok(until)
    })?;
    b.request::<Planted>(|tx, r| {
        let w = tx.edit::<Farms>()?.work.remove(&r.person).unwrap();
        let ready = tx.now().after(catalog::crop(tx, w.plot)?.growth)?;
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
        let crop = catalog::crop(tx, r.plot)?;
        let season = tx.get::<game_environment::Climate>()?.at(tx.now())?;
        let productive = season.warmth >= crop.minimum_warmth && season.rain >= crop.minimum_rain;
        let plot = tx.edit::<Farms>()?.plots.get_mut(&r.plot).unwrap();
        plot.phase = Phase::Ripe {
            yield_units: if productive { crop.yield_units } else { 0 },
        };
        Ok(())
    })?;
    b.request::<Harvest>(|tx, r| {
        local(tx, r.person, r.plot, r.destination)?;
        let Phase::Ripe { yield_units } = tx.get::<Farms>()?.plots[&r.plot].phase else {
            return Err(Error::Rejected("crop is not ripe".into()));
        };
        let reservation = if yield_units > 0 {
            Some(tx.call(game_cargo::ReserveSpace {
                good: catalog::crop(tx, r.plot)?.output,
                inventory: r.destination,
                quantity: yield_units,
            })?)
        } else {
            None
        };
        let until = tx.now().after(catalog::crop(tx, r.plot)?.harvest_labor)?;
        tx.start_action(r.person, "farm", Some(until), Cancel { person: r.person })?;
        let job = tx.schedule(until, 10, Finish { person: r.person })?;
        let s = tx.edit::<Farms>()?;
        s.plots.get_mut(&r.plot).unwrap().phase = Phase::Harvesting { yield_units };
        s.work.insert(
            r.person,
            Work {
                plot: r.plot,
                job,
                destination: r.destination,
                reservation,
            },
        );
        Ok(until)
    })?;
    b.request::<Finish>(|tx, r| {
        let w = tx.get::<Farms>()?.work[&r.person].clone();
        let Phase::Harvesting { yield_units } = tx.get::<Farms>()?.plots[&w.plot].phase else {
            return Err(Error::Rejected("harvest has changed".into()));
        };
        if yield_units > 0 {
            tx.call(game_cargo::DepositReserved {
                reservation: w.reservation.unwrap(),
                inventory: w.destination,
                good: catalog::crop(tx, w.plot)?.output,
                quantity: yield_units,
            })?;
        }
        tx.edit::<Farms>()?.plots.get_mut(&w.plot).unwrap().phase = Phase::Fallow;
        tx.edit::<Farms>()?.work.remove(&r.person);
        tx.finish_action(r.person, "farm")?;
        tx.call(game_skills::Practice {
            person: r.person,
            skill: "farm".into(),
            effort: 60,
            attribution: w.job.0,
        })?;
        Ok(())
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
            let seed_input = catalog::crop(tx, w.plot)?.seed_input;
            let s = tx.edit::<Farms>()?;
            let p = s.plots.get_mut(&w.plot).unwrap();
            p.phase = match p.phase {
                Phase::Harvesting { yield_units } => Phase::Ripe { yield_units },
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

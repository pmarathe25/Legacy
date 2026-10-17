//! Renewable resource patches and timed gathering with reservations.
use game_core::*;
use game_sim::*;
use game_storage::{Records, Table};
mod indexes;
use serde::{Deserialize, Serialize};
pub const GATHERING_TIME: SimDuration = SimDuration(5 * MINUTE.0);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Patch {
    pub area: AreaId,
    pub capacity: u64,
    pub good: GoodId,
    pub renew: u64,
    pub stock: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gathering {
    pub patch: u64,
    pub quantity: u64,
    pub destination: InventoryId,
    pub job: ScheduleId,
    pub reservation: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "indexes::SavedEcology")]
pub struct Ecology {
    pub patches: Table<u64, Patch>,
    pub gathering: Table<PersonId, Gathering>,
    pub renewing: bool,
    #[serde(default)]
    pub renewal: Table<u64, Renewal>,
    #[serde(skip)]
    by_area: Records<AreaId, Table<u64, ()>>,
    #[serde(skip)]
    reserved: Table<u64, u64>,
}
impl Resource for Ecology {
    fn allocated_ids(&self) -> Vec<u64> {
        self.patches.keys().copied().collect()
    }
    const KEY: &'static str = "ecology.patches";
    fn validate(&self) -> Result<()> {
        self.validate_rows(self.patches.values())?;
        self.validate_indexes()
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        self.validate_rows(self.patches.changed(&previous.patches).map(|(_, row)| row))
    }
}
impl Ecology {
    pub fn available(&self, id: u64) -> Result<u64> {
        let patch = self
            .patches
            .get(&id)
            .ok_or_else(|| Error::Missing("patch".into()))?;
        let reserved = self.reserved.get(&id).copied().unwrap_or(0);
        patch
            .stock
            .checked_sub(reserved)
            .ok_or_else(|| Error::Rejected("patch reservations exceed stock".into()))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Renewal {
    pub minimum_warmth: i32,
    pub minimum_rain: u32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SetRenewal {
    pub patch: u64,
    pub minimum_warmth: i32,
    pub minimum_rain: u32,
}
impl Request for SetRenewal {
    const KEY: &'static str = "ecology.set-renewal";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Create(pub Patch);
impl Request for Create {
    const KEY: &'static str = "ecology.create";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Gather {
    pub person: PersonId,
    pub patch: u64,
    pub quantity: u64,
    pub destination: InventoryId,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PlantGrowth {
    pub patch: u64,
    pub plant: Patch,
    pub reserved: u64,
    pub minimum_warmth: Option<i32>,
    pub minimum_rain: u32,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct RenewalOutcome {
    pub growth: u64,
    pub decay: u64,
}
impl Request for PlantGrowth {
    const KEY: &'static str = "ecology.plant-growth";
    type Response = Option<RenewalOutcome>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PlantOrigin {
    pub patch: u64,
    pub quantity: u64,
}
impl Request for PlantOrigin {
    const KEY: &'static str = "ecology.plant-origin";
    type Response = Vec<game_cultivars::Lot>;
}
impl Request for Gather {
    const KEY: &'static str = "ecology.gather";
    type Response = SimTime;
}
/// Collect up to a preferred reserve, accepting a smaller useful harvest.
#[derive(Clone, Serialize, Deserialize)]
pub struct GatherUpTo {
    pub person: PersonId,
    pub patch: u64,
    pub target: u64,
    pub minimum: u64,
    pub destination: InventoryId,
}
impl Request for GatherUpTo {
    const KEY: &'static str = "ecology.gather-up-to";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
struct Finish {
    person: PersonId,
}
impl Request for Finish {
    const KEY: &'static str = "ecology.finish";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel {
    person: PersonId,
}
impl Request for Cancel {
    const KEY: &'static str = "ecology.cancel";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct StartRenewal;
impl Request for StartRenewal {
    const KEY: &'static str = "ecology.start_renewal";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Renew;
impl Request for Renew {
    const KEY: &'static str = "ecology.renew";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("ecology", 1)?;
    b.resource::<Ecology>()?;
    b.request::<SetRenewal>(|tx, r| {
        require(
            tx.get::<Ecology>()?.patches.contains_key(&r.patch),
            "unknown renewal patch",
        )?;
        tx.edit::<Ecology>()?.renewal.insert(
            r.patch,
            Renewal {
                minimum_warmth: r.minimum_warmth,
                minimum_rain: r.minimum_rain,
            },
        );
        Ok(())
    })?;
    b.validate_save(|h| {
        let s = h.get::<Ecology>()?;
        for id in s.patches.keys() {
            s.available(*id)?;
        }
        for (person, g) in &s.gathering {
            require(
                h.get::<Actions>()?
                    .0
                    .get(person)
                    .is_some_and(|a| a.kind == "gather")
                    && h.get::<game_cargo::Cargo>()?
                        .0
                        .get(&g.destination)
                        .is_some_and(|i| i.reserved.contains_key(&g.reservation)),
                "invalid gathering reservation",
            )?;
        }
        Ok(())
    });
    b.request::<Create>(|tx, r| {
        require(r.0.stock <= r.0.capacity, "invalid patch")?;
        let id = tx.allocate()?;
        let state = tx.edit::<Ecology>()?;
        state.by_area.entry(r.0.area).or_default().insert(id, ());
        state.patches.insert(id, r.0);
        Ok(id)
    })?;
    b.request::<Gather>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        require(r.quantity > 0, "empty gathering")?;
        let patch = tx
            .get::<Ecology>()?
            .patches
            .get(&r.patch)
            .ok_or_else(|| Error::Missing("patch".into()))?
            .clone();
        require(
            game_transport::area(tx.get()?, r.person)? == patch.area
                && game_cargo::locality(tx, r.destination)? == patch.area,
            "gathering is not local",
        )?;
        require(
            tx.get::<Ecology>()?.available(r.patch)? >= r.quantity,
            "patch exhausted",
        )?;
        let reservation = tx.call(game_cargo::ReserveSpace {
            good: patch.good,
            inventory: r.destination,
            quantity: r.quantity,
        })?;
        let until = tx.now().after(GATHERING_TIME)?;
        tx.start_action(r.person, "gather", Some(until), Cancel { person: r.person })?;
        let job = tx.schedule(until, 10, Finish { person: r.person })?;
        let state = tx.edit::<Ecology>()?;
        let reserved = state.reserved.entry(r.patch).or_default();
        *reserved = reserved.checked_add(r.quantity).ok_or(Error::Overflow)?;
        state.gathering.insert(
            r.person,
            Gathering {
                patch: r.patch,
                quantity: r.quantity,
                destination: r.destination,
                job,
                reservation,
            },
        );
        Ok(until)
    })?;
    b.request::<GatherUpTo>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        require(
            r.minimum > 0 && r.minimum <= r.target,
            "invalid harvest bounds",
        )?;
        let patch = tx
            .get::<Ecology>()?
            .patches
            .get(&r.patch)
            .ok_or_else(|| Error::Missing("patch".into()))?;
        require(
            game_transport::area(tx.get()?, r.person)? == patch.area
                && game_cargo::locality(tx, r.destination)? == patch.area,
            "gathering is not local",
        )?;
        let mass = tx.get::<game_objects::Objects>()?.mass(patch.good, 1)?;
        let held = &tx.get::<game_cargo::Cargo>()?.0[&r.destination];
        let fits = held
            .capacity
            .saturating_sub(held.used()?)
            .checked_div(mass)
            .unwrap_or(r.target);
        let quantity = r
            .target
            .min(tx.get::<Ecology>()?.available(r.patch)?)
            .min(fits);
        require(
            quantity >= r.minimum,
            "patch exhausted or no useful capacity",
        )?;
        tx.call(Gather {
            person: r.person,
            patch: r.patch,
            quantity,
            destination: r.destination,
        })?;
        Ok(quantity)
    })?;
    b.request::<Finish>(|tx, r| {
        let work = tx.get::<Ecology>()?.gathering[&r.person].clone();
        let good = tx.get::<Ecology>()?.patches[&work.patch].good;
        if tx.registered::<PlantOrigin>() {
            let lots = tx.call(PlantOrigin {
                patch: work.patch,
                quantity: work.quantity,
            })?;
            if !lots.is_empty() {
                tx.call(game_cultivars::ExpectLots {
                    inventory: work.destination,
                    good,
                    lots,
                })?;
            }
        }
        tx.call(game_cargo::DepositReserved {
            reservation: work.reservation,
            inventory: work.destination,
            good,
            quantity: work.quantity,
        })?;
        tx.edit::<Ecology>()?
            .patches
            .get_mut(&work.patch)
            .unwrap()
            .stock -= work.quantity;
        tx.finish_action(r.person, "gather")?;
        let state = tx.edit::<Ecology>()?;
        state.gathering.remove(&r.person);
        state.release(work.patch, work.quantity);
        tx.call(game_skills::Practice {
            person: r.person,
            skill: "forage".into(),
            effort: 5,
            attribution: work.job.0,
        })?;
        Ok(())
    })?;
    b.request::<Cancel>(|tx, r| {
        if let Some(work) = tx.edit::<Ecology>()?.gathering.remove(&r.person) {
            tx.edit::<Ecology>()?.release(work.patch, work.quantity);
            tx.unschedule(work.job);
            tx.call(game_cargo::ReleaseSpace {
                inventory: work.destination,
                reservation: work.reservation,
            })?;
        }
        Ok(())
    })?;
    b.request::<StartRenewal>(|tx, _| {
        require(!tx.get::<Ecology>()?.renewing, "already renewing")?;
        tx.edit::<Ecology>()?.renewing = true;
        tx.schedule(tx.now().after(DAY)?, 30, Renew)?;
        Ok(())
    })?;
    b.request::<Renew>(|tx, _| {
        let mut dormant = std::collections::BTreeSet::new();
        for (id, rule) in &tx.get::<Ecology>()?.renewal {
            let season = game_environment::at(tx, tx.get::<Ecology>()?.patches[id].area)?;
            if season.warmth < rule.minimum_warmth || season.rain < rule.minimum_rain {
                dormant.insert(*id);
            }
        }
        let mut growths = std::collections::BTreeMap::new();
        if tx.registered::<PlantGrowth>() {
            let patches: Vec<_> = tx
                .get::<Ecology>()?
                .patches
                .iter()
                .map(|(id, p)| (*id, p.clone()))
                .collect();
            for (id, plant) in patches {
                let s = tx.get::<Ecology>()?;
                let rule = s.renewal.get(&id);
                let request = PlantGrowth {
                    patch: id,
                    plant,
                    reserved: s.reserved.get(&id).copied().unwrap_or(0),
                    minimum_warmth: rule.map(|r| r.minimum_warmth),
                    minimum_rain: rule.map_or(0, |r| r.minimum_rain),
                };
                if let Some(result) = tx.call(request)? {
                    growths.insert(id, result);
                }
            }
        }
        tx.edit_with_compute::<Ecology, _>(|state, pool| {
            state.patches.update(pool, |id, patch| {
                if let Some(result) = growths.get(id) {
                    patch.stock = patch
                        .stock
                        .saturating_sub(result.decay)
                        .saturating_add(result.growth)
                        .min(patch.capacity);
                } else if !dormant.contains(id) {
                    patch.stock = patch.stock.saturating_add(patch.renew).min(patch.capacity);
                }
            })
        })?;
        tx.schedule(tx.now().after(DAY)?, 30, Renew)?;
        Ok(())
    })
}

impl Ecology {
    fn validate_rows<'a>(&'a self, mut rows: impl Iterator<Item = &'a Patch>) -> Result<()> {
        require(rows.all(|p| p.stock <= p.capacity), "patch over capacity")
    }
}

//! Shared diploid plant genetics and physical seed/harvest provenance.
mod genome;
mod stocks;
use game_core::*;
use game_sim::*;
use game_storage::Table;
pub use genome::{Expression, Genome, Strain};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, sync::Arc};
pub use stocks::representative;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lot {
    pub quantity: u64,
    pub strain: Strain,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cultivars {
    pub enabled: bool,
    pub sequence: u64,
    pub goods: Table<GoodId, GoodId>,
    pub stocks: Table<InventoryId, Table<GoodId, VecDeque<Lot>>>,
    pub observed: Table<PersonId, Table<u64, i64>>,
    #[serde(default)]
    pub trials: Table<PersonId, Table<u64, Trial>>,
    #[serde(skip)]
    incoming: Table<(InventoryId, GoodId), Vec<Lot>>,
}
impl Resource for Cultivars {
    const KEY: &'static str = "cultivars.germplasm";
    fn validate(&self) -> Result<()> {
        require(self.incoming.is_empty(), "unfulfilled plant provenance")
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trial {
    pub count: u64,
    pub failures: u64,
    pub mean: i64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Define {
    pub seed: GoodId,
    pub produce: GoodId,
}
impl Request for Define {
    const KEY: &'static str = "cultivars.define";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "cultivars.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Make {
    pub species: GoodId,
    pub mother: Option<Strain>,
    pub father: Option<Strain>,
    pub clonal: bool,
}
impl Request for Make {
    const KEY: &'static str = "cultivars.make";
    type Response = Strain;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Created {
    pub strain: Strain,
}
impl Event for Created {
    const KEY: &'static str = "cultivars.created";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Expect {
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
    pub strain: Strain,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ExpectLots {
    pub inventory: InventoryId,
    pub good: GoodId,
    pub lots: Vec<Lot>,
}
impl Request for ExpectLots {
    const KEY: &'static str = "cultivars.expect-lots";
    type Response = ();
}
impl Request for Expect {
    const KEY: &'static str = "cultivars.expect";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Select {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub good: GoodId,
}
impl Request for Select {
    const KEY: &'static str = "cultivars.select";
    type Response = Option<Strain>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Observe {
    pub person: PersonId,
    pub strain: u64,
    pub yield_units: u64,
    pub duration: SimDuration,
}
impl Request for Observe {
    const KEY: &'static str = "cultivars.observe";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("cultivars", 1)?;
    b.resource::<Cultivars>()?;
    b.request::<Define>(|tx, r| {
        for good in [r.seed, r.produce] {
            let s = tx.get::<Cultivars>()?;
            require(
                s.goods.get(&good).is_none_or(|g| *g == r.produce),
                "ambiguous plant species",
            )?;
            let fresh = !s.goods.contains_key(&good);
            tx.edit::<Cultivars>()?.goods.insert(good, r.produce);
            if fresh && tx.get::<Cultivars>()?.enabled {
                let held: Vec<_> = tx
                    .get::<game_cargo::Cargo>()?
                    .0
                    .iter()
                    .filter_map(|(i, c)| (c.amount(good) > 0).then_some((*i, c.amount(good))))
                    .collect();
                for (inventory, quantity) in held {
                    let strain = tx.call(Make {
                        species: r.produce,
                        mother: None,
                        father: None,
                        clonal: false,
                    })?;
                    stocks::put(tx, inventory, good, Lot { quantity, strain })?;
                }
            }
        }
        Ok(())
    })?;
    b.request::<Make>(genome::make)?;
    b.request::<Expect>(|tx, r| {
        tx.call(ExpectLots {
            inventory: r.inventory,
            good: r.good,
            lots: vec![Lot {
                quantity: r.quantity,
                strain: r.strain,
            }],
        })
    })?;
    b.request::<ExpectLots>(|tx, r| {
        if !tx.get::<Cultivars>()?.enabled {
            return Ok(());
        }
        require(
            !r.lots.is_empty()
                && r.lots.iter().all(|l| {
                    l.quantity > 0
                        && tx.get::<Cultivars>().unwrap().goods.get(&r.good)
                            == Some(&l.strain.species)
                }),
            "incompatible harvest provenance",
        )?;
        require(
            tx.edit::<Cultivars>()?
                .incoming
                .insert((r.inventory, r.good), r.lots)
                .is_none(),
            "pending plant provenance",
        )?;
        Ok(())
    })?;
    b.request::<Enable>(|tx, _| {
        if tx.get::<Cultivars>()?.enabled {
            return Ok(());
        }
        tx.edit::<Cultivars>()?.enabled = true;
        let goods: Vec<_> = tx
            .get::<game_cargo::Cargo>()?
            .0
            .iter()
            .flat_map(|(i, c)| c.goods.iter().map(move |(g, q)| (*i, *g, *q)))
            .collect();
        for (inventory, good, q) in goods {
            if q > 0
                && let Some(species) = tx.get::<Cultivars>()?.goods.get(&good).copied()
            {
                let strain = tx.call(Make {
                    species,
                    mother: None,
                    father: None,
                    clonal: false,
                })?;
                stocks::put(
                    tx,
                    inventory,
                    good,
                    Lot {
                        quantity: q,
                        strain,
                    },
                )?;
            }
        }
        Ok(())
    })?;
    b.request::<Observe>(|tx, r| {
        let score = (u128::from(r.yield_units) * u128::from(DAY.0) * 1000
            / u128::from(r.duration.0.max(1)))
        .min(i64::MAX as u128) as i64;
        let trial = tx
            .edit::<Cultivars>()?
            .trials
            .entry(r.person)
            .or_default()
            .entry(r.strain)
            .or_default();
        trial.count = trial.count.saturating_add(1);
        trial.failures = trial.failures.saturating_add(u64::from(r.yield_units == 0));
        trial.mean = trial
            .mean
            .saturating_add((score - trial.mean) / trial.count.min(i64::MAX as u64) as i64);
        tx.edit::<Cultivars>()?
            .observed
            .entry(r.person)
            .or_default()
            .insert(r.strain, score);
        Ok(())
    })?;
    stocks::register(b)?;
    b.validate_save(|h| {
        let s = h.get::<Cultivars>()?;
        if !s.enabled {
            return Ok(());
        }
        for (i, c) in &h.get::<game_cargo::Cargo>()?.0 {
            for (g, q) in &c.goods {
                if !s.goods.contains_key(g) {
                    continue;
                }
                let tracked: u64 = s
                    .stocks
                    .get(i)
                    .and_then(|r| r.get(g))
                    .into_iter()
                    .flatten()
                    .map(|l| l.quantity)
                    .sum();
                require(tracked == *q, "plant quantity mismatch")?;
            }
        }
        Ok(())
    });
    Ok(())
}

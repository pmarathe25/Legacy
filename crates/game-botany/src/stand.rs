use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plant {
    pub id: u64,
    pub strain: Strain,
    pub expression: game_cultivars::Expression,
    pub born: SimTime,
    #[serde(default)]
    pub age_offset: u64,
    pub family: Option<Strain>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seed {
    pub strain: Strain,
    pub quantity: u64,
    pub expires: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stand {
    pub sequence: u64,
    pub plants: Vec<Plant>,
    pub seedbank: VecDeque<Seed>,
    pub fruit: BTreeMap<u64, Lot>,
    pub minimum: i32,
    pub maximum: i32,
    pub rain_min: u32,
    pub rain_max: u32,
    pub maturity: SimDuration,
    pub lifespan: SimDuration,
    pub shelf: Option<u64>,
    pub last: SimTime,
    pub flowering: SimTime,
}
impl Stand {
    pub fn productive(&self, p: &Plant, now: SimTime, warmth: i32, rain: u32) -> bool {
        now.0.saturating_sub(p.born.0).saturating_add(p.age_offset)
            >= p.expression.growth(self.maturity).map_or(u64::MAX, |d| d.0)
            && self.suitable(p.expression, warmth, rain)
    }
    pub fn suitable(&self, e: game_cultivars::Expression, warmth: i32, rain: u32) -> bool {
        let min_rain = (i64::from(self.rain_min) - i64::from(e.drought)).max(0) as u32;
        let max_rain = (i64::from(self.rain_max) + i64::from(e.wet)).max(0) as u32;
        warmth >= self.minimum - e.cold
            && warmth <= self.maximum + e.heat - e.cold / 4
            && (min_rain..=max_rain).contains(&rain)
    }
    pub fn take(&mut self, quantity: u64) -> Result<Vec<Lot>> {
        let mut left = quantity;
        let mut result = vec![];
        while left > 0 {
            let (id, mut lot) = self
                .fruit
                .pop_first()
                .ok_or_else(|| Error::Rejected("wild plant stock mismatch".into()))?;
            let q = left.min(lot.quantity);
            left -= q;
            lot.quantity -= q;
            result.push(Lot {
                quantity: q,
                strain: lot.strain.clone(),
            });
            if lot.quantity > 0 {
                self.fruit.insert(id, lot);
            }
        }
        Ok(result)
    }
    pub fn add(&mut self, lot: Lot) -> Result<()> {
        let entry = self.fruit.entry(lot.strain.id).or_insert(Lot {
            quantity: 0,
            strain: lot.strain,
        });
        entry.quantity = entry
            .quantity
            .checked_add(lot.quantity)
            .ok_or(Error::Overflow)?;
        Ok(())
    }
}
pub(super) fn ensure(tx: &mut Transaction<'_>, r: &game_ecology::PlantGrowth) -> Result<bool> {
    if tx.get::<Botany>()?.stands.contains_key(&r.patch) {
        return Ok(true);
    }
    let object = tx.get::<game_objects::Objects>()?.by_id(r.plant.good)?;
    if !["plant", "woody", "fungus", "seed"]
        .iter()
        .any(|t| object.tags.contains(*t))
    {
        return Ok(false);
    }
    let woody = object.tags.contains("woody");
    let shelf = object.shelf_days();
    let c = tx
        .get::<game_crops::Crops>()
        .ok()
        .and_then(|c| {
            c.items
                .values()
                .find(|c| c.output == r.plant.good || c.seed == r.plant.good)
        })
        .cloned();
    let species = tx
        .get::<game_cultivars::Cultivars>()?
        .goods
        .get(&r.plant.good)
        .copied();
    let species = if let Some(species) = species {
        species
    } else {
        tx.call(game_cultivars::Define {
            seed: r.plant.good,
            produce: r.plant.good,
        })?;
        r.plant.good
    };
    let at = tx.now();
    let mut s = Stand {
        sequence: 32,
        plants: vec![],
        seedbank: VecDeque::new(),
        fruit: BTreeMap::new(),
        minimum: r
            .minimum_warmth
            .or_else(|| c.as_ref().map(|c| c.minimum_warmth))
            .unwrap_or(-5),
        maximum: c.as_ref().and_then(|c| c.maximum_warmth).unwrap_or(40),
        rain_min: r.minimum_rain.max(c.as_ref().map_or(0, |c| c.minimum_rain)),
        rain_max: c.as_ref().and_then(|c| c.maximum_rain).unwrap_or(20),
        maturity: c
            .as_ref()
            .map(|c| c.natural_growth.unwrap_or(c.growth))
            .unwrap_or(SimDuration(if woody { 720 } else { 30 } * DAY.0)),
        lifespan: c
            .as_ref()
            .and_then(|c| c.lifespan)
            .unwrap_or(SimDuration(if woody { 20 * 365 } else { 365 } * DAY.0)),
        shelf,
        last: at,
        flowering: at.after(SimDuration(30 * DAY.0))?,
    };
    for index in 0..32 {
        let strain = tx.call(game_cultivars::Make {
            species,
            mother: None,
            father: None,
            clonal: false,
        })?;
        let expression = strain.expression();
        let span = (u128::from(s.lifespan.0) * u128::from(expression.lifespan_factor) / 1000)
            .max(1)
            .min(u128::from(u64::MAX)) as u64;
        let age_offset = random(tx.seed(), r.patch, index) % span;
        let q = r.plant.stock / 32 + u64::from(index < r.plant.stock % 32);
        if q > 0 {
            s.add(Lot {
                quantity: q,
                strain: strain.clone(),
            })?;
        }
        s.seedbank.push_back(Seed {
            strain: strain.clone(),
            quantity: 2,
            expires: at.after(SimDuration(730 * DAY.0))?,
        });
        s.plants.push(Plant {
            id: index + 1,
            strain,
            expression,
            born: at,
            age_offset,
            family: None,
        });
    }
    tx.edit::<Botany>()?.stands.insert(r.patch, s);
    Ok(true)
}

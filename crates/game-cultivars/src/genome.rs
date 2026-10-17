use super::*;
pub type Genome = BTreeMap<String, game_genetics::Alleles>;
pub const LOCI: [&str; 7] = [
    "yield", "growth", "cold", "heat", "drought", "wet", "lifespan",
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strain {
    pub id: u64,
    pub species: GoodId,
    pub mother: Option<u64>,
    pub father: Option<u64>,
    pub generation: u32,
    pub genome: Arc<Genome>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expression {
    pub yield_factor: u64,
    pub growth_factor: u64,
    pub cold: i32,
    pub heat: i32,
    pub drought: i32,
    pub wet: i32,
    pub lifespan_factor: u64,
}
impl Strain {
    pub fn expression(&self) -> Expression {
        let v = |key: &str| {
            i64::from(
                self.genome
                    .get(key)
                    .copied()
                    .map(game_genetics::expression)
                    .unwrap_or(32767),
            )
        };
        let factor = |key| (250 + v(key) * 1500 / 65535) as u64;
        let shift = |key, span| ((v(key) - 32767) * span / 32767) as i32;
        Expression {
            yield_factor: factor("yield"),
            growth_factor: factor("growth"),
            cold: shift("cold", 10),
            heat: shift("heat", 10),
            drought: shift("drought", 3),
            wet: shift("wet", 3),
            lifespan_factor: factor("lifespan"),
        }
    }
}
impl Expression {
    pub fn growth(&self, base: SimDuration) -> Result<SimDuration> {
        // Larger harvests take extra time; faster development can offset that cost.
        let time = u128::from(base.0) * u128::from(750 + self.yield_factor / 4)
            / u128::from(self.growth_factor);
        Ok(SimDuration(
            time.max(1).try_into().map_err(|_| Error::Overflow)?,
        ))
    }
}
pub(super) fn make(tx: &mut Transaction<'_>, r: Make) -> Result<Strain> {
    let _measurement = game_diagnostics::span("plant genetics");
    let id = tx
        .get::<Cultivars>()?
        .sequence
        .checked_add(1)
        .ok_or(Error::Overflow)?;
    let parents = r
        .mother
        .as_ref()
        .zip(r.father.as_ref().or(r.mother.as_ref()));
    if let Some((m, f)) = parents {
        require(
            m.species == r.species && f.species == r.species,
            "incompatible plant species",
        )?;
    }
    let genome = LOCI
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let seed = random(tx.seed(), r.species.0, 700 + index as u64);
            let alleles = if let Some((mother, father)) = parents {
                let default = game_genetics::Alleles([32767, 32767]);
                let m = *mother.genome.get(*key).unwrap_or(&default);
                let f = *father.genome.get(*key).unwrap_or(&default);
                if r.clonal {
                    game_genetics::mutate(seed, id, m, 500)
                } else {
                    game_genetics::inherit_bounded(seed, PersonId(id), m, f, 500, 65535)
                }
            } else {
                game_genetics::Alleles([
                    (random(seed, id, 0) % 65536) as u16,
                    (random(seed, id, 1) % 65536) as u16,
                ])
            };
            (key.to_string(), alleles)
        })
        .collect();
    let strain = Strain {
        id,
        species: r.species,
        mother: r.mother.as_ref().map(|m| m.id),
        father: r.father.as_ref().or(r.mother.as_ref()).map(|f| f.id),
        generation: parents.map_or(0, |(m, f)| m.generation.min(f.generation).saturating_add(1)),
        genome: Arc::new(genome),
    };
    tx.edit::<Cultivars>()?.sequence = id;
    tx.emit(
        Created {
            strain: strain.clone(),
        },
        vec![],
    )?;
    Ok(strain)
}

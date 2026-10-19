//! Individual animals own physical ancestry, condition, feeding and reproduction.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
mod life;
pub type Genome = BTreeMap<String, game_genetics::Alleles>;
pub use game_animal_species::Species;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Animal {
    pub species: String,
    pub area: AreaId,
    pub born: SimTime,
    pub health: u64,
    pub hunger: u64,
    pub genome: Arc<Genome>,
    pub parents: [Option<u64>; 2],
    pub generation: u32,
    pub caregiver: Option<PersonId>,
    pub familiarity: Table<PersonId, u32>,
    pub pregnancy: Option<life::Pregnancy>,
}
impl Animal {
    pub fn ability(&self, key: &str) -> u64 {
        self.genome
            .get(key)
            .copied()
            .map(game_genetics::expression)
            .map_or(1000, u64::from)
    }
    pub fn speed(&self, species: &Species, at: SimTime) -> u64 {
        let maturity =
            (at.0.saturating_sub(self.born.0) / DAY.0 * 1000 / species.maturity_days).min(1000);
        self.ability("speed")
            .saturating_mul(species.speed)
            .saturating_mul(maturity)
            / 1_000_000
    }
    pub fn size(&self, species: &Species, at: SimTime) -> u64 {
        let growth = (at.0.saturating_sub(self.born.0) / DAY.0 * 1000
            / species.maturity_days.max(1))
        .clamp(100, 1000);
        self.ability("size")
            .saturating_mul(growth)
            .saturating_mul(species.body_mass)
            / 1_000_000
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fauna {
    pub species: Table<String, Species>,
    pub animals: Table<u64, Animal>,
    pub running: bool,
    pub births: u64,
    pub deaths: u64,
    pub highest: u64,
}
impl Resource for Fauna {
    const KEY: &'static str = "fauna.animals";
    fn allocated_ids(&self) -> Vec<u64> {
        let mut ids: Vec<_> = self.animals.keys().copied().collect();
        if self.highest > 0 && !self.animals.contains_key(&self.highest) {
            ids.push(self.highest);
        }
        ids
    }
    fn validate(&self) -> Result<()> {
        require(
            self.species.values().all(Species::valid),
            "invalid animal biology",
        )?;
        require(
            self.animals.values().all(|a| {
                a.health <= 100
                    && self.species.contains_key(&a.species)
                    && a.genome.values().all(|g| g.0.iter().all(|v| *v <= 16000))
            }),
            "invalid living animal",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Install(pub BTreeMap<String, Species>);
impl Request for Install {
    const KEY: &'static str = "fauna.install";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Populate {
    pub area: AreaId,
    pub species: String,
    pub quantity: u32,
}
impl Request for Populate {
    const KEY: &'static str = "fauna.populate";
    type Response = Vec<u64>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Start;
impl Request for Start {
    const KEY: &'static str = "fauna.start";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Feed {
    pub person: PersonId,
    pub animal: u64,
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for Feed {
    const KEY: &'static str = "fauna.feed";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Tame {
    pub person: PersonId,
    pub animal: u64,
}
impl Request for Tame {
    const KEY: &'static str = "fauna.tame";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Catch {
    pub person: PersonId,
    pub animal: u64,
    pub inventory: InventoryId,
}
impl Request for Catch {
    const KEY: &'static str = "fauna.catch";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Born {
    pub animal: u64,
    pub body: Animal,
}
impl Event for Born {
    const KEY: &'static str = "fauna.born";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Died {
    pub animal: u64,
    pub area: AreaId,
}
impl Event for Died {
    const KEY: &'static str = "fauna.died";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Tended {
    pub person: PersonId,
    pub animal: u64,
}
impl Event for Tended {
    const KEY: &'static str = "fauna.tended";
}
pub fn load(store: &mut game_content::Store, objects: &game_objects::Objects) -> Result<Install> {
    Ok(Install(game_animal_species::load(store, objects)?))
}
fn local(tx: &Transaction<'_>, person: PersonId, animal: u64) -> Result<Animal> {
    game_physiology::capable(tx, person)?;
    require(
        game_vitality::can_manipulate(tx, person),
        "animal handling exceeds motor capacity",
    )?;
    let body = tx
        .get::<Fauna>()?
        .animals
        .get(&animal)
        .cloned()
        .ok_or_else(|| Error::Missing("animal".into()))?;
    require(
        game_transport::area(tx.get()?, person)? == body.area,
        "animal is elsewhere",
    )?;
    Ok(body)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Seed {
    pub per_habitat: u32,
}
impl Request for Seed {
    const KEY: &'static str = "fauna.seed";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("fauna", 1)?;
    b.resource::<Fauna>()?;
    b.validate_save(|h| {
        for s in h.get::<Fauna>()?.species.values() {
            h.get::<game_objects::Objects>()?.by_id(s.meat)?;
        }
        require(
            h.get::<Fauna>()?.animals.values().all(|a| {
                a.born <= h.now()
                    && h.get::<game_land::Land>().unwrap().0.contains_key(&a.area)
                    && a.pregnancy.as_ref().is_none_or(|p| p.valid(h.now()))
            }),
            "invalid animal habitat, age or pregnancy",
        )
    });
    b.request::<Seed>(|tx, r| {
        require(
            tx.get::<Fauna>()?.animals.is_empty() && !tx.get::<Fauna>()?.running,
            "fauna already founded",
        )?;
        let areas: Vec<_> = tx.get::<game_land::Land>()?.0.keys().copied().collect();
        let species: Vec<_> = tx.get::<Fauna>()?.species.keys().cloned().collect();
        for area in areas {
            for key in &species {
                let s = &tx.get::<Fauna>()?.species[key];
                let warmth = game_environment::at(tx, area)?.warmth;
                if (s.minimum_warmth..=s.maximum_warmth).contains(&warmth) {
                    tx.call(Populate {
                        area,
                        species: key.clone(),
                        quantity: r.per_habitat,
                    })?;
                }
            }
        }
        Ok(())
    })?;
    life::register(b)?;
    b.request::<Install>(|tx, r| {
        require(
            !tx.get::<Fauna>()?.running && tx.get::<Fauna>()?.animals.is_empty(),
            "animal definitions are in use",
        )?;
        tx.edit::<Fauna>()?.species = r.0.into_iter().collect();
        tx.get::<Fauna>()?.validate()
    })?;
    b.request::<Populate>(|tx, r| {
        require(
            tx.get::<game_land::Land>()?.0.contains_key(&r.area),
            "unknown animal habitat",
        )?;
        require(
            r.quantity <= 1000 && tx.get::<Fauna>()?.species.contains_key(&r.species),
            "unknown or excessive founding fauna",
        )?;
        let mut ids = vec![];
        for _ in 0..r.quantity {
            let id = tx.allocate()?;
            tx.edit::<Fauna>()?.highest = id;
            let genome = game_genetics::founder_genome(
                tx.seed(),
                id,
                [
                    "size",
                    "speed",
                    "strength",
                    "hardiness",
                    "longevity",
                    "temperament",
                    "reproductive_role",
                ],
                500,
                1500,
            )?;
            let maturity = tx.get::<Fauna>()?.species[&r.species].maturity_days;
            let body = Animal {
                species: r.species.clone(),
                area: r.area,
                born: SimTime(tx.now().0.saturating_sub(maturity * DAY.0)),
                health: 100,
                hunger: 0,
                genome: Arc::new(genome),
                parents: [None, None],
                generation: 0,
                caregiver: None,
                familiarity: Table::new(),
                pregnancy: None,
            };
            tx.edit::<Fauna>()?.animals.insert(id, body.clone());
            tx.emit(Born { animal: id, body }, vec![])?;
            ids.push(id);
        }
        Ok(ids)
    })?;
    b.request::<Feed>(|tx, r| {
        local(tx, r.person, r.animal)?;
        require(
            game_cargo::locality(tx, r.inventory)? == game_transport::area(tx.get()?, r.person)?,
            "feed is elsewhere",
        )?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.inventory,
            purpose: "feeding an animal".into(),
        })?;
        let food = tx
            .get::<game_objects::Objects>()?
            .by_id(r.good)?
            .food
            .as_ref()
            .ok_or_else(|| Error::Rejected("not animal feed".into()))?;
        let relief = food.relief.saturating_mul(r.quantity) / food.portion.max(1);
        tx.call(game_cargo::Consume {
            inventory: r.inventory,
            good: r.good,
            quantity: r.quantity,
        })?;
        let animal = tx.edit::<Fauna>()?.animals.get_mut(&r.animal).unwrap();
        animal.hunger = animal.hunger.saturating_sub(relief);
        let familiarity = animal.familiarity.entry(r.person).or_default();
        *familiarity = familiarity.saturating_add(10).min(100);
        tx.emit(
            Tended {
                person: r.person,
                animal: r.animal,
            },
            vec![r.person],
        )
    })?;
    b.request::<Tame>(|tx, r| {
        let animal = local(tx, r.person, r.animal)?;
        let regard = animal.familiarity.get(&r.person).copied().unwrap_or(0);
        let accepted = u64::from(regard) * 10 > animal.ability("temperament") / 2;
        if accepted {
            tx.edit::<Fauna>()?
                .animals
                .get_mut(&r.animal)
                .unwrap()
                .caregiver = Some(r.person);
        }
        Ok(accepted)
    })?;
    b.request::<Catch>(|tx, r| {
        require(
            game_vitality::can_walk(tx, r.person),
            "hunting requires locomotion",
        )?;
        let animal = local(tx, r.person, r.animal)?;
        require(
            game_cargo::locality(tx, r.inventory)? == animal.area,
            "hunt destination moved away",
        )?;
        let pursuit = game_vitality::ability(tx, r.person, "speed")
            .saturating_add(game_vitality::ability(tx, r.person, "agility"));
        let speed = animal.speed(&tx.get::<Fauna>()?.species[&animal.species], tx.now());
        let contest = pursuit.saturating_add(speed).max(1);
        if random(tx.seed(), r.person.0, tx.now().0) % contest >= pursuit {
            return Ok(false);
        }
        let meat = tx.get::<Fauna>()?.species[&animal.species].meat;
        let quantity = animal
            .size(&tx.get::<Fauna>()?.species[&animal.species], tx.now())
            .saturating_mul(tx.get::<Fauna>()?.species[&animal.species].yield_units)
            / tx.get::<Fauna>()?.species[&animal.species].body_mass.max(1);
        tx.call(game_cargo::Deposit {
            inventory: r.inventory,
            good: meat,
            quantity: quantity.max(1),
        })?;
        tx.edit::<Fauna>()?.animals.remove(&r.animal);
        tx.edit::<Fauna>()?.deaths += 1;
        tx.emit(
            Died {
                animal: r.animal,
                area: animal.area,
            },
            vec![r.person],
        )?;
        Ok(true)
    })
}

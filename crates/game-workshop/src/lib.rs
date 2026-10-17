//! Recipes and timed manufacturing. Inputs are consumed once, never by previews.
use game_storage::{Records, Table};
mod catalog;
mod reachability;
pub use catalog::{InstallRecipes, load};
use game_core::*;
use game_sim::*;
pub use reachability::RecipeChoices;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub inputs: BTreeMap<GoodId, u64>,
    pub output: GoodId,
    pub quantity: u64,
    pub duration: SimDuration,
    pub skill: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Work {
    recipe: Recipe,
    inventory: InventoryId,
    job: ScheduleId,
    reservation: u64,
    #[serde(default)]
    strain: Option<game_cultivars::Strain>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workshop {
    pub stamps: Vec<game_content::Stamp>,
    pub recipes: Records<String, Recipe>,
    work: Table<PersonId, Work>,
    pub waste: BTreeMap<GoodId, u64>,
}
impl Resource for Workshop {
    const KEY: &'static str = "workshop.recipes";
    fn validate(&self) -> Result<()> {
        self.validate_recipes(self.recipes.values())
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        self.validate_recipes(self.recipes.changed(&previous.recipes).map(|(_, r)| r))
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Define {
    pub name: String,
    pub recipe: Recipe,
}
impl Request for Define {
    const KEY: &'static str = "workshop.define";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Craft {
    pub person: PersonId,
    pub recipe: String,
    pub inventory: InventoryId,
}
impl Request for Craft {
    const KEY: &'static str = "workshop.craft";
    type Response = SimTime;
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Produced {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
    pub skill: String,
}
impl Event for Produced {
    const KEY: &'static str = "workshop.produced";
}
#[derive(Clone, Serialize, Deserialize)]
struct Complete {
    person: PersonId,
}
impl Request for Complete {
    const KEY: &'static str = "workshop.complete";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel {
    person: PersonId,
}
impl Request for Cancel {
    const KEY: &'static str = "workshop.cancel";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("workshop", 1)?;
    b.resource::<Workshop>()?;
    catalog::register(b)?;
    b.validate_save(|h| {
        for (person, w) in &h.get::<Workshop>()?.work {
            require(
                h.get::<Actions>()?
                    .0
                    .get(person)
                    .is_some_and(|a| a.kind == "craft")
                    && h.get::<game_cargo::Cargo>()?
                        .0
                        .get(&w.inventory)
                        .is_some_and(|i| i.reserved.contains_key(&w.reservation)),
                "invalid workshop reservation",
            )?;
        }
        Ok(())
    });
    b.request::<Define>(|tx, r| {
        require(!r.name.is_empty(), "recipe needs name")?;
        tx.edit::<Workshop>()?.recipes.insert(r.name, r.recipe);
        Ok(())
    })?;
    b.request::<Craft>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.inventory,
            purpose: "crafting".into(),
        })?;
        require(
            game_transport::area(tx.get()?, r.person)? == game_cargo::locality(tx, r.inventory)?,
            "workshop is elsewhere",
        )?;
        let recipe = tx
            .get::<Workshop>()?
            .recipes
            .get(&r.recipe)
            .cloned()
            .ok_or_else(|| Error::Missing("recipe".into()))?;
        let species = tx
            .get::<game_cultivars::Cultivars>()
            .ok()
            .filter(|s| s.enabled)
            .and_then(|s| s.goods.get(&recipe.output))
            .copied();
        let strain = species.and_then(|species| {
            recipe.inputs.keys().find_map(|g| {
                game_cultivars::representative(tx, r.inventory, *g).filter(|s| s.species == species)
            })
        });
        for (good, quantity) in &recipe.inputs {
            tx.call(game_cargo::Consume {
                inventory: r.inventory,
                good: *good,
                quantity: *quantity,
            })?;
        }
        let reservation = tx.call(game_cargo::ReserveSpace {
            good: recipe.output,
            inventory: r.inventory,
            quantity: recipe.quantity,
        })?;
        let until = tx.now().after(recipe.duration)?;
        tx.start_action(r.person, "craft", Some(until), Cancel { person: r.person })?;
        let job = tx.schedule(until, 10, Complete { person: r.person })?;
        tx.edit::<Workshop>()?.work.insert(
            r.person,
            Work {
                recipe,
                inventory: r.inventory,
                job,
                reservation,
                strain,
            },
        );
        Ok(until)
    })?;
    b.request::<Complete>(|tx, r| {
        let w = tx.get::<Workshop>()?.work[&r.person].clone();
        if game_transport::area(tx.get()?, r.person)
            .ok()
            .is_none_or(|area| game_cargo::locality(tx, w.inventory).ok() != Some(area))
        {
            tx.cancel_action(r.person)?;
            return Ok(());
        }
        if let Some(strain) = w.strain.clone() {
            tx.call(game_cultivars::Expect {
                inventory: w.inventory,
                good: w.recipe.output,
                quantity: w.recipe.quantity,
                strain,
            })?;
        }
        tx.call(game_cargo::DepositReserved {
            reservation: w.reservation,
            inventory: w.inventory,
            good: w.recipe.output,
            quantity: w.recipe.quantity,
        })?;
        tx.finish_action(r.person, "craft")?;
        tx.edit::<Workshop>()?.work.remove(&r.person);
        tx.call(game_skills::Practice {
            person: r.person,
            skill: w.recipe.skill.clone(),
            effort: w.recipe.duration.0 / MINUTE.0 + 1,
            attribution: w.job.0,
        })?;
        tx.emit(
            Produced {
                person: r.person,
                inventory: w.inventory,
                good: w.recipe.output,
                quantity: w.recipe.quantity,
                skill: w.recipe.skill,
            },
            vec![r.person],
        )
    })?;
    b.request::<Cancel>(|tx, r| {
        if let Some(w) = tx.edit::<Workshop>()?.work.remove(&r.person) {
            tx.unschedule(w.job);
            tx.call(game_cargo::ReleaseSpace {
                inventory: w.inventory,
                reservation: w.reservation,
            })?;
            for (good, quantity) in w.recipe.inputs {
                let total = tx.edit::<Workshop>()?.waste.entry(good).or_default();
                *total = total.checked_add(quantity).ok_or(Error::Overflow)?;
            }
        }
        Ok(())
    })
}

impl Workshop {
    fn validate_recipes<'a>(&self, mut rows: impl Iterator<Item = &'a Recipe>) -> Result<()> {
        require(
            rows.all(|r| {
                r.duration.0 > 0
                    && r.quantity > 0
                    && !r.skill.is_empty()
                    && r.inputs.values().all(|q| *q > 0)
            }),
            "invalid recipe",
        )
    }
}

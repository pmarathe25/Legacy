use super::*;
use game_content::{Document, Stamp, Store};
use game_objects::Objects;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    inputs: BTreeMap<String, u64>,
    output: String,
    quantity: u64,
    duration: SimDuration,
    skill: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Definitions {
    recipes: BTreeMap<String, Spec>,
}
#[derive(Serialize, Deserialize)]
pub struct InstallRecipes {
    pub recipes: BTreeMap<String, Recipe>,
    pub stamps: Vec<Stamp>,
}
impl Request for InstallRecipes {
    const KEY: &'static str = "workshop.install_recipes";
    type Response = ();
}
pub fn load(store: &mut Store, objects: &Objects) -> Result<InstallRecipes> {
    let mut recipes = BTreeMap::new();
    let mut stamps = vec![];
    for path in store.scan("workshop")? {
        let doc: Document<Definitions> = store.load(&path)?;
        stamps.push(doc.stamp()?);
        for (key, r) in doc.data.recipes {
            let inputs = r
                .inputs
                .into_iter()
                .map(|(key, q)| Ok((objects.resolve(&key)?, q)))
                .collect::<Result<_>>()?;
            let recipe = Recipe {
                inputs,
                output: objects.resolve(&r.output)?,
                quantity: r.quantity,
                duration: r.duration,
                skill: r.skill,
            };
            require(
                recipes.insert(key, recipe).is_none(),
                "duplicate recipe key",
            )?;
        }
    }
    let check = Workshop {
        recipes: recipes.clone().into(),
        ..Default::default()
    };
    check.validate()?;
    Ok(InstallRecipes { recipes, stamps })
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<InstallRecipes>(|tx, r| {
        require(tx.get::<Workshop>()?.work.is_empty(), "workshop is active")?;
        let workshop = tx.edit::<Workshop>()?;
        workshop.recipes = r.recipes.into();
        workshop.stamps = r.stamps;
        Ok(())
    })
}

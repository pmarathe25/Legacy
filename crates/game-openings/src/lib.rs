//! Bind validated, declarative content to feature-owned construction requests.
mod assets;
mod build;
mod history;
pub use game_content::Store;
use game_core::*;
pub use game_scenarios::Info as Scenario;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Opening {
    pub scenario: String,
    pub version: u32,
    pub names: BTreeMap<String, u64>,
    pub sources: Vec<game_content::Stamp>,
    pub definition: Option<game_scenarios::Definition>,
    pub generated: Option<history::History>,
}
impl Resource for Opening {
    const KEY: &'static str = "openings.identity";
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Remember(pub Opening);
impl Request for Remember {
    const KEY: &'static str = "openings.remember";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("openings", 1)?;
    b.resource::<Opening>()?;
    build::register(b)?;
    history::register(b)?;
    b.request::<Remember>(|tx, r| {
        require(
            tx.get::<Opening>()?.scenario.is_empty(),
            "opening already initialized",
        )?;
        *tx.edit::<Opening>()? = r.0;
        Ok(())
    })
}
pub fn scenarios(store: &mut Store) -> Result<Vec<Scenario>> {
    let mut list = game_scenarios::list(store)?;
    list.push(Scenario {
        id: "generated".into(),
        label: "Generated world with simulated history".into(),
        version: 1,
        fixture: false,
    });
    Ok(list)
}
pub fn default_scenario(store: &mut Store) -> Result<String> {
    game_scenarios::default_scenario(store)
}
pub fn initialize(host: &mut Host, store: &mut Store, id: &str, seed: u64) -> Result<()> {
    require(
        host.get::<Opening>()?.scenario.is_empty(),
        "opening already initialized",
    )?;
    if id == "generated" {
        return history::initialize(host, store, seed, true);
    }
    let definitions = game_scenarios::load(store)?;
    let selected = if id == "random" {
        let eligible: Vec<_> = definitions.values().filter(|d| !d.data.fixture).collect();
        require(
            !eligible.is_empty(),
            "no player scenarios for random selection",
        )?;
        eligible[(random(seed, 0, 0) % (eligible.len() as u64)) as usize]
            .id
            .as_str()
    } else {
        id
    };
    let definition = definitions
        .get(selected)
        .cloned()
        .ok_or_else(|| Error::InvalidConfiguration(format!("unknown scenario {selected}")))?;
    let assets = assets::Assets::load(store, &definition.data)?;
    build::initialize(host, &definition.data, assets, seed, true)?;
    let mut opening = host.get::<Opening>()?.clone();
    opening.scenario = definition.id;
    opening.version = definition.version;
    opening.sources = store.stamps();
    opening.definition = Some(definition.data);
    host.dispatch(Remember(opening))?;
    Ok(())
}
/// Physical initial conditions, with every person autonomous and no protected player.
pub fn seed_world(host: &mut Host, store: &mut Store, seed: u64) -> Result<()> {
    require(
        host.get::<Opening>()?.scenario.is_empty(),
        "opening already initialized",
    )?;
    history::initialize(host, store, seed, false)
}

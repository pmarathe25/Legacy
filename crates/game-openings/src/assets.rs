use super::*;
use game_scenarios::Definition;
pub(super) struct Assets {
    pub objects: game_objects::Objects,
    pub traits: game_traits::Traits,
    pub crops: game_agriculture::Crops,
    pub recipes: game_workshop::InstallRecipes,
    pub body: game_physiology::Rules,
    pub characters: game_characters::Rules,
    pub climate: game_environment::Climate,
    pub health: game_health::TreatmentRules,
    pub invocation: Option<game_invocation::Configure>,
}
impl Assets {
    pub fn load(store: &mut Store, scenario: &Definition) -> Result<Self> {
        let objects = game_objects::load(store)?;
        let traits = game_traits::load(store)?;
        let crops = game_agriculture::load(store, &objects)?;
        let recipes = game_workshop::load(store, &objects)?;
        let characters = game_characters::load(store, &scenario.characters)?;
        let body = game_physiology::load(store, &scenario.physiology)?;
        let climate = game_environment::load(store, &scenario.environment)?;
        let health = game_health::load(store, &scenario.health, &objects)?;
        let invocation = scenario
            .invocation
            .as_ref()
            .map(|key| game_invocation::load(store, key))
            .transpose()?;
        for p in &scenario.people {
            traits.validate_profile(&p.traits)?;
        }
        if let Some(r) = &scenario.reincarnation {
            traits.validate_profile(&r.traits)?;
        }
        let count = scenario.actors().len() as u64;
        for inventory in &scenario.inventories {
            let mut mass = 0u64;
            for (key, q) in &inventory.goods {
                mass = mass
                    .checked_add(objects.mass(objects.resolve(key)?, *q)?)
                    .ok_or(Error::Overflow)?;
            }
            for (key, q) in &inventory.per_person {
                let amount = q.checked_mul(count).ok_or(Error::Overflow)?;
                mass = mass
                    .checked_add(objects.mass(objects.resolve(key)?, amount)?)
                    .ok_or(Error::Overflow)?;
            }
            require(
                mass <= inventory.capacity,
                "starting inventory exceeds capacity",
            )?;
        }
        for plot in &scenario.plots {
            require(
                crops.items.contains_key(&plot.crop),
                "scenario references unknown crop",
            )?;
        }
        for patch in &scenario.patches {
            objects.resolve(&patch.object)?;
        }
        Ok(Self {
            objects,
            traits,
            crops,
            recipes,
            body,
            characters,
            climate,
            health,
            invocation,
        })
    }
    pub fn install(self, host: &mut Host) -> Result<()> {
        host.dispatch(game_objects::Install(self.objects))?;
        host.dispatch(game_traits::Install(self.traits))?;
        host.dispatch(game_agriculture::InstallCrops(self.crops))?;
        host.dispatch(self.recipes)?;
        host.dispatch(game_physiology::Configure(self.body))?;
        host.dispatch(game_characters::Configure(self.characters))?;
        host.dispatch(game_environment::Configure(self.climate))?;
        host.dispatch(game_health::Configure(self.health))?;
        if let Some(invocation) = self.invocation {
            host.dispatch(invocation)?;
        }
        Ok(())
    }
}

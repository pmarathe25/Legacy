//! Adapts packed and ordinary goods cargo to compiled commands.
use game_parallel::{GoodsExchange, GoodsTransfer, World};
use game_parallel_goods_packed::PackedCargo;
use game_parallel_production_model::PersonSnapshot;
pub struct Context<'a> {
    pub world: &'a mut World,
    pub people: &'a [PersonSnapshot],
    pub packed: &'a mut Option<PackedCargo>,
}
pub fn extract(world: &mut World, enabled: bool, people: &[PersonSnapshot]) -> Option<PackedCargo> {
    if !enabled {
        return None;
    }
    world.goods_cargo.as_mut().map(|cargo| {
        let packed = PackedCargo::from_cargo(cargo, people);
        cargo.holdings.clear();
        packed
    })
}
pub fn export(world: &mut World, packed: Option<&PackedCargo>, people: &[PersonSnapshot]) {
    if let Some(packed) = packed {
        world.goods_cargo = Some(packed.cargo(people));
    }
}
impl Context<'_> {
    fn suspended(&self) -> Option<u64> {
        self.world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.suspended_person(self.world.tick))
    }
    pub fn transfer(&mut self, requests: &[GoodsTransfer], busy: &[u64]) -> Result<(), String> {
        if let Some(property) = &self.world.property {
            property.check_transfers(self.people, requests)?;
        }
        if self
            .suspended()
            .is_some_and(|person| requests.iter().any(|request| request.person == person))
        {
            return Err("player control is suspended".into());
        }
        if let Some(packed) = self.packed {
            let production = self
                .world
                .production
                .as_mut()
                .ok_or("production is disabled")?;
            packed.transfer(&mut production.inventories, self.people, busy, requests)
        } else {
            game_parallel_block_actions::Context {
                world: self.world,
                people: self.people,
            }
            .transfer_goods(requests, busy)
        }
    }
    fn with_cargo<T>(
        &mut self,
        action: impl FnOnce(&mut game_parallel_block_actions::Context<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        if let Some(packed) = self.packed.take() {
            self.world.goods_cargo = Some(packed.cargo(self.people));
            let result = action(&mut game_parallel_block_actions::Context {
                world: self.world,
                people: self.people,
            });
            let cargo = self.world.goods_cargo.as_mut().expect("packed cargo");
            *self.packed = Some(PackedCargo::from_cargo(cargo, self.people));
            cargo.holdings.clear();
            result
        } else {
            action(&mut game_parallel_block_actions::Context {
                world: self.world,
                people: self.people,
            })
        }
    }
    pub fn exchange(&mut self, requests: &[GoodsExchange], busy: &[u64]) -> Result<(), String> {
        if self.suspended().is_some_and(|person| {
            requests
                .iter()
                .any(|request| request.a == person || request.b == person)
        }) {
            return Err("player control is suspended".into());
        }
        self.with_cargo(|actions| actions.exchange_goods(requests, busy))
    }
    pub fn recover(
        &mut self,
        actor: u64,
        deceased: u64,
        good: u32,
        quantity: u64,
        busy: &[u64],
    ) -> Result<(), String> {
        if self.suspended() == Some(actor) {
            return Err("player control is suspended".into());
        }
        if let Some(property) = &self.world.property {
            property.check_recovery(self.people, actor, good)?;
        }
        self.with_cargo(|actions| actions.recover_goods(actor, deceased, good, quantity, busy))
    }
}

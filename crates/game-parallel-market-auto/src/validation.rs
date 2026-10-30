//! Validation for the composed production and exchange owners.
use game_parallel_autowork::Autowork;
use game_parallel_goods_cargo::GoodsCargo;
use game_parallel_market::Market;
use game_parallel_people::Region;
use game_parallel_production::Production;
use game_parallel_transport::Traveler;
pub struct Owners<'a> {
    pub production: Option<&'a Production>,
    pub cargo: Option<&'a GoodsCargo>,
    pub market: Option<&'a Market>,
    pub autowork: Option<&'a Autowork>,
    pub property: Option<&'a game_parallel_property::Property>,
    pub regions: &'a [Region],
    pub travelers: &'a [Traveler],
    pub tick: u64,
}
impl Owners<'_> {
    pub fn validate(&self) -> Result<(), String> {
        let Some(production) = self.production else {
            if self.cargo.is_some()
                || self.market.is_some()
                || self.autowork.is_some()
                || self.property.is_some()
            {
                return Err("goods owners require production".into());
            }
            return Ok(());
        };
        let people = production.validate_people(self.regions, self.travelers, self.tick)?;
        if let Some(property) = self.property {
            property.validate(
                &people.iter().map(|person| person.0).collect(),
                &production.inventories,
                self.tick,
            )?;
        }
        if let Some(cargo) = self.cargo {
            cargo.validate_index(&people, &production.goods)?;
        }
        if let Some(market) = self.market {
            if self.cargo.is_none() {
                return Err("market requires goods cargo".into());
            }
            let goods = production.goods.iter().map(|g| g.id).collect();
            market.validate_index(&people, &goods, self.tick)?;
        }
        if let Some(autowork) = self.autowork {
            autowork.validate(&people, production)?;
        }
        Ok(())
    }
}

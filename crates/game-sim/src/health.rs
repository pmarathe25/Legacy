//! Water lots retain contaminant load across actual collection and transport.
//! Clinical effects are independent of custody and of who knows the water is unsafe.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaterLot {
    pub basis_quantity: u64,
    pub basis_load: u128,
    pub removed_quantity: u64,
    pub removed_load: u128,
}
impl WaterLot {
    fn new(quantity: u64, load: u128) -> Self {
        Self {
            basis_quantity: quantity,
            basis_load: load,
            removed_quantity: 0,
            removed_load: 0,
        }
    }
    fn quantity(&self) -> u64 {
        self.basis_quantity - self.removed_quantity
    }
    fn load(&self) -> u128 {
        self.basis_load - self.removed_load
    }
    fn take(&mut self, quantity: u64) -> u128 {
        assert!(quantity <= self.quantity(), "validated water consumption");
        self.removed_quantity += quantity;
        let removed = proportional(self.basis_load, self.removed_quantity, self.basis_quantity);
        let taken = removed - self.removed_load;
        self.removed_load = removed;
        taken
    }
}
fn proportional(load: u128, quantity: u64, basis: u64) -> u128 {
    if basis == 0 {
        return 0;
    }
    // Both terms fit even for maximal fixed-point stocks; multiplying the
    // complete load by quantity first would overflow u128.
    load / u128::from(basis) * u128::from(quantity)
        + (load % u128::from(basis)) * u128::from(quantity) / u128::from(basis)
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub water: BTreeMap<InventoryId, WaterLot>,
}
impl Simulation {
    /// Called after the physical water enters a stock. Existing contamination
    /// is mixed with this source; collection never changes physical quantity.
    pub(crate) fn collected_water_quality(
        &mut self,
        inventory: InventoryId,
        amount: Quantity,
        contamination: u16,
    ) {
        assert!(contamination <= 1_000);
        let stock = &self.state.inventories[&inventory];
        assert_eq!(stock.good, Good::Water);
        assert!(stock.quantity >= amount);
        let quantity = stock.quantity.0;
        let world = self.state.health.get_or_insert_with(World::default);
        let existing = world.water.get(&inventory).map_or(0, WaterLot::load);
        world.water.insert(
            inventory,
            WaterLot::new(
                quantity,
                existing + u128::from(amount.0) * u128::from(contamination),
            ),
        );
    }
    pub(crate) fn update_water_event(&mut self, event: &EventKind) {
        let Some(world) = &mut self.state.health else {
            return;
        };
        match *event {
            EventKind::Transferred {
                source,
                destination,
                quantity,
                ..
            } if self.state.inventories[&source].good == Good::Water => {
                let source_before = self.state.inventories[&source].quantity.0 + quantity.0;
                let dose = world
                    .water
                    .entry(source)
                    .or_insert_with(|| WaterLot::new(source_before, 0))
                    .take(quantity.0);
                let remaining = world.water.get(&destination).map_or(0, WaterLot::load);
                world.water.insert(
                    destination,
                    WaterLot::new(
                        self.state.inventories[&destination].quantity.0,
                        remaining + dose,
                    ),
                );
            }
            EventKind::Consumed {
                source, quantity, ..
            } if self.state.inventories[&source].good == Good::Water => {
                let before = self.state.inventories[&source].quantity.0 + quantity.0;
                world
                    .water
                    .entry(source)
                    .or_insert_with(|| WaterLot::new(before, 0))
                    .take(quantity.0);
            }
            EventKind::Produced {
                output, quantity, ..
            } if self.state.inventories[&output].good == Good::Water => {
                let quantity_now = self.state.inventories[&output].quantity.0;
                let load = world.water.get(&output).map_or(0, WaterLot::load);
                let _ = quantity;
                world
                    .water
                    .insert(output, WaterLot::new(quantity_now, load));
            }
            EventKind::Spoiled { inventory, .. }
                if self.state.inventories[&inventory].good == Good::Water =>
            {
                world.water.insert(inventory, WaterLot::new(0, 0));
            }
            _ => {}
        }
    }
}
pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.health else {
        return Ok(());
    };
    for (id, lot) in &world.water {
        let stock = state
            .inventories
            .get(id)
            .ok_or_else(|| Error::InvalidSave("unknown water lot".into()))?;
        if stock.good != Good::Water
            || lot.removed_quantity > lot.basis_quantity
            || lot.basis_load > u128::from(lot.basis_quantity) * 1_000
            || lot.removed_load
                != proportional(lot.basis_load, lot.removed_quantity, lot.basis_quantity)
            || lot.quantity() != stock.quantity.0
        {
            return Err(Error::InvalidSave("inconsistent water quality".into()));
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixing_and_partial_transport_preserve_load_and_water_without_chunk_rounding() {
        let mut whole = WaterLot::new(10, 2);
        let mut split = whole.clone();
        assert_eq!(whole.take(5), split.take(1) + split.take(4));
        assert_eq!(whole, split);
        let remaining = whole.load();
        assert_eq!(whole.take(5), remaining);
        assert_eq!(whole.load(), 0);
        assert_eq!(whole.quantity(), 0);
    }
    #[test]
    fn maximal_stocks_do_not_overflow_quality_arithmetic() {
        let mut lot = WaterLot::new(u64::MAX, u128::from(u64::MAX) * 1_000);
        assert_eq!(lot.take(u64::MAX), u128::from(u64::MAX) * 1_000);
        assert_eq!(lot.load(), 0);
    }
}

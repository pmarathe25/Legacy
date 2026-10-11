use game_core::Error;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaterLot {
    pub basis_quantity: u64,
    pub basis_load: u128,
    pub removed_quantity: u64,
    pub removed_load: u128,
}
impl WaterLot {
    pub fn new(quantity: u64, load: u128) -> Self {
        Self {
            basis_quantity: quantity,
            basis_load: load,
            removed_quantity: 0,
            removed_load: 0,
        }
    }
    pub fn quantity(&self) -> u64 {
        self.basis_quantity - self.removed_quantity
    }
    pub fn load(&self) -> u128 {
        self.basis_load - self.removed_load
    }
    pub fn take(&mut self, quantity: u64) -> Result<u128, Error> {
        self.validate_fields()?;
        if quantity > self.quantity() {
            return Err(Error::Unavailable);
        }
        self.removed_quantity += quantity;
        let removed = proportional(self.basis_load, self.removed_quantity, self.basis_quantity);
        let taken = removed - self.removed_load;
        self.removed_load = removed;
        Ok(taken)
    }
}
pub fn proportional(load: u128, quantity: u64, basis: u64) -> u128 {
    if basis == 0 {
        return 0;
    }
    // Both terms fit even for maximal fixed-point stocks; multiplying the
    // complete load by quantity first would overflow u128.
    load / u128::from(basis) * u128::from(quantity)
        + (load % u128::from(basis)) * u128::from(quantity) / u128::from(basis)
}

impl WaterLot {
    pub fn validate_fields(&self) -> Result<(), Error> {
        let quantity = self
            .basis_quantity
            .checked_sub(self.removed_quantity)
            .ok_or_else(|| Error::InvalidSave("inconsistent water quality".into()))?;
        self.validate(quantity)
    }
    pub fn validate(&self, quantity: u64) -> Result<(), Error> {
        if self.removed_quantity > self.basis_quantity
            || self.basis_load > u128::from(self.basis_quantity) * 1_000
            || self.removed_load
                != proportional(self.basis_load, self.removed_quantity, self.basis_quantity)
            || self.quantity() != quantity
        {
            return Err(Error::InvalidSave("inconsistent water quality".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CollectionInput {
    pub quantity_after: u64,
    pub existing_load: u128,
    pub collected: u64,
    pub contamination: u16,
}
pub fn collect_water(input: CollectionInput) -> Result<WaterLot, Error> {
    if input.collected > input.quantity_after
        || input.contamination > 1_000
        || input.existing_load > u128::from(input.quantity_after - input.collected) * 1_000
    {
        return Err(Error::Unavailable);
    }
    let load = input
        .existing_load
        .checked_add(u128::from(input.collected) * u128::from(input.contamination))
        .ok_or(Error::Overflow)?;
    Ok(WaterLot::new(input.quantity_after, load))
}
#[derive(Clone, Copy, Debug)]
pub struct WaterTransferInput<'a> {
    pub source: &'a WaterLot,
    pub destination: &'a WaterLot,
    pub quantity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaterTransfer {
    pub source: WaterLot,
    pub destination: WaterLot,
    pub transferred_load: u128,
}
pub fn transfer_water(input: WaterTransferInput<'_>) -> Result<WaterTransfer, Error> {
    input.source.validate_fields()?;
    input.destination.validate_fields()?;
    let destination_quantity = input
        .destination
        .quantity()
        .checked_add(input.quantity)
        .ok_or(Error::Overflow)?;
    let mut source = input.source.clone();
    let transferred_load = source.take(input.quantity)?;
    let load = input
        .destination
        .load()
        .checked_add(transferred_load)
        .ok_or(Error::Overflow)?;
    Ok(WaterTransfer {
        source,
        destination: WaterLot::new(destination_quantity, load),
        transferred_load,
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaterConsumption {
    pub remaining: WaterLot,
    pub consumed_load: u128,
}
pub fn consume_water(lot: &WaterLot, quantity: u64) -> Result<WaterConsumption, Error> {
    lot.validate_fields()?;
    let mut remaining = lot.clone();
    let consumed_load = remaining.take(quantity)?;
    Ok(WaterConsumption {
        remaining,
        consumed_load,
    })
}
pub fn purify_water(lot: &WaterLot, quantity: u64) -> Result<WaterConsumption, Error> {
    let consumed = consume_water(lot, quantity)?;
    Ok(WaterConsumption {
        remaining: WaterLot::new(lot.quantity(), consumed.remaining.load()),
        consumed_load: consumed.consumed_load,
    })
}

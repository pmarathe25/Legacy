//! Physical inventory accounting. Permission is a separate property concern.
use game_core::*;
use game_sim::*;
use game_storage::{Records, Table};
use serde::{Deserialize, Serialize};
pub const FOOD: GoodId = GoodId(1);
pub const TIMBER: GoodId = GoodId(2);
pub const SEED: GoodId = GoodId(3);
pub const MEDICINE: GoodId = GoodId(4);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Custody {
    Area(AreaId),
    Carried(PersonId),
    Road {
        from: AreaId,
        to: AreaId,
        elapsed: SimDuration,
        duration: SimDuration,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    pub custody: Custody,
    pub capacity: u64,
    pub goods: Table<GoodId, u64>,
    pub mass: u64,
    pub reserved: Table<u64, u64>,
    pub reserved_mass: u64,
}
impl Inventory {
    pub fn amount(&self, good: GoodId) -> u64 {
        *self.goods.get(&good).unwrap_or(&0)
    }
    pub fn used(&self) -> Result<u64> {
        self.mass
            .checked_add(self.reserved_mass)
            .ok_or(Error::Overflow)
    }
    pub fn total(&self) -> Result<u64> {
        self.goods
            .values()
            .try_fold(0u64, |sum, q| sum.checked_add(*q).ok_or(Error::Overflow))
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cargo(pub Records<InventoryId, Inventory>);
impl Resource for Cargo {
    fn allocated_ids(&self) -> Vec<u64> {
        self.0
            .iter()
            .flat_map(|(id, inv)| std::iter::once(id.0).chain(inv.reserved.keys().copied()))
            .collect()
    }
    const KEY: &'static str = "cargo.inventories";
    fn validate(&self) -> Result<()> {
        self.validate_rows(self.0.values())?;
        for inventory in self.0.values() {
            let reserved = inventory
                .reserved
                .values()
                .try_fold(0u64, |sum, q| sum.checked_add(*q).ok_or(Error::Overflow))?;
            require(reserved == inventory.reserved_mass, "invalid reserved mass")?;
        }
        Ok(())
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        self.validate_rows(self.0.changed(&previous.0).map(|(_, row)| row))
    }
}
pub fn locality(tx: &Transaction<'_>, id: InventoryId) -> Result<AreaId> {
    let inv = tx
        .get::<Cargo>()?
        .0
        .get(&id)
        .ok_or_else(|| Error::Missing("inventory".into()))?;
    match inv.custody {
        Custody::Area(a) => Ok(a),
        Custody::Carried(p) => game_transport::area(tx.get()?, p),
        Custody::Road { .. } => Err(Error::Rejected("inventory is on a road".into())),
    }
}
#[derive(Serialize, Deserialize)]
pub struct Create {
    pub custody: Custody,
    pub capacity: u64,
}
impl Request for Create {
    const KEY: &'static str = "cargo.create";
    type Response = InventoryId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Deposit {
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for Deposit {
    const KEY: &'static str = "cargo.deposit";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Consume {
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for Consume {
    const KEY: &'static str = "cargo.consume";
    type Response = ();
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transfer {
    pub from: InventoryId,
    pub to: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for Transfer {
    const KEY: &'static str = "cargo.transfer";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct DropCarried {
    pub person: PersonId,
}
impl Request for DropCarried {
    const KEY: &'static str = "cargo.drop_carried";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct ReserveSpace {
    pub good: GoodId,
    pub inventory: InventoryId,
    pub quantity: u64,
}
impl Request for ReserveSpace {
    const KEY: &'static str = "cargo.reserve_space";
    type Response = u64;
}
#[derive(Serialize, Deserialize)]
pub struct ReleaseSpace {
    pub inventory: InventoryId,
    pub reservation: u64,
}
impl Request for ReleaseSpace {
    const KEY: &'static str = "cargo.release_space";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct DepositReserved {
    pub inventory: InventoryId,
    pub reservation: u64,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for DepositReserved {
    const KEY: &'static str = "cargo.deposit_reserved";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("cargo", 1)?;
    b.resource::<Cargo>()?;
    b.validate_save(|h| {
        for inventory in h.get::<Cargo>()?.0.values() {
            let mass = inventory.goods.iter().try_fold(0u64, |sum, (good, q)| {
                sum.checked_add(h.get::<game_objects::Objects>()?.mass(*good, *q)?)
                    .ok_or(Error::Overflow)
            })?;
            require(mass == inventory.mass, "invalid inventory mass")?;
        }
        Ok(())
    });
    b.request::<Create>(|tx, r| {
        let id = InventoryId(tx.allocate()?);
        tx.edit::<Cargo>()?.0.insert(
            id,
            Inventory {
                custody: r.custody,
                capacity: r.capacity,
                goods: Table::new(),
                mass: 0,
                reserved: Table::new(),
                reserved_mass: 0,
            },
        );
        Ok(id)
    })?;
    b.request::<ReserveSpace>(|tx, r| {
        require(r.quantity > 0, "empty reservation")?;
        let mass = tx
            .get::<game_objects::Objects>()?
            .mass(r.good, r.quantity)?;
        let id = tx.allocate()?;
        let inventory = tx
            .edit::<Cargo>()?
            .0
            .get_mut(&r.inventory)
            .ok_or_else(|| Error::Missing("inventory".into()))?;
        require(
            inventory.used()?.checked_add(mass).ok_or(Error::Overflow)? <= inventory.capacity,
            "no output capacity",
        )?;
        inventory.reserved.insert(id, mass);
        inventory.reserved_mass = inventory
            .reserved_mass
            .checked_add(mass)
            .ok_or(Error::Overflow)?;
        Ok(id)
    })?;
    b.request::<ReleaseSpace>(|tx, r| {
        let inventory = tx
            .edit::<Cargo>()?
            .0
            .get_mut(&r.inventory)
            .ok_or_else(|| Error::Missing("inventory".into()))?;
        let mass = inventory
            .reserved
            .remove(&r.reservation)
            .ok_or_else(|| Error::Missing("capacity reservation".into()))?;
        inventory.reserved_mass = inventory
            .reserved_mass
            .checked_sub(mass)
            .ok_or(Error::Overflow)?;
        Ok(())
    })?;
    b.request::<DepositReserved>(|tx, r| {
        let mass = tx
            .get::<game_objects::Objects>()?
            .mass(r.good, r.quantity)?;
        let inventory = tx
            .edit::<Cargo>()?
            .0
            .get_mut(&r.inventory)
            .ok_or_else(|| Error::Missing("inventory".into()))?;
        let reserved = inventory
            .reserved
            .remove(&r.reservation)
            .ok_or_else(|| Error::Missing("capacity reservation".into()))?;
        inventory.reserved_mass = inventory
            .reserved_mass
            .checked_sub(reserved)
            .ok_or(Error::Overflow)?;
        require(mass <= reserved, "output exceeds reservation")?;
        tx.call(Deposit {
            inventory: r.inventory,
            good: r.good,
            quantity: r.quantity,
        })
    })?;
    b.request::<Deposit>(|tx, r| {
        require(r.quantity > 0, "quantity must be positive")?;
        let mass = tx
            .get::<game_objects::Objects>()?
            .mass(r.good, r.quantity)?;
        let inv = tx
            .edit::<Cargo>()?
            .0
            .get_mut(&r.inventory)
            .ok_or_else(|| Error::Missing("inventory".into()))?;
        require(
            inv.used()?.checked_add(mass).ok_or(Error::Overflow)? <= inv.capacity,
            "capacity exceeded",
        )?;
        inv.mass = inv.mass.checked_add(mass).ok_or(Error::Overflow)?;
        let q = inv.goods.entry(r.good).or_default();
        *q = q.checked_add(r.quantity).ok_or(Error::Overflow)?;
        Ok(())
    })?;
    b.request::<Consume>(|tx, r| {
        require(r.quantity > 0, "quantity must be positive")?;
        let mass = tx
            .get::<game_objects::Objects>()?
            .mass(r.good, r.quantity)?;
        let inv = tx
            .edit::<Cargo>()?
            .0
            .get_mut(&r.inventory)
            .ok_or_else(|| Error::Missing("inventory".into()))?;
        let q = inv.goods.entry(r.good).or_default();
        require(*q >= r.quantity, "insufficient stock")?;
        *q -= r.quantity;
        inv.mass = inv.mass.checked_sub(mass).ok_or(Error::Overflow)?;
        Ok(())
    })?;
    b.request::<DropCarried>(|tx, r| {
        let location = tx
            .get::<game_transport::Positions>()?
            .0
            .get(&r.person)
            .cloned();
        let custody = match location {
            Some(game_transport::Location::At(area)) => Custody::Area(area),
            Some(game_transport::Location::Road {
                from,
                to,
                elapsed,
                duration,
            }) => Custody::Road {
                from,
                to,
                elapsed,
                duration,
            },
            Some(game_transport::Location::Transit {
                from,
                to,
                departed,
                arrives,
                ..
            }) => Custody::Road {
                from,
                to,
                elapsed: SimDuration(tx.now().0 - departed.0),
                duration: SimDuration(arrives.0 - departed.0),
            },
            None => return Ok(()),
        };
        let carried: Vec<_> = tx
            .get::<Cargo>()?
            .0
            .iter()
            .filter(|(_, inv)| inv.custody == Custody::Carried(r.person))
            .map(|(id, _)| *id)
            .collect();
        for id in carried {
            tx.edit::<Cargo>()?.0.get_mut(&id).unwrap().custody = custody.clone();
        }
        Ok(())
    })?;
    b.request::<Transfer>(|tx, r| {
        require(r.from != r.to, "identical inventories")?;
        require(
            locality(tx, r.from)? == locality(tx, r.to)?,
            "goods must be carried between areas",
        )?;
        tx.call(Consume {
            inventory: r.from,
            good: r.good,
            quantity: r.quantity,
        })?;
        tx.call(Deposit {
            inventory: r.to,
            good: r.good,
            quantity: r.quantity,
        })
    })
}

impl Cargo {
    fn validate_rows<'a>(&'a self, rows: impl Iterator<Item = &'a Inventory>) -> Result<()> {
        for inventory in rows {
            require(
                inventory.used()? <= inventory.capacity,
                "inventory over capacity",
            )?;
        }
        Ok(())
    }
}

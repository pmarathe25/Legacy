//! Delegated authority with budgets, removal, and physical spending.
use game_core::*;
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mandate {
    pub institution: InstitutionId,
    pub delegate: PersonId,
    pub jurisdiction: AreaId,
    pub treasury: InventoryId,
    pub budget: u64,
    pub spent: u64,
    pub active: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Realm(pub Records<u64, Mandate>);
impl Resource for Realm {
    fn allocated_ids(&self) -> Vec<u64> {
        self.0.keys().copied().collect()
    }
    const KEY: &'static str = "realm.mandates";
    fn validate(&self) -> Result<()> {
        self.validate_rows(self.0.values())
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        self.validate_rows(self.0.changed(&previous.0).map(|(_, row)| row))
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Delegate {
    pub issuer: PersonId,
    pub institution: InstitutionId,
    pub person: PersonId,
    pub jurisdiction: AreaId,
    pub treasury: InventoryId,
    pub budget: u64,
}
impl Request for Delegate {
    const KEY: &'static str = "realm.delegate";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Spend {
    pub person: PersonId,
    pub mandate: u64,
    pub destination: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for Spend {
    const KEY: &'static str = "realm.spend";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Revoke {
    pub issuer: PersonId,
    pub mandate: u64,
}
impl Request for Revoke {
    const KEY: &'static str = "realm.revoke";
    type Response = ();
}
fn authorized(tx: &Transaction<'_>, person: PersonId, institution: InstitutionId) -> Result<()> {
    require(
        game_institutions::holds(tx.get()?, person, institution, "ruler"),
        "office authority unavailable",
    )
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("realm", 1)?;
    b.resource::<Realm>()?;
    b.request::<Delegate>(|tx, r| {
        authorized(tx, r.issuer, r.institution)?;
        tx.call(game_property::Permit {
            issuer: r.issuer,
            person: r.person,
            inventory: r.treasury,
            allow: true,
        })?;
        let id = tx.allocate()?;
        tx.edit::<Realm>()?.0.insert(
            id,
            Mandate {
                institution: r.institution,
                delegate: r.person,
                jurisdiction: r.jurisdiction,
                treasury: r.treasury,
                budget: r.budget,
                spent: 0,
                active: true,
            },
        );
        Ok(id)
    })?;
    b.request::<Spend>(|tx, r| {
        let m = tx
            .get::<Realm>()?
            .0
            .get(&r.mandate)
            .ok_or_else(|| Error::Missing("mandate".into()))?
            .clone();
        require(m.active && m.delegate == r.person, "mandate unavailable")?;
        let spent = m.spent.checked_add(r.quantity).ok_or(Error::Overflow)?;
        require(spent <= m.budget, "budget exceeded")?;
        require(
            game_cargo::locality(tx, m.treasury)? == m.jurisdiction
                && game_cargo::locality(tx, r.destination)? == m.jurisdiction,
            "outside mandate jurisdiction",
        )?;
        tx.call(game_property::Take {
            actor: r.person,
            transfer: game_cargo::Transfer {
                from: m.treasury,
                to: r.destination,
                good: r.good,
                quantity: r.quantity,
            },
            unauthorized: false,
        })?;
        tx.edit::<Realm>()?.0.get_mut(&r.mandate).unwrap().spent = spent;
        Ok(())
    })?;
    b.request::<Revoke>(|tx, r| {
        let m = tx
            .get::<Realm>()?
            .0
            .get(&r.mandate)
            .ok_or_else(|| Error::Missing("mandate".into()))?
            .clone();
        authorized(tx, r.issuer, m.institution)?;
        tx.edit::<Realm>()?.0.get_mut(&r.mandate).unwrap().active = false;
        Ok(())
    })
}

impl Realm {
    fn validate_rows<'a>(&'a self, mut rows: impl Iterator<Item = &'a Mandate>) -> Result<()> {
        require(rows.all(|m| m.spent <= m.budget), "mandate overspent")
    }
}

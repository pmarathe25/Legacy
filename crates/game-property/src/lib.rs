//! Claims and standing permissions never replace physical custody.
use game_core::*;
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub claimant: PersonId,
    pub recognized_by: BTreeSet<PersonId>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "SavedProperty")]
pub struct Property {
    pub claims: Records<InventoryId, Vec<Claim>>,
    pub access: Records<InventoryId, game_storage::Table<PersonId, ()>>,
    #[serde(skip)]
    permitted: Records<PersonId, game_storage::Table<InventoryId, ()>>,
}
impl Resource for Property {
    const KEY: &'static str = "property.claims";
}
impl Property {
    pub fn permitted_set(&self, person: PersonId) -> Option<&game_storage::Table<InventoryId, ()>> {
        self.permitted.get(&person)
    }
    pub fn permitted(&self, person: PersonId) -> impl Iterator<Item = InventoryId> + '_ {
        self.permitted
            .get(&person)
            .into_iter()
            .flat_map(|ids| ids.keys().copied())
    }
    fn permit(&mut self, person: PersonId, inventory: InventoryId, allow: bool) {
        if allow {
            self.access.entry(inventory).or_default().insert(person, ());
            self.permitted
                .entry(person)
                .or_default()
                .insert(inventory, ());
        } else {
            if let Some(ids) = self.access.get_mut(&inventory) {
                ids.remove(&person);
            }
            if let Some(ids) = self.permitted.get_mut(&person) {
                ids.remove(&inventory);
            }
        }
    }
    pub fn allowed(&self, person: PersonId, inventory: InventoryId) -> bool {
        self.access
            .get(&inventory)
            .is_some_and(|p| p.contains_key(&person))
    }
}
#[derive(Serialize, Deserialize)]
pub struct ClaimInventory {
    pub claimant: PersonId,
    pub inventory: InventoryId,
    pub recognized_by: BTreeSet<PersonId>,
}
impl Request for ClaimInventory {
    const KEY: &'static str = "property.claim";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct Permit {
    pub issuer: PersonId,
    pub person: PersonId,
    pub inventory: InventoryId,
    pub allow: bool,
}
impl Request for Permit {
    const KEY: &'static str = "property.permit";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Take {
    pub actor: PersonId,
    pub transfer: game_cargo::Transfer,
    pub unauthorized: bool,
}
impl Request for Take {
    const KEY: &'static str = "property.take";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct Taking {
    pub actor: PersonId,
    pub inventory: InventoryId,
    pub unauthorized: bool,
}
impl Event for Taking {
    const KEY: &'static str = "property.taken";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("property", 1)?;
    b.resource::<Property>()?;
    b.validate_save(|h| {
        let p = h.get::<Property>()?;
        for id in p.claims.keys().chain(p.access.keys()) {
            require(
                h.get::<game_cargo::Cargo>()?.0.contains_key(id),
                "claim references missing goods",
            )?;
        }
        Ok(())
    });
    b.request::<ClaimInventory>(|tx, r| {
        require(
            tx.get::<game_cargo::Cargo>()?.0.contains_key(&r.inventory),
            "unknown property",
        )?;
        let p = tx.edit::<Property>()?;
        p.claims.entry(r.inventory).or_default().push(Claim {
            claimant: r.claimant,
            recognized_by: r.recognized_by,
        });
        p.permit(r.claimant, r.inventory, true);
        Ok(())
    })?;
    b.request::<Permit>(|tx, r| {
        require(
            tx.get::<Property>()?
                .claims
                .get(&r.inventory)
                .is_some_and(|c| c.iter().any(|c| c.claimant == r.issuer)),
            "issuer has no claim",
        )?;
        tx.edit::<Property>()?
            .permit(r.person, r.inventory, r.allow);
        Ok(())
    })?;
    b.request::<Take>(|tx, r| {
        require(
            r.unauthorized || tx.get::<Property>()?.allowed(r.actor, r.transfer.from),
            "access denied",
        )?;
        require(
            game_transport::area(tx.get()?, r.actor)? == game_cargo::locality(tx, r.transfer.from)?,
            "actor is elsewhere",
        )?;
        let from = r.transfer.from;
        tx.call(r.transfer)?;
        tx.emit(
            Taking {
                actor: r.actor,
                inventory: from,
                unauthorized: r.unauthorized,
            },
            vec![r.actor],
        )
    })
}

#[derive(Deserialize)]
struct SavedProperty {
    claims: Records<InventoryId, Vec<Claim>>,
    access: Records<InventoryId, game_storage::Table<PersonId, ()>>,
}
impl From<SavedProperty> for Property {
    fn from(saved: SavedProperty) -> Self {
        let mut permitted: Records<PersonId, game_storage::Table<InventoryId, ()>> = Records::new();
        for (inventory, people) in &saved.access {
            for person in people.keys() {
                permitted.entry(*person).or_default().insert(*inventory, ());
            }
        }
        Self {
            claims: saved.claims,
            access: saved.access,
            permitted,
        }
    }
}

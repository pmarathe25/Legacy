//! Claims and standing permissions never replace physical custody.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub claimant: PersonId,
    pub recognized_by: BTreeSet<PersonId>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Property {
    pub claims: BTreeMap<InventoryId, Vec<Claim>>,
    pub access: BTreeMap<InventoryId, BTreeSet<PersonId>>,
}
impl Resource for Property {
    const KEY: &'static str = "property.claims";
}
impl Property {
    pub fn allowed(&self, person: PersonId, inventory: InventoryId) -> bool {
        self.access
            .get(&inventory)
            .is_some_and(|p| p.contains(&person))
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
        p.access.entry(r.inventory).or_default().insert(r.claimant);
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
        let people = tx
            .edit::<Property>()?
            .access
            .entry(r.inventory)
            .or_default();
        if r.allow {
            people.insert(r.person);
        } else {
            people.remove(&r.person);
        }
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

//! Finite physical apparatus, manual power and personally observed mechanical behavior.
use game_core::*;
pub use game_mechanics_state::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};
macro_rules! request {
    ($ty:ty,$key:literal,$response:ty) => {
        impl game_sim::Request for $ty {
            const KEY: &'static str = $key;
            type Response = $response;
        }
    };
}
mod catalog;
pub use catalog::{Install, load};
mod operation;
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("mechanics", 2)?;
    b.resource::<Machines>()?;
    catalog::register(b)?;
    operation::register(b)?;
    b.validate_save(|h| {
        h.get::<Machines>()?.config.validate_objects(h.get()?)?;
        for (person, w) in &h.get::<Machines>()?.work {
            require(
                h.get::<Actions>()?.0.contains_key(person)
                    && w.session.as_ref().is_none_or(|s| {
                        h.get::<Actions>().unwrap().0[person].id == s.action
                            && h.get::<Actions>().unwrap().0[person].cancel.key()
                                == "mechanics.cancel-session"
                    })
                    && w.device.is_none_or(|id| {
                        h.get::<Machines>()
                            .unwrap()
                            .devices
                            .get(&id)
                            .is_some_and(|d| w.outlet < d.parts.len())
                    }),
                "physical work lacks its actor or apparatus",
            )?;
        }
        for d in h.get::<Machines>()?.devices.values() {
            require(
                h.get::<game_cargo::Cargo>()?
                    .0
                    .get(&d.inventory)
                    .is_some_and(|i| {
                        i.area(h.get::<game_transport::Positions>().unwrap()) == Some(d.area)
                    }),
                "apparatus moved from its site",
            )?;
        }
        Ok(())
    });
    Ok(())
}

//! Owner-controlled disjoint resting flags; action lifecycle remains with physiology.
use crate::nourishment::NourishmentOwner;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct SetRestingMany {
    pub people: Vec<PersonId>,
    pub resting: bool,
}
impl Request for SetRestingMany {
    const KEY: &'static str = "physiology.set-resting-many";
    type Response = ();
}
pub fn register_resting_batch<S: NourishmentOwner>(b: &mut Builder) -> Result<()> {
    b.request::<SetRestingMany>(|tx, r| {
        require(r.people.windows(2).all(|p| p[0] < p[1]), "unordered resting rows")?;
        require(r.people.iter().all(|p| tx.get::<S>().unwrap().body_rows().contains_key(p)), "missing resting body")?;
        game_diagnostics::interaction_profile!(batch "resting", r.people.len(), r.people.len() >= 32 && tx.compute().current_num_threads() > 1);
        tx.edit_with_compute::<S, _>(|state, pool| {
            state.body_rows_mut().update_selected(pool, &r.people, 32, |_, body| {
                body.resting = r.resting;
            });
        })?;
        Ok(())
    })
}

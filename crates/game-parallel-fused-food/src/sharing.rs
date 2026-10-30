//! Turn existing personal feelings into possible gifts without inventing relationships.
use game_parallel::{FoodAidPolicy, World};
use rayon::prelude::*;

pub fn candidates(world: &World) -> Vec<FoodAidPolicy> {
    let Some(cargo) = world
        .cargo
        .as_ref()
        .filter(|cargo| !cargo.food_sharing.is_empty())
    else {
        return Vec::new();
    };
    let Some(opinions) = world.opinions.as_ref() else {
        return Vec::new();
    };
    let mut preferences: Vec<_> = cargo
        .food_sharing
        .par_iter()
        .filter(|p| p.enabled)
        .collect();
    if preferences
        .par_windows(2)
        .any(|p| p[0].person > p[1].person)
    {
        preferences.par_sort_unstable_by_key(|p| p.person);
    }
    let dense = preferences.first().and_then(|first| {
        preferences.last().and_then(|last| {
            (last.person.checked_sub(first.person) == Some((preferences.len() - 1) as u64))
                .then_some(first.person)
        })
    });
    opinions
        .records
        .par_iter()
        .filter_map(|opinion| {
            let i = dense
                .and_then(|first| opinion.observer.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&i| i < preferences.len())
                .or_else(|| {
                    if dense.is_none() {
                        preferences
                            .binary_search_by_key(&opinion.observer, |p| p.person)
                            .ok()
                    } else {
                        None
                    }
                })?;
            let p = preferences[i];
            (opinion.fear.saturating_add(opinion.obligation) >= p.threshold).then_some(
                FoodAidPolicy {
                    giver: p.person,
                    recipient: opinion.target,
                    amount: p.amount,
                    retain: p.retain,
                    help_value: opinion.fear.saturating_add(opinion.obligation),
                    threshold: p.threshold,
                    interval: p.interval,
                    requires_request: false,
                    enabled: true,
                },
            )
        })
        .collect()
}

#[macro_export]
macro_rules! food_aid_methods {
    () => {
        game_parallel_block_food_cargo::store_transfer_methods!();
        game_parallel_store_witnesses::claim_methods!();
        game_parallel_fused_food_deposit::deposit_methods!();
        /// Choose a local physical food source for this person, or return to the regional source.
        pub fn set_food_source(&mut self, person: u64, store: Option<u64>) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let index = self
                .positions
                .binary_search_by_key(&person, |row| row.0)
                .map_err(|_| "unknown person")?;
            let (_, block, slot) = self.positions[index];
            let actor = &self.blocks[block];
            if actor.health[slot] == 0 || actor.journey.is_some() {
                return Err("person must be alive and local".into());
            }
            let region = self.template.regions[actor.region].id;
            let cargo = self
                .template
                .cargo
                .as_mut()
                .filter(|cargo| cargo.personal_food_access)
                .ok_or("personal food access is disabled")?;
            if store.is_some_and(|id| {
                !cargo
                    .food_stores
                    .iter()
                    .any(|source| source.id == id && source.region == region)
            }) {
                return Err("food store must be local".into());
            }
            cargo
                .food_store_choices
                .retain(|choice| choice.person != person);
            if let Some(store) = store {
                cargo
                    .food_store_choices
                    .push(game_parallel::FoodStoreChoice {
                        person,
                        store,
                        reconsider_on_failure: false,
                    });
            }
            self.revision = revision;
            Ok(())
        }
        /// Change an individual's willingness to consider their configured food help.
        pub fn set_food_aid(&mut self, person: u64, enabled: bool) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let index = self
                .positions
                .binary_search_by_key(&person, |row| row.0)
                .map_err(|_| "unknown person")?;
            let (_, block, slot) = self.positions[index];
            if self.blocks[block].health[slot] == 0 {
                return Err("person is dead".into());
            }
            let cargo = self
                .template
                .cargo
                .as_mut()
                .ok_or("personal cargo is disabled")?;
            if !cargo.food_aid.iter().any(|policy| policy.giver == person)
                && !cargo
                    .food_sharing
                    .iter()
                    .any(|policy| policy.person == person)
            {
                return Err("no food aid preference".into());
            }
            for policy in cargo
                .food_aid
                .iter_mut()
                .filter(|policy| policy.giver == person)
            {
                policy.enabled = enabled;
            }
            for policy in cargo
                .food_sharing
                .iter_mut()
                .filter(|policy| policy.person == person)
            {
                policy.enabled = enabled;
            }
            self.revision = revision;
            Ok(())
        }
    };
}

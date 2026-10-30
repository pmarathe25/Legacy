//! Stage dependent care and apply neglect effects to unpublished physical state.
mod feeding;
mod prepare;
pub use feeding::{Feeding, prepare_feeding};
use game_parallel_dependent_care::Draft;
use game_parallel_fused_data::Block;
pub use prepare::prepare;
use rayon::prelude::*;
pub fn apply(draft: &Draft, damage: u32, blocks: &mut [Block], positions: &[(u64, usize, usize)]) {
    if damage == 0 {
        return;
    }
    let dense = positions.first().and_then(|first| {
        (positions.last()?.0.checked_sub(first.0) == Some((positions.len() - 1) as u64))
            .then_some(first.0)
    });
    let mut slots: Vec<_> = draft
        .events
        .par_iter()
        .filter(|event| event.caregiver.is_none())
        .filter_map(|event| {
            let index = dense
                .and_then(|first| event.child.checked_sub(first))
                .and_then(|n| usize::try_from(n).ok())
                .filter(|&i| i < positions.len())
                .or_else(|| {
                    if dense.is_none() {
                        positions
                            .binary_search_by_key(&event.child, |row| row.0)
                            .ok()
                    } else {
                        None
                    }
                })?;
            Some((positions[index].1, positions[index].2))
        })
        .collect();
    if slots.par_windows(2).any(|pair| pair[0] > pair[1]) {
        slots.par_sort_unstable();
    }
    blocks
        .par_iter_mut()
        .enumerate()
        .for_each(|(index, block)| {
            let start = slots.partition_point(|row| row.0 < index);
            let end = slots.partition_point(|row| row.0 <= index);
            for &(_, offset) in &slots[start..end] {
                block.next_health[offset] = block.next_health[offset].saturating_sub(damage);
            }
        });
}
/// Attach a checked personal caregiver preference to a fused host.
#[macro_export]
macro_rules! caregiver_policy_methods {
    () => {
        pub fn set_caregiver_policy(
            &mut self,
            policy: game_parallel::CaregivingPolicy,
        ) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(policy.person)
            {
                return Err("player control is suspended".into());
            }
            let index = self
                .positions
                .binary_search_by_key(&policy.person, |row| row.0)
                .map_err(|_| "unknown person")?;
            let (_, block, slot) = self.positions[index];
            if self.blocks[block].health[slot] == 0 {
                return Err("person is dead".into());
            }
            let family = self
                .owners
                .family
                .as_ref()
                .ok_or("caregiving requires families")?;
            let index = family
                .store
                .locate(policy.person)
                .ok_or("person has no family record")?;
            if family.store.state(index, false).age < family.rules.adult_age {
                return Err("caregiver is not an adult".into());
            }
            let care = self
                .template
                .caregiving
                .as_mut()
                .ok_or("caregiving is disabled")?;
            if let Some(existing) = care
                .policies
                .iter_mut()
                .find(|row| row.person == policy.person)
            {
                existing.willing = policy.willing;
            } else {
                care.policies.push(policy);
            }
            self.revision = revision;
            Ok(())
        }
    };
}

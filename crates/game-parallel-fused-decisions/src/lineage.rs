//! Stage player lineage and guardian state.
use game_parallel::{Guardians, World};
use game_parallel_fused_data::Block;
use game_parallel_lineage::Invocation;
pub struct Draft {
    invocation: Option<Invocation>,
    guardians: Option<Guardians>,
    pub controlled: Option<u64>,
    pub strain: Option<(u64, u32)>,
    pub lifespan_loss: Option<(u64, u64)>,
}
pub fn prepare(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
) -> Result<Draft, &'static str> {
    let invocation = world
        .lineage
        .as_mut()
        .map(|lineage| lineage.prepare_invocation(world.tick))
        .transpose()?
        .flatten();
    let controlled = world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.controlled_person(world.tick));
    let guardians = world.guardians.as_ref().map(|guardians| {
        let target = world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.current)
            .map(|id| {
                let region = positions
                    .binary_search_by_key(&id, |row| row.0)
                    .ok()
                    .and_then(|index| {
                        let block = &blocks[positions[index].1];
                        block
                            .journey
                            .is_none()
                            .then_some(world.regions[block.region].id)
                    });
                (id, region)
            });
        guardians.advance(target, &world.routes)
    });
    let strain = invocation.as_ref().and_then(|invocation| {
        (!invocation.active.is_empty()
            && invocation.burden >= invocation.rules.strain_threshold
            && invocation.rules.strain_damage_per_tick > 0)
            .then_some((invocation.owner?, invocation.rules.strain_damage_per_tick))
    });
    let lifespan_loss = invocation.as_ref().and_then(|invocation| {
        (!invocation.active.is_empty()
            && invocation.rules.lifespan_loss_per_tick > 0
            && invocation.exposure >= invocation.rules.permanent_exposure_threshold)
            .then_some((invocation.owner?, invocation.rules.lifespan_loss_per_tick))
    });
    Ok(Draft {
        invocation,
        strain,
        lifespan_loss,
        guardians,
        controlled,
    })
}
impl Draft {
    pub fn commit(
        self,
        world: &mut World,
        next: u64,
        family: Option<&game_parallel_family_lifecycle::Runtime>,
    ) {
        if let Some(invocation) = self.invocation {
            world.lineage.as_mut().unwrap().invocation = Some(invocation);
        }
        if let (Some(lineage), Some(family)) = (&mut world.lineage, family) {
            let alive = lineage.current.and_then(|id| family.alive(id));
            lineage.advance_boundary(alive, family.newborn(), next);
        }
        if let Some(guardians) = self.guardians {
            world.guardians = Some(guardians);
        }
    }
}

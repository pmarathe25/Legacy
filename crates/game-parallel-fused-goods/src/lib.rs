//! Packed custody and batch transfer for fused blocks.
use game_parallel::{GoodsTransfer, World};
use game_parallel_fused_data::{Block, NONE};
use game_parallel_goods_packed::{PackedCargo, PeopleView};
use rayon::prelude::*;
pub fn extract(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
) -> Option<PackedCargo> {
    // Construction reads and mutates the shared custody view after trade publication.
    if (world
        .cargo
        .as_ref()
        .is_some_and(|cargo| cargo.auto_eat_local || cargo.auto_eat_travel)
        && world
            .production
            .as_ref()
            .is_some_and(|production| production.goods.iter().any(|good| good.food.is_some())))
        || world
            .cargo
            .as_ref()
            .is_some_and(|cargo| !cargo.food_store_builders.is_empty())
        || world.market.is_some()
        || world
            .agreements
            .as_ref()
            .is_some_and(|agreements| agreements.policies.iter().any(|policy| policy.automatic()))
        || world.goods_cargo.is_none()
    {
        return None;
    }
    let people = game_parallel_fused_index::people(blocks, &world.regions, positions);
    game_parallel_block_goods::extract(world, true, &people)
}
pub fn export(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    packed: Option<&PackedCargo>,
) {
    if packed.is_some() {
        let people = game_parallel_fused_index::people(blocks, &world.regions, positions);
        game_parallel_block_goods::export(world, packed, &people);
    }
}
pub fn extend(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    packed: &mut Option<PackedCargo>,
) {
    if let Some(packed) = packed {
        let people = game_parallel_fused_index::people(blocks, &world.regions, positions);
        packed.extend_people(&people);
    }
}
struct View<'a> {
    regions: &'a [game_parallel::Region],
    blocks: &'a [Block],
    positions: &'a [(u64, usize, usize)],
    dense: Option<u64>,
}
impl PeopleView for View<'_> {
    fn len(&self) -> usize {
        self.positions.len()
    }
    fn actor(&self, id: u64) -> Option<(usize, Option<u64>, u32, bool)> {
        let index = if let Some(first) = self.dense {
            usize::try_from(id.checked_sub(first)?)
                .ok()
                .filter(|&index| index < self.positions.len())?
        } else {
            self.positions.binary_search_by_key(&id, |row| row.0).ok()?
        };
        let (_, block, offset) = self.positions[index];
        let block = &self.blocks[block];
        Some((
            index,
            block
                .journey
                .is_none()
                .then_some(self.regions[block.region].id),
            block.health[offset],
            block.work[offset] != NONE,
        ))
    }
}
pub fn transfer(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    packed: &mut Option<PackedCargo>,
    requests: &[GoodsTransfer],
) -> Result<(), String> {
    if world.conflict.is_some() || world.care.is_some() {
        game_parallel_fused_conflict::check(
            world,
            &requests
                .par_iter()
                .map(|request| request.person)
                .collect::<Vec<_>>(),
        )?;
    }
    if packed.is_some() {
        if world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.suspended_person(world.tick))
            .is_some_and(|person| requests.par_iter().any(|request| request.person == person))
        {
            return Err("player control is suspended".into());
        }
        if let Some(property) = &world.property {
            let people = game_parallel_fused_index::people(blocks, &world.regions, positions);
            property.check_transfers(&people, requests)?;
        }
        let dense = positions.first().and_then(|first| {
            positions.last().and_then(|last| {
                (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64))
                    .then_some(first.0)
            })
        });
        let regions = &world.regions;
        let view = View {
            regions,
            blocks,
            positions,
            dense,
        };
        let production = world.production.as_mut().ok_or("production is disabled")?;
        return packed.as_mut().unwrap().transfer_view(
            &mut production.inventories,
            &view,
            requests,
        );
    }
    let people = game_parallel_fused_index::people(blocks, &world.regions, positions);
    let busy: Vec<_> = blocks
        .par_iter()
        .flat_map_iter(|block| {
            block
                .ids
                .iter()
                .zip(&block.work)
                .filter_map(|(&id, &handle)| (handle != NONE).then_some(id))
        })
        .collect();
    game_parallel_block_goods::Context {
        world,
        people: &people,
        packed,
    }
    .transfer(requests, &busy)
}

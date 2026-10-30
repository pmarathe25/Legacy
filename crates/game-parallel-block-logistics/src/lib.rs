//! Stage food shipments against compiled regional food totals.
use game_parallel::{Region, Rules, Shipment, World};
use rayon::prelude::*;
pub fn advance(
    regions: &[Region],
    food: &[u32],
    shipments: &[Shipment],
    rules: &Rules,
) -> (Vec<u32>, Vec<Shipment>) {
    let mut regions: Vec<_> = regions.par_iter().cloned().collect();
    regions
        .par_iter_mut()
        .zip(food.par_iter())
        .for_each(|(region, &food)| region.food = food);
    let mut shipments: Vec<_> = shipments.par_iter().cloned().collect();
    game_parallel_logistics::advance(&mut regions, &mut shipments, rules);
    (
        regions.par_iter().map(|region| region.food).collect(),
        shipments,
    )
}
pub fn launch(world: &mut World, food: &mut [u32], requests: &[Shipment]) -> Result<(), String> {
    let mut regions: Vec<_> = world.regions.par_iter().cloned().collect();
    regions
        .par_iter_mut()
        .zip(food.par_iter())
        .for_each(|(region, &food)| region.food = food);
    let mut shipments: Vec<_> = world.shipments.par_iter().cloned().collect();
    game_parallel_logistics::Context {
        regions: &mut regions,
        routes: &world.routes,
        shipments: &mut shipments,
    }
    .launch_shipments(requests)?;
    food.par_iter_mut()
        .zip(regions.par_iter())
        .for_each(|(food, region)| *food = region.food);
    world.shipments = shipments;
    Ok(())
}

//! Host adapter for family-enabled atomic staging.
use crate::World;
pub(super) fn step(world: &mut World, next: u64, busy: &[u64]) -> Result<(), &'static str> {
    game_parallel_family_host::step(
        &mut game_parallel_family_host::Context {
            regions: &mut world.regions,
            travelers: &mut world.travelers,
            shipments: &mut world.shipments,
            agriculture: &mut world.agriculture,
            cargo: &mut world.cargo,
            families: &mut world.families,
            rules: &world.rules,
            seed: world.seed,
            tick: world.tick,
        },
        next,
        busy,
    )?;
    world.tick = next;
    Ok(())
}

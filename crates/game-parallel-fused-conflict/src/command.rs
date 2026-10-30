//! Checked commands; all validation precedes publication.
use game_parallel::{CombatCommand, CombatEncounter, TacticalBattle, TacticalMove, World};
use game_parallel_fused_data::Block;
pub enum Command<'a> {
    Begin(CombatEncounter, Option<TacticalBattle>),
    Orders(&'a [CombatCommand]),
    Move(&'a [TacticalMove]),
}
pub fn execute(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    runtime: Option<&mut super::packed::Runtime>,
    revision: &mut u64,
    command: Command<'_>,
) -> Result<u64, String> {
    let next = revision.checked_add(1).ok_or("generation overflow")?;
    let result = apply(
        world,
        blocks,
        positions,
        runtime.ok_or("conflict is disabled")?,
        command,
    )?;
    *revision = next;
    Ok(result)
}
fn apply(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    runtime: &mut super::packed::Runtime,
    command: Command<'_>,
) -> Result<u64, String> {
    match command {
        Command::Begin(fight, battle) => {
            let tactics = match (world.tactics.as_ref(), battle) {
                (None, None) => None,
                (Some(state), Some(battle)) => {
                    let mut next = state.clone();
                    let mut conflict = world.conflict.clone().ok_or("conflict is disabled")?;
                    conflict.encounters.push(fight.clone());
                    next.battles.push(battle);
                    let regions = world.regions.iter().map(|region| region.id).collect();
                    next.validate_world(&conflict, &regions, world.tick)?;
                    Some(next)
                }
                _ => {
                    return Err(
                        "tactical combat requires enabled tactics and participant placements"
                            .into(),
                    );
                }
            };
            let id = super::begin(world, blocks, positions, fight)?;
            world.tactics = tactics;
            runtime.invalidate();
            runtime.invalidate_tactics();
            Ok(id)
        }
        Command::Orders(commands) => {
            super::order(world, commands)?;
            runtime.orders(world.conflict.as_ref().expect("enabled conflict"), commands);
            Ok(0)
        }
        Command::Move(commands) => {
            if world
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(world.tick))
                .is_some_and(|person| commands.iter().any(|command| command.person == person))
            {
                return Err("player control is suspended".into());
            }
            runtime
                .move_tactical(
                    world.tactics.as_mut().ok_or("tactics is disabled")?,
                    commands,
                    world.tick,
                )
                .map_err(str::to_owned)?;
            Ok(0)
        }
    }
}

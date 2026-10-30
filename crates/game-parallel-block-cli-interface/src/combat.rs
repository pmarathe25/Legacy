//! Text commands for combat encounters.
use game_parallel::{CombatCommand, CombatEncounter, CombatFighter, CombatOrder};
use game_parallel_fused_physical::Kernel;
fn number<T: std::str::FromStr>(
    words: &mut std::str::SplitWhitespace<'_>,
    name: &str,
) -> Result<T, String> {
    words
        .next()
        .ok_or_else(|| format!("missing {name}"))?
        .parse()
        .map_err(|_| format!("invalid {name}"))
}
fn end(words: &mut std::str::SplitWhitespace<'_>) -> Result<(), String> {
    if words.next().is_some() {
        Err("unexpected argument".into())
    } else {
        Ok(())
    }
}
pub fn handle(engine: &mut Kernel, line: &str) -> Result<bool, String> {
    let mut words = line.split_whitespace();
    match words.next() {
        Some("combat") => {
            end(&mut words)?;
            let world = engine.export();
            let conflict = world.conflict.as_ref().ok_or("conflict is disabled")?;
            for fight in &conflict.encounters {
                println!(
                    "encounter={} region={} fighters={:?}",
                    fight.id, fight.region, fight.fighters
                );
            }
            for event in &conflict.events {
                println!(
                    "tick={} encounter={} attacker={} target={} damage={}",
                    event.tick, event.encounter, event.attacker, event.target, event.damage
                );
            }
        }
        Some(command @ ("fight" | "tactical-fight")) => {
            let id = number(&mut words, "encounter")?;
            let region = number(&mut words, "region")?;
            let a = number(&mut words, "fighter")?;
            let b = number(&mut words, "fighter")?;
            let placements = if command == "tactical-fight" {
                let mut people = Vec::new();
                for person in [a, b] {
                    people.push(game_parallel::TacticalPlacement {
                        person,
                        position: game_parallel::TacticalPoint {
                            x: number(&mut words, "x")?,
                            y: number(&mut words, "y")?,
                        },
                        last_move: None,
                    });
                }
                Some(game_parallel::TacticalBattle {
                    encounter: id,
                    region,
                    people,
                })
            } else {
                None
            };
            end(&mut words)?;
            let fight = CombatEncounter {
                id,
                region,
                fighters: vec![
                    CombatFighter {
                        person: a,
                        side: 1,
                        order: CombatOrder::Attack { target: b },
                    },
                    CombatFighter {
                        person: b,
                        side: 2,
                        order: CombatOrder::Attack { target: a },
                    },
                ],
            };
            let id = match placements {
                Some(battle) => engine.begin_tactical_combat(fight, battle)?,
                None => engine.begin_combat(fight)?,
            };
            println!("encounter={id}");
        }
        Some("tactical-move") => {
            let encounter = number(&mut words, "encounter")?;
            let person = number(&mut words, "fighter")?;
            let destination = game_parallel::TacticalPoint {
                x: number(&mut words, "x")?,
                y: number(&mut words, "y")?,
            };
            end(&mut words)?;
            engine.tactical_moves(&[game_parallel::TacticalMove {
                encounter,
                person,
                destination,
            }])?;
        }
        Some("combat-order") => {
            let encounter = number(&mut words, "encounter")?;
            let person = number(&mut words, "fighter")?;
            let order = match words.next() {
                Some("attack") => CombatOrder::Attack {
                    target: number(&mut words, "target")?,
                },
                Some("defend") => CombatOrder::Defend,
                Some("retreat") => CombatOrder::Retreat,
                _ => return Err("order must be attack, defend, or retreat".into()),
            };
            end(&mut words)?;
            engine.combat_orders(&[CombatCommand {
                encounter,
                person,
                order,
            }])?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

//! Stage agriculture using committed fused person blocks.
use game_parallel::{Agriculture, World};
use game_parallel_fused_data::{Block, NONE};
use rayon::prelude::*;
use std::collections::HashSet;
pub fn prepare(
    world: &World,
    blocks: &[Block],
    food: &[u32],
    planned: &[u64],
    tick: u64,
) -> Option<(Agriculture, Vec<u32>)> {
    let mut agriculture = world.agriculture.clone()?;
    let rules = &agriculture.rules;
    let volunteers = agriculture.volunteers_index();
    let mut leaving = HashSet::new();
    if let Some(recruitment) = agriculture
        .recruitment
        .as_ref()
        .filter(|rules| tick.is_multiple_of(rules.interval))
    {
        let farms: HashSet<_> = agriculture
            .farms
            .par_iter()
            .filter(|farm| !farm.fields.is_empty())
            .map(|farm| farm.region)
            .collect();
        let refusals: HashSet<_> = agriculture.refusals.par_iter().copied().collect();
        let (mut joining, departed): (Vec<_>, Vec<_>) = blocks
            .par_iter()
            .filter(|block| {
                block.journey.is_none() && farms.contains(&world.regions[block.region].id)
            })
            .map(|block| {
                let mut joining = Vec::new();
                let mut departed = Vec::new();
                for i in 0..block.ids.len() {
                    let person = block.ids[i];
                    if block.work[i] != NONE
                        || planned.binary_search(&person).is_ok()
                        || block.health[i] == 0
                        || block.health[i] < rules.min_health
                        || block.hunger[i] > rules.max_hunger
                    {
                        continue;
                    }
                    if volunteers
                        .as_ref()
                        .is_some_and(|rows| rows.contains(&person))
                    {
                        if recruitment.leave_one_in != 0
                            && game_parallel_genetics::random(
                                world.seed ^ 0x6c65617665,
                                person,
                                tick,
                            )
                            .is_multiple_of(recruitment.leave_one_in)
                        {
                            departed.push(person);
                        }
                    } else if !refusals.contains(&person)
                        && game_parallel_genetics::random(world.seed ^ 0x6661726d, person, tick)
                            .is_multiple_of(recruitment.one_in)
                    {
                        joining.push(person);
                    }
                }
                (joining, departed)
            })
            .reduce(
                || (Vec::new(), Vec::new()),
                |(mut a, mut b), (c, d)| {
                    a.extend(c);
                    b.extend(d);
                    (a, b)
                },
            );
        if joining.par_windows(2).any(|pair| pair[0] > pair[1]) {
            joining.par_sort_unstable();
        }
        leaving = departed.into_par_iter().collect();
        let members = agriculture
            .volunteers
            .as_mut()
            .expect("validated recruitment");
        if !leaving.is_empty() {
            *members = members
                .par_iter()
                .copied()
                .filter(|person| !leaving.contains(person))
                .collect();
        }
        members.par_extend(joining);
    }
    let block_counts: Vec<_> = blocks
        .par_iter()
        .map(|block| {
            let eligible = if block.journey.is_some() {
                0
            } else {
                (0..block.ids.len())
                    .filter(|&i| {
                        volunteers
                            .as_ref()
                            .is_none_or(|rows| rows.contains(&block.ids[i]))
                            && (leaving.is_empty() || !leaving.contains(&block.ids[i]))
                            && block.work[i] == NONE
                            && planned.binary_search(&block.ids[i]).is_err()
                            && block.health[i] > 0
                            && block.health[i] >= rules.min_health
                            && block.hunger[i] <= rules.max_hunger
                    })
                    .count()
            };
            (block.region, eligible)
        })
        .collect();
    let mut counts = vec![0usize; world.regions.len()];
    for (region, count) in block_counts {
        counts[region] += count;
    }
    let mut regions = world.regions.clone();
    for (region, &amount) in regions.iter_mut().zip(food) {
        region.food = amount;
    }
    let window = world
        .environment
        .as_ref()
        .and_then(|environment| environment.season(tick))
        .map_or_else(game_parallel_agriculture::FarmWindow::default, |season| {
            game_parallel_agriculture::FarmWindow {
                planting: season.planting_allowed,
                tending: season.tending_allowed,
                harvesting: season.harvest_allowed,
            }
        });
    game_parallel_agriculture::advance_counts_window(
        &mut regions,
        &mut agriculture,
        world.rules.food_capacity,
        &counts,
        window,
    );
    Some((
        agriculture,
        regions.into_iter().map(|region| region.food).collect(),
    ))
}
/// Attach an explicit, individual farm-labor preference to the fused host.
#[macro_export]
macro_rules! volunteer_methods {
    () => {
        pub fn set_farm_volunteer(&mut self, person: u64, willing: bool) -> Result<(), String> {
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
            let owner = self
                .template
                .agriculture
                .as_mut()
                .ok_or("agriculture is disabled")?;
            let volunteers = owner
                .volunteers
                .as_mut()
                .ok_or("farm labor is unrestricted in this scenario")?;
            volunteers.sort_unstable();
            match (volunteers.binary_search(&person), willing) {
                (Err(index), true) => volunteers.insert(index, person),
                (Ok(index), false) => {
                    volunteers.remove(index);
                }
                _ => {}
            }
            owner.refusals.sort_unstable();
            match (owner.refusals.binary_search(&person), willing) {
                (Ok(index), true) => {
                    owner.refusals.remove(index);
                }
                (Err(index), false) => owner.refusals.insert(index, person),
                _ => {}
            }
            self.revision = revision;
            Ok(())
        }
    };
}

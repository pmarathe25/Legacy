//! Bounded, person-owned alternatives for combining carried components.
use super::*;
use game_mechanics::{Apparatus, Assembly, Part};

const PART_SEARCH_LIMIT: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CandidatePart {
    rank: u64,
    item: u64,
    component: u32,
}

fn assemble(
    seed: u64,
    round: u64,
    actor: u64,
    input: CandidatePart,
    outputs: &[CandidatePart],
) -> Assembly {
    let mut id = game_genetics::random(seed ^ 0x6d65_6368_6465_7369, actor, round);
    for part in std::iter::once(&input).chain(outputs) {
        id = game_genetics::random(id, part.item, u64::from(part.component));
    }
    let part = |proposal: CandidatePart, parent, share_per_mille| Part {
        id: proposal.item,
        component: proposal.component,
        parent,
        share_per_mille,
        broken: false,
    };
    let mut parts = Vec::with_capacity(outputs.len() + 1);
    parts.push(part(input, None, 1000));
    let share_per_mille = 1000 / outputs.len() as u16;
    parts.extend(
        outputs
            .iter()
            .map(|&candidate| part(candidate, Some(input.item), share_per_mille)),
    );
    parts.sort_unstable_by_key(|row| row.id);
    Assembly {
        actor,
        apparatus: Apparatus {
            id: id.max(1),
            parts: std::sync::Arc::new(parts),
        },
    }
}

impl CoreHost {
    /// Generate a bounded set of viable shapes without inspecting component ratings.
    pub fn preview_mechanical_designs(
        &self,
        designers: &[u64],
        occupied: &[u64],
    ) -> Result<Vec<Assembly>, String> {
        if designers.par_windows(2).any(|row| row[0] >= row[1])
            || occupied.par_windows(2).any(|row| row[0] >= row[1])
        {
            return Err("invalid mechanical design people".into());
        }
        let Some(owner) = self.mechanics.as_ref().filter(|_| !designers.is_empty()) else {
            return Ok(Vec::new());
        };
        self.core.elapsed_millis().ok_or("no physical interval")?;
        let round = self.core.decision_time_key();
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|row| row.phase == game_lot_runtime::Phase::Working)
            .map(|row| row.job.worker)
            .collect();
        let mut designs: Vec<_> = designers
            .par_iter()
            .flat_map_iter(|&actor| {
                (|| {
                    let person = indexed(&self.core.people, actor, |row| row.id)?;
                    if person.controlled
                        || person.health == 0
                        || person.region.is_none()
                        || occupied.binary_search(&actor).is_ok()
                        || active.contains(&actor)
                    {
                        return None;
                    }
                    let container = self
                        .core
                        .physical
                        .custody(game_lots::Place::Person(actor))?;
                    let mut parts = Vec::with_capacity(PART_SEARCH_LIMIT + 1);
                    for lot in &container.lots {
                        let Some(item) = lot.item.filter(|&item| !owner.owns_part(item)) else {
                            continue;
                        };
                        let index = owner
                            .materials_by_good
                            .partition_point(|row| row.good < lot.good);
                        let Some(row) = owner
                            .materials_by_good
                            .get(index)
                            .filter(|row| row.good == lot.good)
                        else {
                            continue;
                        };
                        let candidate = CandidatePart {
                            rank: game_genetics::random(self.core.seed ^ actor, item, round),
                            item,
                            component: row.component,
                        };
                        let position = parts.partition_point(|part| *part < candidate);
                        if position < PART_SEARCH_LIMIT {
                            parts.insert(position, candidate);
                            parts.truncate(PART_SEARCH_LIMIT);
                        }
                    }
                    let mut designs = Vec::new();
                    for &input in &parts {
                        for &output in parts.iter().filter(|output| output.item != input.item) {
                            designs.push(assemble(self.core.seed, round, actor, input, &[output]));
                        }
                    }
                    if let Some(parts) = parts.get(..3) {
                        for &input in parts {
                            let outputs: Vec<_> = parts
                                .iter()
                                .copied()
                                .filter(|part| part.item != input.item)
                                .collect();
                            designs.push(assemble(self.core.seed, round, actor, input, &outputs));
                        }
                    }
                    designs.retain(|design| {
                        indexed(owner.world.apparatus(), design.apparatus.id, |row| row.id)
                            .is_none()
                    });
                    Some(designs)
                })()
                .unwrap_or_default()
            })
            .collect();
        designs.par_sort_unstable_by_key(|design| (design.actor, design.apparatus.id));
        designs.dedup_by_key(|design| (design.actor, design.apparatus.id));
        Ok(designs)
    }

    pub(in crate::core_interval::host) fn mechanical_design_value(&self, design: &Assembly) -> i64 {
        let Some(preferences) = self
            .mechanics
            .as_ref()
            .and_then(|owner| indexed(&owner.memories, design.actor, |row| row.person))
            .and_then(|memory| memory.design.as_ref())
        else {
            return 1;
        };
        let input = design
            .apparatus
            .parts
            .iter()
            .find(|part| part.parent.is_none())
            .expect("generated root");
        let input_value = indexed(&preferences.inputs, u64::from(input.component), |row| {
            u64::from(row.component)
        })
        .map_or(0, |row| row.value);
        let extra_parts = design.apparatus.parts.len().saturating_sub(2) as i64;
        1i64.saturating_add(input_value)
            .saturating_add(preferences.extra_part_value.saturating_mul(extra_parts))
    }
}

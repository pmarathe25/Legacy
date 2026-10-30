//! Cached strike generations and sparse transactional roster edits.
use game_parallel::{CombatEncounter, CombatEvent, CombatOrder, Conflict, Region};
use game_parallel_conflict_overlay::{Plan, Round};
use game_parallel_fused_data::Block;
use rayon::prelude::*;

enum Receipts {
    Raw(Vec<CombatEvent>),
    Packed(Round),
}
pub struct Runtime {
    plan: Option<Plan>,
    receipts: Receipts,
    tactics: Option<game_parallel_tactics::Index>,
}
pub struct Draft {
    round: Round,
    plan: Plan,
    edits: Vec<(usize, Option<CombatEncounter>)>,
}
impl Draft {
    /// Expand individual strikes only when a dependent owner needs attribution.
    pub fn experienced_harm(&self) -> Vec<(u64, u64, u32)> {
        self.round
            .expand()
            .into_par_iter()
            .filter(|event| event.damage > 0)
            .map(|event| (event.target, event.attacker, event.damage))
            .collect()
    }
}

fn locate(positions: &[(u64, usize, usize)], id: u64) -> Option<(usize, usize)> {
    let first = positions.first()?.0;
    let last = positions.last()?.0;
    let index = if last.checked_sub(first) == Some((positions.len() - 1) as u64) {
        usize::try_from(id.checked_sub(first)?)
            .ok()
            .filter(|&i| i < positions.len())?
    } else {
        positions.binary_search_by_key(&id, |row| row.0).ok()?
    };
    Some((positions[index].1, positions[index].2))
}

impl Runtime {
    pub fn compile(conflict: &mut Conflict) -> Result<Self, &'static str> {
        let plan = Plan::compile(conflict)?;
        Ok(Self {
            plan: Some(plan),
            receipts: Receipts::Raw(std::mem::take(&mut conflict.events)),
            tactics: None,
        })
    }
    /// Commands change topology only after their receiver validates the batch.
    /// Old receipts retain their own immutable generation.
    pub fn invalidate(&mut self) {
        self.plan = None;
    }
    pub fn orders(&mut self, conflict: &Conflict, commands: &[game_parallel::CombatCommand]) {
        if self
            .plan
            .as_mut()
            .is_some_and(|plan| !plan.orders(conflict, commands))
        {
            self.plan = None;
        }
    }
    pub fn invalidate_tactics(&mut self) {
        self.tactics = None;
    }
    pub fn move_tactical(
        &mut self,
        state: &mut game_parallel::Tactics,
        commands: &[game_parallel::TacticalMove],
        tick: u64,
    ) -> Result<(), &'static str> {
        if self.tactics.is_none() {
            self.tactics = Some(state.index()?);
        }
        self.tactics
            .as_mut()
            .unwrap()
            .move_batch(state, commands, tick)
    }
    pub fn prepare(
        &mut self,
        conflict: &Conflict,
        tactics: Option<&game_parallel::Tactics>,
        blocks: &[Block],
        positions: &[(u64, usize, usize)],
        regions: &[Region],
        tick: u64,
    ) -> Result<Draft, &'static str> {
        if self.plan.is_none() {
            self.plan = Some(Plan::compile(conflict)?);
        }
        let plan = self.plan.as_ref().unwrap();
        if self.tactics.is_none() {
            self.tactics = tactics.map(|state| state.index()).transpose()?;
        }
        let round = plan.prepare_with_filter(
            tick,
            |person, region| {
                locate(positions, person).is_some_and(|(b, i)| {
                    blocks[b].health[i] > 0
                        && blocks[b].journey.is_none()
                        && regions[blocks[b].region].id == region
                })
            },
            |attacker, target| {
                self.tactics
                    .as_ref()
                    .is_none_or(|index| index.can_attack_between(attacker, target))
            },
        );
        Ok(Draft {
            round,
            plan: plan.clone(),
            edits: Vec::new(),
        })
    }
    pub fn commit(
        &mut self,
        conflict: &mut Conflict,
        tactics: Option<&mut game_parallel::Tactics>,
        draft: Draft,
    ) {
        if !draft.edits.is_empty() {
            if let Some(tactics) = tactics {
                // Membership only removes actors. Unchanged positions in the cached
                // index remain valid; the filtered strike plan cannot address removals.
                let changes: std::collections::HashMap<_, _> = draft
                    .edits
                    .iter()
                    .map(|(index, fight)| (conflict.encounters[*index].id, fight.as_ref()))
                    .collect();
                tactics.sync_changed(&changes, self.tactics.as_mut());
            }
            let mut edits = draft.edits.into_iter().peekable();
            conflict.encounters = std::mem::take(&mut conflict.encounters)
                .into_iter()
                .enumerate()
                .filter_map(|(index, fight)| {
                    if edits.peek().is_some_and(|edit| edit.0 == index) {
                        edits.next().unwrap().1
                    } else {
                        Some(fight)
                    }
                })
                .collect();
        }
        conflict.events.clear();
        self.plan = Some(draft.plan);
        self.receipts = Receipts::Packed(draft.round);
    }
    pub fn export(&self, conflict: &mut Conflict) {
        conflict.events = match &self.receipts {
            Receipts::Raw(events) => events.par_iter().cloned().collect(),
            Receipts::Packed(round) => round.expand(),
        };
    }
}

pub fn apply(draft: Option<&Draft>, blocks: &mut [Block], positions: &[(u64, usize, usize)]) {
    let Some(draft) = draft else {
        return;
    };
    let mut damage: Vec<_> = draft
        .round
        .damage()
        .par_iter()
        .map(|damage| {
            let (block, offset) =
                locate(positions, damage.target).expect("validated conflict target");
            (block, offset, damage.amount)
        })
        .collect();
    if damage
        .par_windows(2)
        .any(|pair| (pair[0].0, pair[0].1) > (pair[1].0, pair[1].1))
    {
        damage.par_sort_unstable_by_key(|row| (row.0, row.1));
    }
    blocks
        .par_iter_mut()
        .enumerate()
        .for_each(|(index, block)| {
            let start = damage.partition_point(|row| row.0 < index);
            let end = damage.partition_point(|row| row.0 <= index);
            for &(_, offset, amount) in &damage[start..end] {
                block.next_health[offset] = block.next_health[offset].saturating_sub(amount);
            }
        });
}

/// Stage only encounters whose membership changes. Physical/lifecycle state is
/// still unpublished. Unchanged encounters and every old receipt stay untouched.
pub fn finish(
    draft: Option<&mut Draft>,
    conflict: Option<&Conflict>,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
) {
    let (Some(draft), Some(conflict)) = (draft, conflict) else {
        return;
    };
    let mut retreats: Vec<_> = conflict
        .encounters
        .par_iter()
        .flat_map(|fight| {
            fight
                .fighters
                .par_iter()
                .filter(|f| matches!(f.order, CombatOrder::Retreat))
                .map(|f| f.person)
        })
        .collect();
    retreats.par_sort_unstable();
    let survives = |id| {
        retreats.binary_search(&id).is_err()
            && locate(positions, id).is_some_and(|(b, i)| blocks[b].next_health[i] > 0)
    };
    let edits: Vec<_> = conflict
        .encounters
        .par_iter()
        .enumerate()
        .filter_map(|(index, fight)| {
            if fight.fighters.par_iter().all(|f| survives(f.person)) {
                return None;
            }
            let fighters: Vec<_> = fight
                .fighters
                .par_iter()
                .filter(|f| survives(f.person))
                .map(|fighter| {
                    let mut next = fighter.clone();
                    if matches!(next.order, CombatOrder::Attack { target } if !survives(target)) {
                        next.order = CombatOrder::Defend;
                    }
                    next
                })
                .collect();
            let keep = fighters
                .first()
                .is_some_and(|first| fighters.par_iter().any(|f| f.side != first.side));
            Some((
                index,
                keep.then_some(CombatEncounter {
                    id: fight.id,
                    region: fight.region,
                    fighters,
                }),
            ))
        })
        .collect();
    if edits.is_empty() {
        return;
    }
    let changed: std::collections::HashMap<_, _> = edits
        .iter()
        .filter_map(|(_, fight)| fight.as_ref().map(|fight| (fight.id, fight)))
        .collect();
    draft.plan = draft.plan.filtered(survives, |encounter, id| {
        changed.get(&encounter).is_some_and(|fight| {
            fight
                .fighters
                .binary_search_by_key(&id, |fighter| fighter.person)
                .ok()
                .is_some_and(|index| matches!(fight.fighters[index].order, CombatOrder::Defend))
        })
    });
    draft.edits = edits;
}

//! Indexed identity joins for family phases.
use game_parallel_family_model::{LifeInput, Member, PhysicalView};
use rayon::prelude::*;
pub struct PhysicalIndex<'a> {
    pub rows: Vec<&'a LifeInput>,
    dense: Option<u64>,
}
impl<'a> PhysicalIndex<'a> {
    /// Build from a private host's sorted, unique identity index.
    pub fn from_ordered(inputs: &'a [LifeInput]) -> Self {
        let rows: Vec<_> = inputs.par_iter().collect();
        let dense = rows.first().and_then(|first| {
            rows.last().and_then(|last| {
                (last.id.checked_sub(first.id) == Some((rows.len() - 1) as u64)).then_some(first.id)
            })
        });
        Self { rows, dense }
    }
    pub fn new(inputs: &'a [LifeInput]) -> Result<Self, &'static str> {
        let mut rows: Vec<_> = inputs.par_iter().collect();
        if rows.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            rows.par_sort_unstable_by_key(|p| p.id);
        }
        if rows.par_windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err("family identity set or clock differs");
        }
        let dense = rows.first().and_then(|first| {
            rows.last().and_then(|last| {
                (last.id.checked_sub(first.id) == Some((rows.len() - 1) as u64)).then_some(first.id)
            })
        });
        Ok(Self { rows, dense })
    }
}
impl<'a> std::ops::Index<&u64> for PhysicalIndex<'a> {
    type Output = LifeInput;
    fn index(&self, id: &u64) -> &Self::Output {
        let index = self
            .dense
            .and_then(|first| id.checked_sub(first))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&index| index < self.rows.len())
            .or_else(|| {
                if self.dense.is_none() {
                    self.rows.binary_search_by_key(id, |p| p.id).ok()
                } else {
                    None
                }
            })
            .expect("validated person");
        self.rows[index]
    }
}
pub struct MemberIndex<'a> {
    pub rows: Vec<&'a Member>,
    dense: Option<u64>,
}
impl<'a> MemberIndex<'a> {
    pub fn new(members: &'a [Member]) -> Result<Self, &'static str> {
        let mut rows: Vec<_> = members.par_iter().collect();
        if rows.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            rows.par_sort_unstable_by_key(|m| m.id);
        }
        if rows.par_windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err("family identity set or clock differs");
        }
        let dense = rows.first().and_then(|first| {
            rows.last().and_then(|last| {
                (last.id.checked_sub(first.id) == Some((rows.len() - 1) as u64)).then_some(first.id)
            })
        });
        Ok(Self { rows, dense })
    }
    pub fn get(&self, id: &u64) -> Option<&'a Member> {
        self.dense
            .and_then(|first| id.checked_sub(first))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&index| index < self.rows.len())
            .or_else(|| {
                if self.dense.is_none() {
                    self.rows.binary_search_by_key(id, |m| m.id).ok()
                } else {
                    None
                }
            })
            .map(|index| self.rows[index])
    }
    pub fn contains_key(&self, id: &u64) -> bool {
        self.get(id).is_some()
    }
}
impl std::ops::Index<&u64> for MemberIndex<'_> {
    type Output = Member;
    fn index(&self, id: &u64) -> &Self::Output {
        self.get(id).expect("validated member")
    }
}

impl PhysicalView for PhysicalIndex<'_> {
    fn read(&self, id: u64) -> LifeInput {
        self[&id]
    }
}

/// Stage each person's independent choice to leave a no-longer-compatible partnership.
pub fn reconsider(
    members: &mut [Member],
    physical: &impl PhysicalView,
    fertile_until: u64,
) -> Result<(), &'static str> {
    if !members
        .par_iter()
        .any(|member| member.reconsider_partnership && member.partner.is_some())
    {
        return Ok(());
    }
    let index = MemberIndex::new(members)?;
    let mut separations: Vec<_> = members
        .par_iter()
        .filter_map(|member| {
            let other = index.get(&member.partner?)?;
            let wants = |own: &Member, mate: &Member| {
                !physical.controlled(own.id)
                    && own.reconsider_partnership
                    && own.alive
                    && own.reproduction_allowed
                    && own.age < fertile_until
                    && (!mate.reproduction_allowed || mate.age >= fertile_until)
            };
            (wants(member, other) || wants(other, member)).then_some(member.id)
        })
        .collect();
    separations.par_sort_unstable();
    members.par_iter_mut().for_each(|member| {
        if separations.binary_search(&member.id).is_ok() {
            member.partner = None;
        }
    });
    Ok(())
}

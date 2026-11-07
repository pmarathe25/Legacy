//! Indexed identity joins for family phases.
use game_family_model::{LifeInput, Member, PhysicalView};
use rayon::prelude::*;

/// Direct lookup for the contiguous suffix of a validated sorted, unique ID table.
pub struct DenseSuffix {
    first: u64,
    last: u64,
    offset: usize,
}
impl DenseSuffix {
    pub fn new(len: usize, id: impl Fn(usize) -> u64) -> Option<Self> {
        if len == 0 {
            return None;
        }
        let last = id(len - 1);
        let (mut low, mut high) = (0, len - 1);
        while low < high {
            let middle = low + (high - low) / 2;
            if last.checked_sub(id(middle)) == u64::try_from(len - middle - 1).ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        Some(Self {
            first: id(low),
            last,
            offset: low,
        })
    }
    #[inline]
    pub fn index(&self, id: u64) -> Option<usize> {
        if id > self.last {
            return None;
        }
        id.checked_sub(self.first)
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| self.offset.checked_add(n))
    }
}
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
/// Borrow an already validated, sorted identity table without rebuilding references.
pub struct OrderedMemberIndex<'a> {
    rows: &'a [Member],
    suffix: Option<DenseSuffix>,
}
impl<'a> OrderedMemberIndex<'a> {
    pub fn new(rows: &'a [Member]) -> Self {
        Self {
            rows,
            suffix: DenseSuffix::new(rows.len(), |i| rows[i].id),
        }
    }
    pub fn get(&self, id: &u64) -> Option<&'a Member> {
        self.suffix
            .as_ref()
            .and_then(|suffix| suffix.index(*id))
            .or_else(|| self.rows.binary_search_by_key(id, |row| row.id).ok())
            .map(|index| &self.rows[index])
    }
}
impl std::ops::Index<&u64> for OrderedMemberIndex<'_> {
    type Output = Member;
    fn index(&self, id: &u64) -> &Self::Output {
        self.get(id).expect("validated member")
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use game_genetics::{Genome, Sex};

    fn member(id: u64) -> Member {
        Member {
            id,
            born: None,
            age: 20,
            lifespan_loss: 0,
            genome: Genome {
                sex: Sex::Female,
                alleles: [[0, 0]; 4],
            },
            parents: vec![].into(),
            partner: None,
            pregnancy: None,
            next_conception: 0,
            alive: true,
            reproduction_allowed: true,
            reconsider_partnership: false,
        }
    }

    #[test]
    fn borrowed_sorted_index_matches_validated_index_for_dense_sparse_and_empty_rows() {
        for ids in [
            &[][..],
            &[1, 2, 3][..],
            &[2, 7, 50][..],
            &[1, 2, 1000, 1001, 1002][..],
            &[u64::MAX - 1, u64::MAX][..],
        ] {
            let rows: Vec<_> = ids.iter().copied().map(member).collect();
            let validated = MemberIndex::new(&rows).unwrap();
            let borrowed = OrderedMemberIndex::new(&rows);
            for id in (0..=1003).chain([u64::MAX - 2, u64::MAX - 1, u64::MAX]) {
                assert_eq!(borrowed.get(&id), validated.get(&id));
            }
        }
    }
    #[test]
    fn dense_suffix_directly_indexes_only_the_contiguous_tail() {
        let ids = [1, 2, 1000, 1001, 1002];
        let suffix = DenseSuffix::new(ids.len(), |i| ids[i]).unwrap();
        assert_eq!(suffix.index(2), None);
        assert_eq!(suffix.index(1000), Some(2));
        assert_eq!(suffix.index(1002), Some(4));
        assert_eq!(suffix.index(1003), None);
    }
}

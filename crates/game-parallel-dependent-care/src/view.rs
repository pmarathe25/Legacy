//! The care phase reads only family identities, parents, ages, and survival.
use game_parallel_family_columns::Store;
use game_parallel_family_model::Member;
use rayon::prelude::*;

#[derive(Clone, Copy)]
pub struct CareMember {
    pub id: u64,
    pub age: u64,
    pub alive: bool,
    pub parents: [u64; 2],
    pub parent_count: usize,
}

/// An identity-ordered, validated family snapshot for caregiving.
pub trait CareFamily: Sync {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn member(&self, slot: usize) -> CareMember;
    fn locate(&self, id: u64) -> Option<usize>;
}

impl CareFamily for Store {
    fn len(&self) -> usize {
        self.metadata.len()
    }
    fn member(&self, slot: usize) -> CareMember {
        let meta = &self.metadata[slot];
        let state = self.state(slot, false);
        CareMember {
            id: meta.id,
            age: state.age,
            alive: state.alive,
            parents: meta.parents,
            parent_count: meta.parent_count,
        }
    }
    fn locate(&self, id: u64) -> Option<usize> {
        Store::locate(self, id)
    }
}

/// Borrows canonical family rows, allocating an index only for unordered input.
pub struct FamilyView<'a> {
    members: &'a [Member],
    order: Option<Vec<usize>>,
    dense: bool,
}
impl<'a> FamilyView<'a> {
    pub fn compile(members: &'a [Member]) -> Result<Self, &'static str> {
        if members
            .par_iter()
            .any(|m| m.id == 0 || ![0, 2].contains(&m.parents.len()))
        {
            return Err("invalid family metadata");
        }
        let order = if members.par_windows(2).any(|p| p[0].id > p[1].id) {
            let mut order: Vec<_> = (0..members.len()).into_par_iter().collect();
            order.par_sort_unstable_by_key(|&i| members[i].id);
            Some(order)
        } else {
            None
        };
        let mut view = Self {
            members,
            order,
            dense: false,
        };
        if (1..view.len())
            .into_par_iter()
            .any(|i| view.row(i - 1).id == view.row(i).id)
        {
            return Err("duplicate family identity");
        }
        view.dense = !view.is_empty()
            && view.row(view.len() - 1).id.checked_sub(view.row(0).id)
                == Some((view.len() - 1) as u64);
        if (0..view.len()).into_par_iter().any(|i| {
            view.row(i)
                .parents
                .iter()
                .any(|&id| view.locate(id).is_none())
        }) {
            return Err("missing family parent");
        }
        Ok(view)
    }
    fn row(&self, slot: usize) -> &Member {
        &self.members[self.order.as_ref().map_or(slot, |order| order[slot])]
    }
}
impl CareFamily for FamilyView<'_> {
    fn len(&self) -> usize {
        self.members.len()
    }
    fn member(&self, slot: usize) -> CareMember {
        let member = self.row(slot);
        let mut parents = [0; 2];
        parents[..member.parents.len()].copy_from_slice(&member.parents);
        CareMember {
            id: member.id,
            age: member.age,
            alive: member.alive,
            parents,
            parent_count: member.parents.len(),
        }
    }
    fn locate(&self, id: u64) -> Option<usize> {
        if self.dense {
            usize::try_from(id.checked_sub(self.row(0).id)?)
                .ok()
                .filter(|&i| i < self.len())
        } else {
            let (mut low, mut high) = (0, self.len());
            while low < high {
                let mid = low + (high - low) / 2;
                match self.row(mid).id.cmp(&id) {
                    std::cmp::Ordering::Less => low = mid + 1,
                    std::cmp::Ordering::Greater => high = mid,
                    std::cmp::Ordering::Equal => return Some(mid),
                }
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_parallel_genetics::{Genome, Sex};

    fn member(id: u64, parents: &[u64]) -> Member {
        Member {
            id,
            born: None,
            age: 3,
            lifespan_loss: 0,
            genome: Genome {
                sex: Sex::Female,
                alleles: [[0; 2]; 4],
            },
            parents: parents.iter().copied().collect(),
            partner: None,
            pregnancy: None,
            next_conception: 0,
            alive: true,
            reproduction_allowed: false,
            reconsider_partnership: false,
        }
    }

    #[test]
    fn view_validates_and_indexes_sparse_reordered_families() {
        for workers in [1, 4] {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let members = [member(30, &[10, 20]), member(10, &[]), member(20, &[])];
                    let view = FamilyView::compile(&members).unwrap();
                    assert_eq!(
                        (0..view.len())
                            .map(|i| view.member(i).id)
                            .collect::<Vec<_>>(),
                        [10, 20, 30]
                    );
                    assert_eq!(view.locate(20), Some(1));
                    assert_eq!(view.locate(21), None);
                    assert!(FamilyView::compile(&[member(1, &[]), member(1, &[])]).is_err());
                    assert!(FamilyView::compile(&[member(3, &[1, 2])]).is_err());
                });
        }
    }
}
